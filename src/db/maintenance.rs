//! On-demand SQLite integrity, index and storage maintenance.

use std::path::{Path, PathBuf};
use std::time::Instant;

use rusqlite::Connection;

/// A maintenance operation requested by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceKind {
    Integrity,
    RebuildIndexes,
    Compress,
}

/// Result of one maintenance operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceReport {
    pub kind: MaintenanceKind,
    pub healthy: bool,
    pub issues: Vec<String>,
    pub foreign_key_violations: usize,
    pub indexes_rebuilt: i64,
    pub page_count: i64,
    pub freelist_count: i64,
    pub size_before: u64,
    pub size_after: u64,
    pub duration_ms: u128,
}

/// Runs one maintenance operation against `db_path`.
pub fn run(db_path: &Path, kind: MaintenanceKind) -> Result<MaintenanceReport, String> {
    if !db_path.is_file() {
        return Err(format!("no tree database at {}", db_path.display()));
    }
    let size_before = database_size(db_path);
    let started = Instant::now();
    let conn = crate::db::open_connection(db_path).map_err(|error| error.to_string())?;
    let mut report = MaintenanceReport {
        kind,
        healthy: true,
        issues: Vec::new(),
        foreign_key_violations: 0,
        indexes_rebuilt: 0,
        page_count: 0,
        freelist_count: 0,
        size_before,
        size_after: size_before,
        duration_ms: 0,
    };
    match kind {
        MaintenanceKind::Integrity => {
            report.issues = integrity_issues(&conn)?;
            report.foreign_key_violations = foreign_key_issues(&conn, &mut report.issues)?;
            report.healthy = report.issues.is_empty();
            report.page_count = pragma_i64(&conn, "page_count")?;
            report.freelist_count = pragma_i64(&conn, "freelist_count")?;
        }
        MaintenanceKind::RebuildIndexes => {
            conn.execute_batch("REINDEX; ANALYZE;")
                .map_err(|error| error.to_string())?;
            checkpoint(&conn)?;
            report.indexes_rebuilt = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'index' AND sql IS NOT NULL",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            report.page_count = pragma_i64(&conn, "page_count")?;
            report.freelist_count = pragma_i64(&conn, "freelist_count")?;
        }
        MaintenanceKind::Compress => {
            checkpoint(&conn)?;
            conn.execute_batch("VACUUM;")
                .map_err(|error| error.to_string())?;
            checkpoint(&conn)?;
            report.page_count = pragma_i64(&conn, "page_count")?;
            report.freelist_count = pragma_i64(&conn, "freelist_count")?;
        }
    }
    drop(conn);
    report.size_after = database_size(db_path);
    report.duration_ms = started.elapsed().as_millis();
    Ok(report)
}

fn integrity_issues(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("PRAGMA integrity_check")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut issues: Vec<String> = rows.into_iter().filter(|row| row != "ok").collect();
    truncate_issues(&mut issues, "integrity");
    Ok(issues)
}

fn foreign_key_issues(conn: &Connection, issues: &mut Vec<String>) -> Result<usize, String> {
    let mut stmt = conn
        .prepare("PRAGMA foreign_key_check")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let violations = rows.len();
    for (table, rowid, parent, foreign_key) in rows.into_iter().take(100) {
        let row = rowid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "without rowid".to_string());
        issues.push(format!(
            "foreign key {foreign_key} in {table} row {row} references missing {parent}"
        ));
    }
    if violations > 100 {
        issues.push(format!(
            "… and {} more foreign key violations",
            violations - 100
        ));
    }
    Ok(violations)
}

fn truncate_issues(issues: &mut Vec<String>, label: &str) {
    if issues.len() > 100 {
        let remaining = issues.len() - 100;
        issues.truncate(100);
        issues.push(format!("… and {remaining} more {label} issues"));
    }
}

fn pragma_i64(conn: &Connection, name: &str) -> Result<i64, String> {
    conn.query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn checkpoint(conn: &Connection) -> Result<(), String> {
    let (busy, _, _): (i64, i64, i64) = conn
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| error.to_string())?;
    if busy != 0 {
        return Err(
            "could not checkpoint the database; close other connections and retry".to_string(),
        );
    }
    Ok(())
}

