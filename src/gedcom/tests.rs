//! Unit tests for the GEDCOM 5.5.1 import and export engine.

use std::io::{BufReader, Cursor};

use rusqlite::{Connection, params};

use super::import::parse_name;
use super::{GedcomError, export_to_gedcom, import_gedcom};
use crate::db::init_db;

const SAMPLE: &str = "\
0 HEAD
1 SOUR TEST
1 GEDC
2 VERS 5.5.1
1 CHAR UTF-8
0 @I1@ INDI
1 NAME Johan /Ahlberg/
1 SEX M
1 BIRT
2 DATE 1 JAN 1900
2 PLAC Ornskoldsvik, Sweden
1 DEAT
2 DATE 2 FEB 1970
2 PLAC Stockholm, Sweden
1 FAMS @F1@
0 @I2@ INDI
1 NAME Maria /Ekdahl/
1 SEX F
1 FAMS @F1@
0 @I3@ INDI
1 NAME Elsa /Ahlberg/
1 SEX F
1 BIRT
2 DATE 3 MAR 1930
1 FAMC @F1@
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I2@
1 CHIL @I3@
1 MARR
2 DATE 4 APR 1925
2 PLAC Umea, Sweden
0 TRLR
";

/// A family whose FAM record carries no pointers at all: every link has to
/// be recovered from the individuals' FAMC/FAMS back-references.
const BACKFILL: &str = "\
0 @I1@ INDI
1 NAME Pa /Back/
1 SEX M
1 FAMS @F1@
0 @I2@ INDI
1 NAME Wa /Back/
1 SEX F
1 FAMS @F1@
0 @I3@ INDI
1 NAME Ch /Back/
1 SEX U
1 FAMC @F1@
0 @F1@ FAM
0 TRLR
";

const BROKEN: &str = "\
0 HEAD
0 @I1@ INDI
1 NAME Fine /Person/
1 SEX M
1 FAMS @F1@
0 @F1@ FAM
1 HUSB @I99@
1 CHIL @I42@
totally broken line here
0 TRLR
";

const BOMB: &str = "\
0 @I1@ INDI
1 NAME Good /Person/
0 @I2@ INDI
1 NAME Bomb /Person/
0 TRLR
";

const DANGLING_SOUR: &str = "\
0 HEAD
0 @I1@ INDI
1 NAME Fine /Person/
1 BIRT
2 DATE 1 JAN 1900
2 SOUR @S9@
0 TRLR
";

fn expected_header() -> String {
    format!(
        "0 HEAD\n1 SOUR STEMMA_FLOW\n2 VERS {}\n1 GEDC\n2 VERS 5.5.1\n2 FORM LINEAGE-LINKED\n1 CHAR UTF-8\n",
        env!("CARGO_PKG_VERSION")
    )
}

fn setup_test_db() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    init_db(&conn)?;
    Ok(conn)
}

fn scalar_count(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [], |row| row.get(0))
}

#[test]
fn import_populates_all_tables_with_uuid_keys() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(SAMPLE))?;

    assert_eq!(
        (
            report.people,
            report.families,
            report.child_links,
            report.events
        ),
        (3, 1, 1, 4),
        "import counters must match the sample"
    );
    assert!(
        report.warnings.is_empty(),
        "clean sample must import without warnings: {:?}",
        report.warnings
    );

    let joined: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM family_children fc
         JOIN people child ON child.id = fc.child_id
         JOIN families f ON f.id = fc.family_id
         JOIN people husb ON husb.id = f.husband_id
         JOIN people wife ON wife.id = f.wife_id",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        joined, 1,
        "junction row must join to the child, family and both spouses"
    );

    let leftover_xrefs: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM people WHERE id LIKE '@%' OR id = '')
               + (SELECT COUNT(*) FROM families WHERE id LIKE '@%' OR id = '')",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        leftover_xrefs, 0,
        "stored keys must be UUIDs, not raw GEDCOM xrefs"
    );

    let orphan_events: i64 = conn.query_row(
        "SELECT COUNT(*) FROM events e
         LEFT JOIN people p ON p.id = e.person_id
         LEFT JOIN families f ON f.id = e.family_id
         WHERE (e.person_id IS NOT NULL AND p.id IS NULL)
            OR (e.family_id IS NOT NULL AND f.id IS NULL)",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        orphan_events, 0,
        "every event must keep a valid foreign key"
    );
    Ok(())
}

