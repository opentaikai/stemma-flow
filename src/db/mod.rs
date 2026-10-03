//! Local storage layer: SQLite schema, connection setup and entity models.

pub mod models;

pub use models::{Citation, Event, Family, Person};

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
