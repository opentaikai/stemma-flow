//! Persisted dismissals for research problem alerts.

use std::collections::HashSet;

use rusqlite::{Connection, params};

/// Loads the stable keys of all alerts marked as not a problem.
pub fn list_dismissed(conn: &Connection) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare("SELECT alert_key FROM problem_dismissals ORDER BY alert_key")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Marks one alert as not a problem.
pub fn dismiss(conn: &Connection, alert_key: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO problem_dismissals (alert_key) VALUES (?1)",
        params![alert_key],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Restores one dismissed alert.
pub fn restore(conn: &Connection, alert_key: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM problem_dismissals WHERE alert_key = ?1",
        params![alert_key],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Restores every dismissed alert.
pub fn clear(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM problem_dismissals", [])
        .map_err(|error| error.to_string())?;
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
        conn
    }

    #[test]
    fn dismiss_restore_and_clear_round_trip() {
        let conn = connection();
        dismiss(&conn, "missing_birth|p1||").expect("dismiss");
        dismiss(&conn, "missing_birth|p1||").expect("idempotent dismiss");
        dismiss(&conn, "death_before_birth|p2||1900|1890").expect("dismiss second");
        assert_eq!(
            list_dismissed(&conn).expect("list"),
            HashSet::from([
                "missing_birth|p1||".to_string(),
                "death_before_birth|p2||1900|1890".to_string()
            ])
        );

        restore(&conn, "missing_birth|p1||").expect("restore");
        assert_eq!(
            list_dismissed(&conn).expect("list"),
            HashSet::from(["death_before_birth|p2||1900|1890".to_string()])
        );
        clear(&conn).expect("clear");
        assert!(list_dismissed(&conn).expect("list").is_empty());
    }

    #[test]
    fn dismissals_persist_across_connections() {
        let path =
            std::env::temp_dir().join(format!("stemma-flow-problems-{}.db", uuid::Uuid::new_v4()));
        {
            let conn = crate::db::open_connection(&path).expect("open");
            init_db(&conn).expect("schema");
            dismiss(&conn, "unnamed|p1||").expect("dismiss");
        }
        {
            let conn = crate::db::open_connection(&path).expect("reopen");
            assert!(
                list_dismissed(&conn)
                    .expect("list")
                    .contains("unnamed|p1||")
            );
        }
        let _ = std::fs::remove_file(path);
    }
}
