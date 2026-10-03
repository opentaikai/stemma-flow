//! Event-centric relational schema (Gramps-style) for stemma-flow.

use rusqlite::Connection;

const SCHEMA_SQL: &str = r#"
BEGIN IMMEDIATE;

CREATE TABLE IF NOT EXISTS people (
    id TEXT PRIMARY KEY,
    given_name TEXT NOT NULL,
    surname TEXT NOT NULL,
    gender TEXT NOT NULL CHECK (gender IN ('M', 'F', 'U')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS families (
    id TEXT PRIMARY KEY,
    husband_id TEXT REFERENCES people(id) ON DELETE SET NULL,
    wife_id TEXT REFERENCES people(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS family_children (
    family_id TEXT NOT NULL REFERENCES families(id) ON DELETE CASCADE,
    child_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    PRIMARY KEY (family_id, child_id)
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    date TEXT,
    place TEXT,
    description TEXT,
    person_id TEXT REFERENCES people(id) ON DELETE CASCADE,
    family_id TEXT REFERENCES families(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS citations (
    id TEXT PRIMARY KEY,
    source_title TEXT NOT NULL,
    page_or_reference TEXT,
    notes TEXT,
    event_id TEXT REFERENCES events(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_families_husband ON families(husband_id);
CREATE INDEX IF NOT EXISTS idx_families_wife ON families(wife_id);
CREATE INDEX IF NOT EXISTS idx_family_children_child ON family_children(child_id);
CREATE INDEX IF NOT EXISTS idx_events_person ON events(person_id);
CREATE INDEX IF NOT EXISTS idx_events_family ON events(family_id);
CREATE INDEX IF NOT EXISTS idx_citations_event ON citations(event_id);
CREATE INDEX IF NOT EXISTS idx_people_names ON people(surname, given_name);

COMMIT;
"#;

/// Creates the stemma-flow tables and indexes if they do not exist yet.
///
/// Runs the whole DDL batch inside a single transaction; foreign-key and WAL
/// pragmas are the caller's responsibility (see [`crate::db::open_connection`]).
pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)
}