#[test]
fn import_translates_external_pointers_to_uuids() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    import_gedcom(&mut conn, Cursor::new(SAMPLE))?;

    let (husband, wife): (String, String) = conn.query_row(
        "SELECT h.given_name, w.given_name
         FROM families f
         JOIN people h ON h.id = f.husband_id
         JOIN people w ON w.id = f.wife_id",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(husband, "Johan", "HUSB @I1@ must resolve to the husband");
    assert_eq!(wife, "Maria", "WIFE @I2@ must resolve to the wife");

    let child: String = conn.query_row(
        "SELECT p.given_name
         FROM family_children fc
         JOIN people p ON p.id = fc.child_id",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(child, "Elsa", "CHIL @I3@ must resolve to the child");

    let person_links: i64 = scalar_count(
        &conn,
        "SELECT COUNT(*) FROM people WHERE id LIKE '@I%' OR id LIKE '@F%'",
    )?;
    assert_eq!(person_links, 0, "no GEDCOM pointer text may survive in ids");
    Ok(())
}

#[test]
fn import_backfills_famc_and_fams_pointers() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(BACKFILL))?;

    assert_eq!(
        (
            report.people,
            report.families,
            report.child_links,
            report.events
        ),
        (3, 1, 1, 0),
        "FAMC must backfill the missing CHIL link"
    );
    assert!(
        report.warnings.is_empty(),
        "backfill sample must import without warnings: {:?}",
        report.warnings
    );

    let (husband, wife): (String, String) = conn.query_row(
        "SELECT h.given_name, w.given_name
         FROM families f
         JOIN people h ON h.id = f.husband_id
         JOIN people w ON w.id = f.wife_id",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(
        husband, "Pa",
        "FAMS must fill the empty husband slot by SEX"
    );
    assert_eq!(wife, "Wa", "FAMS must fill the empty wife slot by SEX");
    Ok(())
}

#[test]
fn import_reports_dangling_and_malformed_lines() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(BROKEN))?;

    assert_eq!(
        (report.people, report.families, report.child_links),
        (1, 1, 0),
        "valid records must still import alongside warnings"
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("dangling HUSB pointer @I99@")),
        "missing HUSB target must be reported: {:?}",
        report.warnings
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("dangling CHIL pointer @I42@")),
        "missing CHIL target must be reported: {:?}",
        report.warnings
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("malformed line")),
        "broken line must be reported: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn failed_import_rolls_back_all_writes() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    conn.execute_batch(
        "CREATE TRIGGER bomb BEFORE INSERT ON people
         WHEN NEW.given_name = 'Bomb'
         BEGIN SELECT RAISE(ABORT, 'boom'); END;",
    )
    .map_err(GedcomError::Db)?;

    let outcome = import_gedcom(&mut conn, Cursor::new(BOMB));
    match outcome {
        Err(GedcomError::Db(_)) => {}
        Err(other) => panic!("expected database error, got {other}"),
        Ok(_) => panic!("expected the import to fail"),
    }

    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM people").map_err(GedcomError::Db)?,
        0,
        "the row written before the failure must be rolled back too"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM families").map_err(GedcomError::Db)?,
        0,
        "no family may survive a failed import"
    );
    assert_eq!(
        scalar_count(&conn, "SELECT COUNT(*) FROM events").map_err(GedcomError::Db)?,
        0,
        "no event may survive a failed import"
    );
    Ok(())
}

#[test]
fn export_round_trips_imported_data() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    import_gedcom(&mut conn, Cursor::new(SAMPLE))?;
    let exported = export_to_gedcom(&conn)?;

    assert!(
        exported.starts_with(&expected_header()),
        "GEDCOM 5.5.1 header must come first: {exported:?}"
    );
    assert!(
        exported.ends_with("0 TRLR\n"),
        "document must end with the trailer"
    );
    for expected in [
        "0 @I1@ INDI",
        "1 NAME Johan /Ahlberg/",
        "1 SEX M",
        "2 DATE 1 JAN 1900",
        "2 PLAC Ornskoldsvik, Sweden",
        "1 FAMC @F1@",
        "1 FAMS @F1@",
        "0 @F1@ FAM",
        "1 HUSB @I1@",
        "1 WIFE @I2@",
        "1 CHIL @I3@",
        "1 MARR",
        "2 DATE 4 APR 1925",
    ] {
        assert!(
            exported.contains(expected),
            "export must contain {expected:?}"
        );
    }
    assert_eq!(
        export_to_gedcom(&conn)?,
        exported,
        "exporting twice must be deterministic"
    );

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(exported.clone()))?;
    assert_eq!(
        (
            report.people,
            report.families,
            report.child_links,
            report.events
        ),
        (3, 1, 1, 4),
        "re-import must reconstruct every record"
    );
    assert!(
        report.warnings.is_empty(),
        "exported document must re-import cleanly: {:?}",
        report.warnings
    );
    assert_eq!(
        export_to_gedcom(&fresh)?,
        exported,
        "import -> export -> import must preserve the document"
    );

    let child: (String, String) = fresh.query_row(
        "SELECT p.given_name, p.surname
         FROM family_children fc
         JOIN people p ON p.id = fc.child_id",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(
        child,
        ("Elsa".to_string(), "Ahlberg".to_string()),
        "relationships and names must survive the round trip"
    );
    Ok(())
}

