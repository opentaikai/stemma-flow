//! Unit tests for the SQLite storage layer.
//!
//! Every test gets its own isolated in-memory database via
//! [`setup_test_db`].

use std::collections::HashSet;

use rusqlite::{Connection, params};

use super::{Citation, Event, Family, Person, init_db, open_connection};

const TABLES: [&str; 16] = [
    "people",
    "families",
    "family_children",
    "repositories",
    "sources",
    "source_fields",
    "places",
    "fact_types",
    "events",
    "citations",
    "media",
    "media_links",
    "tasks",
    "task_links",
    "groups",
    "group_members",
];
const INDEXES: [&str; 29] = [
    "idx_families_husband",
    "idx_families_wife",
    "idx_family_children_child",
    "idx_events_person",
    "idx_events_family",
    "idx_events_place",
    "idx_citations_source",
    "idx_citations_event",
    "idx_people_names",
    "idx_places_name",
    "idx_sources_title",
    "idx_sources_repository",
    "idx_source_fields_source",
    "idx_media_links_media",
    "idx_media_links_person",
    "idx_media_links_family",
    "idx_media_links_event",
    "idx_media_links_source",
    "idx_media_links_citation",
    "idx_media_links_place",
    "idx_task_links_task",
    "idx_task_links_person",
    "idx_task_links_family",
    "idx_task_links_event",
    "idx_task_links_source",
    "idx_task_links_citation",
    "idx_task_links_place",
    "idx_task_links_media",
    "idx_group_members_person",
];

struct Tree {
    husband: Person,
    wife: Person,
    child: Person,
    family: Family,
}

/// A fresh in-memory database with foreign keys enabled and the schema
/// applied.
fn setup_test_db() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    init_db(&conn)?;
    Ok(conn)
}

fn table_count(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )
}

fn index_count(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [name],
        |row| row.get(0),
    )
}

fn scalar_count(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [], |row| row.get(0))
}

fn count_matching(conn: &Connection, sql: &str, param: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [param], |row| row.get(0))
}

fn insert_person(conn: &Connection, person: &Person) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO people (id, given_name, surname, gender) VALUES (?1, ?2, ?3, ?4)",
        params![person.id, person.given_name, person.surname, person.gender],
    )?;
    Ok(())
}

fn insert_family(conn: &Connection, family: &Family) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO families (id, husband_id, wife_id) VALUES (?1, ?2, ?3)",
        params![family.id, family.husband_id, family.wife_id],
    )?;
    Ok(())
}

fn link_child(conn: &Connection, family_id: &str, child_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO family_children (family_id, child_id) VALUES (?1, ?2)",
        params![family_id, child_id],
    )?;
    Ok(())
}

/// Husband, wife and one child joined through a family and its junction row.
fn family_tree(conn: &Connection) -> rusqlite::Result<Tree> {
    let husband = Person::new("Johan", "Ahlberg", "M");
    let wife = Person::new("Maria", "Ekdahl", "F");
    let child = Person::new("Elsa", "Ahlberg", "F");
    insert_person(conn, &husband)?;
    insert_person(conn, &wife)?;
    insert_person(conn, &child)?;

    let family = Family::new(Some(husband.id.clone()), Some(wife.id.clone()));
    insert_family(conn, &family)?;
    link_child(conn, &family.id, &child.id)?;

    Ok(Tree {
        husband,
        wife,
        child,
        family,
    })
}

#[test]
fn test_schema_initialization() -> rusqlite::Result<()> {
    let conn = Connection::open_in_memory()?;
    init_db(&conn)?;

    for table in TABLES {
        assert_eq!(table_count(&conn, table)?, 1, "missing table {table}");
    }
    for index in INDEXES {
        assert_eq!(index_count(&conn, index)?, 1, "missing index {index}");
    }

    init_db(&conn)?;
    for table in TABLES {
        assert_eq!(
            table_count(&conn, table)?,
            1,
            "re-running init_db must not duplicate {table}"
        );
    }
    Ok(())
}

