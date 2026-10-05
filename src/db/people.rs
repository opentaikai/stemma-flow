//! Person-level CRUD and relational link helpers.
//!
//! Every write runs inside a transaction with foreign keys enforced, so
//! `family_children` and `events` never keep orphaned rows after a delete
//! and partial edits roll back as a unit.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

/// The fields the inspector and watch list edit on a person.
///
/// Dates are free GEDCOM text; blank values normalise to `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonDetails {
    pub given_name: String,
    /// Patronymic or middle name; blank when the source has none.
    pub middle_name: String,
    pub surname: String,
    /// Preferred given name or nickname (GEDCOM `_PGVN`/`NICK`).
    pub nickname: String,
    /// One of `"M"`, `"F"`, `"U"` (enforced by a CHECK constraint).
    pub gender: String,
    pub birth_date: Option<String>,
    pub death_date: Option<String>,
}

impl PersonDetails {
    pub fn new(
        given_name: impl Into<String>,
        middle_name: impl Into<String>,
        surname: impl Into<String>,
        nickname: impl Into<String>,
        gender: impl Into<String>,
        birth_date: Option<String>,
        death_date: Option<String>,
    ) -> Self {
        Self {
            given_name: given_name.into().trim().to_string(),
            middle_name: middle_name.into().trim().to_string(),
            surname: surname.into().trim().to_string(),
            nickname: nickname.into().trim().to_string(),
            gender: gender.into(),
            birth_date: clean(birth_date),
            death_date: clean(death_date),
        }
    }

    /// Blank slate for the instant "Add Person" action.
    pub fn unnamed() -> Self {
        Self::new("", "", "", "", "U", None, None)
    }

