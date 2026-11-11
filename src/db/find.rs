//! Global text search across the tree's top-level record types.
//!
//! Nested text such as event details, notes, citations and associations is
//! folded into the owning person, family or source result. Searches run on
//! demand and are capped per kind so large trees stay responsive.

use std::path::Path;

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, params};

use crate::model::name::format_name;

/// Number of hits returned for one record kind.
pub const HITS_PER_KIND: usize = 25;

/// A record type represented in the Find Everywhere results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FindKind {
    #[default]
    Person,
    Family,
    Place,
    Source,
    Repository,
    Task,
    Group,
    Media,
    FactType,
}

impl FindKind {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Person => "find.kind_person",
            Self::Family => "find.kind_family",
            Self::Place => "find.kind_place",
            Self::Source => "find.kind_source",
            Self::Repository => "find.kind_repository",
            Self::Task => "find.kind_task",
            Self::Group => "find.kind_group",
            Self::Media => "find.kind_media",
            Self::FactType => "find.kind_fact_type",
        }
    }
}

/// The record to open when a result is clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindTarget {
    Person(String),
    Family(String),
    Place(String),
    Source(String),
    Repository(String),
    Task(String),
    Group(String),
    Media(String),
    FactType(String),
}

/// One clickable result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindHit {
    pub title: String,
    pub context: String,
    pub target: FindTarget,
}

/// All hits for one record type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindGroup {
    pub kind: FindKind,
    pub hits: Vec<FindHit>,
    pub truncated: bool,
}

/// Grouped search results in display order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindResults {
    pub groups: Vec<FindGroup>,
}

impl FindResults {
    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(|group| group.hits.is_empty())
    }

    pub fn total(&self) -> usize {
        self.groups.iter().map(|group| group.hits.len()).sum()
    }
}

/// Searches every supported record type, returning up to `limit` hits per kind.
pub fn find_everywhere(
    conn: &Connection,
    query: &str,
    surname_first: bool,
    limit: usize,
) -> Result<FindResults, String> {
    let needle = fold_text(query);
    if needle.is_empty() {
        return Ok(FindResults::default());
    }
    register_fold(conn)?;
    let limit = limit.max(1);
    let groups = [
        search_people(conn, &needle, surname_first, limit)?,
        search_families(conn, &needle, surname_first, limit)?,
        search_places(conn, &needle, limit)?,
        search_sources(conn, &needle, limit)?,
        search_repositories(conn, &needle, limit)?,
        search_tasks(conn, &needle, limit)?,
        search_groups(conn, &needle, limit)?,
        search_media(conn, &needle, limit)?,
        search_fact_types(conn, &needle, limit)?,
    ]
    .into_iter()
    .flatten()
    .collect();
    Ok(FindResults { groups })
}

fn register_fold(conn: &Connection) -> Result<(), String> {
    conn.create_scalar_function(
        "stemma_fold",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let value: Option<String> = context.get(0)?;
            Ok(fold_text(&value.unwrap_or_default()))
        },
    )
    .map_err(|error| error.to_string())
}

/// Lowercases text and collapses whitespace so field joins match phrases.
fn fold_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn group(kind: FindKind, hits: Vec<FindHit>, truncated: bool) -> Option<FindGroup> {
    (!hits.is_empty()).then_some(FindGroup {
        kind,
        hits,
        truncated,
    })
}

fn collect_hits<T>(
    mut rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
    limit: usize,
) -> Result<(Vec<T>, bool), String> {
    let mut values = Vec::new();
    for row in rows.by_ref() {
        if values.len() == limit {
            return Ok((values, true));
        }
        values.push(row.map_err(|error| error.to_string())?);
    }
    Ok((values, false))
}

