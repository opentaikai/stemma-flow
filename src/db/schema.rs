//! Event-centric relational schema (Gramps-style) for stemma-flow.
//!
//! Schema version 2 normalises sources and places: citations reference
//! `sources` and events reference `places`. The version is stored in
//! `PRAGMA user_version`; [`init_db`] creates a fresh v2 database, upgrades
//! v1 databases in place, and rejects databases from newer builds.

use rusqlite::{Connection, Transaction, TransactionBehavior};

/// Current schema version stored in `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 10;

/// Tables shared by fresh databases and migrations.
pub(super) const CORE_TABLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS people (
    id TEXT PRIMARY KEY,
    given_name TEXT NOT NULL,
    middle_name TEXT NOT NULL DEFAULT '',
    surname TEXT NOT NULL,
    nickname TEXT NOT NULL DEFAULT '',
    gender TEXT NOT NULL CHECK (gender IN ('M', 'F', 'U')),
    notes TEXT NOT NULL DEFAULT '',
    private INTEGER NOT NULL DEFAULT 0 CHECK (private IN (0, 1)),
    living_override INTEGER CHECK (living_override IN (0, 1)),
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

CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    address TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    author TEXT,
    publication TEXT,
    repository_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,
    template_id TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS source_fields (
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    field_key TEXT NOT NULL,
    field_value TEXT NOT NULL,
    PRIMARY KEY (source_id, field_key)
);

CREATE TABLE IF NOT EXISTS places (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    latitude REAL,
    longitude REAL,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS fact_types (
    key TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    category TEXT,
    gedcom_tag TEXT,
    scope TEXT NOT NULL DEFAULT 'person' CHECK (scope IN ('person', 'family', 'both')),
    kind TEXT NOT NULL DEFAULT 'event' CHECK (kind IN ('event', 'attribute')),
    is_builtin INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    date TEXT,
    place_id TEXT REFERENCES places(id) ON DELETE SET NULL,
    description TEXT,
    private INTEGER NOT NULL DEFAULT 0 CHECK (private IN (0, 1)),
    person_id TEXT REFERENCES people(id) ON DELETE CASCADE,
    family_id TEXT REFERENCES families(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS media (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL,
    format TEXT,
    caption TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done')),
    priority INTEGER,
    folder TEXT,
    due_date TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// The citations table references sources. `event_id` is legacy and is kept
/// nullable for v5 databases; v6 attaches citations through `citation_links`.
pub(super) const CITATIONS_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS citations (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    page_or_reference TEXT,
    notes TEXT,
    quality_source TEXT,
    quality_information TEXT,
    quality_evidence TEXT,
    event_id TEXT REFERENCES events(id) ON DELETE CASCADE
);
"#;

/// One canonical citation can support many events (schema v6).
pub(super) const CITATION_LINKS_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS citation_links (
    citation_id TEXT NOT NULL REFERENCES citations(id) ON DELETE CASCADE,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    PRIMARY KEY (citation_id, event_id)
);
"#;

pub(super) const CITATION_LINKS_INDEXES_SQL: &str = r#"
CREATE INDEX IF NOT EXISTS idx_citation_links_citation ON citation_links(citation_id);
CREATE INDEX IF NOT EXISTS idx_citation_links_event ON citation_links(event_id);
"#;

/// Join tables that attach media and tasks to any record and people to groups.
pub(super) const LINK_TABLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS media_links (
    media_id TEXT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    person_id TEXT REFERENCES people(id) ON DELETE CASCADE,
    family_id TEXT REFERENCES families(id) ON DELETE CASCADE,
    event_id TEXT REFERENCES events(id) ON DELETE CASCADE,
    source_id TEXT REFERENCES sources(id) ON DELETE CASCADE,
    citation_id TEXT REFERENCES citations(id) ON DELETE CASCADE,
    place_id TEXT REFERENCES places(id) ON DELETE CASCADE,
    CHECK ((person_id IS NOT NULL) + (family_id IS NOT NULL) + (event_id IS NOT NULL)
         + (source_id IS NOT NULL) + (citation_id IS NOT NULL) + (place_id IS NOT NULL) = 1)
);

CREATE TABLE IF NOT EXISTS task_links (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    person_id TEXT REFERENCES people(id) ON DELETE CASCADE,
    family_id TEXT REFERENCES families(id) ON DELETE CASCADE,
    event_id TEXT REFERENCES events(id) ON DELETE CASCADE,
    source_id TEXT REFERENCES sources(id) ON DELETE CASCADE,
    citation_id TEXT REFERENCES citations(id) ON DELETE CASCADE,
    place_id TEXT REFERENCES places(id) ON DELETE CASCADE,
    media_id TEXT REFERENCES media(id) ON DELETE CASCADE,
    CHECK ((person_id IS NOT NULL) + (family_id IS NOT NULL) + (event_id IS NOT NULL)
         + (source_id IS NOT NULL) + (citation_id IS NOT NULL) + (place_id IS NOT NULL)
         + (media_id IS NOT NULL) = 1)
);

CREATE TABLE IF NOT EXISTS group_members (
    group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, person_id)
);
"#;

/// Indexes for the v2 schema. Safe to re-run.
pub(super) const INDEXES_SQL: &str = r#"
CREATE INDEX IF NOT EXISTS idx_families_husband ON families(husband_id);
CREATE INDEX IF NOT EXISTS idx_families_wife ON families(wife_id);
CREATE INDEX IF NOT EXISTS idx_family_children_child ON family_children(child_id);
CREATE INDEX IF NOT EXISTS idx_events_person ON events(person_id);
CREATE INDEX IF NOT EXISTS idx_events_family ON events(family_id);
CREATE INDEX IF NOT EXISTS idx_events_place ON events(place_id);
CREATE INDEX IF NOT EXISTS idx_citations_source ON citations(source_id);
CREATE INDEX IF NOT EXISTS idx_citations_event ON citations(event_id);
CREATE INDEX IF NOT EXISTS idx_people_names ON people(surname, given_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_places_name ON places(name);
CREATE INDEX IF NOT EXISTS idx_sources_title ON sources(title);
CREATE INDEX IF NOT EXISTS idx_sources_repository ON sources(repository_id);
CREATE INDEX IF NOT EXISTS idx_source_fields_source ON source_fields(source_id);
CREATE INDEX IF NOT EXISTS idx_media_links_media ON media_links(media_id);
CREATE INDEX IF NOT EXISTS idx_media_links_person ON media_links(person_id);
CREATE INDEX IF NOT EXISTS idx_media_links_family ON media_links(family_id);
CREATE INDEX IF NOT EXISTS idx_media_links_event ON media_links(event_id);
CREATE INDEX IF NOT EXISTS idx_media_links_source ON media_links(source_id);
CREATE INDEX IF NOT EXISTS idx_media_links_citation ON media_links(citation_id);
CREATE INDEX IF NOT EXISTS idx_media_links_place ON media_links(place_id);
CREATE INDEX IF NOT EXISTS idx_task_links_task ON task_links(task_id);
CREATE INDEX IF NOT EXISTS idx_task_links_person ON task_links(person_id);
CREATE INDEX IF NOT EXISTS idx_task_links_family ON task_links(family_id);
CREATE INDEX IF NOT EXISTS idx_task_links_event ON task_links(event_id);
CREATE INDEX IF NOT EXISTS idx_task_links_source ON task_links(source_id);
CREATE INDEX IF NOT EXISTS idx_task_links_citation ON task_links(citation_id);
CREATE INDEX IF NOT EXISTS idx_task_links_place ON task_links(place_id);
CREATE INDEX IF NOT EXISTS idx_task_links_media ON task_links(media_id);
CREATE INDEX IF NOT EXISTS idx_group_members_person ON group_members(person_id);
"#;

/// Columns added in schema v3.
pub(super) const V3_COLUMNS: [(&str, &str, &str); 7] = [
    ("sources", "abbreviation", "TEXT"),
    ("sources", "data_date", "TEXT"),
    ("sources", "data_text", "TEXT"),
    ("citations", "quality_certainty", "TEXT"),
    ("citations", "media_type", "TEXT"),
    ("citations", "data_date", "TEXT"),
    ("citations", "data_text", "TEXT"),
];

/// A source can cite more than one repository in GEDCOM.
pub(super) const V3_TABLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS source_repositories (
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    call_number TEXT,
    media_type TEXT,
    PRIMARY KEY (source_id, repository_id)
);
"#;

pub(super) const V3_INDEXES_SQL: &str = r#"
CREATE INDEX IF NOT EXISTS idx_source_repositories_source ON source_repositories(source_id);
CREATE INDEX IF NOT EXISTS idx_source_repositories_repository ON source_repositories(repository_id);
"#;

/// Free-form citation outputs added in schema v5.
pub(super) const V5_COLUMNS: [(&str, &str, &str); 3] = [
    ("sources", "footnote", "TEXT"),
    ("sources", "short_footnote", "TEXT"),
    ("sources", "bibliography", "TEXT"),
];

/// Roles belong to one fact type and feed future shared events.
pub(super) const FACT_ROLES_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS fact_type_roles (
    fact_type_key TEXT NOT NULL REFERENCES fact_types(key) ON DELETE CASCADE,
    role_key TEXT NOT NULL,
    label TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (fact_type_key, role_key)
);
"#;

/// Built-in and custom GEDCOM tags must not collide.
pub(super) const FACT_TAG_INDEX_SQL: &str = r#"
CREATE UNIQUE INDEX IF NOT EXISTS idx_fact_types_gedcom_tag
    ON fact_types(gedcom_tag)
    WHERE gedcom_tag IS NOT NULL AND trim(gedcom_tag) <> '';
"#;

/// Additional people sharing an event, each with a role.
pub(super) const EVENT_PARTICIPANTS_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS event_participants (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    role_key TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (event_id, person_id)
);
"#;

pub(super) const EVENT_PARTICIPANTS_INDEX_SQL: &str = r#"
CREATE INDEX IF NOT EXISTS idx_event_participants_person
    ON event_participants(person_id);
"#;

/// Source templates and their fields.
pub(super) const V4_TABLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS source_templates (
    key TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT,
    footnote TEXT NOT NULL,
    short_footnote TEXT NOT NULL,
    bibliography TEXT NOT NULL,
    gedcom TEXT NOT NULL,
    is_builtin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS source_template_fields (
    template_key TEXT NOT NULL REFERENCES source_templates(key) ON DELETE CASCADE,
    field_key TEXT NOT NULL,
    label TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('source', 'citation')),
    required INTEGER NOT NULL DEFAULT 0,
    hint TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (template_key, field_key)
);

CREATE INDEX IF NOT EXISTS idx_source_template_fields_template
    ON source_template_fields(template_key);
"#;

/// The v1 tables, used only to fill gaps in partially created databases
/// before migrating them.
const V1_SCHEMA_SQL: &str = r#"
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
"#;

/// Creates or upgrades the stemma-flow schema.
///
/// Runs DDL and migrations inside a transaction; foreign-key and WAL pragmas
/// are the caller's responsibility (see [`crate::db::open_connection`]).
pub fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    let version = user_version(conn)?;
    if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "database schema version {version} is newer than supported version {SCHEMA_VERSION}"
        )));
    }
    if version == 0 && !table_exists(conn, "people")? {
        create_v2(conn)?;
        super::migrations::migrate_v2_to_v3(conn)?;
        super::migrations::migrate_v3_to_v4(conn)?;
        super::migrations::migrate_v4_to_v5(conn)?;
        super::migrations::migrate_v5_to_v6(conn)?;
        super::migrations::migrate_v6_to_v7(conn)?;
        super::migrations::migrate_v7_to_v8(conn)?;
        super::migrations::migrate_v8_to_v9(conn)?;
        return super::migrations::migrate_v9_to_v10(conn);
    }
    if version < SCHEMA_VERSION {
        if version < 2 {
            if version == 0 {
                conn.execute_batch(V1_SCHEMA_SQL)?;
            }
            ensure_v1_columns(conn)?;
            super::migrations::migrate_v1_to_v2(conn)?;
        }
        if version < 3 {
            super::migrations::migrate_v2_to_v3(conn)?;
        }
        if version < 4 {
            super::migrations::migrate_v3_to_v4(conn)?;
        }
        if version < 5 {
            super::migrations::migrate_v4_to_v5(conn)?;
        }
        if version < 6 {
            super::migrations::migrate_v5_to_v6(conn)?;
        }
        if version < 7 {
            super::migrations::migrate_v6_to_v7(conn)?;
        }
        if version < 8 {
            super::migrations::migrate_v7_to_v8(conn)?;
        }
        if version < 9 {
            super::migrations::migrate_v8_to_v9(conn)?;
        }
        return super::migrations::migrate_v9_to_v10(conn);
    }
    ensure_v10(conn)
}

