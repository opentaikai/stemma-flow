//! Source and source-template persistence.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::source_template::{FieldScope, SourceTemplate, TemplateField, gedcom_columns};

/// One row in the sources list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSummary {
    pub id: String,
    pub title: String,
    pub template_key: Option<String>,
    pub citation_count: i64,
}

/// A stored source loaded for editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord {
    pub id: String,
    pub template_key: Option<String>,
    pub title: String,
    pub author: Option<String>,
    pub publication: Option<String>,
    pub abbreviation: Option<String>,
    pub notes: Option<String>,
    pub data_date: Option<String>,
    pub data_text: Option<String>,
    pub footnote: Option<String>,
    pub short_footnote: Option<String>,
    pub bibliography: Option<String>,
    pub fields: BTreeMap<String, String>,
}

/// An editable source draft.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceDraft {
    pub id: Option<String>,
    pub template_key: Option<String>,
    pub title: String,
    pub author: String,
    pub publication: String,
    pub abbreviation: String,
    pub notes: String,
    pub data_date: String,
    pub data_text: String,
    pub footnote: String,
    pub short_footnote: String,
    pub bibliography: String,
    /// Template field values; empty for free-form sources.
    pub fields: BTreeMap<String, String>,
}

impl SourceDraft {
    pub fn from_record(record: &SourceRecord) -> Self {
        Self {
            id: Some(record.id.clone()),
            template_key: record.template_key.clone(),
            title: record.title.clone(),
            author: record.author.clone().unwrap_or_default(),
            publication: record.publication.clone().unwrap_or_default(),
            abbreviation: record.abbreviation.clone().unwrap_or_default(),
            notes: record.notes.clone().unwrap_or_default(),
            data_date: record.data_date.clone().unwrap_or_default(),
            data_text: record.data_text.clone().unwrap_or_default(),
            footnote: record.footnote.clone().unwrap_or_default(),
            short_footnote: record.short_footnote.clone().unwrap_or_default(),
            bibliography: record.bibliography.clone().unwrap_or_default(),
            fields: record.fields.clone(),
        }
    }
}

fn clean(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Loads the template catalogue, ordered by category and name.
pub fn load_templates(conn: &Connection) -> Result<Vec<SourceTemplate>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT key, name, category, footnote, short_footnote, bibliography, gedcom
             FROM source_templates
             ORDER BY category COLLATE NOCASE, name COLLATE NOCASE, rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;

    let mut templates = Vec::with_capacity(rows.len());
    for (key, name, category, footnote, short_footnote, bibliography, gedcom) in rows {
        let gedcom = toml::from_str::<BTreeMap<String, String>>(&gedcom)
            .map_err(|error| format!("template {key}: invalid GEDCOM mapping: {error}"))?;
        templates.push(SourceTemplate {
            fields: load_fields(conn, &key)?,
            key,
            name,
            category: category.unwrap_or_default(),
            footnote,
            short_footnote,
            bibliography,
            gedcom,
        });
    }
    Ok(templates)
}

fn load_fields(conn: &Connection, key: &str) -> Result<Vec<TemplateField>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT field_key, label, scope, required, hint
             FROM source_template_fields
             WHERE template_key = ?1
             ORDER BY sort_order, field_key",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![key], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(field_key, label, scope, required, hint)| TemplateField {
            key: field_key,
            label,
            scope: if scope == "citation" {
                FieldScope::Citation
            } else {
                FieldScope::Source
            },
            required: required != 0,
            hint,
        })
        .collect())
}

/// Lists sources with their citation counts.
pub fn list_sources(conn: &Connection) -> Result<Vec<SourceSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.title, s.template_id,
                    (SELECT COUNT(*) FROM citations c WHERE c.source_id = s.id)
             FROM sources s
             ORDER BY s.title COLLATE NOCASE, s.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SourceSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                template_key: row.get(2)?,
                citation_count: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads one source and its template field values.