/// Total size of the database file and its WAL sidecar.
pub fn database_size(db_path: &Path) -> u64 {
    let main = std::fs::metadata(db_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let wal = std::fs::metadata(sidecar(db_path, "-wal"))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    main + wal
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "stemma-flow-maintenance-{}.db",
            uuid::Uuid::new_v4()
        ))
    }

    fn seeded() -> PathBuf {
        let path = temp_path();
        let conn = crate::db::open_connection(&path).expect("database opens");
        init_db(&conn).expect("schema");
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person");
        path
    }

    #[test]
    fn healthy_database_passes_integrity_and_foreign_keys() {
        let path = seeded();
        let report = run(&path, MaintenanceKind::Integrity).expect("check");
        assert!(report.healthy);
        assert!(report.issues.is_empty());
        assert_eq!(report.foreign_key_violations, 0);
        assert!(report.page_count > 0);
        assert_eq!(report.size_before, report.size_after);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn foreign_key_orphans_are_reported() {
        let path = seeded();
        {
            let conn = Connection::open(&path).expect("raw open");
            conn.execute_batch("PRAGMA foreign_keys = OFF;")
                .expect("disable fks");
            conn.execute(
                "INSERT INTO events (id, event_type, person_id)
                 VALUES ('e1', 'BIRTH', 'missing-person')",
                [],
            )
            .expect("orphan event");
        }
        let report = run(&path, MaintenanceKind::Integrity).expect("check");
        assert!(!report.healthy);
        assert_eq!(report.foreign_key_violations, 1);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.contains("events") && issue.contains("foreign key"))
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reindex_refreshes_indexes_and_statistics() {
        let path = seeded();
        let report = run(&path, MaintenanceKind::RebuildIndexes).expect("reindex");
        assert!(report.healthy);
        assert!(report.indexes_rebuilt > 0);
        let conn = Connection::open(&path).expect("db");
        let stat: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'sqlite_stat1'",
                [],
                |row| row.get(0),
            )
            .expect("stat table");
        assert_eq!(stat, 1);
        let people: i64 = conn
            .query_row("SELECT COUNT(*) FROM people", [], |row| row.get(0))
            .expect("people");
        assert_eq!(people, 1);
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn compression_reclaims_deleted_pages() {
        let path = temp_path();
        {
            let conn = crate::db::open_connection(&path).expect("db");
            init_db(&conn).expect("schema");
            let tx = conn.unchecked_transaction().expect("transaction");
            for index in 0..2000 {
                tx.execute(
                    "INSERT INTO people (id, given_name, surname, gender)
                     VALUES (?1, 'Given', 'Surname', 'U')",
                    rusqlite::params![format!("p{index}")],
                )
                .expect("person");
            }
            tx.commit().expect("commit");
            conn.execute("DELETE FROM people", []).expect("delete");
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .expect("checkpoint");
        }
        let before = database_size(&path);
        let report = run(&path, MaintenanceKind::Compress).expect("compress");
        assert!(
            report.size_after < before,
            "{before} -> {}",
            report.size_after
        );
        assert!(report.freelist_count <= report.page_count);
        assert!(report.size_before >= before - 1);
        let conn = Connection::open(&path).expect("db");
        let people: i64 = conn
            .query_row("SELECT COUNT(*) FROM people", [], |row| row.get(0))
            .expect("people");
        assert_eq!(people, 0);
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_database_is_rejected() {
        let path = temp_path();
        let _ = std::fs::remove_file(&path);
        assert!(run(&path, MaintenanceKind::Integrity).is_err());
        assert_eq!(database_size(&path), 0);
    }

    #[test]
    fn repeated_operations_leave_data_intact() {
        let path = seeded();
        for kind in [
            MaintenanceKind::Integrity,
            MaintenanceKind::RebuildIndexes,
            MaintenanceKind::Compress,
        ] {
            let report = run(&path, kind).expect("maintenance succeeds");
            assert!(report.duration_ms < 60_000);
        }
        let conn = Connection::open(&path).expect("db");
        let given: String = conn
            .query_row("SELECT given_name FROM people WHERE id = 'p1'", [], |row| {
                row.get(0)
            })
            .expect("person");
        assert_eq!(given, "Ada");
        drop(conn);
        let _ = std::fs::remove_file(path);
    }
}
