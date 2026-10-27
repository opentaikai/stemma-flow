//! Place authority persistence.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

/// One place row with its event usage count.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceSummary {
    pub id: String,
    pub name: String,
    pub notes: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub event_count: i64,
}

/// A stored place loaded for editing.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceRecord {
    pub id: String,
    pub name: String,
    pub notes: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// An editable place draft.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaceDraft {
    pub id: Option<String>,
    pub name: String,
    pub notes: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

fn clean(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Lists places with their event counts.
pub fn list_places(conn: &Connection) -> Result<Vec<PlaceSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name, p.notes, p.latitude, p.longitude,
                    (SELECT COUNT(*) FROM events e WHERE e.place_id = p.id)
             FROM places p
             ORDER BY p.name COLLATE NOCASE, p.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PlaceSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                notes: row.get(2)?,
                latitude: row.get(3)?,
                longitude: row.get(4)?,
                event_count: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads one place for editing.
pub fn load_place(conn: &Connection, id: &str) -> Result<Option<PlaceRecord>, String> {
    conn.query_row(
        "SELECT id, name, notes, latitude, longitude FROM places WHERE id = ?1",
        params![id],
        |row| {
            Ok(PlaceRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                notes: row.get(2)?,
                latitude: row.get(3)?,
                longitude: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// Inserts or updates a place.
pub fn save_place(conn: &mut Connection, draft: &PlaceDraft) -> Result<String, String> {
    let name = clean(&draft.name).ok_or_else(|| "Place name is required".to_string())?;
    let notes = clean(&draft.notes);
    if let Some(latitude) = draft.latitude
        && (!latitude.is_finite() || !(-90.0..=90.0).contains(&latitude))
    {
        return Err("Latitude must be between -90 and 90".to_string());
    }
    if let Some(longitude) = draft.longitude
        && (!longitude.is_finite() || !(-180.0..=180.0).contains(&longitude))
    {
        return Err("Longitude must be between -180 and 180".to_string());
    }

    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let duplicate: Option<String> = tx
        .query_row(
            "SELECT id FROM places WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(existing_id) = duplicate
        && existing_id != id
    {
        return Err("A place with this name already exists".to_string());
    }

    let changed = tx
        .execute(
            "UPDATE places SET name = ?1, notes = ?2, latitude = ?3, longitude = ?4,
                updated_at = datetime('now')
             WHERE id = ?5",
            params![name, notes, draft.latitude, draft.longitude, id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        tx.execute(
            "INSERT INTO places (id, name, notes, latitude, longitude)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, name, notes, draft.latitude, draft.longitude],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(id)
}

/// Deletes a place; events lose their place and place links cascade.
pub fn delete_place(conn: &mut Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM places WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no place with id {id}"));
    }
    Ok(())
}

/// Reverses comma-separated place components for read-only display.
pub fn reverse_name(name: &str) -> String {
    name.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .rev()
        .collect::<Vec<_>>()
        .join(", ")
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

    fn person_and_event(conn: &Connection, place_id: &str) -> String {
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person insert");
        conn.execute(
            "INSERT INTO events (id, event_type, place_id, person_id)
             VALUES ('e1', 'BIRTH', ?1, 'p1')",
            params![place_id],
        )
        .expect("event insert");
        "e1".to_string()
    }

    #[test]
    fn saves_loads_and_counts_places() {
        let mut conn = connection();
        let id = save_place(
            &mut conn,
            &PlaceDraft {
                name: "  Uppsala, Sweden  ".to_string(),
                notes: "Cathedral archive".to_string(),
                latitude: Some(59.8586),
                longitude: Some(17.6389),
                ..PlaceDraft::default()
            },
        )
        .expect("place saves");
        person_and_event(&conn, &id);

        let loaded = load_place(&conn, &id).expect("load").expect("place exists");
        assert_eq!(loaded.name, "Uppsala, Sweden");
        assert_eq!(loaded.notes.as_deref(), Some("Cathedral archive"));
        assert_eq!(loaded.latitude, Some(59.8586));
        assert_eq!(loaded.longitude, Some(17.6389));

        let list = list_places(&conn).expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].event_count, 1);
    }

    #[test]
    fn renaming_a_place_updates_its_events() {
        let mut conn = connection();
        let id = save_place(
            &mut conn,
            &PlaceDraft {
                name: "Uppsala".to_string(),
                ..PlaceDraft::default()
            },
        )
        .expect("place saves");
        person_and_event(&conn, &id);

        save_place(
            &mut conn,
            &PlaceDraft {
                id: Some(id),
                name: "Uppsala, Sweden".to_string(),
                ..PlaceDraft::default()
            },
        )
        .expect("place updates");

        let name: String = conn
            .query_row(
                "SELECT p.name FROM events e JOIN places p ON p.id = e.place_id
                 WHERE e.id = 'e1'",
                [],
                |row| row.get(0),
            )
            .expect("event place");
        assert_eq!(name, "Uppsala, Sweden");
    }

    #[test]
    fn rejects_bad_names_and_coordinates() {
        let mut conn = connection();
        let id = save_place(
            &mut conn,
            &PlaceDraft {
                name: "Uppsala".to_string(),
                ..PlaceDraft::default()
            },
        )
        .expect("place saves");

        assert!(
            save_place(
                &mut conn,
                &PlaceDraft {
                    name: "   ".to_string(),
                    ..PlaceDraft::default()
                }
            )
            .is_err()
        );
        assert!(
            save_place(
                &mut conn,
                &PlaceDraft {
                    name: "Uppsala".to_string(),
                    ..PlaceDraft::default()
                }
            )
            .is_err(),
            "duplicate names are rejected"
        );
        assert!(
            save_place(
                &mut conn,
                &PlaceDraft {
                    id: Some(id.clone()),
                    name: "Uppsala".to_string(),
                    latitude: Some(91.0),
                    ..PlaceDraft::default()
                }
            )
            .is_err()
        );
        assert!(
            save_place(
                &mut conn,
                &PlaceDraft {
                    id: Some(id),
                    name: "Uppsala".to_string(),
                    longitude: Some(f64::NAN),
                    ..PlaceDraft::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn deleting_a_place_clears_events_and_links() {
        let mut conn = connection();
        let id = save_place(
            &mut conn,
            &PlaceDraft {
                name: "Uppsala".to_string(),
                ..PlaceDraft::default()
            },
        )
        .expect("place saves");
        person_and_event(&conn, &id);
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m1', 'a.jpg')",
            [],
        )
        .expect("media insert");
        conn.execute(
            "INSERT INTO media_links (media_id, place_id) VALUES ('m1', ?1)",
            params![id],
        )
        .expect("media link insert");

        delete_place(&mut conn, &id).expect("place deletes");

        let place_id: Option<String> = conn
            .query_row("SELECT place_id FROM events WHERE id = 'e1'", [], |row| {
                row.get(0)
            })
            .expect("event");
        assert_eq!(place_id, None);
        let links: i64 = conn
            .query_row("SELECT COUNT(*) FROM media_links", [], |row| row.get(0))
            .expect("link count");
        assert_eq!(links, 0);
        assert!(load_place(&conn, &id).expect("load").is_none());
    }

    #[test]
    fn reverses_comma_components() {
        assert_eq!(reverse_name("Uppsala, Sweden"), "Sweden, Uppsala");
        assert_eq!(reverse_name(" Uppsala ,  Sweden "), "Sweden, Uppsala");
        assert_eq!(reverse_name("Uppsala"), "Uppsala");
        assert_eq!(reverse_name("A, , B"), "B, A");
        assert_eq!(reverse_name("   "), "");
    }
}