#[test]
fn test_foreign_keys_and_pragmas() -> rusqlite::Result<()> {
    let conn = setup_test_db()?;

    let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    assert_eq!(fk, 1, "foreign key enforcement must be enabled");

    let kid = Person::new("Kid", "Fksample", "U");
    insert_person(&conn, &kid)?;
    conn.execute("INSERT INTO families (id) VALUES ('fam-1')", [])?;

    let orphan_family = link_child(&conn, "missing-family", &kid.id);
    assert!(
        orphan_family.is_err(),
        "Expected foreign key constraint violation for unknown family_id"
    );
    let orphan_child = link_child(&conn, "fam-1", "missing-child");
    assert!(
        orphan_child.is_err(),
        "Expected foreign key constraint violation for unknown child_id"
    );
    link_child(&conn, "fam-1", &kid.id)?;

    // The bundled SQLite build compiles with SQLITE_DEFAULT_FOREIGN_KEYS=1,
    // so the pragma must be switched off to model a connection that never
    // enabled it: only then is an orphaned row accepted.
    let lenient = Connection::open_in_memory()?;
    lenient.pragma_update(None, "foreign_keys", "OFF")?;
    init_db(&lenient)?;
    let fk_off: i64 = lenient.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    assert_eq!(fk_off, 0, "the foreign_keys pragma must be switchable off");
    let orphan = lenient.execute(
        "INSERT INTO family_children (family_id, child_id) VALUES ('ghost-family', 'ghost-child')",
        [],
    );
    assert!(
        orphan.is_ok(),
        "Expected orphan row to be accepted while foreign_keys is OFF"
    );
    Ok(())
}

#[test]
fn test_create_person_and_parent_child_relationship() -> rusqlite::Result<()> {
    let conn = setup_test_db()?;
    let tree = family_tree(&conn)?;

    let (family_id, husband_id, wife_id): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT f.id, f.husband_id, f.wife_id
             FROM family_children fc
             JOIN families f ON f.id = fc.family_id
             WHERE fc.child_id = ?1",
            [&tree.child.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    assert_eq!(
        family_id, tree.family.id,
        "child must reference the family unit"
    );
    assert_eq!(
        husband_id.as_deref(),
        Some(tree.husband.id.as_str()),
        "family must reference the father"
    );
    assert_eq!(
        wife_id.as_deref(),
        Some(tree.wife.id.as_str()),
        "family must reference the mother"
    );

    let mut parents = HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT p.id
         FROM family_children fc
         JOIN families f ON f.id = fc.family_id
         JOIN people p ON p.id = f.husband_id OR p.id = f.wife_id
         WHERE fc.child_id = ?1",
    )?;
    let rows = stmt.query_map([&tree.child.id], |row| row.get::<_, String>(0))?;
    for id in rows {
        parents.insert(id?);
    }

    assert_eq!(parents.len(), 2, "child must resolve exactly two parents");
    assert!(
        parents.contains(&tree.husband.id),
        "child must reference the father"
    );
    assert!(
        parents.contains(&tree.wife.id),
        "child must reference the mother"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM family_children")?,
        1,
        "exactly one junction row expected"
    );
    Ok(())
}

