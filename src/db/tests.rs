//! Unit tests for the SQLite storage layer.
//!
//! Every test gets its own isolated in-memory database via
//! [`setup_test_db`].

use std::collections::HashSet;

use rusqlite::{Connection, params};

use super::{Citation, Event, Family, Person, init_db, open_connection};

const TABLES: [&str; 5] = [
    "people",
    "families",
    "family_children",
    "events",
    "citations",
];
const INDEXES: [&str; 7] = [
    "idx_families_husband",
    "idx_families_wife",
    "idx_family_children_child",
    "idx_events_person",
    "idx_events_family",
    "idx_citations_event",
    "idx_people_names",
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
        let mut citation = Citation::new("Parish register, Ornskoldsvik");
        citation.event_id = Some(birth.id.clone());
        conn.execute(
            "INSERT INTO citations (id, source_title, event_id) VALUES (?1, ?2, ?3)",
            params![citation.id, citation.source_title, citation.event_id],
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
