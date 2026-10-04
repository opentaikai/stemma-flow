//! Event-centric relational schema (Gramps-style) for stemma-flow.

use rusqlite::Connection;

const SCHEMA_SQL: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE IF NOT EXISTS people (
    id TEXT PRIMARY KEY,
    given_name TEXT NOT NULL,
    surname TEXT NOT NULL,
    gender TEXT NOT NULL CHECK (gender IN ('M', 'F', 'U')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS families (
    id TEXT PRIMARY KEY,
    husband_id TEXT REFERENCES people(id) ON DELETE SET NULL,
    wife_id TEXT REFERENCES people(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS family_children (
    family_id TEXT NOT NULL REFERENCES families(id) ON DELETE CASCADE,
    child_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    PRIMARY KEY (family_id, child_id)
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    date TEXT,
    place TEXT,
    description TEXT,
    person_id TEXT REFERENCES people(id) ON DELETE CASCADE,
    family_id TEXT REFERENCES families(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS citations (
    id TEXT PRIMARY KEY,
    source_title TEXT NOT NULL,
    page_or_reference TEXT,
    notes TEXT,
    event_id TEXT REFERENCES events(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_families_husband ON families(husband_id);
CREATE INDEX IF NOT EXISTS idx_families_wife ON families(wife_id);
CREATE INDEX IF NOT EXISTS idx_family_children_child ON family_children(child_id);
CREATE INDEX IF NOT EXISTS idx_events_person ON events(person_id);
CREATE INDEX IF NOT EXISTS idx_events_family ON events(family_id);
CREATE INDEX IF NOT EXISTS idx_citations_event ON citations(event_id);
CREATE INDEX IF NOT EXISTS idx_people_names ON people(surname, given_name);

COMMIT;
"#;

/// Creates the stemma-flow tables and indexes if they do not exist yet.
///
/// Runs the whole DDL batch inside a single transaction; foreign-key and WAL
/// pragmas are the caller's responsibility (see [`crate::db::open_connection`]).
pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rusqlite::{Connection, params};

    use super::init_db;
    use crate::db::{Citation, Event, Family, Person, open_connection};

    const TABLES: [&str; 5] = [
        "people",
        "families",
        "family_children",
        "events",
        "citations",
    ];
    const INDEXES: [&str; 7] = [
        "idx_families_husband",
        "idx_families_wife",
        "idx_family_children_child",
        "idx_events_person",
        "idx_events_family",
        "idx_citations_event",
        "idx_people_names",
    ];

    struct Tree {
        husband: Person,
        wife: Person,
        child: Person,
        family: Family,
    }

    fn table_count(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get(0),
        )
    }

    fn index_count(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [name],
            |row| row.get(0),
        )
    }

    fn scalar_count(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
        conn.query_row(sql, [], |row| row.get(0))
    }

    fn insert_person(conn: &Connection, person: &Person) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender) VALUES (?1, ?2, ?3, ?4)",
            params![person.id, person.given_name, person.surname, person.gender],
        )?;
        Ok(())
    }

    fn insert_family(conn: &Connection, family: &Family) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO families (id, husband_id, wife_id) VALUES (?1, ?2, ?3)",
            params![family.id, family.husband_id, family.wife_id],
        )?;
        Ok(())
    }

    fn link_child(conn: &Connection, family_id: &str, child_id: &str) -> rusqlite::Result<()> {
        conn.execute(
            "INSERT INTO family_children (family_id, child_id) VALUES (?1, ?2)",
            params![family_id, child_id],
        )?;
        Ok(())
    }

    fn family_tree(conn: &Connection) -> rusqlite::Result<Tree> {
        let husband = Person::new("Johan", "Ahlberg", "M");
        let wife = Person::new("Maria", "Ekdahl", "F");
        let child = Person::new("Elsa", "Ahlberg", "F");
        insert_person(conn, &husband)?;
        insert_person(conn, &wife)?;
        insert_person(conn, &child)?;

        let family = Family::new(Some(husband.id.clone()), Some(wife.id.clone()));
        insert_family(conn, &family)?;
        link_child(conn, &family.id, &child.id)?;

        Ok(Tree {
            husband,
            wife,
            child,
            family,
        })
    }

    #[test]
    fn init_db_creates_all_tables_and_indexes() -> rusqlite::Result<()> {
        let conn = open_connection(":memory:")?;
        init_db(&conn)?;

        for table in TABLES {
            assert_eq!(table_count(&conn, table)?, 1, "missing table {table}");
        }
        for index in INDEXES {
            assert_eq!(index_count(&conn, index)?, 1, "missing index {index}");
        }

        init_db(&conn)?;
        for table in TABLES {
            assert_eq!(table_count(&conn, table)?, 1, "duplicate table {table}");
        }
        Ok(())
    }

    #[test]
    fn gender_is_constrained_and_timestamps_defaulted() -> rusqlite::Result<()> {
        let conn = open_connection(":memory:")?;
        init_db(&conn)?;

        let invalid = Person::new("Unknown", "Gender", "X");
        assert!(insert_person(&conn, &invalid).is_err());

        let person = Person::new("Lars", "Ohlin", "M");
        insert_person(&conn, &person)?;
        let (created_at, updated_at): (String, String) = conn.query_row(
            "SELECT created_at, updated_at FROM people WHERE id = ?1",
            [&person.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert!(!created_at.is_empty());
        assert!(!updated_at.is_empty());
        Ok(())
    }

    #[test]
    fn orphaned_family_children_need_foreign_keys_enabled() -> rusqlite::Result<()> {
        let enforced = open_connection(":memory:")?;
        init_db(&enforced)?;
        assert!(link_child(&enforced, "missing-family", "missing-child").is_err());

        // The bundled SQLite build compiles with SQLITE_DEFAULT_FOREIGN_KEYS=1,
        // so the pragma must be switched off to model a connection that never
        // enabled it.
        let lenient = Connection::open(":memory:")?;
        lenient.pragma_update(None, "foreign_keys", "OFF")?;
        init_db(&lenient)?;
        let fk: i64 = lenient.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        assert_eq!(fk, 0);
        link_child(&lenient, "missing-family", "missing-child")?;
        Ok(())
    }

    #[test]
    fn child_resolves_both_parents() -> rusqlite::Result<()> {
        let conn = open_connection(":memory:")?;
        init_db(&conn)?;
        let tree = family_tree(&conn)?;

        let mut parents = HashSet::new();
        let mut stmt = conn.prepare(
            "SELECT p.id
             FROM family_children fc
             JOIN families f ON f.id = fc.family_id
             JOIN people p ON p.id = f.husband_id OR p.id = f.wife_id
             WHERE fc.child_id = ?1",
        )?;
        let rows = stmt.query_map([&tree.child.id], |row| row.get::<_, String>(0))?;
        for id in rows {
            parents.insert(id?);
        }

        assert_eq!(parents.len(), 2);
        assert!(parents.contains(&tree.husband.id));
        assert!(parents.contains(&tree.wife.id));
        Ok(())
    }

    #[test]
    fn deleting_person_cascades_events_citations_and_child_links() -> rusqlite::Result<()> {
        let conn = open_connection(":memory:")?;
        init_db(&conn)?;
        let tree = family_tree(&conn)?;

        let birth = Event::new("BIRTH", Some(tree.child.id.clone()), None);
        conn.execute(
            "INSERT INTO events (id, event_type, person_id) VALUES (?1, ?2, ?3)",
            params![birth.id, birth.event_type, birth.person_id],
        )?;

        let mut citation = Citation::new("Parish register, Ornskoldsvik");
        citation.event_id = Some(birth.id.clone());
        conn.execute(
            "INSERT INTO citations (id, source_title, event_id) VALUES (?1, ?2, ?3)",
            params![citation.id, citation.source_title, citation.event_id],
        )?;

        conn.execute("DELETE FROM people WHERE id = ?1", [&tree.child.id])?;

        assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM events")?, 0);
        assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM citations")?, 0);
        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM family_children")?,
            0
        );
        assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM people")?, 2);
        Ok(())
    }

    #[test]
    fn deleting_family_cascades_child_links_and_nulls_spouses() -> rusqlite::Result<()> {
        let conn = open_connection(":memory:")?;
        init_db(&conn)?;
        let tree = family_tree(&conn)?;

        let marriage = Event::new("MARRIAGE", None, Some(tree.family.id.clone()));
        conn.execute(
            "INSERT INTO events (id, event_type, family_id) VALUES (?1, ?2, ?3)",
            params![marriage.id, marriage.event_type, marriage.family_id],
        )?;

        conn.execute("DELETE FROM people WHERE id = ?1", [&tree.husband.id])?;
        let (husband_id, wife_id): (Option<String>, Option<String>) = conn.query_row(
            "SELECT husband_id, wife_id FROM families WHERE id = ?1",
            [&tree.family.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert!(husband_id.is_none());
        assert_eq!(wife_id.as_deref(), Some(tree.wife.id.as_str()));

        conn.execute("DELETE FROM families WHERE id = ?1", [&tree.family.id])?;
        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM family_children")?,
            0
        );
        assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM events")?, 0);

        let wife_rows: i64 = conn.query_row(
            "SELECT COUNT(*) FROM people WHERE id = ?1",
            [&tree.wife.id],
            |row| row.get(0),
        )?;
        assert_eq!(wife_rows, 1);
        conn.execute("DELETE FROM people WHERE id = ?1", [&tree.wife.id])?;
        assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM people")?, 1);
        Ok(())
    }
}
