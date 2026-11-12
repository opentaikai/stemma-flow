//! Place-event loading and editing.

use rusqlite::{Connection, OptionalExtension, params};

use crate::model::date;

/// An event loaded for editing from the place list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceEventRecord {
    pub id: String,
    pub event_type: String,
    pub event_label: String,
    pub date: Option<String>,
    pub place_id: Option<String>,
    pub place_name: Option<String>,
    pub description: Option<String>,
    pub private: bool,
    pub person_id: Option<String>,
    pub family_id: Option<String>,
}

/// The editable fields of a place event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceEventDraft {
    pub id: String,
    pub date: String,
    pub description: String,
    pub private: bool,
}

/// Loads one event with its fact label and place name.
pub fn load_event(conn: &Connection, event_id: &str) -> Result<Option<PlaceEventRecord>, String> {
    conn.query_row(
        "SELECT e.id, e.event_type, COALESCE(ft.label, e.event_type), e.date,
                e.place_id, pl.name, e.description, e.private,
                e.person_id, e.family_id
         FROM events e
         LEFT JOIN fact_types ft ON ft.key = e.event_type
         LEFT JOIN places pl ON pl.id = e.place_id
         WHERE e.id = ?1",
        params![event_id],
        |row| {
            Ok(PlaceEventRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                event_label: row.get(2)?,
                date: row.get(3)?,
                place_id: row.get(4)?,
                place_name: row.get(5)?,
                description: row.get(6)?,
                private: row.get::<_, i64>(7)? != 0,
                person_id: row.get(8)?,
                family_id: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// Updates the editable event fields; an empty date or description clears it.
pub fn save_event(conn: &Connection, draft: &PlaceEventDraft) -> Result<(), String> {
    let date = match draft.date.trim() {
        "" => None,
        text => Some(date::canonicalize(text).map_err(|error| error.to_string())?),
    };
    let description = draft.description.trim();
    let description = if description.is_empty() {
        None
    } else {
        Some(description)
    };
    let changed = conn
        .execute(
            "UPDATE events SET date = ?1, description = ?2, private = ?3 WHERE id = ?4",
            params![date, description, draft.private as i64, draft.id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no event with id {}", draft.id));
    }
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
        conn.execute_batch(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F');
             INSERT INTO places (id, name) VALUES ('pl1', 'Uppsala, Sweden');
             INSERT INTO events (id, event_type, date, place_id, description, private, person_id)
             VALUES ('e1', 'CENSUS', '12 jan 1850', 'pl1', 'First census', 1, 'p1');",
        )
        .expect("event fixtures");
        conn
    }

    #[test]
    fn loads_event_with_its_label_and_place() {
        let conn = connection();
        let record = load_event(&conn, "e1")
            .expect("load succeeds")
            .expect("event exists");
        assert_eq!(record.id, "e1");
        assert_eq!(record.event_type, "CENSUS");
        assert_eq!(record.event_label, "Census");
        assert_eq!(record.date.as_deref(), Some("12 jan 1850"));
        assert_eq!(record.place_id.as_deref(), Some("pl1"));
        assert_eq!(record.place_name.as_deref(), Some("Uppsala, Sweden"));
        assert_eq!(record.description.as_deref(), Some("First census"));
        assert!(record.private);
        assert_eq!(record.person_id.as_deref(), Some("p1"));
        assert_eq!(record.family_id, None);
    }

    #[test]
    fn missing_events_load_as_none() {
        let conn = connection();
        assert!(load_event(&conn, "nope").expect("load succeeds").is_none());
    }

    #[test]
    fn saves_canonical_date_description_and_privacy() {
        let conn = connection();
        save_event(
            &conn,
            &PlaceEventDraft {
                id: "e1".to_string(),
                date: "  3 mar 1851  ".to_string(),
                description: "  Moved into the parish  ".to_string(),
                private: false,
            },
        )
        .expect("event saves");

        let record = load_event(&conn, "e1")
            .expect("load succeeds")
            .expect("event exists");
        assert_eq!(record.date.as_deref(), Some("3 MAR 1851"));
        assert_eq!(record.description.as_deref(), Some("Moved into the parish"));
        assert!(!record.private);
    }

    #[test]
    fn blank_fields_clear_the_columns() {
        let conn = connection();
        save_event(
            &conn,
            &PlaceEventDraft {
                id: "e1".to_string(),
                date: "   ".to_string(),
                description: String::new(),
                private: true,
            },
        )
        .expect("event saves");

        let record = load_event(&conn, "e1")
            .expect("load succeeds")
            .expect("event exists");
        assert_eq!(record.date, None);
        assert_eq!(record.description, None);
        assert!(record.private);
        assert_eq!(
            record.place_id.as_deref(),
            Some("pl1"),
            "place is unchanged"
        );
    }

    #[test]
    fn rejects_invalid_dates_and_unknown_events() {
        let conn = connection();
        let error = save_event(
            &conn,
            &PlaceEventDraft {
                id: "e1".to_string(),
                date: "not a date".to_string(),
                description: String::new(),
                private: false,
            },
        )
        .expect_err("invalid date is rejected");
        assert!(!error.is_empty(), "invalid date reports an error");

        let error = save_event(
            &conn,
            &PlaceEventDraft {
                id: "missing".to_string(),
                date: "1850".to_string(),
                description: String::new(),
                private: false,
            },
        )
        .expect_err("unknown event is rejected");
        assert!(error.contains("missing"));

        let record = load_event(&conn, "e1")
            .expect("load succeeds")
            .expect("event exists");
        assert_eq!(
            record.date.as_deref(),
            Some("12 jan 1850"),
            "a failed save leaves the event unchanged"
        );
    }
}