#[test]
fn test_cascading_deletes() -> rusqlite::Result<()> {
    // Person deletion cascades to events, citations and child links.
    {
        let conn = setup_test_db()?;
        let tree = family_tree(&conn)?;

        let birth = Event::new("BIRTH", Some(tree.child.id.clone()), None);
        conn.execute(
            "INSERT INTO events (id, event_type, person_id) VALUES (?1, ?2, ?3)",
            params![birth.id, birth.event_type, birth.person_id],
        )?;
        let death = Event::new("DEATH", Some(tree.child.id.clone()), None);
        conn.execute(
            "INSERT INTO events (id, event_type, person_id) VALUES (?1, ?2, ?3)",
            params![death.id, death.event_type, death.person_id],
        )?;
        conn.execute(
            "INSERT INTO sources (id, title) VALUES ('source-1', 'Parish register, Ornskoldsvik')",
            [],
        )?;
        let mut citation = Citation::new("source-1");
        citation.event_id = Some(birth.id.clone());
        conn.execute(
            "INSERT INTO citations (id, source_id, event_id) VALUES (?1, ?2, ?3)",
            params![citation.id, citation.source_id, citation.event_id],
        )?;

        conn.execute("DELETE FROM people WHERE id = ?1", [&tree.child.id])?;

        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM events")?,
            0,
            "person delete must cascade to their events"
        );
        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM citations")?,
            0,
            "event delete must cascade to citations"
        );
        assert_eq!(
            count_matching(
                &conn,
                "SELECT COUNT(*) FROM family_children WHERE child_id = ?1",
                &tree.child.id,
            )?,
            0,
            "person delete must purge their child links"
        );
        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM people")?,
            2,
            "the parents must survive their child's deletion"
        );
    }

    // Family deletion purges junction rows without touching people.
    {
        let conn = setup_test_db()?;
        let tree = family_tree(&conn)?;

        let marriage = Event::new("MARRIAGE", None, Some(tree.family.id.clone()));
        conn.execute(
            "INSERT INTO events (id, event_type, family_id) VALUES (?1, ?2, ?3)",
            params![marriage.id, marriage.event_type, marriage.family_id],
        )?;

        conn.execute("DELETE FROM families WHERE id = ?1", [&tree.family.id])?;

        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM family_children")?,
            0,
            "family delete must purge all child links"
        );
        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM events")?,
            0,
            "family delete must cascade to family events"
        );
        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM people")?,
            3,
            "people must survive their family's deletion"
        );
    }

    // Spouses are detached with SET NULL, children stay linked.
    {
        let conn = setup_test_db()?;
        let tree = family_tree(&conn)?;

        conn.execute("DELETE FROM people WHERE id = ?1", [&tree.husband.id])?;
        let (husband_id, wife_id): (Option<String>, Option<String>) = conn.query_row(
            "SELECT husband_id, wife_id FROM families WHERE id = ?1",
            [&tree.family.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert!(
            husband_id.is_none(),
            "husband_id must become NULL after deleting the husband"
        );
        assert_eq!(
            wife_id.as_deref(),
            Some(tree.wife.id.as_str()),
            "wife_id must survive the husband's deletion"
        );

        conn.execute("DELETE FROM people WHERE id = ?1", [&tree.wife.id])?;
        let wife_id: Option<String> = conn.query_row(
            "SELECT wife_id FROM families WHERE id = ?1",
            [&tree.family.id],
            |row| row.get(0),
        )?;
        assert!(
            wife_id.is_none(),
            "wife_id must become NULL after deleting the wife"
        );
        assert_eq!(
            scalar_count(&conn, "SELECT COUNT(*) FROM family_children")?,
            1,
            "the child link must outlive both spouses"
        );
    }
    Ok(())
}

#[test]
fn test_transactional_updates() -> rusqlite::Result<()> {
    // Commit: parents, family link and birth event persist together.
    let mut conn = setup_test_db()?;
    let tx = conn.transaction()?;

    let father = Person::new("Sven", "Tranberg", "M");
    let mother = Person::new("Alva", "Tranberg", "F");
    let child = Person::new("Nils", "Tranberg", "M");
    insert_person(&tx, &father)?;
    insert_person(&tx, &mother)?;
    insert_person(&tx, &child)?;

    let family = Family::new(Some(father.id.clone()), Some(mother.id.clone()));
    insert_family(&tx, &family)?;
    link_child(&tx, &family.id, &child.id)?;

    let birth = Event::new("BIRTH", Some(child.id.clone()), None);
    tx.execute(
        "INSERT INTO events (id, event_type, person_id) VALUES (?1, ?2, ?3)",
        params![birth.id, birth.event_type, birth.person_id],
    )?;
    tx.commit()?;

    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM people")?,
        3,
        "committed person and both parents must persist"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM families")?,
        1,
        "committed family must persist"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM family_children")?,
        1,
        "committed child link must persist"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM events")?,
        1,
        "committed birth event must persist"
    );
    let stored_person: Option<String> = conn.query_row(
        "SELECT person_id FROM events WHERE id = ?1",
        [&birth.id],
        |row| row.get(0),
    )?;
    assert_eq!(
        stored_person.as_deref(),
        Some(child.id.as_str()),
        "birth event must stay linked to the person after commit"
    );

    // Rollback: a failing write mid-transaction leaves no partial data.
    let mut conn = setup_test_db()?;
    let tx = conn.transaction()?;

    let father = Person::new("Sven", "Tranberg", "M");
    let child = Person::new("Nils", "Tranberg", "M");
    insert_person(&tx, &father)?;
    insert_person(&tx, &child)?;
    let family = Family::new(Some(father.id.clone()), None);
    insert_family(&tx, &family)?;
    link_child(&tx, &family.id, &child.id)?;
    let birth = Event::new("BIRTH", Some(child.id.clone()), None);
    tx.execute(
        "INSERT INTO events (id, event_type, person_id) VALUES (?1, ?2, ?3)",
        params![birth.id, birth.event_type, birth.person_id],
    )?;

    let duplicate = insert_person(&tx, &child);
    assert!(
        duplicate.is_err(),
        "Expected primary key violation to simulate a failed step"
    );
    tx.rollback()?;

    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM people")?,
        0,
        "rollback must not persist people"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM families")?,
        0,
        "rollback must not persist families"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM family_children")?,
        0,
        "rollback must not persist child links"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM events")?,
        0,
        "rollback must not persist events"
    );
    Ok(())
}