    /// Name used in status messages; falls back for blank records.
    pub fn display_name(&self) -> String {
        let combined = [
            self.given_name.as_str(),
            self.middle_name.as_str(),
            self.surname.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        if combined.is_empty() {
            "(unnamed)".to_string()
        } else {
            combined
        }
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Which parental slot a newly linked parent occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentRole {
    Father,
    Mother,
}

impl ParentRole {
    pub fn gender(self) -> &'static str {
        match self {
            Self::Father => "M",
            Self::Mother => "F",
        }
    }
}

/// Birth and death dates lifted from a person's events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VitalDates {
    pub birth: Option<String>,
    pub death: Option<String>,
}

fn validate(details: &PersonDetails) -> Result<(), String> {
    if !matches!(details.gender.as_str(), "M" | "F" | "U") {
        return Err(format!("unknown gender code {}", details.gender));
    }
    Ok(())
}

fn to_err(error: rusqlite::Error) -> String {
    error.to_string()
}

/// Writes a new person plus optional birth/death events, returning the id.
pub fn insert_person(conn: &mut Connection, details: &PersonDetails) -> Result<String, String> {
    validate(details)?;
    let tx = conn.transaction().map_err(to_err)?;
    let id = write_person(&tx, details)?;
    write_dates(&tx, &id, details).map_err(to_err)?;
    tx.commit().map_err(to_err)?;
    Ok(id)
}

/// Applies edited profile fields and refreshes the vitals events.
///
/// Existing birth/death rows are updated in place (keeping their
/// citations) instead of being replaced; clearing a date sets it NULL.
pub fn update_person(
    conn: &mut Connection,
    person_id: &str,
    details: &PersonDetails,
) -> Result<(), String> {
    validate(details)?;
    let tx = conn.transaction().map_err(to_err)?;
    let changed = tx
        .execute(
            "UPDATE people SET given_name = ?1, middle_name = ?2, surname = ?3,
             nickname = ?4, gender = ?5, updated_at = datetime('now') WHERE id = ?6",
            params![
                details.given_name,
                details.middle_name,
                details.surname,
                details.nickname,
                details.gender,
                person_id
            ],
        )
        .map_err(to_err)?;
    if changed == 0 {
        return Err(format!("no person with id {person_id}"));
    }
    write_dates(&tx, person_id, details).map_err(to_err)?;
    tx.commit().map_err(to_err)?;
    Ok(())
}

/// Deletes a person in one transaction: FK cascades remove their events,
/// citations and child links, spouses detach with `SET NULL`, and
/// families left with no spouse and no children are pruned.
pub fn delete_person(conn: &mut Connection, person_id: &str) -> Result<(), String> {
    let tx = conn.transaction().map_err(to_err)?;
    let removed = tx
        .execute("DELETE FROM people WHERE id = ?1", params![person_id])
        .map_err(to_err)?;
    if removed == 0 {
        return Err(format!("no person with id {person_id}"));
    }
    tx.execute(
        "DELETE FROM families
         WHERE husband_id IS NULL AND wife_id IS NULL
           AND NOT EXISTS (SELECT 1 FROM family_children WHERE family_id = families.id)",
        [],
    )
    .map_err(to_err)?;
    tx.commit().map_err(to_err)?;
    Ok(())
}

/// Creates a parent for `child_id`: fills a free matching spouse slot in
/// one of the child's families, or starts a new family and links the
/// child into it.
pub fn link_parent(
    conn: &mut Connection,
    child_id: &str,
    role: ParentRole,
    details: &PersonDetails,
) -> Result<String, String> {
    validate(details)?;
    let tx = conn.transaction().map_err(to_err)?;
    let parent_id = write_person(&tx, details)?;
    write_dates(&tx, &parent_id, details).map_err(to_err)?;
    let free_family = find_family_with_free_slot(&tx, child_id, role).map_err(to_err)?;
    match free_family {
        Some(family_id) => fill_slot(&tx, &family_id, role, &parent_id).map_err(to_err)?,
        None => {
            let family_id = Uuid::new_v4().to_string();
            let (husband, wife) = match role {
                ParentRole::Father => (Some(parent_id.clone()), None),
                ParentRole::Mother => (None, Some(parent_id.clone())),
            };
            tx.execute(
                "INSERT INTO families (id, husband_id, wife_id) VALUES (?1, ?2, ?3)",
                params![family_id, husband, wife],
            )
            .map_err(to_err)?;
            tx.execute(
                "INSERT INTO family_children (family_id, child_id) VALUES (?1, ?2)",
                params![family_id, child_id],
            )
            .map_err(to_err)?;
        }
    }
    tx.commit().map_err(to_err)?;
    Ok(parent_id)
}

/// Creates a partner for `person_id` in a brand-new family record; the
/// person's gender decides which spouse slot they keep.
pub fn link_spouse(
    conn: &mut Connection,
    person_id: &str,
    details: &PersonDetails,
) -> Result<String, String> {
    validate(details)?;
    let tx = conn.transaction().map_err(to_err)?;
    let spouse_id = write_person(&tx, details)?;
    write_dates(&tx, &spouse_id, details).map_err(to_err)?;
    let own_gender: String = tx
        .query_row(
            "SELECT gender FROM people WHERE id = ?1",
            params![person_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_err)?
        .ok_or_else(|| format!("no person with id {person_id}"))?;
    let (husband, wife) = match own_gender.as_str() {
        "F" => (Some(spouse_id.clone()), Some(person_id.to_string())),
        _ => (Some(person_id.to_string()), Some(spouse_id.clone())),
    };
    let family_id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO families (id, husband_id, wife_id) VALUES (?1, ?2, ?3)",
        params![family_id, husband, wife],
    )
    .map_err(to_err)?;
    tx.commit().map_err(to_err)?;
    Ok(spouse_id)
}

/// Adds a new child to `person_id`'s primary family (the first family
/// where they occupy a spouse slot), creating that family when the
/// person has none yet.
pub fn link_child(
    conn: &mut Connection,
    person_id: &str,
    details: &PersonDetails,
) -> Result<String, String> {
    validate(details)?;
    let tx = conn.transaction().map_err(to_err)?;
    let child_id = write_person(&tx, details)?;
    write_dates(&tx, &child_id, details).map_err(to_err)?;
    let primary: Option<String> = tx
        .query_row(
            "SELECT id FROM families WHERE husband_id = ?1 OR wife_id = ?1
             ORDER BY rowid LIMIT 1",
            params![person_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_err)?;
    let family_id = match primary {
        Some(family_id) => family_id,
        None => {
            let gender: String = tx
                .query_row(
                    "SELECT gender FROM people WHERE id = ?1",
                    params![person_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(to_err)?
                .ok_or_else(|| format!("no person with id {person_id}"))?;
            let (husband, wife) = if gender == "F" {
                (None, Some(person_id.to_string()))
            } else {
                (Some(person_id.to_string()), None)
            };
            let family_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO families (id, husband_id, wife_id) VALUES (?1, ?2, ?3)",
                params![family_id, husband, wife],
            )
            .map_err(to_err)?;
            family_id
        }
    };
    tx.execute(
        "INSERT INTO family_children (family_id, child_id) VALUES (?1, ?2)",
        params![family_id, child_id],
    )
    .map_err(to_err)?;
    tx.commit().map_err(to_err)?;
    Ok(child_id)
}

/// Groups each person's `BIRTH`/`DEATH` event dates by person id.
pub fn load_vital_dates(conn: &Connection) -> Result<HashMap<String, VitalDates>, rusqlite::Error> {
    let mut dates: HashMap<String, VitalDates> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT person_id, event_type, date FROM events
         WHERE person_id IS NOT NULL AND event_type IN ('BIRTH', 'DEATH')
         ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;
    for row in rows {
        let (person_id, event_type, date) = row?;
        let entry = dates.entry(person_id).or_default();
        match event_type.as_str() {
            "BIRTH" if entry.birth.is_none() => entry.birth = date,
            "DEATH" if entry.death.is_none() => entry.death = date,
            _ => {}
        }
    }
    Ok(dates)
}

fn write_person(conn: &Connection, details: &PersonDetails) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO people (id, given_name, middle_name, surname, nickname, gender)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            details.given_name,
            details.middle_name,
            details.surname,
            details.nickname,
            details.gender
        ],
    )
    .map_err(to_err)?;
    Ok(id)
}

