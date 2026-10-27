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

fn seed_event(conn: &Connection, date: &str) -> Result<(), GedcomError> {
    conn.execute(
        "INSERT INTO people (id, given_name, surname, gender) VALUES ('p1', 'Ada', 'King', 'F')",
        [],
    )
    .map_err(GedcomError::Db)?;
    conn.execute(
        "INSERT INTO events (id, event_type, date, person_id)
         VALUES ('e1', 'BIRTH', ?1, 'p1')",
        params![date],
    )
    .map_err(GedcomError::Db)?;
    Ok(())
}

#[test]
fn export_canonicalises_stored_dates() -> Result<(), GedcomError> {
    let conn = setup_test_db().map_err(GedcomError::Db)?;
    seed_event(&conn, "12 jan 1850")?;
    let exported = export_to_gedcom(&conn)?;
    assert!(
        exported.contains("2 DATE 12 JAN 1850"),
        "stored dates are canonicalised on export: {exported}"
    );
    Ok(())
}

#[test]
fn export_wraps_unparseable_dates_as_phrases() -> Result<(), GedcomError> {
    let conn = setup_test_db().map_err(GedcomError::Db)?;
    seed_event(&conn, "circa whenever\nnext line")?;
    let exported = export_to_gedcom(&conn)?;
    assert!(
        exported.contains("2 DATE (circa whenever next line)"),
        "invalid dates become single-line phrases: {exported}"
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
        "INSERT INTO sources (id, title, notes)
         VALUES ('src-cited', 'Parish Church Book', 'Abbot 1900')",
        [],
    )
    .map_err(GedcomError::Db)?;
    conn.execute(
        "INSERT INTO citations (id, source_id, page_or_reference)
         VALUES ('cit-cited', 'src-cited', '12')",
        [],
    )
    .map_err(GedcomError::Db)?;
    conn.execute(
        "INSERT INTO citation_links (citation_id, event_id)
         VALUES ('cit-cited', ?1)",
        params![birth_id],
    )
    .map_err(GedcomError::Db)?;
    conn.execute(
        "INSERT INTO sources (id, title) VALUES ('src-standalone', 'Municipal Archive')",
        [],
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
    assert_eq!(report.citations, 1, "only a cited source yields a citation");
    assert!(
        report.warnings.is_empty(),
        "citation export must re-import cleanly: {:?}",
        report.warnings
    );

    let cited: (String, Option<String>, Option<String>) = fresh
        .query_row(
            "SELECT s.title, c.page_or_reference, s.notes
             FROM citations c
             JOIN sources s ON s.id = c.source_id
             JOIN citation_links l ON l.citation_id = c.id
             JOIN events e ON e.id = l.event_id
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
        "SELECT COUNT(*) FROM sources s WHERE s.title = 'Municipal Archive'
         AND NOT EXISTS (SELECT 1 FROM citations c WHERE c.source_id = s.id)",
    )
    .map_err(GedcomError::Db)?;
    assert_eq!(standalone, 1, "the uncited SOUR must stay a source");

    assert_eq!(
        export_to_gedcom(&fresh)?,
        exported,
        "import -> export must preserve the document byte for byte"
    );
    Ok(())
}

#[test]
fn shared_citations_export_to_every_event_and_import_separately() -> Result<(), GedcomError> {
    let conn = setup_test_db().map_err(GedcomError::Db)?;
    conn.execute_batch(
        "INSERT INTO people (id, given_name, surname, gender) VALUES
            ('p1', 'Ada', 'King', 'F'),
            ('p2', 'Otto', 'King', 'M');
         INSERT INTO families (id, husband_id, wife_id) VALUES ('f1', 'p2', 'p1');
         INSERT INTO events (id, event_type, person_id, family_id) VALUES
            ('e1', 'BIRTH', 'p1', NULL),
            ('e2', 'DEATH', 'p1', NULL),
            ('e3', 'MARRIAGE', NULL, 'f1');
         INSERT INTO sources (id, title) VALUES ('s1', 'Parish Book');
         INSERT INTO citations (id, source_id, page_or_reference)
            VALUES ('c1', 's1', '12');
         INSERT INTO citation_links (citation_id, event_id) VALUES
            ('c1', 'e1'), ('c1', 'e2'), ('c1', 'e3');",
    )
    .map_err(GedcomError::Db)?;

    let exported = export_to_gedcom(&conn)?;
    assert_eq!(
        exported.matches("2 SOUR @S1@").count(),
        3,
        "the shared citation appears under every linked event: {exported:?}"
    );
    assert_eq!(exported.matches("3 PAGE 12").count(), 3);

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(exported.clone()))?;
    assert_eq!(report.citations, 3, "imports keep citations separate");
    let citations: i64 = fresh
        .query_row("SELECT COUNT(*) FROM citations", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(citations, 3);
    let links: i64 = fresh
        .query_row("SELECT COUNT(*) FROM citation_links", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(links, 3);
    let event_types: i64 = fresh
        .query_row(
            "SELECT COUNT(DISTINCT e.event_type)
             FROM citation_links l JOIN events e ON e.id = l.event_id",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(event_types, 3, "birth, death and marriage keep a citation");
    let pages: i64 = fresh
        .query_row(
            "SELECT COUNT(*) FROM citations WHERE page_or_reference = '12'",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(pages, 3);
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

#[test]
fn import_reassembles_utf8_characters_split_across_conc() -> Result<(), GedcomError> {
    // WikiTree wraps at a fixed byte width: "Лит" + the lead byte of "в"
    // lands on line one, the continuation byte opens the `2 CONC` payload.
    let mut sample = Vec::new();
    sample.extend_from_slice(b"0 HEAD\n1 SOUR TEST\n1 CHAR UTF-8\n0 @I1@ INDI\n1 NAME ");
    sample.extend_from_slice(b"\xd0\x9b\xd0\xb8\xd1\x82\xd0");
    sample.extend_from_slice(
        b"\n2 CONC \xb2\xd0\xb8\xd1\x86\xd0\xb5\xd0\xb2\xd0\xb0\n1 SEX F\n0 TRLR\n",
    );

    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    let given: String = conn
        .query_row("SELECT given_name FROM people", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(report.people, 1);
    assert_eq!(given, "Литвицева");
    assert!(
        report.warnings.is_empty(),
        "reassembly must leave no warnings: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_joins_conc_and_cont_payloads_byte_wise() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @S1@ SOUR
1 NOTE ab
2 CONC cd
2 CONT ef
2 CONC gh
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    let note: String = conn
        .query_row("SELECT notes FROM sources", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(note, "abcd\nefgh");
    assert!(
        report.warnings.is_empty(),
        "joined continuations must not warn: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_tolerates_crlf_line_endings() -> Result<(), GedcomError> {
    let sample =
        "0 HEAD\r\n1 CHAR UTF-8\r\n0 @I1@ INDI\r\n1 NAME Jan /Smit/\r\n1 SEX M\r\n0 TRLR\r\n";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.people, 1);
    assert!(
        report.warnings.is_empty(),
        "CRLF endings must parse cleanly: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_skips_utf8_byte_order_mark() -> Result<(), GedcomError> {
    let mut sample = Vec::from(&b"\xef\xbb\xbf"[..]);
    sample.extend_from_slice(b"0 HEAD\n0 @I1@ INDI\n1 NAME Ann /Lee/\n0 TRLR\n");

    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.people, 1);
    assert!(
        report.warnings.is_empty(),
        "a leading BOM must not break the first line: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn imports_the_local_test_data_sample_when_present() -> Result<(), GedcomError> {
    // Private WikiTree export in gitignored `test_data/`; skipped in CI.
    let Ok(entries) = std::fs::read_dir("test_data") else {
        return Ok(());
    };
    let sample = entries
        .flatten()
        .find(|entry| entry.path().extension().is_some_and(|ext| ext == "ged"));
    let Some(sample) = sample else {
        return Ok(());
    };
    let file = std::fs::File::open(sample.path()).map_err(GedcomError::Io)?;
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, BufReader::new(file))?;

    assert!(
        report.people > 0 && report.families > 0,
        "the real export must import entities: {:?}",
        report
    );
    let patronymics: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM people WHERE middle_name <> ''",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert!(
        patronymics > 0,
        "WikiTree _MIDN patronymics must land in middle_name"
    );
    let noted: i64 = conn
        .query_row("SELECT COUNT(*) FROM people WHERE notes <> ''", [], |row| {
            row.get(0)
        })
        .map_err(GedcomError::Db)?;
    assert!(noted > 0, "INDI notes must land in people.notes");
    assert_eq!(
        report.warnings.len(),
        2324,
        "every unmapped tag must be listed for the report window"
    );
    Ok(())
}

#[test]
fn import_maps_name_children_into_person_fields() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
1 CHAR UTF-8
0 @I1@ INDI
1 NAME Михаил Андреевич /Соловьев/
2 GIVN Михаил
2 _MIDN Андреевич
2 SURN Соловьев
2 _PGVN Миша
1 SEX M
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    let (given, middle, surname, nickname): (String, String, String, String) = conn
        .query_row(
            "SELECT given_name, middle_name, surname, nickname FROM people",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(given, "Михаил", "GIVN keeps the given name bare");
    assert_eq!(middle, "Андреевич");
    assert_eq!(surname, "Соловьев");
    assert_eq!(nickname, "Миша");
    assert!(
        report.warnings.is_empty(),
        "known name parts must not warn: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_maps_nick_tag_to_nickname() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @I1@ INDI
1 NAME Мария /Барахтенко/
2 NICK Маруся
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    import_gedcom(&mut conn, Cursor::new(sample))?;

    let nickname: String = conn
        .query_row("SELECT nickname FROM people", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(nickname, "Маруся");
    Ok(())
}

#[test]
fn import_keeps_payload_only_names_unchanged() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @I1@ INDI
1 NAME Иван Иванович /Иванов/
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    import_gedcom(&mut conn, Cursor::new(sample))?;

    let (given, middle): (String, String) = conn
        .query_row("SELECT given_name, middle_name FROM people", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(GedcomError::Db)?;
    assert_eq!(given, "Иван Иванович");
    assert_eq!(middle, "", "no _MIDN means no middle column");
    Ok(())
}

#[test]
fn middle_name_and_nickname_round_trip_through_export() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let profile = crate::db::people::PersonDetails::new(
        "Михаил",
        "Андреевич",
        "Соловьев",
        "Миша",
        "M",
        None,
        None,
    );
    crate::db::people::insert_person(&mut conn, &profile).expect("insert succeeds");

    let ged = export_to_gedcom(&conn)?;
    assert!(ged.contains("1 NAME Михаил Андреевич /Соловьев/"), "{ged}");
    assert!(ged.contains("2 GIVN Михаил"), "{ged}");
    assert!(ged.contains("2 _MIDN Андреевич"), "{ged}");
    assert!(ged.contains("2 _PGVN Миша"), "{ged}");

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(ged.as_str()))?;
    let (given, middle, surname, nickname): (String, String, String, String) = fresh
        .query_row(
            "SELECT given_name, middle_name, surname, nickname FROM people",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(report.people, 1);
    assert_eq!(
        (
            given.as_str(),
            middle.as_str(),
            surname.as_str(),
            nickname.as_str()
        ),
        ("Михаил", "Андреевич", "Соловьев", "Миша")
    );
    assert!(
        report.warnings.is_empty(),
        "round trip must stay warning-free: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_keeps_only_the_first_indi_note() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @I1@ INDI
1 NAME Ann /Lee/
1 NOTE First line
2 CONT second line
2 CONC  joined
1 NOTE Ignored note
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.people, 1);
    assert!(
        report.warnings.is_empty(),
        "notes are supported now: {:?}",
        report.warnings
    );
    let notes: String = conn
        .query_row("SELECT notes FROM people", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(notes, "First line\nsecond line joined");
    Ok(())
}

#[test]
fn person_notes_round_trip_through_export() -> Result<(), GedcomError> {
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let mut profile = crate::db::people::PersonDetails::new("Ann", "", "Lee", "", "F", None, None);
    profile.notes = "Line one\nLine two".to_string();
    crate::db::people::insert_person(&mut conn, &profile).expect("insert succeeds");

    let ged = export_to_gedcom(&conn)?;
    assert!(ged.contains("1 NOTE Line one\n2 CONT Line two\n"), "{ged}");

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(ged.as_str()))?;
    let notes: String = fresh
        .query_row("SELECT notes FROM people", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(notes, "Line one\nLine two");
    assert!(
        report.warnings.is_empty(),
        "round trip must stay warning-free: {:?}",
        report.warnings
    );
    Ok(())
}

/// "Иван" and "Иванов" encoded as Windows-1251 bytes.
const CP1251_IVAN: [u8; 4] = [0xC8, 0xE2, 0xE0, 0xED];
const CP1251_IVANOV: [u8; 6] = [0xC8, 0xE2, 0xE0, 0xED, 0xEE, 0xE2];

#[test]
fn import_falls_back_to_windows_1251_on_utf8_failure() -> Result<(), GedcomError> {
    let mut sample = Vec::new();
    sample.extend_from_slice(b"0 HEAD\n0 @I1@ INDI\n1 NAME ");
    sample.extend_from_slice(&CP1251_IVAN);
    sample.extend_from_slice(b" /");
    sample.extend_from_slice(&CP1251_IVANOV);
    sample.extend_from_slice(b"/\n1 SEX M\n0 TRLR\n");

    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    let (given, surname): (String, String) = conn
        .query_row("SELECT given_name, surname FROM people", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(GedcomError::Db)?;
    assert_eq!((given.as_str(), surname.as_str()), ("Иван", "Иванов"));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("decoding the rest of the stream as windows-1251")),
        "the fallback must be reported once: {:?}",
        report.warnings
    );
    assert_eq!(
        report
            .warnings
            .iter()
            .filter(|warning| warning.contains("windows-1251"))
            .count(),
        1,
        "the stream must escalate only once: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_honours_cp1251_char_header_without_fallback() -> Result<(), GedcomError> {
    let mut sample = Vec::new();
    sample.extend_from_slice(b"0 HEAD\n1 CHAR ANSI\n0 @I1@ INDI\n1 NAME ");
    sample.extend_from_slice(&CP1251_IVAN);
    sample.extend_from_slice(b" /");
    sample.extend_from_slice(&CP1251_IVANOV);
    sample.extend_from_slice(b"/\n1 SEX M\n0 TRLR\n");

    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    let given: String = conn
        .query_row("SELECT given_name FROM people", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(given, "Иван");
    assert!(
        report.warnings.is_empty(),
        "a declared charset must decode without warnings: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_honours_latin1_char_header() -> Result<(), GedcomError> {
    let mut sample = Vec::new();
    sample.extend_from_slice(
        b"0 HEAD\n1 CHAR LATIN1\n0 @I1@ INDI\n1 NAME Jos\xe9 /Doe/\n1 SEX M\n0 TRLR\n",
    );

    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    let given: String = conn
        .query_row("SELECT given_name FROM people", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(given, "Jos\u{e9}");
    assert!(
        report.warnings.is_empty(),
        "the header charset must decode cleanly: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_warns_on_unknown_char_charset() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
1 CHAR UNICODE
0 @I1@ INDI
1 NAME Ann /Lee/
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.people, 1);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("unknown CHAR charset [UNICODE]")),
        "an unknown header must warn: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_logs_unknown_tags_without_dropping_records() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
1 SOUR TEST
1 CHAR UTF-8
0 @I1@ INDI
1 NAME Ada /King/
1 REFN 42
2 TYPE wikitree.privacy
1 OBJE
2 AUTH WikiTree
2 TEXT https://example.com/genealogy/king
1 SEX F
0 @F1@ FAM
1 HUSB @I1@
1 NOTE no license
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.people, 1, "the person still imports");
    assert_eq!(report.families, 1, "the family still imports");
    assert_eq!(report.warnings.len(), 6, "{:?}", report.warnings);
    assert_eq!(
        report.warnings[0],
        "Line 6: Tag recognized/unsupported or ignored [REFN 42]"
    );
    assert!(report.warnings[1].contains("[TYPE wikitree.privacy]"));
    assert_eq!(
        report.warnings[2],
        "Line 8: Tag recognized/unsupported or ignored [OBJE]"
    );
    assert!(report.warnings[3].contains("[AUTH WikiTree]"));
    assert!(report.warnings[4].contains("[TEXT https://example.com/genealogy/king]"));
    assert!(report.warnings[5].contains("[NOTE no license]"));
    Ok(())
}

#[test]
fn unknown_tag_warnings_truncate_long_values() -> Result<(), GedcomError> {
    let payload = "a".repeat(150);
    let sample = format!("0 HEAD\n0 @I1@ INDI\n1 NAME X /Y/\n1 ZZZZ {payload}\n0 TRLR\n");
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample.as_str()))?;

    assert_eq!(report.people, 1);
    let warning = report.warnings.first().expect("the unmapped tag must warn");
    assert!(warning.starts_with("Line 4: Tag recognized/unsupported or ignored [ZZZZ "));
    assert!(warning.ends_with("…]"), "{warning}");
    assert!(warning.contains(&"a".repeat(100)), "first 100 chars kept");
    assert!(!warning.contains(&"a".repeat(101)), "value is truncated");
    Ok(())
}

#[test]
fn unknown_level_zero_records_warn_once_with_a_silent_subtree() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @N1@ NOTE A research note
1 PLAC ignored
0 @I1@ INDI
1 NAME Ann /Lee/
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.people, 1);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert!(
        report.warnings[0].contains("[NOTE A research note]"),
        "{:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn import_creates_places_and_links_events() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 DATE 12 JAN 1850
2 PLAC Uppsala, Sweden
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.events, 1);
    let place: String = conn
        .query_row(
            "SELECT p.name
             FROM events e JOIN places p ON p.id = e.place_id
             WHERE e.event_type = 'BIRTH'",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(place, "Uppsala, Sweden");
    let places: i64 = conn
        .query_row("SELECT COUNT(*) FROM places", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(places, 1, "one place row for the event");
    Ok(())
}

#[test]
fn built_in_facts_round_trip() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @I1@ INDI
1 NAME Ada /King/
1 OCCU Teacher
2 DATE 1870
2 PLAC Uppsala, Sweden
3 MAP
4 LATI N59.8586
4 LONG E17.6389
1 CENS
2 DATE 1860
2 PLAC Uppsala, Sweden
1 BURI
2 DATE 1900
1 _MILI
2 DATE 1863
0 @I2@ INDI
1 NAME Bo /King/
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I2@
1 DIV
2 DATE 1880
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;
    assert!(
        report.warnings.is_empty(),
        "built-in facts must not warn: {:?}",
        report.warnings
    );
    assert_eq!(report.events, 5, "four person facts and one family fact");

    let occupation: (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT description, date FROM events WHERE event_type = 'OCCUPATION'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(occupation.0.as_deref(), Some("Teacher"));
    assert_eq!(occupation.1.as_deref(), Some("1870"));
    let military: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event_type = 'MILITARY'",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(military, 1);
    let divorce: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event_type = 'DIVORCE' AND family_id IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(divorce, 1);

    let exported = export_to_gedcom(&conn)?;
    for expected in ["1 OCCU Teacher", "1 CENS", "1 BURI", "1 _MILI", "1 DIV"] {
        assert!(
            exported.contains(expected),
            "export must contain {expected:?}: {exported:?}"
        );
    }

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(exported.clone()))?;
    assert!(
        report.warnings.is_empty(),
        "fact export must re-import cleanly: {:?}",
        report.warnings
    );
    assert_eq!(
        export_to_gedcom(&fresh)?,
        exported,
        "built-in facts must round-trip byte for byte"
    );
    Ok(())
}

#[test]
fn place_coordinates_round_trip() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 PLAC Uppsala, Sweden
3 MAP
4 LATI N59.8586
4 LONG E17.6389
1 DEAT
2 PLAC Quito, Ecuador
3 MAP
4 LATI S0.1807
4 LONG W78.4678
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;
    assert!(
        report.warnings.is_empty(),
        "valid coordinates must not warn: {:?}",
        report.warnings
    );

    let uppsala: (f64, f64) = conn
        .query_row(
            "SELECT latitude, longitude FROM places WHERE name = 'Uppsala, Sweden'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(uppsala, (59.8586, 17.6389));
    let quito: (f64, f64) = conn
        .query_row(
            "SELECT latitude, longitude FROM places WHERE name = 'Quito, Ecuador'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(quito, (-0.1807, -78.4678));

    let exported = export_to_gedcom(&conn)?;
    for expected in [
        "2 PLAC Uppsala, Sweden",
        "3 MAP",
        "4 LATI N59.8586",
        "4 LONG E17.6389",
        "2 PLAC Quito, Ecuador",
        "4 LATI S0.1807",
        "4 LONG W78.4678",
    ] {
        assert!(
            exported.contains(expected),
            "export must contain {expected:?}: {exported:?}"
        );
    }

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(exported.clone()))?;
    assert!(
        report.warnings.is_empty(),
        "coordinate export must re-import cleanly: {:?}",
        report.warnings
    );
    assert_eq!(
        export_to_gedcom(&fresh)?,
        exported,
        "place coordinates must round-trip byte for byte"
    );
    Ok(())
}

#[test]
fn invalid_place_coordinates_warn_and_are_ignored() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 PLAC Uppsala, Sweden
3 MAP
4 LATI X59.8586
4 LONG E17.6389
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("invalid latitude")),
        "invalid latitude must warn: {:?}",
        report.warnings
    );
    let coordinates: (Option<f64>, Option<f64>) = conn
        .query_row(
            "SELECT latitude, longitude FROM places WHERE name = 'Uppsala, Sweden'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(coordinates.0, None);
    assert_eq!(coordinates.1, Some(17.6389));
    Ok(())
}

#[test]
fn import_reuses_one_source_for_repeated_pointers() -> Result<(), GedcomError> {
    let sample = "\
0 HEAD
0 @S1@ SOUR
1 TITL Parish Book
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 DATE 1850
2 SOUR @S1@
3 PAGE 1
1 DEAT
2 DATE 1900
2 SOUR @S1@
3 PAGE 2
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.citations, 2, "both pointers become citations");
    let sources: i64 = conn
        .query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(sources, 1, "the shared SOUR record is stored once");
    let pages: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM citations WHERE page_or_reference IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(pages, 2);
    Ok(())
}

#[test]
fn distinct_sources_with_the_same_title_stay_separate() -> Result<(), GedcomError> {
    let sample = "\
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 DATE 1850
2 SOUR @S1@
0 @I2@ INDI
1 NAME Bo /King/
1 BIRT
2 DATE 1851
2 SOUR @S2@
0 @S1@ SOUR
1 TITL Census
1 AUTH One
0 @S2@ SOUR
1 TITL Census
1 AUTH Two
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;

    assert_eq!(report.citations, 2);
    let sources: i64 = conn
        .query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(sources, 2, "same-titled sources keep their identity");
    let distinct: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT source_id) FROM citations",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(distinct, 2, "each citation points at its own source");
    let authors: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT author FROM sources ORDER BY rowid")
            .map_err(GedcomError::Db)?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(GedcomError::Db)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(GedcomError::Db)?
    };
    assert_eq!(authors, vec!["One".to_string(), "Two".to_string()]);
    Ok(())
}

#[test]
fn import_does_not_merge_into_existing_sources() -> Result<(), GedcomError> {
    let sample = "\
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 DATE 1850
2 SOUR @S1@
0 @S1@ SOUR
1 TITL Census
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    import_gedcom(&mut conn, Cursor::new(sample))?;
    import_gedcom(&mut conn, Cursor::new(sample))?;

    let sources: i64 = conn
        .query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(sources, 2, "each import creates its own source rows");
    let citations: i64 = conn
        .query_row("SELECT COUNT(*) FROM citations", [], |row| row.get(0))
        .map_err(GedcomError::Db)?;
    assert_eq!(citations, 2);
    Ok(())
}

#[test]
#[allow(clippy::type_complexity)]
fn citation_data_does_not_overwrite_the_event_date() -> Result<(), GedcomError> {
    let sample = "\
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 DATE 12 JAN 1850
2 SOUR @S1@
3 PAGE 12
3 QUAY 3
3 NOTE Citation note
3 DATA
4 DATE 1900
4 TEXT Transcript
0 @S1@ SOUR
1 TITL Parish Book
1 AUTH Ada Author
1 PUBL Publisher
1 ABBR PB
1 REPO @R1@
2 CALN 12/34
1 NOTE Source note
1 DATA
2 TEXT Source data
0 @R1@ REPO
1 NAME Uppsala Archive
1 ADDR Main Street
2 CITY Uppsala
1 NOTE Repo note
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;
    assert!(
        report.warnings.is_empty(),
        "supported source tags must not warn: {:?}",
        report.warnings
    );

    let event_date: String = conn
        .query_row(
            "SELECT date FROM events WHERE event_type = 'BIRTH'",
            [],
            |row| row.get(0),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(event_date, "12 JAN 1850", "citation DATA must not touch it");

    let citation: (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT page_or_reference, quality_certainty, notes, data_date, data_text
             FROM citations",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(citation.0.as_deref(), Some("12"));
    assert_eq!(citation.1.as_deref(), Some("3"));
    assert_eq!(citation.2.as_deref(), Some("Citation note"));
    assert_eq!(citation.3.as_deref(), Some("1900"));
    assert_eq!(citation.4.as_deref(), Some("Transcript"));

    let source: (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT title, author, publication, abbreviation, data_text FROM sources",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(source.0, "Parish Book");
    assert_eq!(source.1.as_deref(), Some("Ada Author"));
    assert_eq!(source.2.as_deref(), Some("Publisher"));
    assert_eq!(source.3.as_deref(), Some("PB"));
    assert_eq!(source.4.as_deref(), Some("Source data"));

    let repository: (String, Option<String>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT r.name, r.address, r.notes, sr.call_number
             FROM repositories r JOIN source_repositories sr ON sr.repository_id = r.id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(GedcomError::Db)?;
    assert_eq!(repository.0, "Uppsala Archive");
    assert_eq!(repository.1.as_deref(), Some("Main Street\nUppsala"));
    assert_eq!(repository.2.as_deref(), Some("Repo note"));
    assert_eq!(repository.3.as_deref(), Some("12/34"));
    Ok(())
}

#[test]
fn ui_created_repositories_round_trip() -> Result<(), GedcomError> {
    use crate::db::repositories::{
        RepositoryDraft, RepositoryLinkDraft, list_repositories, load_repository, save_repository,
    };

    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    conn.execute(
        "INSERT INTO sources (id, title) VALUES ('s1', 'Parish Book')",
        [],
    )
    .map_err(GedcomError::Db)?;
    save_repository(
        &mut conn,
        &RepositoryDraft {
            name: "Uppsala Archive".to_string(),
            address: "Main Street\nUppsala".to_string(),
            notes: "First line\nSecond line".to_string(),
            links: vec![RepositoryLinkDraft {
                source_id: "s1".to_string(),
                call_number: "12/34".to_string(),
                media_type: "film".to_string(),
            }],
            ..RepositoryDraft::default()
        },
    )
    .expect("repository saves");

    let exported = export_to_gedcom(&conn)?;
    for expected in [
        "0 @R1@ REPO",
        "1 NAME Uppsala Archive",
        "1 ADDR Main Street",
        "2 CONT Uppsala",
        "1 NOTE First line",
        "2 CONT Second line",
        "1 REPO @R1@",
        "2 CALN 12/34",
        "2 MEDI film",
    ] {
        assert!(
            exported.contains(expected),
            "export must contain {expected:?}: {exported:?}"
        );
    }

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(exported.clone()))?;
    assert!(
        report.warnings.is_empty(),
        "repository export must re-import cleanly: {:?}",
        report.warnings
    );
    let repositories = list_repositories(&fresh).expect("repositories list");
    assert_eq!(repositories.len(), 1);
    let loaded = load_repository(&fresh, &repositories[0].id)
        .expect("load")
        .expect("repository exists");
    assert_eq!(loaded.name, "Uppsala Archive");
    assert_eq!(loaded.address.as_deref(), Some("Main Street\nUppsala"));
    assert_eq!(loaded.notes.as_deref(), Some("First line\nSecond line"));
    assert_eq!(loaded.links.len(), 1);
    assert_eq!(loaded.links[0].source_title, "Parish Book");
    assert_eq!(loaded.links[0].call_number.as_deref(), Some("12/34"));
    assert_eq!(loaded.links[0].media_type.as_deref(), Some("film"));
    assert_eq!(
        export_to_gedcom(&fresh)?,
        exported,
        "repository metadata must round-trip byte for byte"
    );
    Ok(())
}

#[test]
fn import_warns_on_dangling_repository_pointer() -> Result<(), GedcomError> {
    let sample = "\
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 DATE 1850
2 SOUR @S1@
0 @S1@ SOUR
1 TITL Book
1 REPO @R9@
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut conn, Cursor::new(sample))?;
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("dangling REPO pointer @R9@")),
        "missing repository must warn: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn source_and_citation_metadata_round_trip() -> Result<(), GedcomError> {
    let sample = "\
0 @I1@ INDI
1 NAME Ada /King/
1 BIRT
2 DATE 12 JAN 1850
2 SOUR @S1@
3 PAGE 12
3 QUAY 3
3 NOTE Citation note
3 DATA
4 DATE 1900
4 TEXT Transcript
0 @S1@ SOUR
1 TITL Parish Book
1 AUTH Ada Author
1 PUBL Publisher
1 ABBR PB
1 REPO @R1@
2 CALN 12/34
1 NOTE Source note
1 DATA
2 TEXT Source data
0 @R1@ REPO
1 NAME Uppsala Archive
1 ADDR Main Street
2 CITY Uppsala
1 NOTE Repo note
0 TRLR
";
    let mut conn = setup_test_db().map_err(GedcomError::Db)?;
    import_gedcom(&mut conn, Cursor::new(sample))?;
    let exported = export_to_gedcom(&conn)?;

    for expected in [
        "0 @R1@ REPO",
        "1 NAME Uppsala Archive",
        "1 ADDR Main Street",
        "2 CONT Uppsala",
        "1 NOTE Repo note",
        "0 @S1@ SOUR",
        "1 TITL Parish Book",
        "1 AUTH Ada Author",
        "1 PUBL Publisher",
        "1 ABBR PB",
        "1 REPO @R1@",
        "2 CALN 12/34",
        "1 NOTE Source note",
        "1 DATA",
        "2 TEXT Source data",
        "2 SOUR @S1@",
        "3 PAGE 12",
        "3 QUAY 3",
        "3 NOTE Citation note",
        "3 DATA",
        "4 DATE 1900",
        "4 TEXT Transcript",
    ] {
        assert!(
            exported.contains(expected),
            "export must contain {expected:?}: {exported:?}"
        );
    }

    let mut fresh = setup_test_db().map_err(GedcomError::Db)?;
    let report = import_gedcom(&mut fresh, Cursor::new(exported.clone()))?;
    assert_eq!(report.citations, 1);
    assert!(
        report.warnings.is_empty(),
        "metadata export must re-import cleanly: {:?}",
        report.warnings
    );
    assert_eq!(
        export_to_gedcom(&fresh)?,
        exported,
        "source and citation metadata must round-trip byte for byte"
    );
    Ok(())
}
