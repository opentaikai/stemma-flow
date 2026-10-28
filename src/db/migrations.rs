//! In-place upgrades from older schema versions.

use std::collections::HashMap;

use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use uuid::Uuid;

use super::schema::{
    ALTERNATE_NAMES_INDEX_SQL, ALTERNATE_NAMES_TABLE_SQL, CITATION_LINKS_INDEXES_SQL,
    CITATION_LINKS_TABLE_SQL, CORE_TABLES_SQL, EVENT_PARTICIPANTS_INDEX_SQL,
    EVENT_PARTICIPANTS_TABLE_SQL, FACT_ROLES_TABLE_SQL, FACT_TAG_INDEX_SQL, INDEXES_SQL,
    LINK_TABLES_SQL, NOTES_INDEXES_SQL, NOTES_TABLE_SQL, TREE_SETTINGS_TABLE_SQL, V3_COLUMNS,
    V3_INDEXES_SQL, V3_TABLES_SQL, V4_TABLES_SQL, V5_COLUMNS, ensure_column, seed_source_templates,
};

/// Upgrades a v1 database (source titles and places stored as text) to v2.
pub(super) fn migrate_v1_to_v2(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
    tx.execute_batch(CORE_TABLES_SQL)?;
    super::facts::seed_fact_types(&tx)?;
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
    tx.pragma_update(None, "user_version", 3)?;
    tx.commit()
}

/// Adds the source template catalogue and seeds the built-ins.
pub(super) fn migrate_v3_to_v4(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch(V4_TABLES_SQL)?;
    seed_source_templates(&tx)?;
    tx.pragma_update(None, "user_version", 4)?;
    tx.commit()
}

/// Adds the free-form citation output columns.
pub(super) fn migrate_v4_to_v5(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    for (table, column, definition) in V5_COLUMNS {
        ensure_column(&tx, table, column, definition)?;
    }
    tx.pragma_update(None, "user_version", 5)?;
    tx.commit()
}

/// Moves citation attachments into the v6 link table.
pub(super) fn migrate_v5_to_v6(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch(CITATION_LINKS_TABLE_SQL)?;
    tx.execute_batch(CITATION_LINKS_INDEXES_SQL)?;
    tx.execute_batch(
        "INSERT OR IGNORE INTO citation_links (citation_id, event_id)
         SELECT id, event_id FROM citations WHERE event_id IS NOT NULL;
         UPDATE citations SET event_id = NULL WHERE event_id IS NOT NULL;",
    )?;
    tx.pragma_update(None, "user_version", 6)?;
    tx.commit()
}

/// Adds fact scope/kind columns and refreshes the built-in catalogue.
pub(super) fn migrate_v6_to_v7(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    ensure_column(
        &tx,
        "fact_types",
        "scope",
        "TEXT NOT NULL DEFAULT 'person' CHECK (scope IN ('person', 'family', 'both'))",
    )?;
    ensure_column(
        &tx,
        "fact_types",
        "kind",
        "TEXT NOT NULL DEFAULT 'event' CHECK (kind IN ('event', 'attribute'))",
    )?;
    super::facts::seed_fact_types(&tx)?;
    tx.pragma_update(None, "user_version", 7)?;
    tx.commit()
}

/// Adds fact roles and the unique GEDCOM-tag index.
pub(super) fn migrate_v7_to_v8(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch(
        "UPDATE fact_types SET gedcom_tag = NULL
         WHERE gedcom_tag IS NOT NULL AND trim(gedcom_tag) <> ''
           AND rowid NOT IN (
               SELECT MIN(rowid) FROM fact_types
               WHERE gedcom_tag IS NOT NULL AND trim(gedcom_tag) <> ''
               GROUP BY gedcom_tag
           );",
    )?;
    tx.execute_batch(FACT_ROLES_TABLE_SQL)?;
    tx.execute_batch(FACT_TAG_INDEX_SQL)?;
    tx.pragma_update(None, "user_version", 8)?;
    tx.commit()
}

/// Adds event participants for shared events and witnesses.
pub(super) fn migrate_v8_to_v9(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch(EVENT_PARTICIPANTS_TABLE_SQL)?;
    tx.execute_batch(EVENT_PARTICIPANTS_INDEX_SQL)?;
    tx.pragma_update(None, "user_version", 9)?;
    tx.commit()
}

/// Adds person/event privacy flags and the living override.
pub(super) fn migrate_v9_to_v10(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    ensure_column(
        &tx,
        "people",
        "private",
        "INTEGER NOT NULL DEFAULT 0 CHECK (private IN (0, 1))",
    )?;
    ensure_column(
        &tx,
        "people",
        "living_override",
        "INTEGER CHECK (living_override IN (0, 1))",
    )?;
    ensure_column(
        &tx,
        "events",
        "private",
        "INTEGER NOT NULL DEFAULT 0 CHECK (private IN (0, 1))",
    )?;
    tx.pragma_update(None, "user_version", 10)?;
    tx.commit()
}

/// Adds primary name pieces and alternate names.
pub(super) fn migrate_v10_to_v11(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    ensure_column(&tx, "people", "name_prefix", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(&tx, "people", "name_suffix", "TEXT NOT NULL DEFAULT ''")?;
    tx.execute_batch(ALTERNATE_NAMES_TABLE_SQL)?;
    tx.execute_batch(ALTERNATE_NAMES_INDEX_SQL)?;
    tx.pragma_update(None, "user_version", 11)?;
    tx.commit()
}

/// Adds the per-tree settings table.
pub(super) fn migrate_v11_to_v12(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch(TREE_SETTINGS_TABLE_SQL)?;
    tx.pragma_update(None, "user_version", 12)?;
    tx.commit()
}

/// Adds first-class notes for people, families, events, citations and places.
pub(super) fn migrate_v12_to_v13(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch(NOTES_TABLE_SQL)?;
    tx.execute_batch(NOTES_INDEXES_SQL)?;
    tx.pragma_update(None, "user_version", 13)?;
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