#[test]
fn parse_name_handles_nonstandard_input() {
    assert_eq!(
        parse_name("Johan /Ahlberg/"),
        ("Johan".to_string(), "Ahlberg".to_string())
    );
    assert_eq!(
        parse_name("Johan"),
        ("Johan".to_string(), String::new()),
        "missing slashes leave the surname empty"
    );
    assert_eq!(
        parse_name("/Ahlberg/"),
        (String::new(), "Ahlberg".to_string()),
        "missing given name is tolerated"
    );
    assert_eq!(
        parse_name("  Trailing  /"),
        ("Trailing".to_string(), String::new()),
        "trailing slash with empty surname"
    );
    assert_eq!(
        parse_name("Anna Maria /von R/"),
        ("Anna Maria".to_string(), "von R".to_string())
    );
    assert_eq!(parse_name(""), (String::new(), String::new()));
}

#[test]
fn export_emits_gedcom_551_header() -> Result<(), GedcomError> {
    let conn = setup_test_db().map_err(GedcomError::Db)?;
    let exported = export_to_gedcom(&conn)?;

    let expected = format!("{}0 TRLR\n", expected_header());
    assert_eq!(
        exported, expected,
        "an empty database must export as HEAD + TRLR only"
    );
    Ok(())
}

#[test]
fn citations_round_trip_through_sour_records() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    import_gedcom(&mut conn, Cursor::new(SAMPLE))?;

    let birth_id: String = conn
        .query_row(
            "SELECT id FROM events WHERE event_type = 'BIRTH'",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    conn.execute(
        "INSERT INTO citations (id, source_title, page_or_reference, notes, event_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "cit-cited",
            "Parish Church Book",
            "12",
            "Abbot 1900",
            birth_id
        ],
    )
    .map_err(GedcomError::Db)?;
    conn.execute(
        "INSERT INTO citations (id, source_title, page_or_reference, notes, event_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "cit-standalone",
            "Municipal Archive",
            "roll 7",
            None::<String>,
            None::<String>
        ],
    )
    .map_err(GedcomError::Db)?;

    let exported = export_to_gedcom(&conn)?;
    for expected in [
        "0 @S1@ SOUR",
        "1 TITL Parish Church Book",
        "1 NOTE Abbot 1900",
        "2 SOUR @S1@",
        "3 PAGE 12",
        "0 @S2@ SOUR",
        "1 TITL Municipal Archive",
    ] {
        assert!(
            exported.contains(expected),
            "export must contain {expected:?}: {exported:?}"
        );
    }

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(exported.clone()))?;
    assert_eq!(report.citations, 2, "both SOUR records must be restored");
    assert!(
        report.warnings.is_empty(),
        "citation export must re-import cleanly: {:?}",
        report.warnings
    );

    let cited: (String, Option<String>, Option<String>) = fresh
        .query_row(
            "SELECT c.source_title, c.page_or_reference, c.notes
             FROM citations c
             JOIN events e ON e.id = c.event_id
             WHERE e.event_type = 'BIRTH'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(
        cited,
        (
            "Parish Church Book".to_string(),
            Some("12".to_string()),
            Some("Abbot 1900".to_string())
        ),
        "citation fields must survive the round trip"
    );
    let standalone: i64 = scalar_count(
        &fresh,
        "SELECT COUNT(*) FROM citations WHERE event_id IS NULL",
    )
    .map_err(GedcomError::Db)?;
    assert_eq!(standalone, 1, "the uncited SOUR must stay standalone");

    assert_eq!(
        export_to_gedcom(&fresh)?,
        exported,
        "import -> export must preserve the document byte for byte"
    );
    Ok(())
}

#[test]
fn import_warns_on_dangling_source_pointer() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(DANGLING_SOUR))?;

    assert_eq!(report.citations, 0, "no citation may be fabricated");
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("dangling SOUR pointer @S9@")),
        "missing SOUR target must be reported: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_streams_from_a_file_handle() -> Result<(), GedcomError> {
    let path = std::env::temp_dir().join(format!("stemma-flow-{}.ged", uuid::Uuid::new_v4()));
    std::fs::write(&path, SAMPLE).map_err(GedcomError::Io)?;

    let file = std::fs::File::open(&path).map_err(GedcomError::Io)?;
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, BufReader::new(file))?;

    let _ = std::fs::remove_file(&path);
    assert_eq!(
        (
            report.people,
            report.families,
            report.child_links,
            report.events
        ),
        (3, 1, 1, 4),
        "file-based streaming import must match the in-memory stream"
    );
    Ok(())
}
