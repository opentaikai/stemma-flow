//! Fact catalogue persistence and read-only person facts.

use std::collections::HashMap;

use rusqlite::{Connection, params};

use crate::model::fact::{FactKind, FactScope, builtin_facts};

/// A stored fact type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactTypeRow {
    pub key: String,
    pub label: String,
    pub category: Option<String>,
    pub gedcom_tag: Option<String>,
    pub scope: FactScope,
    pub kind: FactKind,
    pub is_builtin: bool,
}

/// A read-only fact shown on a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonFact {
    pub event_id: String,
    pub event_type: String,
    pub label: String,
    pub date: Option<String>,
    pub place: Option<String>,
    pub value: Option<String>,
}

/// Inserts or refreshes the built-in catalogue without touching custom rows.
pub fn seed_fact_types(conn: &Connection) -> rusqlite::Result<()> {
    for fact in builtin_facts() {
        conn.execute(
            "INSERT OR IGNORE INTO fact_types
             (key, label, category, gedcom_tag, scope, kind, is_builtin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
            params![
                fact.key,
                fact.label,
                fact.category,
                fact.gedcom_tag,
                fact.scope.as_str(),
                fact.kind.as_str()
            ],
        )?;
        conn.execute(
            "UPDATE fact_types
             SET label = ?2, category = ?3, gedcom_tag = ?4, scope = ?5, kind = ?6
             WHERE key = ?1 AND is_builtin = 1",
            params![
                fact.key,
                fact.label,
                fact.category,
                fact.gedcom_tag,
                fact.scope.as_str(),
                fact.kind.as_str()
            ],
        )?;
    }
    Ok(())
}

/// Loads the fact catalogue ordered by category and label.
pub fn load_fact_types(conn: &Connection) -> Result<Vec<FactTypeRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT key, label, category, gedcom_tag, scope, kind, is_builtin
             FROM fact_types
             ORDER BY category COLLATE NOCASE, label COLLATE NOCASE, key",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let scope: String = row.get(4)?;
            let kind: String = row.get(5)?;
            Ok(FactTypeRow {
                key: row.get(0)?,
                label: row.get(1)?,
                category: row.get(2)?,
                gedcom_tag: row.get(3)?,
                scope: match scope.as_str() {
                    "family" => FactScope::Family,
                    "both" => FactScope::Both,
                    _ => FactScope::Person,
                },
                kind: match kind.as_str() {
                    "attribute" => FactKind::Attribute,
                    _ => FactKind::Event,
                },
                is_builtin: row.get::<_, i64>(6)? != 0,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads the non-vital facts for every person, grouped by person id.
pub fn load_person_facts(conn: &Connection) -> Result<HashMap<String, Vec<PersonFact>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT e.person_id, e.id, e.event_type,
                    COALESCE(f.label, e.event_type), e.date, p.name, e.description
             FROM events e
             LEFT JOIN fact_types f ON f.key = e.event_type
             LEFT JOIN places p ON p.id = e.place_id
             WHERE e.person_id IS NOT NULL
               AND e.event_type NOT IN ('BIRTH', 'DEATH')
             ORDER BY e.person_id, e.event_type, e.date, e.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                PersonFact {
                    event_id: row.get(1)?,
                    event_type: row.get(2)?,
                    label: row.get(3)?,
                    date: row.get(4)?,
                    place: row.get(5)?,
                    value: row.get(6)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut facts: HashMap<String, Vec<PersonFact>> = HashMap::new();
    for row in rows {
        let (person_id, fact) = row.map_err(|error| error.to_string())?;
        facts.entry(person_id).or_default().push(fact);
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma applies");
        init_db(&conn).expect("schema initialises");
        conn
    }

    #[test]
    fn seeds_the_built_in_catalogue() {
        let conn = connection();
        let kinds = load_fact_types(&conn).expect("fact types load");
        assert_eq!(kinds.len(), builtin_facts().len());
        assert!(kinds.iter().all(|fact| fact.is_builtin));
        let birth = kinds
            .iter()
            .find(|fact| fact.key == "BIRTH")
            .expect("birth");
        assert_eq!(birth.scope, FactScope::Person);
        assert_eq!(birth.kind, FactKind::Event);
        assert_eq!(birth.gedcom_tag.as_deref(), Some("BIRT"));
    }

    #[test]
    fn seeding_is_idempotent_and_custom_rows_survive() {
        let conn = connection();
        conn.execute(
            "INSERT INTO fact_types (key, label, category, gedcom_tag, scope, kind, is_builtin)
             VALUES ('CUSTOM', 'Custom', 'Other', '_CUST', 'person', 'event', 0)",
            [],
        )
        .expect("custom fact insert");
        let first = load_fact_types(&conn).expect("load").len();
        seed_fact_types(&conn).expect("reseed");
        let second = load_fact_types(&conn).expect("load").len();
        assert_eq!(first, second);
        assert_eq!(second, builtin_facts().len() + 1);
    }

    #[test]
    fn loads_person_facts_without_vitals() {
        let conn = connection();
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person insert");
        conn.execute_batch(
            "INSERT INTO places (id, name) VALUES ('pl1', 'Uppsala');
             INSERT INTO events (id, event_type, date, place_id, person_id, description) VALUES
                ('e1', 'BIRTH', '1850', 'pl1', 'p1', NULL),
                ('e2', 'OCCUPATION', '1870', 'pl1', 'p1', 'Teacher'),
                ('e3', 'CENSUS', '1860', NULL, 'p1', NULL);",
        )
        .expect("events insert");

        let facts = load_person_facts(&conn).expect("facts load");
        let person = facts.get("p1").expect("person facts");
        assert_eq!(person.len(), 2, "birth is excluded");
        assert_eq!(person[0].event_type, "CENSUS");
        assert_eq!(person[0].label, "Census");
        assert_eq!(person[1].event_type, "OCCUPATION");
        assert_eq!(person[1].label, "Occupation");
        assert_eq!(person[1].place.as_deref(), Some("Uppsala"));
        assert_eq!(person[1].value.as_deref(), Some("Teacher"));
    }
}