#[test]
fn test_gender_constraint_and_timestamp_defaults() -> rusqlite::Result<()> {
    let conn = setup_test_db()?;

    let invalid = Person::new("Unknown", "Gender", "X");
    let rejected = insert_person(&conn, &invalid);
    assert!(
        rejected.is_err(),
        "gender outside 'M'/'F'/'U' must violate the CHECK constraint"
    );

    let person = Person::new("Lars", "Ohlin", "M");
    insert_person(&conn, &person)?;
    let (created_at, updated_at): (String, String) = conn.query_row(
        "SELECT created_at, updated_at FROM people WHERE id = ?1",
        [&person.id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert!(!created_at.is_empty(), "created_at must default to now");
    assert!(!updated_at.is_empty(), "updated_at must default to now");
    Ok(())
}

#[test]
fn open_connection_enables_foreign_keys_and_wal() -> rusqlite::Result<()> {
    let path = std::env::temp_dir().join(format!("stemma-flow-{}.db", uuid::Uuid::new_v4()));
    {
        let conn = open_connection(&path)?;

        let fk: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        assert_eq!(fk, 1, "open_connection must enable foreign keys");

        let journal: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        assert_eq!(journal, "wal", "open_connection must enable WAL mode");
    }

    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
    }
    Ok(())
}

#[test]
fn load_family_tree_returns_rows_in_order() -> rusqlite::Result<()> {
    let conn = setup_test_db()?;

    let empty = super::load_family_tree(&conn)?;
    assert!(
        empty.people.is_empty() && empty.families.is_empty() && empty.child_links.is_empty(),
        "an empty database loads as an empty tree"
    );

    let fixture = family_tree(&conn)?;
    let loaded = super::load_family_tree(&conn)?;

    let ids: Vec<&str> = loaded
        .people
        .iter()
        .map(|person| person.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            fixture.husband.id.as_str(),
            fixture.wife.id.as_str(),
            fixture.child.id.as_str()
        ],
        "people must load in insertion order"
    );

    assert_eq!(loaded.families.len(), 1);
    let Some(family) = loaded.families.first() else {
        panic!("family row must be present");
    };
    assert_eq!(family.id, fixture.family.id);
    assert_eq!(
        family.husband_id.as_deref(),
        Some(fixture.husband.id.as_str())
    );
    assert_eq!(family.wife_id.as_deref(), Some(fixture.wife.id.as_str()));
    assert_eq!(
        loaded.child_links,
        vec![(fixture.family.id.clone(), fixture.child.id.clone())]
    );
    Ok(())
}

