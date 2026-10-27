//! Person/event privacy flags and conservative living detection.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};

use crate::model::date;

/// The default living-age cutoff used by the inspector.
pub const DEFAULT_LIVING_AGE: u32 = 110;

/// One event shown in the privacy editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPrivacy {
    pub event_id: String,
    pub event_type: String,
    pub label: String,
    pub date: Option<String>,
    pub private: bool,
    pub family_event: bool,
}

/// Person-level privacy with the derived living state and event flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonPrivacy {
    pub private: bool,
    pub living_override: Option<bool>,
    pub auto_living: bool,
    pub effective_living: bool,
    pub events: Vec<EventPrivacy>,
}

/// Current calendar year from SQLite, used for living detection and export.
pub fn current_year(conn: &Connection) -> Result<i32, String> {
    conn.query_row("SELECT CAST(strftime('%Y', 'now') AS INTEGER)", [], |row| {
        row.get(0)
    })
    .map_err(|error| error.to_string())
}

/// Loads one person's privacy flags and all events the inspector can mark.
pub fn load_person_privacy(
    conn: &Connection,
    person_id: &str,
    current_year: i32,
    living_age: u32,
) -> Result<Option<PersonPrivacy>, String> {
    let flags: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT private, living_override FROM people WHERE id = ?1",
            params![person_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((private, living_override)) = flags else {
        return Ok(None);
    };
    let private = private != 0;
    let living_override = living_override.map(|value| value != 0);
    let auto_living = auto_living(conn, person_id, current_year, living_age)?;
    let effective_living = living_override.unwrap_or(auto_living);

    let mut events = Vec::new();
    let mut seen = HashSet::new();
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.event_type, COALESCE(f.label, e.event_type),
                    e.date, e.private, 0
             FROM events e
             LEFT JOIN fact_types f ON f.key = e.event_type
             WHERE e.person_id = ?1
             ORDER BY e.event_type, e.date, e.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| {
            Ok(EventPrivacy {
                event_id: row.get(0)?,
                event_type: row.get(1)?,
                label: row.get(2)?,
                date: row.get(3)?,
                private: row.get::<_, i64>(4)? != 0,
                family_event: false,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let event = row.map_err(|error| error.to_string())?;
        seen.insert(event.event_id.clone());
        events.push(event);
    }

    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.event_type, COALESCE(f.label, e.event_type),
                    e.date, e.private, 1
             FROM events e
             JOIN families fam ON fam.id = e.family_id
             LEFT JOIN fact_types f ON f.key = e.event_type
             WHERE fam.husband_id = ?1 OR fam.wife_id = ?1
             ORDER BY e.event_type, e.date, e.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| {
            Ok(EventPrivacy {
                event_id: row.get(0)?,
                event_type: row.get(1)?,
                label: row.get(2)?,
                date: row.get(3)?,
                private: row.get::<_, i64>(4)? != 0,
                family_event: true,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let event = row.map_err(|error| error.to_string())?;
        if seen.insert(event.event_id.clone()) {
            events.push(event);
        }
    }
    events.sort_by(|left, right| {
        left.event_type
            .cmp(&right.event_type)
            .then_with(|| left.date.cmp(&right.date))
            .then_with(|| left.event_id.cmp(&right.event_id))
    });

    Ok(Some(PersonPrivacy {
        private,
        living_override,
        auto_living,
        effective_living,
        events,
    }))
}

/// Derives living status without considering the manual override.
pub fn auto_living(
    conn: &Connection,
    person_id: &str,
    current_year: i32,
    living_age: u32,
) -> Result<bool, String> {
    let death: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events
             WHERE person_id = ?1 AND event_type IN ('DEATH', 'BURIAL', 'CREMATION')",
            params![person_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if death > 0 {
        return Ok(false);
    }
    let birth: Option<Option<String>> = conn
        .query_row(
            "SELECT date FROM events
             WHERE person_id = ?1 AND event_type = 'BIRTH'
             ORDER BY rowid LIMIT 1",
            params![person_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(birth_date) = birth.flatten() else {
        return Ok(true);
    };
    let Some(year) = year_hint(&birth_date) else {
        return Ok(true);
    };
    Ok(year > current_year - living_age as i32)
}

/// Effective living status, including the manual override.
pub fn is_living(
    conn: &Connection,
    person_id: &str,
    current_year: i32,
    living_age: u32,
) -> Result<bool, String> {
    let override_value: Option<Option<i64>> = conn
        .query_row(
            "SELECT living_override FROM people WHERE id = ?1",
            params![person_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(override_value) = override_value else {
        return Err(format!("no person with id {person_id}"));
    };
    if let Some(value) = override_value {
        return Ok(value != 0);
    }
    auto_living(conn, person_id, current_year, living_age)
}

/// Updates the private flag.
pub fn set_person_private(conn: &Connection, person_id: &str, private: bool) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE people SET private = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![private as i64, person_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no person with id {person_id}"));
    }
    Ok(())
}

/// Updates the Auto/Living/Deceased override.
pub fn set_living_override(
    conn: &Connection,
    person_id: &str,
    override_value: Option<bool>,
) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE people SET living_override = ?1, updated_at = datetime('now')
             WHERE id = ?2",
            params![override_value.map(|value| value as i64), person_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no person with id {person_id}"));
    }
    Ok(())
}

/// Updates an event's private flag.
pub fn set_event_private(conn: &Connection, event_id: &str, private: bool) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE events SET private = ?1 WHERE id = ?2",
            params![private as i64, event_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no event with id {event_id}"));
    }
    Ok(())
}

/// Best-effort birth year for living detection. Ranges use their latest end.
fn year_hint(value: &str) -> Option<i32> {
    let canonical = date::canonicalize(value).ok()?;
    for prefix in ["ABT ", "CAL ", "EST ", "BEF ", "AFT "] {
        if let Some(rest) = canonical.strip_prefix(prefix) {
            return date::exact_parts(rest).map(|parts| parts.year);
        }
    }
    if let Some(rest) = canonical.strip_prefix("BET ") {
        let (_, end) = rest.split_once(" AND ")?;
        return date::exact_parts(end).map(|parts| parts.year);
    }
    if let Some(rest) = canonical.strip_prefix("FROM ") {
        if let Some((_, end)) = rest.split_once(" TO ") {
            return date::exact_parts(end).map(|parts| parts.year);
        }
        return date::exact_parts(rest).map(|parts| parts.year);
    }
    if let Some(rest) = canonical.strip_prefix("TO ") {
        return date::exact_parts(rest).map(|parts| parts.year);
    }
    date::exact_parts(&canonical).map(|parts| parts.year)
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

    fn person(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender) VALUES (?1, 'Ada', 'King', 'F')",
            params![id],
        )
        .expect("person insert");
    }

    fn event(conn: &Connection, id: &str, event_type: &str, date: Option<&str>, person: &str) {
        conn.execute(
            "INSERT INTO events (id, event_type, date, person_id) VALUES (?1, ?2, ?3, ?4)",
            params![id, event_type, date, person],
        )
        .expect("event insert");
    }

    #[test]
    fn detects_living_from_death_and_birth_year() {
        let conn = connection();
        person(&conn, "p1");
        event(&conn, "e1", "BIRTH", Some("2000"), "p1");
        assert!(auto_living(&conn, "p1", 2026, 110).expect("living"));

        person(&conn, "p2");
        event(&conn, "e2", "BIRTH", Some("1900"), "p2");
        assert!(!auto_living(&conn, "p2", 2026, 110).expect("deceased"));

        person(&conn, "p3");
        event(&conn, "e3", "DEATH", Some("2001"), "p3");
        assert!(!auto_living(&conn, "p3", 2026, 110).expect("death wins"));

        person(&conn, "p4");
        assert!(auto_living(&conn, "p4", 2026, 110).expect("unknown is living"));
    }

    #[test]
    fn overrides_and_privacy_round_trip() {
        let conn = connection();
        person(&conn, "p1");
        event(&conn, "e1", "BIRTH", Some("2000"), "p1");
        set_person_private(&conn, "p1", true).expect("private");
        set_living_override(&conn, "p1", Some(false)).expect("override");
        set_event_private(&conn, "e1", true).expect("event private");
        let privacy = load_person_privacy(&conn, "p1", 2026, 110)
            .expect("load")
            .expect("privacy");
        assert!(privacy.private);
        assert_eq!(privacy.living_override, Some(false));
        assert!(privacy.auto_living);
        assert!(!privacy.effective_living);
        assert!(privacy.events[0].private);
        assert!(is_living(&conn, "p1", 2026, 110).is_ok());
    }

    #[test]
    fn privacy_events_include_family_events() {
        let conn = connection();
        person(&conn, "p1");
        person(&conn, "p2");
        conn.execute(
            "INSERT INTO families (id, husband_id, wife_id) VALUES ('f1', 'p2', 'p1')",
            [],
        )
        .expect("family");
        event(&conn, "e1", "BIRTH", Some("2000"), "p1");
        conn.execute(
            "INSERT INTO events (id, event_type, date, family_id)
             VALUES ('e2', 'MARRIAGE', '2020', 'f1')",
            [],
        )
        .expect("marriage");
        let privacy = load_person_privacy(&conn, "p1", 2026, 110)
            .expect("load")
            .expect("privacy");
        assert_eq!(privacy.events.len(), 2);
        assert!(privacy.events.iter().any(|event| event.family_event));
    }
}
