//! First-class plain-text notes attached to genealogical records.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

/// The record a note belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteOwner {
    Person(String),
    Family(String),
    Event(String),
    Citation(String),
    Place(String),
}

impl NoteOwner {
    fn column(&self) -> &'static str {
        match self {
            Self::Person(_) => "person_id",
            Self::Family(_) => "family_id",
            Self::Event(_) => "event_id",
            Self::Citation(_) => "citation_id",
            Self::Place(_) => "place_id",
        }
    }

    fn id(&self) -> &str {
        match self {
            Self::Person(id)
            | Self::Family(id)
            | Self::Event(id)
            | Self::Citation(id)
            | Self::Place(id) => id,
        }
    }
}

/// One stored note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

fn clean(body: &str) -> Result<&str, String> {
    let body = body.trim();
    if body.is_empty() {
        Err("enter note text".to_string())
    } else {
        Ok(body)
    }
}

/// Lists an owner's notes in stored order.
pub fn list_notes(conn: &Connection, owner: &NoteOwner) -> Result<Vec<Note>, String> {
    let sql = format!(
        "SELECT id, body, created_at, updated_at FROM notes
         WHERE {} = ?1 ORDER BY sort_order, rowid",
        owner.column()
    );
    let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![owner.id()], |row| {
            Ok(Note {
                id: row.get(0)?,
                body: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Adds a note after the owner's existing notes.
pub fn add_note(conn: &Connection, owner: &NoteOwner, body: &str) -> Result<String, String> {
    let body = clean(body)?;
    let id = Uuid::new_v4().to_string();
    let column = owner.column();
    let sql = format!(
        "INSERT INTO notes (id, body, sort_order, {column})
         SELECT ?1, ?2, COALESCE(MAX(sort_order) + 1, 0), ?3
         FROM notes WHERE {column} = ?3"
    );
    conn.execute(&sql, params![id, body, owner.id()])
        .map_err(|error| error.to_string())?;
    Ok(id)
}

/// Replaces a note's text.
pub fn update_note(conn: &Connection, id: &str, body: &str) -> Result<(), String> {
    let body = clean(body)?;
    let changed = conn
        .execute(
            "UPDATE notes SET body = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![body, id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no note with id {id}"));
    }
    Ok(())
}

/// Deletes one note.
pub fn delete_note(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM notes WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no note with id {id}"));
    }
    Ok(())
}

/// Counts an owner's notes.
pub fn note_count(conn: &Connection, owner: &NoteOwner) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM notes WHERE {} = ?1", owner.column());
    conn.query_row(&sql, params![owner.id()], |row| row.get(0))
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "note count unavailable".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma");
        init_db(&conn).expect("schema");
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender) VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person");
        conn
    }

    #[test]
    fn adds_lists_updates_and_deletes_notes_in_order() {
        let conn = connection();
        let owner = NoteOwner::Person("p1".to_string());
        let first = add_note(&conn, &owner, "  First note  ").expect("first");
        add_note(&conn, &owner, "Second note").expect("second");

        let notes = list_notes(&conn, &owner).expect("list");
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].body, "First note");
        assert_eq!(notes[1].body, "Second note");

        update_note(&conn, &first, "Edited").expect("update");
        let notes = list_notes(&conn, &owner).expect("list");
        assert_eq!(notes[0].body, "Edited");

        delete_note(&conn, &first).expect("delete");
        assert_eq!(list_notes(&conn, &owner).expect("list").len(), 1);
        assert!(add_note(&conn, &owner, "   ").is_err(), "blank rejected");
    }

    #[test]
    fn every_owner_type_is_supported_and_cascades() {
        let conn = connection();
        conn.execute(
            "INSERT INTO sources (id, title) VALUES ('s1', 'Source')",
            [],
        )
        .expect("source");
        conn.execute("INSERT INTO families (id) VALUES ('f1')", [])
            .expect("family");
        conn.execute(
            "INSERT INTO events (id, event_type, person_id) VALUES ('e1', 'BIRTH', 'p1')",
            [],
        )
        .expect("event");
        conn.execute(
            "INSERT INTO citations (id, source_id) VALUES ('c1', 's1')",
            [],
        )
        .expect("citation");
        conn.execute(
            "INSERT INTO places (id, name) VALUES ('pl1', 'Uppsala')",
            [],
        )
        .expect("place");

        for owner in [
            NoteOwner::Person("p1".to_string()),
            NoteOwner::Family("f1".to_string()),
            NoteOwner::Event("e1".to_string()),
            NoteOwner::Citation("c1".to_string()),
            NoteOwner::Place("pl1".to_string()),
        ] {
            add_note(&conn, &owner, "Note").expect("owner accepts note");
            assert_eq!(note_count(&conn, &owner).expect("count"), 1);
        }

        conn.execute("DELETE FROM people WHERE id = 'p1'", [])
            .expect("delete person");
        assert!(
            list_notes(&conn, &NoteOwner::Person("p1".to_string()))
                .expect("list")
                .is_empty()
        );
        assert!(
            list_notes(&conn, &NoteOwner::Event("e1".to_string()))
                .expect("event notes")
                .is_empty(),
            "events cascade with their person"
        );
    }
}
