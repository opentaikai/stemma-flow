//! Citation persistence, reusable citation links and Evidence Explained
//! quality values.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::quality::{Certainty, EvidenceQuality, InformationQuality, SourceQuality};

/// One citation attached to one of a person's events.
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
    pub usage: i64,
}

/// An event a citation can be attached to from the person inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationTarget {
    pub key: String,
    pub label: String,
    pub event_type: String,
    pub event_id: Option<String>,
}

/// A citation of one source, used when attaching an existing citation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCitation {
    pub id: String,
    pub page: Option<String>,
    pub quality_source: Option<SourceQuality>,
    pub quality_information: Option<InformationQuality>,
    pub quality_evidence: Option<EvidenceQuality>,
    pub quality_certainty: Option<Certainty>,
    pub usage: i64,
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

const USAGE_SQL: &str = "(SELECT COUNT(*) FROM citation_links u WHERE u.citation_id = c.id)";

fn clean(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn parse_certainty(value: Option<String>) -> Option<Certainty> {
    value.as_deref().and_then(Certainty::from_value)
}

/// Lists citations attached to `person_id`'s events, including events of the
/// families where the person is a spouse.
pub fn list_person_citations(
    conn: &Connection,
    person_id: &str,
) -> Result<Vec<CitationRecord>, String> {
    let sql = format!(
        "SELECT c.id, c.source_id, s.title, l.event_id, e.event_type, e.date,
                c.page_or_reference, c.quality_source, c.quality_information,
                c.quality_evidence, c.quality_certainty, {USAGE_SQL}
         FROM citations c
         JOIN citation_links l ON l.citation_id = c.id
         JOIN events e ON e.id = l.event_id
         JOIN sources s ON s.id = c.source_id
         LEFT JOIN families f ON f.id = e.family_id
         WHERE e.person_id = ?1 OR f.husband_id = ?1 OR f.wife_id = ?1
         ORDER BY e.event_type, e.rowid, c.rowid"
    );
    let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
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
                usage: row.get(11)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Lists the events the inspector can attach a citation to: birth, death and
/// the marriages of the person's families.
pub fn list_citation_targets(
    conn: &Connection,
    person_id: &str,
) -> Result<Vec<CitationTarget>, String> {
    let mut targets = Vec::new();
    for (event_type, label) in [("BIRTH", "Birth"), ("DEATH", "Death")] {
        let event_id: Option<String> = conn
            .query_row(
                "SELECT id FROM events WHERE person_id = ?1 AND event_type = ?2
                 ORDER BY rowid LIMIT 1",
                params![person_id, event_type],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        targets.push(CitationTarget {
            key: event_type.to_string(),
            label: label.to_string(),
            event_type: event_type.to_string(),
            event_id,
        });
    }

    let mut stmt = conn
        .prepare(
            "SELECT f.id,
                    (SELECT e.id FROM events e
                     WHERE e.family_id = f.id AND e.event_type = 'MARRIAGE'
                     ORDER BY e.rowid LIMIT 1),
                    (SELECT e.date FROM events e
                     WHERE e.family_id = f.id AND e.event_type = 'MARRIAGE'
                     ORDER BY e.rowid LIMIT 1),
                    p.given_name, p.middle_name, p.surname
             FROM families f
             LEFT JOIN people p ON p.id =
                 CASE WHEN f.husband_id = ?1 THEN f.wife_id ELSE f.husband_id END
             WHERE f.husband_id = ?1 OR f.wife_id = ?1
             ORDER BY f.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| {
            let event_id: Option<String> = row.get(1)?;
            let date: Option<String> = row.get(2)?;
            let spouse = [row.get::<_, Option<String>>(3)?, row.get(4)?, row.get(5)?]
                .into_iter()
                .flatten()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let label = if spouse.is_empty() {
                "Marriage".to_string()
            } else {
                format!("Marriage to {spouse}")
            };
            let label = match date
                .as_deref()
                .map(str::trim)
                .filter(|date| !date.is_empty())
            {
                Some(date) => format!("{label} ({date})"),
                None => label,
            };
            let family_id: String = row.get(0)?;
            Ok(CitationTarget {
                key: format!("MARRIAGE:{family_id}"),
                label,
                event_type: "MARRIAGE".to_string(),
                event_id,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    targets.extend(rows);
    Ok(targets)
}

/// Lists a source's citations with their usage counts for the reuse picker.
pub fn list_source_citations(
    conn: &Connection,
    source_id: &str,
) -> Result<Vec<SourceCitation>, String> {
    let sql = format!(
        "SELECT c.id, c.page_or_reference, c.quality_source,
                c.quality_information, c.quality_evidence, c.quality_certainty, {USAGE_SQL}
         FROM citations c WHERE c.source_id = ?1 ORDER BY c.rowid"
    );
    let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![source_id], |row| {
            let source: Option<String> = row.get(2)?;
            let information: Option<String> = row.get(3)?;
            let evidence: Option<String> = row.get(4)?;
            let certainty: Option<String> = row.get(5)?;
            Ok(SourceCitation {
                id: row.get(0)?,
                page: row.get(1)?,
                quality_source: source.as_deref().and_then(SourceQuality::from_key),
                quality_information: information
                    .as_deref()
                    .and_then(InformationQuality::from_key),
                quality_evidence: evidence.as_deref().and_then(EvidenceQuality::from_key),
                quality_certainty: parse_certainty(certainty),
                usage: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Updates the page and quality values of a citation; every use sees them.
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

/// Attaches `source_id` to a target event, creating the event when needed.
pub fn add_citation(
    conn: &mut Connection,
    source_id: &str,
    person_id: &str,
    target: &str,
    page: Option<&str>,
) -> Result<String, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    ensure_person(&tx, person_id)?;
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
    let event_id = resolve_target(&tx, person_id, target)?;

    let citation_id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO citations (id, source_id, page_or_reference) VALUES (?1, ?2, ?3)",
        params![citation_id, source_id, clean(page)],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "INSERT INTO citation_links (citation_id, event_id) VALUES (?1, ?2)",
        params![citation_id, event_id],
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(citation_id)
}

/// Attaches an existing citation to another target event.
///
/// Returns `false` when the citation is already linked to that event.
pub fn attach_citation(
    conn: &mut Connection,
    citation_id: &str,
    person_id: &str,
    target: &str,
) -> Result<bool, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    ensure_person(&tx, person_id)?;
    let citation_exists = tx
        .query_row(
            "SELECT 1 FROM citations WHERE id = ?1",
            params![citation_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if citation_exists.is_none() {
        return Err(format!("no citation with id {citation_id}"));
    }
    let event_id = resolve_target(&tx, person_id, target)?;
    let changed = tx
        .execute(
            "INSERT OR IGNORE INTO citation_links (citation_id, event_id) VALUES (?1, ?2)",
            params![citation_id, event_id],
        )
        .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(changed > 0)
}

/// Removes one citation-to-event link; the citation itself stays reusable.
pub fn detach_citation(conn: &Connection, citation_id: &str, event_id: &str) -> Result<(), String> {
    let changed = conn
        .execute(
            "DELETE FROM citation_links WHERE citation_id = ?1 AND event_id = ?2",
            params![citation_id, event_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!(
            "citation {citation_id} is not attached to event {event_id}"
        ));
    }
    Ok(())
}

/// Removes a citation and all of its links; source and events are unaffected.
pub fn delete_citation(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM citations WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no citation with id {id}"));
    }
    Ok(())
}

fn ensure_person(conn: &Connection, person_id: &str) -> Result<(), String> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM people WHERE id = ?1",
            params![person_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if exists.is_none() {
        return Err(format!("no person with id {person_id}"));
    }
    Ok(())
}

fn resolve_target(conn: &Connection, person_id: &str, target: &str) -> Result<String, String> {
    if let Some(family_id) = target.strip_prefix("MARRIAGE:") {
        if family_id.is_empty() {
            return Err("marriage target is missing a family id".to_string());
        }
        let belongs = conn
            .query_row(
                "SELECT 1 FROM families
                 WHERE id = ?1 AND (husband_id = ?2 OR wife_id = ?2)",
                params![family_id, person_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if belongs.is_none() {
            return Err(format!(
                "person {person_id} is not a spouse in family {family_id}"
            ));
        }
        let event_id: Option<String> = conn
            .query_row(
                "SELECT id FROM events WHERE family_id = ?1 AND event_type = 'MARRIAGE'
                 ORDER BY rowid LIMIT 1",
                params![family_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        return match event_id {
            Some(event_id) => Ok(event_id),
            None => create_event(conn, "MARRIAGE", None, Some(family_id)),
        };
    }

    if !matches!(target, "BIRTH" | "DEATH") {
        return Err(format!("unsupported citation target {target}"));
    }
    let event_id: Option<String> = conn
        .query_row(
            "SELECT id FROM events WHERE person_id = ?1 AND event_type = ?2
             ORDER BY rowid LIMIT 1",
            params![person_id, target],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    match event_id {
        Some(event_id) => Ok(event_id),
        None => create_event(conn, target, Some(person_id), None),
    }
}

fn create_event(
    conn: &Connection,
    event_type: &str,
    person_id: Option<&str>,
    family_id: Option<&str>,
) -> Result<String, String> {
    let event_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO events (id, event_type, person_id, family_id) VALUES (?1, ?2, ?3, ?4)",
        params![event_id, event_type, person_id, family_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(event_id)
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

    fn family(conn: &Connection, id: &str, husband: &str, wife: &str) {
        conn.execute(
            "INSERT INTO families (id, husband_id, wife_id) VALUES (?1, ?2, ?3)",
            params![id, husband, wife],
        )
        .expect("family insert");
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
        assert_eq!(citations[0].usage, 1);
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
                 JOIN citation_links l ON l.event_id = e.id
                 JOIN citations c ON c.id = l.citation_id
                 WHERE c.source_id = 's1'",
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
            "unsupported target rejected"
        );
    }

    #[test]
    fn attaches_and_detaches_reusable_citations() {
        let mut conn = connection();
        let person = person(&conn, "p1", "Ada");
        let source = source(&conn, "s1", "Parish Book");
        let id =
            add_citation(&mut conn, &source, &person, "BIRTH", Some("12")).expect("citation added");

        let added = attach_citation(&mut conn, &id, &person, "DEATH").expect("citation attached");
        assert!(added, "a new link is created");
        let added = attach_citation(&mut conn, &id, &person, "DEATH").expect("citation attached");
        assert!(!added, "attaching twice is idempotent");

        let citations = list_person_citations(&conn, &person).expect("citations list");
        assert_eq!(citations.len(), 2);
        assert!(citations.iter().all(|citation| citation.id == id));
        assert!(citations.iter().all(|citation| citation.usage == 2));
        assert_eq!(
            citations
                .iter()
                .filter(|citation| citation.event_type == "DEATH")
                .count(),
            1
        );

        let edit = CitationEdit {
            id: id.clone(),
            page: Some("14".to_string()),
            quality_source: Some(SourceQuality::Original),
            quality_information: None,
            quality_evidence: None,
            quality_certainty: None,
        };
        save_citation(&conn, &edit).expect("citation saved");
        let citations = list_person_citations(&conn, &person).expect("citations list");
        assert!(
            citations
                .iter()
                .all(|citation| citation.page.as_deref() == Some("14")),
            "shared edits reach every use"
        );

        let death_event = citations
            .iter()
            .find(|citation| citation.event_type == "DEATH")
            .expect("death citation")
            .event_id
            .clone();
        detach_citation(&conn, &id, &death_event).expect("citation detached");
        let citations = list_person_citations(&conn, &person).expect("citations list");
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].usage, 1);
        assert_eq!(citations[0].event_type, "BIRTH");
    }

    #[test]
    fn lists_marriage_targets_and_family_citations() {
        let mut conn = connection();
        let ada = person(&conn, "p1", "Ada");
        let otto = person(&conn, "p2", "Otto");
        family(&conn, "f1", &otto, &ada);
        let source = source(&conn, "s1", "Parish Book");

        let targets = list_citation_targets(&conn, &ada).expect("targets");
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].key, "BIRTH");
        assert_eq!(targets[1].key, "DEATH");
        assert_eq!(targets[2].key, "MARRIAGE:f1");
        assert_eq!(targets[2].label, "Marriage to Otto King");
        assert_eq!(targets[2].event_id, None);

        let id = add_citation(&mut conn, &source, &ada, "MARRIAGE:f1", Some("3"))
            .expect("marriage citation");
        let citations = list_person_citations(&conn, &ada).expect("ada citations");
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].id, id);
        assert_eq!(citations[0].event_type, "MARRIAGE");
        assert_eq!(
            list_person_citations(&conn, &otto)
                .expect("otto citations")
                .len(),
            1,
            "the marriage citation is visible from both spouses"
        );

        let targets = list_citation_targets(&conn, &ada).expect("targets");
        assert!(
            targets[2].event_id.is_some(),
            "the marriage event now exists"
        );
    }

    #[test]
    fn marriage_target_without_a_known_spouse_still_lists() {
        let conn = connection();
        let ada = person(&conn, "p1", "Ada");
        conn.execute(
            "INSERT INTO families (id, wife_id) VALUES ('f1', ?1)",
            params![ada],
        )
        .expect("single-parent family");

        let targets = list_citation_targets(&conn, &ada).expect("targets");
        assert_eq!(targets[2].key, "MARRIAGE:f1");
        assert_eq!(targets[2].label, "Marriage");
    }

    #[test]
    fn attach_rejects_citations_from_other_targets() {
        let mut conn = connection();
        let ada = person(&conn, "p1", "Ada");
        let bo = person(&conn, "p2", "Bo");
        let source = source(&conn, "s1", "Parish Book");
        let id = add_citation(&mut conn, &source, &ada, "BIRTH", None).expect("citation added");

        assert!(
            attach_citation(&mut conn, "missing", &ada, "DEATH").is_err(),
            "unknown citation rejected"
        );
        assert!(
            attach_citation(&mut conn, &id, &ada, "MARRIAGE:f1").is_err(),
            "unknown family rejected"
        );
        assert!(
            attach_citation(&mut conn, &id, &bo, "UNKNOWN").is_err(),
            "unsupported target rejected"
        );
    }

    #[test]
    fn lists_source_citations_with_usage() {
        let mut conn = connection();
        let person = person(&conn, "p1", "Ada");
        let source = source(&conn, "s1", "Parish Book");
        let first = add_citation(&mut conn, &source, &person, "BIRTH", Some("12")).expect("first");
        add_citation(&mut conn, &source, &person, "DEATH", Some("13")).expect("second");
        attach_citation(&mut conn, &first, &person, "DEATH").expect("shared");

        let choices = list_source_citations(&conn, &source).expect("choices");
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].id, first);
        assert_eq!(choices[0].usage, 2);
        assert_eq!(choices[1].usage, 1);
        assert_eq!(choices[0].page.as_deref(), Some("12"));
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
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM citation_links", [], |row| row
                .get::<_, i64>(0))
                .expect("link count"),
            0,
            "links cascade with the citation"
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