fn context(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn search_people(
    conn: &Connection,
    needle: &str,
    surname_first: bool,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT p.id,
               p.name_prefix, p.given_name, p.middle_name, p.surname,
               p.name_suffix, p.nickname,
               (SELECT e.date FROM events e
                WHERE e.person_id = p.id AND e.event_type = 'BIRTH'
                ORDER BY e.rowid LIMIT 1),
               (SELECT e.date FROM events e
                WHERE e.person_id = p.id AND e.event_type = 'DEATH'
                ORDER BY e.rowid LIMIT 1)
        FROM people p
        WHERE instr(stemma_fold(
            coalesce(p.name_prefix, '') || ' ' ||
            coalesce(p.given_name, '') || ' ' ||
            coalesce(p.middle_name, '') || ' ' ||
            coalesce(p.surname, '') || ' ' ||
            coalesce(p.name_suffix, '') || ' ' ||
            coalesce(p.nickname, '') || ' ' ||
            coalesce((SELECT group_concat(
                        a.prefix || ' ' || a.given_name || ' ' || a.middle_name || ' ' ||
                        a.surname || ' ' || a.suffix || ' ' || a.nickname, ' ')
                      FROM alternate_names a WHERE a.person_id = p.id), '') || ' ' ||
            coalesce((SELECT group_concat(n.body, ' ')
                      FROM notes n WHERE n.person_id = p.id), '') || ' ' ||
            coalesce((SELECT group_concat(
                        e.event_type || ' ' || coalesce(ft.label, '') || ' ' ||
                        coalesce(e.date, '') || ' ' || coalesce(pl.name, '') || ' ' ||
                        coalesce(e.description, ''), ' ')
                      FROM events e
                      LEFT JOIN fact_types ft ON ft.key = e.event_type
                      LEFT JOIN places pl ON pl.id = e.place_id
                      WHERE e.person_id = p.id), '') || ' ' ||
            coalesce((SELECT group_concat(
                        ep.role_key || ' ' || coalesce(r.label, '') || ' ' ||
                        e.event_type || ' ' || coalesce(ft.label, '') || ' ' ||
                        coalesce(e.date, '') || ' ' || coalesce(pl.name, '') || ' ' ||
                        coalesce(e.description, '') || ' ' ||
                        coalesce(pp.name_prefix || ' ' || pp.given_name || ' ' ||
                                 pp.middle_name || ' ' || pp.surname, ''), ' ')
                      FROM event_participants ep
                      JOIN events e ON e.id = ep.event_id
                      LEFT JOIN fact_types ft ON ft.key = e.event_type
                      LEFT JOIN places pl ON pl.id = e.place_id
                      LEFT JOIN fact_type_roles r
                             ON r.fact_type_key = e.event_type AND r.role_key = ep.role_key
                      LEFT JOIN people pp ON pp.id = e.person_id
                      WHERE ep.person_id = p.id), '') || ' ' ||
            coalesce((SELECT group_concat(
                        c.page_or_reference || ' ' ||
                        coalesce((SELECT group_concat(cn.body, ' ')
                                  FROM notes cn WHERE cn.citation_id = c.id), '') || ' ' ||
                        coalesce(s.title, ''), ' ')
                      FROM citations c
                      JOIN citation_links cl ON cl.citation_id = c.id
                      JOIN events e ON e.id = cl.event_id
                      LEFT JOIN sources s ON s.id = c.source_id
                      WHERE e.person_id = p.id
                         OR e.family_id IN (SELECT f.id FROM families f
                                            WHERE f.husband_id = p.id OR f.wife_id = p.id)), '') || ' ' ||
            coalesce((SELECT group_concat(a.relationship, ' ')
                      FROM associations a
                      WHERE a.person_id = p.id OR a.associate_id = p.id), '')
        ), ?1) > 0
        ORDER BY p.surname COLLATE NOCASE, p.given_name COLLATE NOCASE, p.rowid
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let name = format_name(
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
                &row.get::<_, String>(4)?,
                &row.get::<_, String>(5)?,
                surname_first,
            );
            let birth: Option<String> = row.get(7)?;
            let death: Option<String> = row.get(8)?;
            Ok(FindHit {
                title: if name.trim().is_empty() {
                    "(unnamed)".to_string()
                } else {
                    name
                },
                context: context(&[
                    birth
                        .as_deref()
                        .map(|date| format!("b. {date}"))
                        .as_deref()
                        .unwrap_or(""),
                    death
                        .as_deref()
                        .map(|date| format!("d. {date}"))
                        .as_deref()
                        .unwrap_or(""),
                ]),
                target: FindTarget::Person(row.get(0)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::Person, hits, truncated))
}

fn search_families(
    conn: &Connection,
    needle: &str,
    surname_first: bool,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT f.id,
               h.name_prefix, h.given_name, h.middle_name, h.surname, h.name_suffix,
               w.name_prefix, w.given_name, w.middle_name, w.surname, w.name_suffix,
               (SELECT e.date FROM events e
                WHERE e.family_id = f.id AND e.event_type = 'MARRIAGE'
                ORDER BY e.rowid LIMIT 1),
               (SELECT pl.name FROM events e
                LEFT JOIN places pl ON pl.id = e.place_id
                WHERE e.family_id = f.id AND e.event_type = 'MARRIAGE'
                ORDER BY e.rowid LIMIT 1)
        FROM families f
        LEFT JOIN people h ON h.id = f.husband_id
        LEFT JOIN people w ON w.id = f.wife_id
        WHERE instr(stemma_fold(
            coalesce(h.name_prefix, '') || ' ' || coalesce(h.given_name, '') || ' ' ||
            coalesce(h.middle_name, '') || ' ' || coalesce(h.surname, '') || ' ' ||
            coalesce(h.name_suffix, '') || ' ' ||
            coalesce(w.name_prefix, '') || ' ' || coalesce(w.given_name, '') || ' ' ||
            coalesce(w.middle_name, '') || ' ' || coalesce(w.surname, '') || ' ' ||
            coalesce(w.name_suffix, '') || ' ' ||
            coalesce((SELECT group_concat(
                        e.event_type || ' ' || coalesce(ft.label, '') || ' ' ||
                        coalesce(e.date, '') || ' ' || coalesce(pl.name, '') || ' ' ||
                        coalesce(e.description, ''), ' ')
                      FROM events e
                      LEFT JOIN fact_types ft ON ft.key = e.event_type
                      LEFT JOIN places pl ON pl.id = e.place_id
                      WHERE e.family_id = f.id), '') || ' ' ||
            coalesce((SELECT group_concat(n.body, ' ')
                      FROM notes n WHERE n.family_id = f.id), '') || ' ' ||
            coalesce((SELECT group_concat(
                        c.page_or_reference || ' ' ||
                        coalesce((SELECT group_concat(cn.body, ' ')
                                  FROM notes cn WHERE cn.citation_id = c.id), '') || ' ' ||
                        coalesce(s.title, ''), ' ')
                      FROM citations c
                      JOIN citation_links cl ON cl.citation_id = c.id
                      JOIN events e ON e.id = cl.event_id
                      LEFT JOIN sources s ON s.id = c.source_id
                      WHERE e.family_id = f.id), '')
        ), ?1) > 0
        ORDER BY f.rowid
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let husband = optional_name(row, 1, surname_first)?;
            let wife = optional_name(row, 6, surname_first)?;
            let title = match (husband, wife) {
                (Some(husband), Some(wife)) => format!("{husband} & {wife}"),
                (Some(name), None) | (None, Some(name)) => name,
                (None, None) => "(unnamed couple)".to_string(),
            };
            let date: Option<String> = row.get(11)?;
            let place: Option<String> = row.get(12)?;
            Ok(FindHit {
                title,
                context: context(&[
                    "Couple",
                    date.as_deref().unwrap_or(""),
                    place.as_deref().unwrap_or(""),
                ]),
                target: FindTarget::Family(row.get(0)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::Family, hits, truncated))
}

fn optional_name(
    row: &rusqlite::Row<'_>,
    offset: usize,
    surname_first: bool,
) -> rusqlite::Result<Option<String>> {
    let prefix: Option<String> = row.get(offset)?;
    let given: Option<String> = row.get(offset + 1)?;
    let middle: Option<String> = row.get(offset + 2)?;
    let surname: Option<String> = row.get(offset + 3)?;
    let suffix: Option<String> = row.get(offset + 4)?;
    let name = format_name(
        prefix.as_deref().unwrap_or_default(),
        given.as_deref().unwrap_or_default(),
        middle.as_deref().unwrap_or_default(),
        surname.as_deref().unwrap_or_default(),
        suffix.as_deref().unwrap_or_default(),
        surname_first,
    );
    Ok((!name.trim().is_empty()).then_some(name))
}

fn search_places(
    conn: &Connection,
    needle: &str,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT p.id, p.name,
               (SELECT COUNT(*) FROM events e WHERE e.place_id = p.id)
        FROM places p
        WHERE instr(stemma_fold(
            coalesce(p.name, '') || ' ' ||
            coalesce((SELECT group_concat(n.body, ' ')
                      FROM notes n WHERE n.place_id = p.id), '')
        ), ?1) > 0
        ORDER BY p.name COLLATE NOCASE, p.rowid
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let name: String = row.get(1)?;
            let events: i64 = row.get(2)?;
            Ok(FindHit {
                title: if name.trim().is_empty() {
                    "(unnamed place)".to_string()
                } else {
                    name
                },
                context: format!("{events} events"),
                target: FindTarget::Place(row.get(0)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::Place, hits, truncated))
}

fn search_sources(
    conn: &Connection,
    needle: &str,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT s.id, s.title, s.author, s.publication
        FROM sources s
        WHERE instr(stemma_fold(
            coalesce(s.title, '') || ' ' || coalesce(s.author, '') || ' ' ||
            coalesce(s.publication, '') || ' ' || coalesce(s.abbreviation, '') || ' ' ||
            coalesce(s.notes, '') || ' ' || coalesce(s.data_date, '') || ' ' ||
            coalesce(s.data_text, '') || ' ' || coalesce(s.footnote, '') || ' ' ||
            coalesce(s.short_footnote, '') || ' ' || coalesce(s.bibliography, '') || ' ' ||
            coalesce((SELECT group_concat(sf.field_value, ' ')
                      FROM source_fields sf WHERE sf.source_id = s.id), '') || ' ' ||
            coalesce((SELECT group_concat(
                        c.page_or_reference || ' ' ||
                        coalesce((SELECT group_concat(cn.body, ' ')
                                  FROM notes cn WHERE cn.citation_id = c.id), ' '), ' ')
                      FROM citations c WHERE c.source_id = s.id), '')
        ), ?1) > 0
        ORDER BY s.title COLLATE NOCASE, s.rowid
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let title: String = row.get(1)?;
            let author: Option<String> = row.get(2)?;
            let publication: Option<String> = row.get(3)?;
            Ok(FindHit {
                title: if title.trim().is_empty() {
                    "(untitled source)".to_string()
                } else {
                    title
                },
                context: context(&[
                    "Source",
                    author.as_deref().unwrap_or(""),
                    publication.as_deref().unwrap_or(""),
                ]),
                target: FindTarget::Source(row.get(0)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::Source, hits, truncated))
}

fn search_repositories(
    conn: &Connection,
    needle: &str,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT r.id, r.name, r.address,
               coalesce((SELECT group_concat(
                            s.title || ' ' || coalesce(sr.call_number, '') || ' ' ||
                            coalesce(sr.media_type, ''), ' ')
                         FROM source_repositories sr
                         JOIN sources s ON s.id = sr.source_id
                         WHERE sr.repository_id = r.id), '')
        FROM repositories r
        WHERE instr(stemma_fold(
            coalesce(r.name, '') || ' ' || coalesce(r.address, '') || ' ' ||
            coalesce(r.notes, '') || ' ' ||
            coalesce((SELECT group_concat(
                        s.title || ' ' || coalesce(sr.call_number, '') || ' ' ||
                        coalesce(sr.media_type, ''), ' ')
                      FROM source_repositories sr
                      JOIN sources s ON s.id = sr.source_id
                      WHERE sr.repository_id = r.id), '')
        ), ?1) > 0
        ORDER BY r.name COLLATE NOCASE, r.rowid
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let address: Option<String> = row.get(2)?;
            Ok(FindHit {
                title: row.get(1)?,
                context: context(&["Repository", address.as_deref().unwrap_or("")]),
                target: FindTarget::Repository(row.get(0)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::Repository, hits, truncated))
}

fn search_tasks(
    conn: &Connection,
    needle: &str,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT t.id, t.title, t.status, t.folder
        FROM tasks t
        WHERE instr(stemma_fold(
            coalesce(t.title, '') || ' ' || coalesce(t.description, '') || ' ' ||
            coalesce(t.folder, '') || ' ' || coalesce(t.due_date, '')
        ), ?1) > 0
        ORDER BY CASE t.status WHEN 'open' THEN 0 ELSE 1 END,
                 t.priority IS NULL, t.priority DESC, t.rowid
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let status: String = row.get(2)?;
            let folder: Option<String> = row.get(3)?;
            Ok(FindHit {
                title: row.get(1)?,
                context: context(&[
                    if status == "done" { "Done" } else { "Open" },
                    folder.as_deref().unwrap_or(""),
                ]),
                target: FindTarget::Task(row.get(0)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::Task, hits, truncated))
}

fn search_groups(
    conn: &Connection,
    needle: &str,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT g.id, g.name,
               (SELECT COUNT(*) FROM group_members gm WHERE gm.group_id = g.id)
        FROM groups g
        WHERE instr(stemma_fold(
            coalesce(g.name, '') || ' ' || coalesce(g.description, '')
        ), ?1) > 0
        ORDER BY g.name COLLATE NOCASE, g.rowid
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let members: i64 = row.get(2)?;
            Ok(FindHit {
                title: row.get(1)?,
                context: format!("{members} people"),
                target: FindTarget::Group(row.get(0)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::Group, hits, truncated))
}

fn search_media(
    conn: &Connection,
    needle: &str,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT m.id, m.file_path, m.format, m.caption
        FROM media m
        WHERE instr(stemma_fold(
            coalesce(m.file_path, '') || ' ' || coalesce(m.format, '') || ' ' ||
            coalesce(m.caption, '') || ' ' || coalesce(m.notes, '')
        ), ?1) > 0
        ORDER BY m.caption COLLATE NOCASE, m.file_path COLLATE NOCASE, m.rowid
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let file_path: String = row.get(1)?;
            let format: Option<String> = row.get(2)?;
            let caption: Option<String> = row.get(3)?;
            let title = caption
                .as_deref()
                .map(str::trim)
                .filter(|caption| !caption.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    Path::new(&file_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(&file_path)
                        .to_string()
                });
            Ok(FindHit {
                title,
                context: context(&["Media", format.as_deref().unwrap_or(""), &file_path]),
                target: FindTarget::Media(row.get(0)?),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::Media, hits, truncated))
}

fn search_fact_types(
    conn: &Connection,
    needle: &str,
    limit: usize,
) -> Result<Option<FindGroup>, String> {
    let sql = r#"
        SELECT f.key, f.label, f.category, f.gedcom_tag,
               coalesce((SELECT group_concat(r.role_key || ' ' || r.label, ' ')
                         FROM fact_type_roles r WHERE r.fact_type_key = f.key), '')
        FROM fact_types f
        WHERE instr(stemma_fold(
            coalesce(f.key, '') || ' ' || coalesce(f.label, '') || ' ' ||
            coalesce(f.category, '') || ' ' || coalesce(f.gedcom_tag, '') || ' ' ||
            coalesce((SELECT group_concat(r.role_key || ' ' || r.label, ' ')
                      FROM fact_type_roles r WHERE r.fact_type_key = f.key), '')
        ), ?1) > 0
        ORDER BY f.label COLLATE NOCASE, f.key
        LIMIT ?2
    "#;
    let limit_plus = limit.saturating_add(1) as i64;
    let mut stmt = conn.prepare(sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![needle, limit_plus], |row| {
            let key: String = row.get(0)?;
            let category: Option<String> = row.get(2)?;
            let tag: Option<String> = row.get(3)?;
            Ok(FindHit {
                title: row.get(1)?,
                context: context(&[
                    "Fact type",
                    &key,
                    category.as_deref().unwrap_or(""),
                    tag.as_deref().unwrap_or(""),
                ]),
                target: FindTarget::FactType(key),
            })
        })
        .map_err(|error| error.to_string())?;
    let (hits, truncated) = collect_hits(rows, limit)?;
    Ok(group(FindKind::FactType, hits, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory database");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragmas");
        init_db(&conn).expect("schema");
        seed(&conn);
        conn
    }

    fn seed(conn: &Connection) {
        conn.execute_batch(
            r#"
            INSERT INTO people (id, given_name, surname, nickname, gender)
            VALUES ('p1', 'Ada', 'King', 'Addy', 'F'),
                   ('p2', 'Charles', 'Babbage', 'Charles', 'M');
            INSERT INTO alternate_names (id, person_id, given_name, surname)
            VALUES ('an1', 'p1', 'Augusta', 'King');
            INSERT INTO places (id, name)
            VALUES ('pl1', 'London'),
                   ('pl2', 'Москва');
            INSERT INTO families (id, husband_id, wife_id)
            VALUES ('f1', 'p2', 'p1');
            INSERT INTO events (id, event_type, date, place_id, description, person_id, family_id)
            VALUES ('e1', 'BIRTH', '12 JAN 1815', 'pl1', NULL, 'p1', NULL),
                   ('e2', 'DEATH', '1852', NULL, NULL, 'p1', NULL),
                   ('e3', 'OCCUPATION', '1834', 'pl1', 'Teacher at Cambridge', 'p1', NULL),
                   ('e4', 'MARRIAGE', '1840', 'pl1', NULL, NULL, 'f1'),
                   ('e5', 'EVENT', '1845', 'pl1', 'Family gathering', NULL, 'f1');
            INSERT INTO notes (id, body, person_id)
            VALUES ('n1', 'person-note-token', 'p1');
            INSERT INTO notes (id, body, family_id)
            VALUES ('n2', 'family-note-token', 'f1');
            INSERT INTO notes (id, body, place_id)
            VALUES ('n3', 'baptisms at St Mary', 'pl1'),
                   ('n4', 'Кремль', 'pl2');
            INSERT INTO sources (id, title, author, publication, notes)
            VALUES ('s1', 'Parish Records', 'St Mary', 'London', 'microfilm-token');
            INSERT INTO source_fields (source_id, field_key, field_value)
            VALUES ('s1', 'film', 'film-number-token');
            INSERT INTO repositories (id, name, address, notes)
            VALUES ('r1', 'National Archives', 'Kew', 'reading-room-token');
            INSERT INTO source_repositories (source_id, repository_id, call_number, media_type)
            VALUES ('s1', 'r1', 'CALL-77', 'microfilm');
            INSERT INTO citations (id, source_id, page_or_reference)
            VALUES ('c1', 's1', 'p. 42');
            INSERT INTO notes (id, body, citation_id)
            VALUES ('n5', 'marginal-note-token', 'c1');
            INSERT INTO citation_links (citation_id, event_id) VALUES ('c1', 'e3');
            INSERT INTO tasks (id, title, description, folder)
            VALUES ('t1', 'Trace ancestors', 'task-description-token', 'Research');
            INSERT INTO groups (id, name, description)
            VALUES ('g1', 'London families', 'group-description-token');
            INSERT INTO media (id, file_path, format, caption, notes)
            VALUES ('m1', '/photos/ada.jpg', 'jpg', 'Ada portrait', 'scan-note-token');
            INSERT INTO associations (id, person_id, associate_id, relationship)
            VALUES ('a1', 'p1', 'p2', 'Godparent');
            "#,
        )
        .expect("seed data");
    }

    fn kinds(results: &FindResults) -> Vec<FindKind> {
        results.groups.iter().map(|group| group.kind).collect()
    }

    fn group(results: &FindResults, kind: FindKind) -> Option<&FindGroup> {
        results.groups.iter().find(|group| group.kind == kind)
    }

    #[test]
    fn empty_query_returns_nothing() {
        let conn = connection();
        let results = find_everywhere(&conn, "   ", false, 25).expect("search");
        assert!(results.is_empty());
        assert_eq!(results.total(), 0);
    }

    #[test]
    fn finds_text_across_every_top_level_kind() {
        let conn = connection();

        let results = find_everywhere(&conn, "London", false, 25).expect("london");
        assert!(kinds(&results).contains(&FindKind::Person));
        assert!(kinds(&results).contains(&FindKind::Family));
        assert!(kinds(&results).contains(&FindKind::Place));
        assert!(kinds(&results).contains(&FindKind::Source));
        assert!(kinds(&results).contains(&FindKind::Group));

        assert!(
            group(
                &find_everywhere(&conn, "Augusta", false, 25).unwrap(),
                FindKind::Person
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "person-note-token", false, 25).unwrap(),
                FindKind::Person
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "Teacher at Cambridge", false, 25).unwrap(),
                FindKind::Person
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "family-note-token", false, 25).unwrap(),
                FindKind::Family
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "marginal-note-token", false, 25).unwrap(),
                FindKind::Person
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "microfilm-token", false, 25).unwrap(),
                FindKind::Source
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "reading-room-token", false, 25).unwrap(),
                FindKind::Repository
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "task-description-token", false, 25).unwrap(),
                FindKind::Task
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "group-description-token", false, 25).unwrap(),
                FindKind::Group
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "scan-note-token", false, 25).unwrap(),
                FindKind::Media
            )
            .is_some()
        );
        assert!(
            group(
                &find_everywhere(&conn, "Godparent", false, 25).unwrap(),
                FindKind::Person
            )
            .is_some()
        );

        let fact_types = find_everywhere(&conn, "Occupation", false, 25).unwrap();
        assert!(group(&fact_types, FindKind::FactType).is_some());
        assert!(group(&fact_types, FindKind::Person).is_some());
    }

    #[test]
    fn matching_is_unicode_case_insensitive() {
        let conn = connection();
        let results = find_everywhere(&conn, "МОСКВА", false, 25).expect("unicode");
        let places = group(&results, FindKind::Place).expect("place hit");
        assert_eq!(places.hits[0].title, "Москва");
        assert_eq!(places.hits[0].target, FindTarget::Place("pl2".to_string()));
    }

    #[test]
    fn results_include_titles_contexts_and_targets() {
        let conn = connection();

        let people = find_everywhere(&conn, "Ada King", false, 25).unwrap();
        let person = group(&people, FindKind::Person).expect("person");
        assert_eq!(person.hits[0].title, "Ada King");
        assert_eq!(person.hits[0].target, FindTarget::Person("p1".to_string()));
        assert!(person.hits[0].context.contains("b. 12 JAN 1815"));
        assert!(person.hits[0].context.contains("d. 1852"));

        let family = find_everywhere(&conn, "family-note-token", false, 25).unwrap();
        let family = group(&family, FindKind::Family).expect("family");
        assert_eq!(family.hits[0].target, FindTarget::Family("f1".to_string()));
        assert!(family.hits[0].title.contains("Ada King"));
        assert!(family.hits[0].title.contains("Charles Babbage"));

        let sources = find_everywhere(&conn, "Parish Records", false, 25).unwrap();
        let source = group(&sources, FindKind::Source).expect("source");
        assert_eq!(source.hits[0].title, "Parish Records");
        assert_eq!(source.hits[0].target, FindTarget::Source("s1".to_string()));

        let media = find_everywhere(&conn, "Ada portrait", false, 25).unwrap();
        let media = group(&media, FindKind::Media).expect("media");
        assert_eq!(media.hits[0].title, "Ada portrait");
        assert_eq!(media.hits[0].target, FindTarget::Media("m1".to_string()));

        let fact_types = find_everywhere(&conn, "Employment", false, 25).unwrap();
        let fact_types = group(&fact_types, FindKind::FactType).expect("fact type");
        assert_eq!(fact_types.hits[0].title, "Employment");
        assert_eq!(
            fact_types.hits[0].target,
            FindTarget::FactType("EMPLOYMENT".to_string())
        );
    }

    #[test]
    fn limits_each_kind_and_reports_truncation() {
        let conn = connection();
        conn.execute_batch(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('q1', 'LimitPerson One', 'Limit', 'U'),
                    ('q2', 'LimitPerson Two', 'Limit', 'U'),
                    ('q3', 'LimitPerson Three', 'Limit', 'U');",
        )
        .expect("extra people");

        let results = find_everywhere(&conn, "LimitPerson", false, 2).expect("limited");
        let people = group(&results, FindKind::Person).expect("people");
        assert_eq!(people.hits.len(), 2);
        assert!(people.truncated);
        assert_eq!(results.total(), 2);
    }
}