#[test]
fn init_db_upgrades_a_legacy_people_table() -> rusqlite::Result<()> {
    // Pre-middle-name layout: CREATE TABLE IF NOT EXISTS would keep it as is.
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(
        "CREATE TABLE people (
            id TEXT PRIMARY KEY,
            given_name TEXT NOT NULL,
            surname TEXT NOT NULL,
            gender TEXT NOT NULL CHECK (gender IN ('M', 'F', 'U')),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    init_db(&conn)?;

    conn.execute(
        "INSERT INTO people (id, given_name, middle_name, surname, nickname, gender, notes)
         VALUES ('p1', 'Ivan', 'Petrovich', 'Ivanov', 'Vanya', 'M', 'Lived in Tver')",
        [],
    )?;
    let person: (String, String) = conn.query_row(
        "SELECT middle_name || '/' || nickname, notes FROM people WHERE id = 'p1'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(
        person,
        ("Petrovich/Vanya".to_string(), "Lived in Tver".to_string())
    );
    Ok(())
}

#[test]
fn load_marriage_details_reads_the_first_marriage_per_family() -> rusqlite::Result<()> {
    let conn = setup_test_db()?;
    let fixture = family_tree(&conn)?;

    conn.execute(
        "INSERT INTO places (id, name) VALUES ('place-uppsala', 'Uppsala')",
        [],
    )?;
    let mut first = Event::new("MARRIAGE", None, Some(fixture.family.id.clone()));
    first.date = Some("1861-06-12".to_string());
    first.place_id = Some("place-uppsala".to_string());
    conn.execute(
        "INSERT INTO events (id, event_type, date, place_id, family_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            first.id,
            first.event_type,
            first.date,
            first.place_id,
            first.family_id
        ],
    )?;
    let second = Event::new("MARRIAGE", None, Some(fixture.family.id.clone()));
    conn.execute(
        "INSERT INTO events (id, event_type, family_id) VALUES (?1, ?2, ?3)",
        params![second.id, second.event_type, second.family_id],
    )?;
    let unrelated = Event::new("MARRIAGE", Some(fixture.child.id.clone()), None);
    conn.execute(
        "INSERT INTO events (id, event_type, person_id) VALUES (?1, ?2, ?3)",
        params![unrelated.id, unrelated.event_type, unrelated.person_id],
    )?;

    let marriages = super::load_marriage_details(&conn)?;
    assert_eq!(marriages.len(), 1, "only family-scoped marriages count");
    let info = marriages
        .get(&fixture.family.id)
        .expect("the family has a marriage");
    assert_eq!(info.date.as_deref(), Some("1861-06-12"), "first event wins");
    assert_eq!(info.place.as_deref(), Some("Uppsala"));
    Ok(())
}