fn create_v2(conn: &Connection) -> rusqlite::Result<()> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    tx.execute_batch(CORE_TABLES_SQL)?;
    tx.execute_batch(CITATIONS_TABLE_SQL)?;
    tx.execute_batch(LINK_TABLES_SQL)?;
    tx.execute_batch(INDEXES_SQL)?;
    super::facts::seed_fact_types(&tx)?;
    tx.pragma_update(None, "user_version", 2)?;
    tx.commit()
}

fn ensure_v10(conn: &Connection) -> rusqlite::Result<()> {
    ensure_v1_columns(conn)?;
    conn.execute_batch(CORE_TABLES_SQL)?;
    conn.execute_batch(CITATIONS_TABLE_SQL)?;
    conn.execute_batch(LINK_TABLES_SQL)?;
    conn.execute_batch(INDEXES_SQL)?;
    for (table, column, definition) in V3_COLUMNS {
        ensure_column(conn, table, column, definition)?;
    }
    conn.execute_batch(V3_TABLES_SQL)?;
    conn.execute_batch(V3_INDEXES_SQL)?;
    conn.execute_batch(V4_TABLES_SQL)?;
    seed_source_templates(conn)?;
    for (table, column, definition) in V5_COLUMNS {
        ensure_column(conn, table, column, definition)?;
    }
    conn.execute_batch(CITATION_LINKS_TABLE_SQL)?;
    conn.execute_batch(CITATION_LINKS_INDEXES_SQL)?;
    conn.execute_batch(
        "INSERT OR IGNORE INTO citation_links (citation_id, event_id)
         SELECT id, event_id FROM citations WHERE event_id IS NOT NULL;
         UPDATE citations SET event_id = NULL WHERE event_id IS NOT NULL;",
    )?;
    ensure_column(
        conn,
        "fact_types",
        "scope",
        "TEXT NOT NULL DEFAULT 'person' CHECK (scope IN ('person', 'family', 'both'))",
    )?;
    ensure_column(
        conn,
        "fact_types",
        "kind",
        "TEXT NOT NULL DEFAULT 'event' CHECK (kind IN ('event', 'attribute'))",
    )?;
    super::facts::seed_fact_types(conn)?;
    conn.execute_batch(FACT_ROLES_TABLE_SQL)?;
    conn.execute_batch(FACT_TAG_INDEX_SQL)?;
    conn.execute_batch(EVENT_PARTICIPANTS_TABLE_SQL)?;
    conn.execute_batch(EVENT_PARTICIPANTS_INDEX_SQL)?;
    ensure_column(
        conn,
        "people",
        "private",
        "INTEGER NOT NULL DEFAULT 0 CHECK (private IN (0, 1))",
    )?;
    ensure_column(
        conn,
        "people",
        "living_override",
        "INTEGER CHECK (living_override IN (0, 1))",
    )?;
    ensure_column(
        conn,
        "events",
        "private",
        "INTEGER NOT NULL DEFAULT 0 CHECK (private IN (0, 1))",
    )?;
    Ok(())
}