fn write_dates(
    conn: &Connection,
    person_id: &str,
    details: &PersonDetails,
) -> rusqlite::Result<()> {
    upsert_date(conn, person_id, "BIRTH", details.birth_date.as_deref())?;
    upsert_date(conn, person_id, "DEATH", details.death_date.as_deref())?;
    Ok(())
}

fn upsert_date(
    conn: &Connection,
    person_id: &str,
    event_type: &str,
    date: Option<&str>,
) -> rusqlite::Result<()> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM events WHERE person_id = ?1 AND event_type = ?2
             ORDER BY rowid LIMIT 1",
            params![person_id, event_type],
            |row| row.get(0),
        )
        .optional()?;
    match (existing, date) {
        (Some(event_id), Some(value)) => {
            conn.execute(
                "UPDATE events SET date = ?1 WHERE id = ?2",
                params![value, event_id],
            )?;
        }
        (Some(event_id), None) => {
            conn.execute(
                "UPDATE events SET date = NULL WHERE id = ?1",
                params![event_id],
            )?;
        }
        (None, Some(value)) => {
            let event_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO events (id, event_type, date, person_id) VALUES (?1, ?2, ?3, ?4)",
                params![event_id, event_type, value, person_id],
            )?;
        }
        (None, None) => {}
    }
    Ok(())
}

fn find_family_with_free_slot(
    conn: &Connection,
    child_id: &str,
    role: ParentRole,
) -> rusqlite::Result<Option<String>> {
    let sql = match role {
        ParentRole::Father => {
            "SELECT f.id FROM families f
            JOIN family_children fc ON fc.family_id = f.id
            WHERE fc.child_id = ?1 AND f.husband_id IS NULL
            ORDER BY f.rowid LIMIT 1"
        }
        ParentRole::Mother => {
            "SELECT f.id FROM families f
            JOIN family_children fc ON fc.family_id = f.id
            WHERE fc.child_id = ?1 AND f.wife_id IS NULL
            ORDER BY f.rowid LIMIT 1"
        }
    };
    conn.query_row(sql, params![child_id], |row| row.get(0))
        .optional()
}

