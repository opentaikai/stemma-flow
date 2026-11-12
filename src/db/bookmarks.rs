//! Persistent per-tree person bookmarks.

use rusqlite::{Connection, OptionalExtension, params};

use crate::model::name::format_name;

/// One bookmarked person with a display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkPerson {
    pub person_id: String,
    pub name: String,
}

/// Lists bookmarks in insertion order.
pub fn list_bookmarks(
    conn: &Connection,
    surname_first: bool,
) -> Result<Vec<BookmarkPerson>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix
             FROM bookmarks b JOIN people p ON p.id = b.person_id
             ORDER BY b.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let name = format_name(
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
                &row.get::<_, String>(4)?,
                &row.get::<_, String>(5)?,
                surname_first,
            );
            Ok(BookmarkPerson {
                person_id: row.get(0)?,
                name: if name.is_empty() {
                    "(unnamed)".to_string()
                } else {
                    name
                },
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Returns whether a person is bookmarked.
pub fn is_bookmarked(conn: &Connection, person_id: &str) -> Result<bool, String> {
    let found: i64 = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bookmarks WHERE person_id = ?1)",
            params![person_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(found != 0)
}

/// Adds or removes a bookmark, returning the new bookmarked state.
pub fn toggle_bookmark(conn: &Connection, person_id: &str) -> Result<bool, String> {
    if is_bookmarked(conn, person_id)? {
        remove_bookmark(conn, person_id)?;
        return Ok(false);
    }
    ensure_person(conn, person_id)?;
    conn.execute(
        "INSERT OR IGNORE INTO bookmarks (person_id) VALUES (?1)",
        params![person_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(true)
}

/// Removes a bookmark when present.
pub fn remove_bookmark(conn: &Connection, person_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM bookmarks WHERE person_id = ?1",
        params![person_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn ensure_person(conn: &Connection, person_id: &str) -> Result<(), String> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM people WHERE id = ?1",
            params![person_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if found.is_none() {
        return Err(format!("no person with id {person_id}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("database");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragmas");
        init_db(&conn).expect("schema");
        conn.execute_batch(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F'), ('p2', 'Charles', 'Babbage', 'M');",
        )
        .expect("people");
        conn
    }

    #[test]
    fn lists_insertion_order_and_formats_names() {
        let conn = connection();
        assert!(toggle_bookmark(&conn, "p1").expect("add"));
        assert!(toggle_bookmark(&conn, "p2").expect("add"));
        let bookmarks = list_bookmarks(&conn, false).expect("list");
        assert_eq!(bookmarks.len(), 2);
        assert_eq!(bookmarks[0].person_id, "p1");
        assert_eq!(bookmarks[0].name, "Ada King");
        assert_eq!(bookmarks[1].person_id, "p2");
        assert_eq!(bookmarks[1].name, "Charles Babbage");
        assert!(is_bookmarked(&conn, "p1").expect("check"));
    }

    #[test]
    fn toggles_removes_and_cascades() {
        let conn = connection();
        assert!(toggle_bookmark(&conn, "p1").expect("add"));
        assert!(is_bookmarked(&conn, "p1").expect("check"));
        assert!(!toggle_bookmark(&conn, "p1").expect("remove"));
        assert!(!is_bookmarked(&conn, "p1").expect("check removed"));
        remove_bookmark(&conn, "p1").expect("idempotent remove");

        toggle_bookmark(&conn, "p2").expect("add");
        conn.execute("DELETE FROM people WHERE id = 'p2'", [])
            .expect("delete person");
        assert!(list_bookmarks(&conn, false).expect("list").is_empty());
        assert!(!is_bookmarked(&conn, "p2").expect("cascade"));
    }

    #[test]
    fn rejects_unknown_people() {
        let conn = connection();
        assert!(toggle_bookmark(&conn, "missing").is_err());
    }
}