/// Inserts the built-in templates into a database that lacks them.
pub(super) fn seed_source_templates(conn: &Connection) -> rusqlite::Result<()> {
    use rusqlite::params;

    for template in crate::model::source_template::builtin_templates() {
        let gedcom = toml::to_string(&template.gedcom)
            .map_err(|error| rusqlite::Error::InvalidParameterName(error.to_string()))?;
        conn.execute(
            "INSERT OR IGNORE INTO source_templates
             (key, name, category, footnote, short_footnote, bibliography, gedcom, is_builtin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![
                template.key,
                template.name,
                template.category,
                template.footnote,
                template.short_footnote,
                template.bibliography,
                gedcom
            ],
        )?;
        for (index, field) in template.fields.iter().enumerate() {
            conn.execute(
                "INSERT OR IGNORE INTO source_template_fields
                 (template_key, field_key, label, scope, required, hint, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    template.key,
                    field.key,
                    field.label,
                    match field.scope {
                        crate::model::source_template::FieldScope::Source => "source",
                        crate::model::source_template::FieldScope::Citation => "citation",
                    },
                    field.required as i64,
                    field.hint,
                    index as i64
                ],
            )?;
        }
    }
    Ok(())
}

fn ensure_v1_columns(conn: &Connection) -> rusqlite::Result<()> {
    ensure_column(conn, "people", "middle_name", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(conn, "people", "nickname", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(conn, "people", "notes", "TEXT NOT NULL DEFAULT ''")?;
    Ok(())
}

/// Adds `column` to `table` when a database predates it (`CREATE TABLE IF
/// NOT EXISTS` leaves old layouts untouched).
pub(super) fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut present = false;
    for name in stmt.query_map([], |row| row.get::<_, String>(1))? {
        if name? == column {
            present = true;
            break;
        }
    }
    drop(stmt);
    if !present {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn user_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn table_exists(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}