fn fill_slot(
    conn: &Connection,
    family_id: &str,
    role: ParentRole,
    person_id: &str,
) -> rusqlite::Result<()> {
    let sql = match role {
        ParentRole::Father => "UPDATE families SET husband_id = ?1 WHERE id = ?2",
        ParentRole::Mother => "UPDATE families SET wife_id = ?1 WHERE id = ?2",
    };
    conn.execute(sql, params![person_id, family_id])?;
    Ok(())
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

    fn details(given: &str, gender: &str) -> PersonDetails {
        PersonDetails::new(
            given,
            "",
            "Tester",
            "",
            gender,
            Some("1940".to_string()),
            None,
        )
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |row| row.get(0))
            .expect("count query runs")
    }

    #[test]
    fn person_details_trim_names_and_blank_dates() {
        let trimmed = PersonDetails::new(
            "  Ada  ",
            "  Lynn  ",
            "  King ",
            "  Addy ",
            "F",
            Some("   ".to_string()),
            Some("1815".to_string()),
        );
        assert_eq!(trimmed.given_name, "Ada");
        assert_eq!(trimmed.middle_name, "Lynn");
        assert_eq!(trimmed.surname, "King");
        assert_eq!(trimmed.nickname, "Addy");
        assert_eq!(trimmed.birth_date, None, "blank dates normalise away");
        assert_eq!(trimmed.death_date.as_deref(), Some("1815"));
        assert_eq!(trimmed.display_name(), "Ada Lynn King");
        assert_eq!(PersonDetails::unnamed().display_name(), "(unnamed)");
    }

    #[test]
    fn insert_person_stores_profile_and_birth_event() {
        let mut conn = connection();
        let profile = PersonDetails::new(
            "Ada",
            "Lynn",
            "King",
            "Addy",
            "F",
            Some("1940".to_string()),
            None,
        );
        let id = insert_person(&mut conn, &profile).expect("insert succeeds");

        let (given, middle, surname, nickname, gender): (String, String, String, String, String) =
            conn.query_row(
                "SELECT given_name, middle_name, surname, nickname, gender
                 FROM people WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("person row exists");
        assert_eq!(given, "Ada");
        assert_eq!(middle, "Lynn");
        assert_eq!(surname, "King");
        assert_eq!(nickname, "Addy");
        assert_eq!(gender, "F");

        let births = count(
            &conn,
            "SELECT COUNT(*) FROM events
             WHERE event_type = 'BIRTH' AND date = '1940'",
        );
        assert_eq!(births, 1, "the birth date lands in the events table");
        let deaths = count(
            &conn,
            "SELECT COUNT(*) FROM events WHERE event_type = 'DEATH'",
        );
        assert_eq!(deaths, 0, "no death event when the date is absent");
    }

    #[test]
    fn insert_person_rejects_unknown_gender() {
        let mut conn = connection();
        let error = insert_person(
            &mut conn,
            &PersonDetails::new("X", "", "Y", "", "Z", None, None),
        )
        .expect_err("bad gender fails");
        assert!(
            error.contains("gender"),
            "message names the problem: {error}"
        );
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM people"), 0);
    }

    #[test]
    fn update_person_refreshes_fields_and_vitals() {
        let mut conn = connection();
        let id = insert_person(&mut conn, &details("Ada", "F")).expect("insert succeeds");
        conn.execute(
            "UPDATE people SET updated_at = '2000-01-01 00:00:00' WHERE id = ?1",
            params![id],
        )
        .expect("timestamp rewrite");

        let edited = PersonDetails::new(
            "Augusta",
            "Lee",
            "King",
            "Gus",
            "F",
            None,
            Some("1815".to_string()),
        );
        update_person(&mut conn, &id, &edited).expect("update succeeds");

        let (given, middle, surname, nickname, updated): (String, String, String, String, String) =
            conn.query_row(
                "SELECT given_name, middle_name, surname, nickname, updated_at
                 FROM people WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("person row exists");
        assert_eq!(given, "Augusta");
        assert_eq!(middle, "Lee");
        assert_eq!(surname, "King");
        assert_eq!(nickname, "Gus");
        assert_ne!(updated, "2000-01-01 00:00:00", "updated_at is touched");

        let birth: Option<Option<String>> = conn
            .query_row(
                "SELECT date FROM events WHERE person_id = ?1 AND event_type = 'BIRTH'",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .expect("birth query runs");
        assert_eq!(birth, Some(None), "the birth row survives, date cleared");
        let death: Option<String> = conn
            .query_row(
                "SELECT date FROM events WHERE person_id = ?1 AND event_type = 'DEATH'",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .expect("death query runs");
        assert_eq!(death.as_deref(), Some("1815"), "the death row is created");
    }

    #[test]
    fn update_person_requires_a_known_person() {
        let mut conn = connection();
        let error =
            update_person(&mut conn, "ghost", &details("Ada", "F")).expect_err("missing id fails");
        assert!(error.contains("no person"), "message says why: {error}");
    }

    #[test]
    fn delete_person_cascades_and_prunes_empty_families() {
        let mut conn = connection();
        let father = insert_person(&mut conn, &details("Otto", "M")).expect("father insert");
        let mother = insert_person(&mut conn, &details("Ada", "F")).expect("mother insert");
        let child = insert_person(&mut conn, &details("Kid", "U")).expect("child insert");
        conn.execute(
            "INSERT INTO families (id, husband_id, wife_id) VALUES ('fam-a', ?1, ?2)",
            params![father, mother],
        )
        .expect("family a");
        conn.execute(
            "INSERT INTO family_children (family_id, child_id) VALUES ('fam-a', ?1)",
            params![child],
        )
        .expect("child link");
        let shell = insert_person(&mut conn, &details("Solo", "M")).expect("shell insert");
        conn.execute(
            "INSERT INTO families (id, husband_id) VALUES ('fam-b', ?1)",
            params![shell],
        )
        .expect("family b");
        conn.execute(
            "INSERT INTO citations (id, source_title, event_id)
             SELECT 'cite-1', 'Src', id FROM events WHERE person_id = ?1 LIMIT 1",
            params![father],
        )
        .expect("citation attaches to the father's birth");

        delete_person(&mut conn, &shell).expect("shell delete prunes its family");
        delete_person(&mut conn, &father).expect("delete succeeds");

        let father_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM people WHERE id = ?1",
                params![father],
                |row| row.get(0),
            )
            .expect("count query runs");
        assert_eq!(father_rows, 0, "person gone");
        let father_events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE person_id = ?1",
                params![father],
                |row| row.get(0),
            )
            .expect("count query runs");
        assert_eq!(father_events, 0, "events cascade away");
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM citations WHERE id = 'cite-1'"),
            0,
            "citations cascade away"
        );
        let husband: Option<String> = conn
            .query_row(
                "SELECT husband_id FROM families WHERE id = 'fam-a'",
                [],
                |row| row.get(0),
            )
            .expect("family a row");
        assert_eq!(husband, None, "spouse slots detach with SET NULL");
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM family_children"),
            1,
            "children keep their links"
        );
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM families WHERE id = 'fam-b'"),
            0,
            "childless two-empty-slot families are pruned"
        );
    }

    #[test]
    fn link_parent_fills_a_free_slot_before_creating_a_family() {
        let mut conn = connection();
        let child = insert_person(&mut conn, &details("Kid", "U")).expect("child insert");
        let mother = insert_person(&mut conn, &details("Ada", "F")).expect("mother insert");
        conn.execute(
            "INSERT INTO families (id, wife_id) VALUES ('fam-m', ?1)",
            params![mother],
        )
        .expect("mother-only family");
        conn.execute(
            "INSERT INTO family_children (family_id, child_id) VALUES ('fam-m', ?1)",
            params![child],
        )
        .expect("child link");

        let father = link_parent(&mut conn, &child, ParentRole::Father, &details("Otto", "M"))
            .expect("father link succeeds");
        let slot: String = conn
            .query_row(
                "SELECT husband_id FROM families WHERE id = 'fam-m'",
                [],
                |row| row.get(0),
            )
            .expect("family row");
        assert_eq!(slot, father, "slot filled in place");

        let second = link_parent(
            &mut conn,
            &child,
            ParentRole::Father,
            &details("Erich", "M"),
        )
        .expect("second father link succeeds");
        assert_ne!(second, father);
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM families"),
            2,
            "no free slot means a second family"
        );
        let links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM family_children WHERE child_id = ?1",
                params![child],
                |row| row.get(0),
            )
            .expect("count query runs");
        assert_eq!(links, 2, "the child joins the new family too");
    }

    #[test]
    fn link_spouse_places_partners_by_gender() {
        let mut conn = connection();

        let male = insert_person(&mut conn, &details("Otto", "M")).expect("male insert");
        let spouse_of_male =
            link_spouse(&mut conn, &male, &details("Ada", "F")).expect("spouse of male");
        let (husband, wife): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT husband_id, wife_id FROM families ORDER BY rowid LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("family row");
        assert_eq!(husband.as_deref(), Some(male.as_str()));
        assert_eq!(wife.as_deref(), Some(spouse_of_male.as_str()));

        let female = insert_person(&mut conn, &details("Ada", "F")).expect("female insert");
        let spouse_of_female =
            link_spouse(&mut conn, &female, &details("Otto", "M")).expect("spouse of female");
        let (husband, wife): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT husband_id, wife_id FROM families ORDER BY rowid DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("family row");
        assert_eq!(husband.as_deref(), Some(spouse_of_female.as_str()));
        assert_eq!(wife.as_deref(), Some(female.as_str()));

        let unknown = insert_person(&mut conn, &details("Alex", "U")).expect("unknown insert");
        link_spouse(&mut conn, &unknown, &details("Sam", "U")).expect("spouse of unknown");
        let (husband, wife): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT husband_id, wife_id FROM families ORDER BY rowid DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("family row");
        assert_eq!(
            husband.as_deref(),
            Some(unknown.as_str()),
            "unknown gender falls back to the husband slot"
        );
        assert!(wife.is_some());
    }

    #[test]
    fn link_child_uses_the_primary_family_and_creates_one_when_single() {
        let mut conn = connection();
        let father = insert_person(&mut conn, &details("Otto", "M")).expect("father insert");
        let mother = insert_person(&mut conn, &details("Ada", "F")).expect("mother insert");
        conn.execute(
            "INSERT INTO families (id, husband_id, wife_id) VALUES ('fam-p', ?1, ?2)",
            params![father, mother],
        )
        .expect("primary family");

        let linked = link_child(&mut conn, &father, &details("Kid", "U")).expect("child link");
        let family: String = conn
            .query_row(
                "SELECT family_id FROM family_children WHERE child_id = ?1",
                params![linked],
                |row| row.get(0),
            )
            .expect("junction row");
        assert_eq!(family, "fam-p", "the child joins the existing family");

        let single = insert_person(&mut conn, &details("Solo", "F")).expect("single insert");
        let born = link_child(&mut conn, &single, &details("Babe", "U")).expect("new child");
        let (wife, links): (Option<String>, i64) = conn
            .query_row(
                "SELECT wife_id,
                 (SELECT COUNT(*) FROM family_children WHERE child_id = ?1)
                 FROM families ORDER BY rowid DESC LIMIT 1",
                params![born],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("new family row");
        assert_eq!(wife.as_deref(), Some(single.as_str()), "own slot by gender");
        assert_eq!(links, 1, "and the child is linked");
    }

    #[test]
    fn load_vital_dates_groups_birth_and_death() {
        let mut conn = connection();
        let both = PersonDetails::new(
            "Ada",
            "",
            "King",
            "",
            "F",
            Some("1815".to_string()),
            Some("1852".to_string()),
        );
        let full = insert_person(&mut conn, &both).expect("insert with both dates");
        let partial = insert_person(&mut conn, &details("Kid", "U")).expect("birth only");
        let bare = insert_person(&mut conn, &PersonDetails::unnamed()).expect("no events");

        let dates = load_vital_dates(&conn).expect("dates load");
        let full_dates = dates.get(&full).expect("full person present");
        assert_eq!(full_dates.birth.as_deref(), Some("1815"));
        assert_eq!(full_dates.death.as_deref(), Some("1852"));
        let partial_dates = dates.get(&partial).expect("partial person present");
        assert_eq!(partial_dates.birth.as_deref(), Some("1940"));
        assert_eq!(partial_dates.death, None);
        assert!(
            !dates.contains_key(&bare),
            "people without events are absent"
        );
    }
}
