//! Local storage layer: SQLite schema, connection setup and entity models.

pub mod models;
pub mod schema;

pub use models::{Citation, Event, Family, Person};
pub use schema::init_db;

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

#[cfg(test)]
mod tests {
    use super::open_connection;

    #[test]
    fn open_connection_enables_foreign_keys_and_wal() -> rusqlite::Result<()> {
        let path = std::env::temp_dir().join(format!("stemma-flow-{}.db", uuid::Uuid::new_v4()));
        {
            let conn = open_connection(&path)?;

            let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
            assert_eq!(fk, 1);

            let journal: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            assert_eq!(journal, "wal");
        }

        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
        Ok(())
    }
}
