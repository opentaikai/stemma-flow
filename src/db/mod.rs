//! Local storage layer: SQLite schema, connection setup and entity models.

pub mod citations;
pub mod facts;
mod migrations;
pub mod models;
pub mod names;
pub mod notes;
pub mod participants;
pub mod people;
pub mod places;
pub mod privacy;
pub mod repositories;
pub mod schema;
pub mod settings;
pub mod sources;

pub use models::{Citation, Event, Family, Person};
pub use schema::init_db;

use std::collections::HashMap;
use std::path::Path;

use rusqlite::Connection;

/// Opens a connection to the database at `path` with the pragmas every
/// stemma-flow connection requires: WAL journaling and enforced foreign keys.
///
/// Must be used instead of [`Connection::open`] directly; schema tests rely
/// on the foreign-key pragma to reject orphaned rows.
pub fn open_connection(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

/// Everything the canvas needs to lay out a family tree, in rowid order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FamilyTree {
    pub people: Vec<Person>,
    pub families: Vec<Family>,
    /// `(family_id, child_id)` pairs from the junction table.
    pub child_links: Vec<(String, String)>,
}

/// Loads people, families and child links ordered by rowid, so repeated
/// loads produce identical canvas layouts.
pub fn load_family_tree(conn: &Connection) -> rusqlite::Result<FamilyTree> {
    let mut people = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, given_name, middle_name, surname, nickname, name_prefix, name_suffix,
                gender, created_at, updated_at
         FROM people ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Person {
            id: row.get(0)?,
            given_name: row.get(1)?,
            middle_name: row.get(2)?,
            surname: row.get(3)?,
            nickname: row.get(4)?,
            name_prefix: row.get(5)?,
            name_suffix: row.get(6)?,
            gender: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })?;
    for row in rows {
        people.push(row?);
    }

    let mut families = Vec::new();
    let mut stmt =
        conn.prepare("SELECT id, husband_id, wife_id, created_at FROM families ORDER BY rowid")?;
    let rows = stmt.query_map([], |row| {
        Ok(Family {
            id: row.get(0)?,
            husband_id: row.get(1)?,
            wife_id: row.get(2)?,
            created_at: row.get(3)?,
        })
    })?;
    for row in rows {
        families.push(row?);
    }

    let mut child_links = Vec::new();
    let mut stmt =
        conn.prepare("SELECT family_id, child_id FROM family_children ORDER BY rowid")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        child_links.push(row?);
    }

    Ok(FamilyTree {
        people,
        families,
        child_links,
    })
}

/// Date and place of a family's marriage, when a `MARRIAGE` event exists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarriageInfo {
    pub event_id: Option<String>,
    pub date: Option<String>,
    pub place: Option<String>,
}

/// Maps each family id to its first `MARRIAGE` event, in rowid order, so
/// repeated loads produce identical inspector output.
pub fn load_marriage_details(conn: &Connection) -> rusqlite::Result<HashMap<String, MarriageInfo>> {
    let mut marriages = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT e.family_id, e.id, e.date, p.name
         FROM events e
         LEFT JOIN places p ON p.id = e.place_id
         WHERE e.event_type = 'MARRIAGE' AND e.family_id IS NOT NULL
         ORDER BY e.rowid",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    for row in rows {
        let (family_id, event_id, date, place) = row?;
        marriages.entry(family_id).or_insert(MarriageInfo {
            event_id: Some(event_id),
            date,
            place,
        });
    }
    Ok(marriages)
}

#[cfg(test)]
mod tests;
