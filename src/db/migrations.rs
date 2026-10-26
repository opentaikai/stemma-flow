//! In-place upgrades from older schema versions.

use std::collections::HashMap;

use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use uuid::Uuid;

use super::schema::{
    CORE_TABLES_SQL, FACT_SEED_SQL, INDEXES_SQL, LINK_TABLES_SQL, SCHEMA_VERSION, V3_COLUMNS,
    V3_INDEXES_SQL, V3_TABLES_SQL, ensure_column,
};

/// Upgrades a v1 database (source titles and places stored as text) to v2.
pub(super) fn migrate_v1_to_v2(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
    tx.execute_batch(CORE_TABLES_SQL)?;
    tx.execute_batch(FACT_SEED_SQL)?;
    migrate_places(&tx)?;
    rebuild_citations(&tx)?;
    tx.execute_batch(LINK_TABLES_SQL)?;
    tx.execute_batch(INDEXES_SQL)?;
    tx.pragma_update(None, "user_version", 2)?;
    tx.commit()
}

/// Adds the v3 source/citation fields and the repository link table.
pub(super) fn migrate_v2_to_v3(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
    for (table, column, definition) in V3_COLUMNS {
        ensure_column(&tx, table, column, definition)?;
    }
    tx.execute_batch(V3_TABLES_SQL)?;
    tx.execute_batch(V3_INDEXES_SQL)?;
    tx.execute_batch(
        "INSERT OR IGNORE INTO source_repositories (source_id, repository_id)
         SELECT id, repository_id FROM sources WHERE repository_id IS NOT NULL;",
    )?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()
}

/// Copies distinct event places into `places` and links the events.
fn migrate_places(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let mut stmt = tx.prepare(
        "SELECT DISTINCT trim(place) FROM events
         WHERE place IS NOT NULL AND trim(place) <> ''",
    )?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut by_name: HashMap<String, String> = HashMap::with_capacity(names.len());
    for name in names {
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO places (id, name) VALUES (?1, ?2)",
            params![id, name],
        )?;
        by_name.insert(name, id);
    }

    tx.execute_batch(
        "ALTER TABLE events ADD COLUMN place_id TEXT REFERENCES places(id) ON DELETE SET NULL;",
    )?;
    for (name, id) in by_name {
        tx.execute(
            "UPDATE events SET place_id = ?1 WHERE place IS NOT NULL AND trim(place) = ?2",
            params![id, name],
        )?;
    }
    tx.execute_batch("ALTER TABLE events DROP COLUMN place;")?;
    Ok(())
}

/// Extracts sources from citation titles and rebuilds `citations` with a
/// `source_id` foreign key.
fn rebuild_citations(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let mut stmt = tx.prepare(
        "SELECT id, source_title, page_or_reference, notes, event_id
         FROM citations ORDER BY rowid",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut sources: HashMap<(String, Option<String>), String> = HashMap::new();
    for (_, title, _, notes, _) in &rows {
        let slot = sources.entry((title.clone(), notes.clone())).or_default();
        if !slot.is_empty() {
            continue;
        }
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO sources (id, title, notes) VALUES (?1, ?2, ?3)",
            params![id, title, notes],
        )?;
        *slot = id;
    }

    tx.execute_batch(
        "CREATE TABLE citations_v2 (
            id TEXT PRIMARY KEY,
            source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
            page_or_reference TEXT,
            notes TEXT,
            quality_source TEXT,
            quality_information TEXT,
            quality_evidence TEXT,
            event_id TEXT REFERENCES events(id) ON DELETE CASCADE
        );",
    )?;
    for (id, title, page, notes, event_id) in rows {
        let source_id = sources
            .get(&(title, notes))
            .cloned()
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        tx.execute(
            "INSERT INTO citations_v2 (id, source_id, page_or_reference, notes, event_id)
             VALUES (?1, ?2, ?3, NULL, ?4)",
            params![id, source_id, page, event_id],
        )?;
    }
    tx.execute_batch("DROP TABLE citations; ALTER TABLE citations_v2 RENAME TO citations;")?;
    Ok(())
}