const V1_SCHEMA: &str = r#"
CREATE TABLE people (
    id TEXT PRIMARY KEY,
    given_name TEXT NOT NULL,
    surname TEXT NOT NULL,
    gender TEXT NOT NULL CHECK (gender IN ('M', 'F', 'U')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE families (
    id TEXT PRIMARY KEY,
    husband_id TEXT REFERENCES people(id) ON DELETE SET NULL,
    wife_id TEXT REFERENCES people(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE family_children (
    family_id TEXT NOT NULL REFERENCES families(id) ON DELETE CASCADE,
    child_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    PRIMARY KEY (family_id, child_id)
);
CREATE TABLE events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    date TEXT,
    place TEXT,
    description TEXT,
    person_id TEXT REFERENCES people(id) ON DELETE CASCADE,
    family_id TEXT REFERENCES families(id) ON DELETE CASCADE
);
CREATE TABLE citations (
    id TEXT PRIMARY KEY,
    source_title TEXT NOT NULL,
    page_or_reference TEXT,
    notes TEXT,
    event_id TEXT REFERENCES events(id) ON DELETE CASCADE
);
"#;

fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut found = false;
    for name in stmt.query_map([], |row| row.get::<_, String>(1))? {
        if name? == column {
            found = true;
            break;
        }
    }
    Ok(found)
}

#[test]
fn new_database_is_schema_v2() -> rusqlite::Result<()> {
    let conn = setup_test_db()?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    assert_eq!(version, 2);
    assert!(has_column(&conn, "citations", "source_id")?);
    assert!(!has_column(&conn, "citations", "source_title")?);
    assert!(has_column(&conn, "events", "place_id")?);
    assert!(!has_column(&conn, "events", "place")?);
    assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM fact_types")?, 3);
    Ok(())
}

#[test]
fn init_db_migrates_a_v1_database() -> rusqlite::Result<()> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(V1_SCHEMA)?;
    conn.execute_batch(
        "INSERT INTO people (id, given_name, surname, gender) VALUES
            ('p1', 'Ada', 'King', 'F'),
            ('p2', 'Otto', 'King', 'M');
         INSERT INTO families (id, husband_id, wife_id) VALUES ('f1', 'p2', 'p1');
         INSERT INTO events (id, event_type, date, place, person_id) VALUES
            ('e1', 'BIRTH', '1815', 'Uppsala', 'p1'),
            ('e2', 'DEATH', NULL, 'Uppsala', 'p1');
         INSERT INTO citations (id, source_title, page_or_reference, notes, event_id) VALUES
            ('c1', 'Book A', '12', 'Note A', 'e1'),
            ('c2', 'Book A', '13', 'Note A', 'e2'),
            ('c3', 'Book B', NULL, 'Note B', 'e1');",
    )?;

    init_db(&conn)?;

    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    assert_eq!(version, 2);
    assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM people")?, 2);
    assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM sources")?, 2);
    assert_eq!(scalar_count(&conn, "SELECT COUNT(*) FROM places")?, 1);
    assert_eq!(
        scalar_count(
            &conn,
            "SELECT COUNT(*) FROM events WHERE place_id IS NOT NULL"
        )?,
        2,
        "both events keep their place"
    );
    assert_eq!(
        scalar_count(
            &conn,
            "SELECT COUNT(*) FROM citations WHERE source_id IS NOT NULL"
        )?,
        3,
        "every citation points at a source"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM fact_types")?,
        3,
        "built-in fact types are seeded"
    );
    let (title, notes): (String, Option<String>) = conn.query_row(
        "SELECT s.title, s.notes
         FROM citations c JOIN sources s ON s.id = c.source_id
         WHERE c.id = 'c1'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(title, "Book A");
    assert_eq!(notes.as_deref(), Some("Note A"));
    assert!(!has_column(&conn, "citations", "source_title")?);
    assert!(!has_column(&conn, "events", "place")?);

    init_db(&conn)?;
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM sources")?,
        2,
        "re-running the migration must not duplicate sources"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM places")?,
        1,
        "re-running the migration must not duplicate places"
    );
    Ok(())
}

#[test]
fn init_db_rejects_a_newer_schema() -> rusqlite::Result<()> {
    let conn = setup_test_db()?;
    conn.pragma_update(None, "user_version", 99)?;
    assert!(init_db(&conn).is_err(), "a newer schema must be rejected");
    Ok(())
}

#[test]
fn migrates_the_local_family_tree_db_when_present() -> rusqlite::Result<()> {
    let source = std::path::Path::new("test_data/family_tree.db");
    if !source.exists() {
        return Ok(());
    }
    let temp =
        std::env::temp_dir().join(format!("stemma-flow-migrate-{}.db", uuid::Uuid::new_v4()));
    std::fs::copy(source, &temp).expect("fixture copy");
    for suffix in ["-wal", "-shm"] {
        let from = std::path::PathBuf::from(format!("{}{suffix}", source.display()));
        if from.exists() {
            let to = std::path::PathBuf::from(format!("{}{suffix}", temp.display()));
            std::fs::copy(&from, &to).expect("sidecar copy");
        }
    }

    {
        let conn = open_connection(&temp)?;
        let before: i64 = conn.query_row("SELECT COUNT(*) FROM people", [], |row| row.get(0))?;
        assert!(before > 0, "the fixture must contain people");
        init_db(&conn)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        assert_eq!(version, 2);
        let after: i64 = conn.query_row("SELECT COUNT(*) FROM people", [], |row| row.get(0))?;
        assert_eq!(after, before, "migration preserves every person");
    }

    let _ = std::fs::remove_file(&temp);
    for suffix in ["-wal", "-shm"] {
        let path = std::path::PathBuf::from(format!("{}{suffix}", temp.display()));
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}
