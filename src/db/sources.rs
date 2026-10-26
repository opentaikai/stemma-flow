//! Source and source-template persistence.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::source_template::{
    FieldScope, SourceTemplate, TemplateField, gedcom_columns, render_citation,
};

/// One row in the sources list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSummary {
    pub id: String,
    pub title: String,
    pub template_key: Option<String>,
    pub citation_count: i64,
}

/// Counts of the records moved by a source merge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMergeReport {
    pub citations: i64,
    pub repositories: i64,
    pub media_links: i64,
    pub task_links: i64,
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

/// Inserts or updates a source inside the caller's transaction.
fn save_source_tx(conn: &Connection, draft: &SourceDraft) -> Result<String, String> {
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
        let templates = load_templates(conn)?;
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

    let changed = conn
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
        conn.execute(
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

    conn.execute(
        "DELETE FROM source_fields WHERE source_id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    if template_key.is_some() {
        for (key, value) in &draft.fields {
            if let Some(value) = clean(value) {
                conn.execute(
                    "INSERT OR REPLACE INTO source_fields (source_id, field_key, field_value)
                     VALUES (?1, ?2, ?3)",
                    params![id, key, value],
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }

    Ok(id)
}

/// Inserts or updates a source. Templated sources render their GEDCOM
/// columns from the fields; free-form sources use the draft columns.
pub fn save_source(conn: &mut Connection, draft: &SourceDraft) -> Result<String, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let id = save_source_tx(&tx, draft)?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(id)
}

/// Merges `source_id` into `target_id`. Target values win; empty target
/// values are filled from the absorbed source.
pub fn merge_sources(
    conn: &mut Connection,
    source_id: &str,
    target_id: &str,
) -> Result<SourceMergeReport, String> {
    if source_id == target_id {
        return Err("cannot merge a source into itself".to_string());
    }
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let source =
        load_source(&tx, source_id)?.ok_or_else(|| format!("no source with id {source_id}"))?;
    let target =
        load_source(&tx, target_id)?.ok_or_else(|| format!("no source with id {target_id}"))?;
    let templates = load_templates(&tx)?;
    let mut report = SourceMergeReport::default();

    let merged = if target.template_key.is_none() {
        let (footnote, short_footnote, bibliography) = if source.template_key.is_some() {
            let key = source.template_key.as_deref().unwrap_or_default();
            let template = templates
                .iter()
                .find(|template| template.key == key)
                .ok_or_else(|| format!("unknown source template {key}"))?;
            let rendered = render_citation(template, &source.fields, &BTreeMap::new());
            (
                pick(target.footnote.clone(), Some(rendered.footnote)),
                pick(target.short_footnote.clone(), Some(rendered.short_footnote)),
                pick(target.bibliography.clone(), Some(rendered.bibliography)),
            )
        } else {
            (
                pick(target.footnote.clone(), source.footnote.clone()),
                pick(target.short_footnote.clone(), source.short_footnote.clone()),
                pick(target.bibliography.clone(), source.bibliography.clone()),
            )
        };
        Some(SourceDraft {
            id: Some(target.id.clone()),
            template_key: None,
            title: target.title.clone(),
            author: pick(target.author.clone(), source.author.clone()).unwrap_or_default(),
            publication: pick(target.publication.clone(), source.publication.clone())
                .unwrap_or_default(),
            abbreviation: pick(target.abbreviation.clone(), source.abbreviation.clone())
                .unwrap_or_default(),
            notes: pick(target.notes.clone(), source.notes.clone()).unwrap_or_default(),
            data_date: pick(target.data_date.clone(), source.data_date.clone()).unwrap_or_default(),
            data_text: pick(target.data_text.clone(), source.data_text.clone()).unwrap_or_default(),
            footnote: footnote.unwrap_or_default(),
            short_footnote: short_footnote.unwrap_or_default(),
            bibliography: bibliography.unwrap_or_default(),
            fields: BTreeMap::new(),
        })
    } else if target.template_key == source.template_key {
        let key = target.template_key.as_deref().unwrap_or_default();
        let template = templates
            .iter()
            .find(|template| template.key == key)
            .ok_or_else(|| format!("unknown source template {key}"))?;
        let mut fields = target
            .fields
            .iter()
            .filter(|(field, _)| template.source_fields().any(|item| item.key == **field))
            .map(|(field, value)| (field.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        for (field, value) in &source.fields {
            if template.source_fields().any(|item| item.key == *field) {
                fields.entry(field.clone()).or_insert_with(|| value.clone());
            }
        }
        Some(SourceDraft {
            id: Some(target.id.clone()),
            template_key: target.template_key.clone(),
            fields,
            ..SourceDraft::default()
        })
    } else {
        None
    };

    if let Some(draft) = merged {
        save_source_tx(&tx, &draft)?;
    }

    let mut stmt = tx
        .prepare(
            "SELECT repository_id, call_number, media_type
             FROM source_repositories WHERE source_id = ?1",
        )
        .map_err(|error| error.to_string())?;
    let links = stmt
        .query_map(params![source_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    drop(stmt);
    for (repository_id, call_number, media_type) in links {
        let existing: Option<(Option<String>, Option<String>)> = tx
            .query_row(
                "SELECT call_number, media_type FROM source_repositories
                 WHERE source_id = ?1 AND repository_id = ?2",
                params![target_id, repository_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        match existing {
            Some((target_call, target_media)) => {
                let call_number = pick(target_call, call_number);
                let media_type = pick(target_media, media_type);
                tx.execute(
                    "UPDATE source_repositories SET call_number = ?1, media_type = ?2
                     WHERE source_id = ?3 AND repository_id = ?4",
                    params![call_number, media_type, target_id, repository_id],
                )
                .map_err(|error| error.to_string())?;
            }
            None => {
                tx.execute(
                    "INSERT INTO source_repositories
                     (source_id, repository_id, call_number, media_type)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![target_id, repository_id, call_number, media_type],
                )
                .map_err(|error| error.to_string())?;
            }
        }
        report.repositories += 1;
    }
    tx.execute(
        "DELETE FROM source_repositories WHERE source_id = ?1",
        params![source_id],
    )
    .map_err(|error| error.to_string())?;

    tx.execute(
        "UPDATE sources SET repository_id = COALESCE(
             repository_id,
             (SELECT repository_id FROM sources WHERE id = ?1)
         ) WHERE id = ?2",
        params![source_id, target_id],
    )
    .map_err(|error| error.to_string())?;

    report.citations = tx
        .execute(
            "UPDATE citations SET source_id = ?1 WHERE source_id = ?2",
            params![target_id, source_id],
        )
        .map_err(|error| error.to_string())? as i64;

    tx.execute(
        "DELETE FROM media_links WHERE source_id = ?1 AND media_id IN
             (SELECT media_id FROM media_links WHERE source_id = ?2)",
        params![source_id, target_id],
    )
    .map_err(|error| error.to_string())?;
    report.media_links = tx
        .execute(
            "UPDATE media_links SET source_id = ?1 WHERE source_id = ?2",
            params![target_id, source_id],
        )
        .map_err(|error| error.to_string())? as i64;

    tx.execute(
        "DELETE FROM task_links WHERE source_id = ?1 AND task_id IN
             (SELECT task_id FROM task_links WHERE source_id = ?2)",
        params![source_id, target_id],
    )
    .map_err(|error| error.to_string())?;
    report.task_links = tx
        .execute(
            "UPDATE task_links SET source_id = ?1 WHERE source_id = ?2",
            params![target_id, source_id],
        )
        .map_err(|error| error.to_string())? as i64;

    let deleted = tx
        .execute("DELETE FROM sources WHERE id = ?1", params![source_id])
        .map_err(|error| error.to_string())?;
    if deleted == 0 {
        return Err(format!("no source with id {source_id}"));
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(report)
}

fn pick(target: Option<String>, source: Option<String>) -> Option<String> {
    target
        .filter(|value| !value.trim().is_empty())
        .or_else(|| source.filter(|value| !value.trim().is_empty()))
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
    use crate::db::citations::{self as citation_store, CitationEdit};
    use crate::db::init_db;
    use crate::model::quality::SourceQuality;

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
            "INSERT INTO citations (id, source_id) VALUES ('c1', ?1)",
            params![id],
        )
        .expect("citation insert");
        conn.execute(
            "INSERT INTO citation_links (citation_id, event_id) VALUES ('c1', 'e1')",
            [],
        )
        .expect("citation link insert");

        delete_source(&mut conn, &id).expect("source deletes");
        let citations: i64 = conn
            .query_row("SELECT COUNT(*) FROM citations", [], |row| row.get(0))
            .expect("citation count");
        assert_eq!(citations, 0);
    }

    #[test]
    fn merges_sources_with_the_same_template() {
        let mut conn = connection();
        let mut target = book_draft();
        target.fields.remove("place");
        target.fields.insert("year".to_string(), "1901".to_string());
        let target_id = save_source(&mut conn, &target).expect("target saves");

        let mut source = book_draft();
        source
            .fields
            .insert("place".to_string(), "Paris".to_string());
        source
            .fields
            .insert("publisher".to_string(), "Source Press".to_string());
        source.fields.insert("year".to_string(), "1899".to_string());
        let source_id = save_source(&mut conn, &source).expect("source saves");

        let report = merge_sources(&mut conn, &source_id, &target_id).expect("merge succeeds");
        assert_eq!(report.citations, 0);
        assert!(
            load_source(&conn, &source_id).expect("load").is_none(),
            "the absorbed source is gone"
        );

        let merged = load_source(&conn, &target_id)
            .expect("load")
            .expect("target exists");
        assert_eq!(merged.title, "Family History");
        assert_eq!(merged.author.as_deref(), Some("Ada King"));
        assert_eq!(
            merged.fields.get("place").map(String::as_str),
            Some("Paris"),
            "the target's missing field is filled"
        );
        assert_eq!(
            merged.fields.get("publisher").map(String::as_str),
            Some("Archive Press"),
            "the target's value wins"
        );
        assert_eq!(merged.fields.get("year").map(String::as_str), Some("1901"));
        assert_eq!(
            merged.publication.as_deref(),
            Some("Paris: Archive Press, 1901"),
            "cached columns are re-rendered from the merged fields"
        );
    }

    #[test]
    fn merges_free_form_sources_into_the_target() {
        let mut conn = connection();
        let target_id = save_source(
            &mut conn,
            &SourceDraft {
                title: "Target".to_string(),
                footnote: "Target footnote.".to_string(),
                author: "Target Author".to_string(),
                ..SourceDraft::default()
            },
        )
        .expect("target saves");
        let source_id = save_source(
            &mut conn,
            &SourceDraft {
                title: "Source".to_string(),
                footnote: "Source footnote.".to_string(),
                short_footnote: "Source short.".to_string(),
                bibliography: "Source bibliography.".to_string(),
                author: "Source Author".to_string(),
                notes: "Source notes".to_string(),
                ..SourceDraft::default()
            },
        )
        .expect("source saves");

        merge_sources(&mut conn, &source_id, &target_id).expect("merge succeeds");

        let merged = load_source(&conn, &target_id)
            .expect("load")
            .expect("target exists");
        assert_eq!(merged.title, "Target");
        assert_eq!(merged.footnote.as_deref(), Some("Target footnote."));
        assert_eq!(merged.short_footnote.as_deref(), Some("Source short."));
        assert_eq!(merged.bibliography.as_deref(), Some("Source bibliography."));
        assert_eq!(merged.author.as_deref(), Some("Target Author"));
        assert_eq!(merged.notes.as_deref(), Some("Source notes"));
        assert!(load_source(&conn, &source_id).expect("load").is_none());
    }

    #[test]
    fn merges_a_templated_source_into_a_free_form_target() {
        let mut conn = connection();
        let target_id = save_source(
            &mut conn,
            &SourceDraft {
                title: "Target".to_string(),
                ..SourceDraft::default()
            },
        )
        .expect("target saves");
        let source_id = save_source(&mut conn, &book_draft()).expect("source saves");

        merge_sources(&mut conn, &source_id, &target_id).expect("merge succeeds");

        let merged = load_source(&conn, &target_id)
            .expect("load")
            .expect("target exists");
        assert_eq!(merged.template_key, None);
        assert_eq!(merged.title, "Target");
        assert_eq!(merged.author.as_deref(), Some("Ada King"));
        assert_eq!(
            merged.publication.as_deref(),
            Some("London: Archive Press, 1900")
        );
        let footnote = merged.footnote.as_deref().unwrap_or_default();
        assert!(footnote.contains("Family History"), "{footnote}");
        assert!(footnote.contains("Archive Press"), "{footnote}");
        assert!(!footnote.contains("{page}"), "{footnote}");
    }

    #[test]
    fn merges_different_templates_with_the_target_style() {
        let mut conn = connection();
        let target_id = save_source(&mut conn, &book_draft()).expect("target saves");
        let mut census = SourceDraft {
            template_key: Some("census".to_string()),
            ..SourceDraft::default()
        };
        census.fields.insert("year".to_string(), "1850".to_string());
        census
            .fields
            .insert("country".to_string(), "Sweden".to_string());
        let source_id = save_source(&mut conn, &census).expect("source saves");

        merge_sources(&mut conn, &source_id, &target_id).expect("merge succeeds");

        let merged = load_source(&conn, &target_id)
            .expect("load")
            .expect("target exists");
        assert_eq!(merged.template_key.as_deref(), Some("book"));
        assert_eq!(merged.title, "Family History");
        assert_eq!(
            merged.fields.get("year").map(String::as_str),
            Some("1900"),
            "the target's own fields stay unchanged"
        );
        assert!(!merged.fields.contains_key("country"));
        assert!(load_source(&conn, &source_id).expect("load").is_none());
    }

    #[test]
    fn merge_keeps_citation_links_and_quality() {
        let mut conn = connection();
        let target_id = save_source(
            &mut conn,
            &SourceDraft {
                title: "Target".to_string(),
                ..SourceDraft::default()
            },
        )
        .expect("target saves");
        let source_id = save_source(
            &mut conn,
            &SourceDraft {
                title: "Source".to_string(),
                ..SourceDraft::default()
            },
        )
        .expect("source saves");
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person insert");
        let citation_id =
            citation_store::add_citation(&mut conn, &source_id, "p1", "BIRTH", Some("12"))
                .expect("citation adds");
        citation_store::save_citation(
            &conn,
            &CitationEdit {
                id: citation_id,
                page: Some("12".to_string()),
                quality_source: Some(SourceQuality::Original),
                quality_information: None,
                quality_evidence: None,
                quality_certainty: None,
            },
        )
        .expect("quality saves");

        merge_sources(&mut conn, &source_id, &target_id).expect("merge succeeds");

        let citations = citation_store::list_person_citations(&conn, "p1").expect("citations list");
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].source_title, "Target");
        assert_eq!(citations[0].page.as_deref(), Some("12"));
        assert_eq!(citations[0].quality_source, Some(SourceQuality::Original));
    }

    #[test]
    fn merge_unions_repositories() {
        let mut conn = connection();
        let target_id = save_source(&mut conn, &book_draft()).expect("target saves");
        let source_id = save_source(&mut conn, &book_draft()).expect("source saves");
        conn.execute_batch(
            "INSERT INTO repositories (id, name) VALUES ('r1', 'Archive'), ('r2', 'Library');",
        )
        .expect("repositories insert");
        conn.execute(
            "INSERT INTO source_repositories (source_id, repository_id, call_number)
             VALUES (?1, 'r1', 'A'), (?1, 'r2', NULL)",
            params![target_id],
        )
        .expect("target repositories");
        conn.execute(
            "INSERT INTO source_repositories (source_id, repository_id, call_number)
             VALUES (?1, 'r1', 'B'), (?1, 'r2', 'C')",
            params![source_id],
        )
        .expect("source repositories");

        let report = merge_sources(&mut conn, &source_id, &target_id).expect("merge succeeds");
        assert_eq!(report.repositories, 2);

        let first: Option<String> = conn
            .query_row(
                "SELECT call_number FROM source_repositories
                 WHERE source_id = ?1 AND repository_id = 'r1'",
                params![target_id],
                |row| row.get(0),
            )
            .expect("r1");
        assert_eq!(first.as_deref(), Some("A"), "the target value wins");
        let second: Option<String> = conn
            .query_row(
                "SELECT call_number FROM source_repositories
                 WHERE source_id = ?1 AND repository_id = 'r2'",
                params![target_id],
                |row| row.get(0),
            )
            .expect("r2");
        assert_eq!(second.as_deref(), Some("C"), "the blank value is filled");
        let source_links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM source_repositories WHERE source_id = ?1",
                params![source_id],
                |row| row.get(0),
            )
            .expect("source link count");
        assert_eq!(source_links, 0);
    }

    #[test]
    fn merge_moves_media_and_task_links_without_duplicates() {
        let mut conn = connection();
        let target_id = save_source(&mut conn, &book_draft()).expect("target saves");
        let source_id = save_source(&mut conn, &book_draft()).expect("source saves");
        conn.execute_batch(
            "INSERT INTO media (id, file_path) VALUES ('m1', 'a.jpg'), ('m2', 'b.jpg');
             INSERT INTO tasks (id, title) VALUES ('t1', 'Task 1'), ('t2', 'Task 2');",
        )
        .expect("media and tasks insert");
        conn.execute(
            "INSERT INTO media_links (media_id, source_id)
             VALUES ('m1', ?1), ('m1', ?2), ('m2', ?2)",
            params![target_id, source_id],
        )
        .expect("media links insert");
        conn.execute(
            "INSERT INTO task_links (task_id, source_id)
             VALUES ('t1', ?1), ('t1', ?2), ('t2', ?2)",
            params![target_id, source_id],
        )
        .expect("task links insert");

        let report = merge_sources(&mut conn, &source_id, &target_id).expect("merge succeeds");
        assert_eq!(report.media_links, 1, "the duplicate media link is dropped");
        assert_eq!(report.task_links, 1, "the duplicate task link is dropped");

        let media: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media_links WHERE source_id = ?1",
                params![target_id],
                |row| row.get(0),
            )
            .expect("media count");
        assert_eq!(media, 2);
        let tasks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_links WHERE source_id = ?1",
                params![target_id],
                |row| row.get(0),
            )
            .expect("task count");
        assert_eq!(tasks, 2);
        let source_media: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM media_links WHERE source_id = ?1",
                params![source_id],
                |row| row.get(0),
            )
            .expect("source media count");
        assert_eq!(source_media, 0);
        let source_tasks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_links WHERE source_id = ?1",
                params![source_id],
                |row| row.get(0),
            )
            .expect("source task count");
        assert_eq!(source_tasks, 0);
    }

    #[test]
    fn merge_rejects_self_and_missing_sources() {
        let mut conn = connection();
        let id = save_source(&mut conn, &book_draft()).expect("source saves");

        assert!(merge_sources(&mut conn, &id, &id).is_err());
        assert!(merge_sources(&mut conn, "missing", &id).is_err());
        assert!(merge_sources(&mut conn, &id, "missing").is_err());
        assert!(load_source(&conn, &id).expect("load").is_some());
    }
}