pub fn load_source(conn: &Connection, id: &str) -> Result<Option<SourceRecord>, String> {
    let record = conn
        .query_row(
            "SELECT id, template_id, title, author, publication, abbreviation,
                    notes, data_date, data_text, footnote, short_footnote, bibliography
             FROM sources WHERE id = ?1",
            params![id],
            |row| {
                Ok(SourceRecord {
                    id: row.get(0)?,
                    template_key: row.get(1)?,
                    title: row.get(2)?,
                    author: row.get(3)?,
                    publication: row.get(4)?,
                    abbreviation: row.get(5)?,
                    notes: row.get(6)?,
                    data_date: row.get(7)?,
                    data_text: row.get(8)?,
                    footnote: row.get(9)?,
                    short_footnote: row.get(10)?,
                    bibliography: row.get(11)?,
                    fields: BTreeMap::new(),
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(mut record) = record else {
        return Ok(None);
    };

    let mut stmt = conn
        .prepare(
            "SELECT field_key, field_value FROM source_fields
             WHERE source_id = ?1 ORDER BY field_key",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (key, value) = row.map_err(|error| error.to_string())?;
        record.fields.insert(key, value);
    }
    Ok(Some(record))
}

/// Inserts or updates a source. Templated sources render their GEDCOM
/// columns from the fields; free-form sources use the draft columns.
pub fn save_source(conn: &mut Connection, draft: &SourceDraft) -> Result<String, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let (
        template_key,
        title,
        author,
        publication,
        abbreviation,
        notes,
        data_date,
        data_text,
        footnote,
        short_footnote,
        bibliography,
    ) = if let Some(key) = draft.template_key.as_deref() {
        let templates = load_templates(&tx)?;
        let template = templates
            .iter()
            .find(|template| template.key == key)
            .ok_or_else(|| format!("unknown source template {key}"))?;
        for field in template.fields.iter() {
            if field.scope == FieldScope::Source
                && field.required
                && draft
                    .fields
                    .get(&field.key)
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
            {
                return Err(format!("{} is required", field.label));
            }
        }
        let columns = gedcom_columns(template, &draft.fields);
        let title = clean(columns.get("title").map(String::as_str).unwrap_or(""))
            .unwrap_or_else(|| template.name.clone());
        (
            Some(key.to_string()),
            title,
            clean(columns.get("author").map(String::as_str).unwrap_or("")),
            clean(columns.get("publication").map(String::as_str).unwrap_or("")),
            clean(
                columns
                    .get("abbreviation")
                    .map(String::as_str)
                    .unwrap_or(""),
            ),
            clean(columns.get("notes").map(String::as_str).unwrap_or("")),
            clean(columns.get("data_date").map(String::as_str).unwrap_or("")),
            clean(columns.get("data_text").map(String::as_str).unwrap_or("")),
            None,
            None,
            None,
        )
    } else {
        let title = clean(&draft.title).ok_or_else(|| "Title is required".to_string())?;
        (
            None,
            title,
            clean(&draft.author),
            clean(&draft.publication),
            clean(&draft.abbreviation),
            clean(&draft.notes),
            clean(&draft.data_date),
            clean(&draft.data_text),
            clean(&draft.footnote),
            clean(&draft.short_footnote),
            clean(&draft.bibliography),
        )
    };

    let changed = tx
        .execute(
            "UPDATE sources SET template_id = ?1, title = ?2, author = ?3,
                publication = ?4, abbreviation = ?5, notes = ?6, data_date = ?7,
                data_text = ?8, footnote = ?9, short_footnote = ?10, bibliography = ?11,
                updated_at = datetime('now')
             WHERE id = ?12",
            params![
                template_key,
                title,
                author,
                publication,
                abbreviation,
                notes,
                data_date,
                data_text,
                footnote,
                short_footnote,
                bibliography,
                id
            ],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        tx.execute(
            "INSERT INTO sources (id, template_id, title, author, publication,
                abbreviation, notes, data_date, data_text, footnote, short_footnote,
                bibliography)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                template_key,
                title,
                author,
                publication,
                abbreviation,
                notes,
                data_date,
                data_text,
                footnote,
                short_footnote,
                bibliography
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    tx.execute(
        "DELETE FROM source_fields WHERE source_id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    if template_key.is_some() {
        for (key, value) in &draft.fields {
            if let Some(value) = clean(value) {
                tx.execute(
                    "INSERT OR REPLACE INTO source_fields (source_id, field_key, field_value)
                     VALUES (?1, ?2, ?3)",
                    params![id, key, value],
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }

    tx.commit().map_err(|error| error.to_string())?;
    Ok(id)
}

/// Deletes a source; citations, fields and links cascade.
pub fn delete_source(conn: &mut Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM sources WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no source with id {id}"));
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

    fn book_draft() -> SourceDraft {
        let mut fields = BTreeMap::new();
        fields.insert("author".to_string(), "Ada King".to_string());
        fields.insert("title".to_string(), "Family History".to_string());
        fields.insert("place".to_string(), "London".to_string());
        fields.insert("publisher".to_string(), "Archive Press".to_string());
        fields.insert("year".to_string(), "1900".to_string());
        SourceDraft {
            template_key: Some("book".to_string()),
            fields,
            ..SourceDraft::default()
        }
    }

    #[test]
    fn loads_the_seeded_catalogue() {
        let conn = connection();
        let templates = load_templates(&conn).expect("templates load");
        assert_eq!(templates.len(), 5);
        let book = templates
            .iter()
            .find(|template| template.key == "book")
            .expect("book template");
        assert!(!book.fields.is_empty());
        assert_eq!(
            book.gedcom.get("title").map(String::as_str),
            Some("{title}")
        );
    }

    #[test]
    fn saves_a_templated_source_and_caches_columns() {
        let mut conn = connection();
        let id = save_source(&mut conn, &book_draft()).expect("source saves");
        let source = load_source(&conn, &id)
            .expect("source loads")
            .expect("source exists");
        assert_eq!(source.template_key.as_deref(), Some("book"));
        assert_eq!(source.title, "Family History");
        assert_eq!(source.author.as_deref(), Some("Ada King"));
        assert_eq!(
            source.publication.as_deref(),
            Some("London: Archive Press, 1900")
        );
        assert_eq!(
            source.fields.get("publisher").map(String::as_str),
            Some("Archive Press")
        );
    }

    #[test]
    fn required_fields_are_enforced() {
        let mut conn = connection();
        let mut draft = SourceDraft {
            template_key: Some("census".to_string()),
            ..SourceDraft::default()
        };
        let error = save_source(&mut conn, &draft).expect_err("year is required");
        assert!(error.contains("Census year"), "{error}");

        draft.fields.insert("year".to_string(), "1850".to_string());
        save_source(&mut conn, &draft).expect("source saves");
    }

    #[test]
    fn saves_and_updates_a_free_form_source() {
        let mut conn = connection();
        let mut draft = SourceDraft {
            title: "Parish register".to_string(),
            author: "Ada King".to_string(),
            notes: "Transcript".to_string(),
            footnote: "Parish register, Ada King.".to_string(),
            short_footnote: "Parish register.".to_string(),
            bibliography: "King, Ada. Parish register.".to_string(),
            ..SourceDraft::default()
        };
        let id = save_source(&mut conn, &draft).expect("source saves");
        draft.id = Some(id.clone());
        draft.title = "Parish register, volume 2".to_string();
        save_source(&mut conn, &draft).expect("source updates");

        let sources = list_sources(&conn).expect("sources list");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "Parish register, volume 2");
        assert_eq!(sources[0].template_key, None);
        assert_eq!(sources[0].citation_count, 0);

        let source = load_source(&conn, &id)
            .expect("source loads")
            .expect("source exists");
        assert_eq!(
            source.footnote.as_deref(),
            Some("Parish register, Ada King.")
        );
        assert_eq!(source.short_footnote.as_deref(), Some("Parish register."));
        assert_eq!(
            source.bibliography.as_deref(),
            Some("King, Ada. Parish register.")
        );
    }

    #[test]
    fn templated_save_clears_free_form_outputs() {
        let mut conn = connection();
        let mut draft = SourceDraft {
            title: "Parish register".to_string(),
            footnote: "Footnote.".to_string(),
            short_footnote: "Short.".to_string(),
            bibliography: "Bibliography.".to_string(),
            ..SourceDraft::default()
        };
        let id = save_source(&mut conn, &draft).expect("free-form saves");
        draft.id = Some(id.clone());
        draft.template_key = Some("book".to_string());
        draft
            .fields
            .insert("title".to_string(), "Family History".to_string());
        save_source(&mut conn, &draft).expect("templated save");

        let source = load_source(&conn, &id)
            .expect("source loads")
            .expect("source exists");
        assert_eq!(source.title, "Family History");
        assert_eq!(source.footnote, None);
        assert_eq!(source.short_footnote, None);
        assert_eq!(source.bibliography, None);
    }

    #[test]
    fn updating_replaces_fields_without_duplicates() {
        let mut conn = connection();
        let mut draft = book_draft();
        let id = save_source(&mut conn, &draft).expect("source saves");
        draft.id = Some(id.clone());
        draft
            .fields
            .insert("title".to_string(), "Revised title".to_string());
        save_source(&mut conn, &draft).expect("source updates");

        let source = load_source(&conn, &id)
            .expect("source loads")
            .expect("source exists");
        assert_eq!(source.title, "Revised title");
        let fields: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM source_fields WHERE source_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .expect("field count");
        assert_eq!(fields, draft.fields.len() as i64);
    }

    #[test]
    fn deleting_a_source_cascades_to_citations() {
        let mut conn = connection();
        let id = save_source(&mut conn, &book_draft()).expect("source saves");
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender) VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person insert");
        conn.execute(
            "INSERT INTO events (id, event_type, person_id) VALUES ('e1', 'BIRTH', 'p1')",
            [],
        )
        .expect("event insert");
        conn.execute(
            "INSERT INTO citations (id, source_id, event_id) VALUES ('c1', ?1, 'e1')",
            params![id],
        )
        .expect("citation insert");

        delete_source(&mut conn, &id).expect("source deletes");
        let citations: i64 = conn
            .query_row("SELECT COUNT(*) FROM citations", [], |row| row.get(0))
            .expect("citation count");
        assert_eq!(citations, 0);
    }
}
