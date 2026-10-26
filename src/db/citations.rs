//! Citation persistence and Evidence Explained quality values.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::quality::{Certainty, EvidenceQuality, InformationQuality, SourceQuality};

/// One citation attached to a person's event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationRecord {
    pub id: String,
    pub source_id: String,
    pub source_title: String,
    pub event_id: String,
    pub event_type: String,
    pub event_date: Option<String>,
    pub page: Option<String>,
    pub quality_source: Option<SourceQuality>,
    pub quality_information: Option<InformationQuality>,
    pub quality_evidence: Option<EvidenceQuality>,
    pub quality_certainty: Option<Certainty>,
}

/// The editable parts of a citation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationEdit {
    pub id: String,
    pub page: Option<String>,
    pub quality_source: Option<SourceQuality>,
    pub quality_information: Option<InformationQuality>,
    pub quality_evidence: Option<EvidenceQuality>,
    pub quality_certainty: Option<Certainty>,
}

fn clean(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn parse_certainty(value: Option<String>) -> Option<Certainty> {
    value.as_deref().and_then(Certainty::from_value)
}

/// Lists citations attached to `person_id`'s events, ordered by event.
pub fn list_person_citations(
    conn: &Connection,
    person_id: &str,
) -> Result<Vec<CitationRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.source_id, s.title, c.event_id, e.event_type, e.date,
                    c.page_or_reference, c.quality_source, c.quality_information,
                    c.quality_evidence, c.quality_certainty
             FROM citations c
             JOIN sources s ON s.id = c.source_id
             JOIN events e ON e.id = c.event_id
             WHERE e.person_id = ?1
             ORDER BY e.event_type, e.rowid, c.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| {
            let source: Option<String> = row.get(7)?;
            let information: Option<String> = row.get(8)?;
            let evidence: Option<String> = row.get(9)?;
            let certainty: Option<String> = row.get(10)?;
            Ok(CitationRecord {
                id: row.get(0)?,
                source_id: row.get(1)?,
                source_title: row.get(2)?,
                event_id: row.get(3)?,
                event_type: row.get(4)?,
                event_date: row.get(5)?,
                page: row.get(6)?,
                quality_source: source.as_deref().and_then(SourceQuality::from_key),
                quality_information: information
                    .as_deref()
                    .and_then(InformationQuality::from_key),
                quality_evidence: evidence.as_deref().and_then(EvidenceQuality::from_key),
                quality_certainty: parse_certainty(certainty),
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Updates the page and quality values of a citation.
pub fn save_citation(conn: &Connection, edit: &CitationEdit) -> Result<(), String> {
    let changed = conn
        .execute(
            "UPDATE citations SET page_or_reference = ?1, quality_source = ?2,
                quality_information = ?3, quality_evidence = ?4, quality_certainty = ?5
             WHERE id = ?6",
            params![
                clean(edit.page.as_deref()),
                edit.quality_source.map(SourceQuality::key),
                edit.quality_information.map(InformationQuality::key),
                edit.quality_evidence.map(EvidenceQuality::key),
                edit.quality_certainty.map(Certainty::value),
                edit.id
            ],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no citation with id {}", edit.id));
    }
    Ok(())
}

/// Attaches `source_id` to a person's event of `event_type`, creating the
/// event when it does not exist yet.
pub fn add_citation(
    conn: &mut Connection,
    source_id: &str,
    person_id: &str,
    event_type: &str,
    page: Option<&str>,
) -> Result<String, String> {
    if !matches!(event_type, "BIRTH" | "DEATH") {
        return Err(format!("unsupported event type {event_type}"));
    }
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let person_exists = tx
        .query_row(
            "SELECT 1 FROM people WHERE id = ?1",
            params![person_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if person_exists.is_none() {
        return Err(format!("no person with id {person_id}"));
    }
    let source_exists = tx
        .query_row(
            "SELECT 1 FROM sources WHERE id = ?1",
            params![source_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if source_exists.is_none() {
        return Err(format!("no source with id {source_id}"));
    }

    let event_id: Option<String> = tx
        .query_row(
            "SELECT id FROM events WHERE person_id = ?1 AND event_type = ?2
             ORDER BY rowid LIMIT 1",
            params![person_id, event_type],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let event_id = match event_id {
        Some(event_id) => event_id,
        None => {
            let event_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO events (id, event_type, person_id) VALUES (?1, ?2, ?3)",
                params![event_id, event_type, person_id],
            )
            .map_err(|error| error.to_string())?;
            event_id
        }
    };

    let citation_id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO citations (id, source_id, page_or_reference, event_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![citation_id, source_id, clean(page), event_id],
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(citation_id)
}

/// Removes a citation; source and event rows are unaffected.
pub fn delete_citation(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM citations WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no citation with id {id}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma applies");
        init_db(&conn).expect("schema initialises");
        conn
    }

    fn person(conn: &Connection, id: &str, given: &str) -> String {
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender) VALUES (?1, ?2, 'King', 'U')",
            params![id, given],
        )
        .expect("person insert");
        id.to_string()
    }

    fn source(conn: &Connection, id: &str, title: &str) -> String {
        conn.execute(
            "INSERT INTO sources (id, title) VALUES (?1, ?2)",
            params![id, title],
        )
        .expect("source insert");
        id.to_string()
    }

    #[test]
    fn adds_and_lists_person_citations() {
        let mut conn = connection();
        let person = person(&conn, "p1", "Ada");
        let source = source(&conn, "s1", "Parish Book");

        let id =
            add_citation(&mut conn, &source, &person, "BIRTH", Some("12")).expect("citation added");
        let citations = list_person_citations(&conn, &person).expect("citations list");
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].id, id);
        assert_eq!(citations[0].source_title, "Parish Book");
        assert_eq!(citations[0].event_type, "BIRTH");
        assert_eq!(citations[0].page.as_deref(), Some("12"));
        assert_eq!(citations[0].quality_source, None);
    }

    #[test]
    fn saves_quality_values() {
        let mut conn = connection();
        let person = person(&conn, "p1", "Ada");
        let source = source(&conn, "s1", "Parish Book");
        let id = add_citation(&mut conn, &source, &person, "DEATH", None).expect("citation added");

        let edit = CitationEdit {
            id: id.clone(),
            page: Some(" 14 ".to_string()),
            quality_source: Some(SourceQuality::Original),
            quality_information: Some(InformationQuality::Primary),
            quality_evidence: Some(EvidenceQuality::Direct),
            quality_certainty: Some(Certainty::Primary),
        };
        save_citation(&conn, &edit).expect("citation saved");

        let citations = list_person_citations(&conn, &person).expect("citations list");
        assert_eq!(citations[0].page.as_deref(), Some("14"));
        assert_eq!(citations[0].quality_source, Some(SourceQuality::Original));
        assert_eq!(
            citations[0].quality_information,
            Some(InformationQuality::Primary)
        );
        assert_eq!(citations[0].quality_evidence, Some(EvidenceQuality::Direct));
        assert_eq!(citations[0].quality_certainty, Some(Certainty::Primary));
    }

    #[test]
    fn citations_are_scoped_to_the_person() {
        let mut conn = connection();
        let ada = person(&conn, "p1", "Ada");
        let bo = person(&conn, "p2", "Bo");
        let source = source(&conn, "s1", "Parish Book");
        add_citation(&mut conn, &source, &ada, "BIRTH", None).expect("ada citation");
        add_citation(&mut conn, &source, &bo, "BIRTH", None).expect("bo citation");

        assert_eq!(
            list_person_citations(&conn, &ada).expect("ada list").len(),
            1
        );
        assert_eq!(list_person_citations(&conn, &bo).expect("bo list").len(), 1);
    }

    #[test]
    fn add_creates_the_event_when_missing() {
        let mut conn = connection();
        let person = person(&conn, "p1", "Ada");
        let source = source(&conn, "s1", "Parish Book");
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .expect("event count");

        add_citation(&mut conn, &source, &person, "BIRTH", None).expect("citation added");

        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(after, before + 1, "the birth event is created");
        let event_type: String = conn
            .query_row(
                "SELECT e.event_type FROM events e
                 JOIN citations c ON c.event_id = e.id WHERE c.source_id = 's1'",
                [],
                |row| row.get(0),
            )
            .expect("event type");
        assert_eq!(event_type, "BIRTH");
    }

    #[test]
    fn add_rejects_unknown_sources_and_event_types() {
        let mut conn = connection();
        let person = person(&conn, "p1", "Ada");
        let source = source(&conn, "s1", "Parish Book");

        assert!(
            add_citation(&mut conn, "missing", &person, "BIRTH", None).is_err(),
            "unknown source rejected"
        );
        assert!(
            add_citation(&mut conn, &source, &person, "MARRIAGE", None).is_err(),
            "unsupported event type rejected"
        );
    }

    #[test]
    fn deleting_a_citation_keeps_the_source_and_event() {
        let mut conn = connection();
        let person = person(&conn, "p1", "Ada");
        let source = source(&conn, "s1", "Parish Book");
        let id = add_citation(&mut conn, &source, &person, "BIRTH", None).expect("citation added");

        delete_citation(&conn, &id).expect("citation deleted");

        assert!(
            list_person_citations(&conn, &person)
                .expect("list")
                .is_empty()
        );
        let sources: i64 = conn
            .query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))
            .expect("source count");
        assert_eq!(sources, 1);
        let events: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .expect("event count");
        assert_eq!(events, 1);
    }
}
