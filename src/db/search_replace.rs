//! Builds and applies global literal search-and-replace plans.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::db;
use crate::db::media::inferred_format;
use crate::db::names::{AlternateName, list_all_alternate_names};
use crate::db::places::{PlaceSummary, list_places};
use crate::model::dataclean::NameParts;
use crate::model::name::format_name;
use crate::model::search_replace::{
    MediaSource, NameSource, NoteSource, PlaceSource, ReplacePlan, ReplaceRequest, ReplaceTarget,
    build_plan,
};

/// Rows selected in the replacement preview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplaceSelection {
    pub keys: HashSet<String>,
}

impl ReplaceSelection {
    pub fn select_all(plan: &ReplacePlan) -> Self {
        Self {
            keys: plan
                .changes
                .iter()
                .map(|change| change.key.clone())
                .collect(),
        }
    }
}

/// Counts of applied replacements by record kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplaceReport {
    pub names: usize,
    pub alternate_names: usize,
    pub places: usize,
    pub notes: usize,
    pub media: usize,
}

impl ReplaceReport {
    pub fn total(&self) -> usize {
        self.names + self.alternate_names + self.places + self.notes + self.media
    }
}

/// Builds a preview plan from the active tree.
pub fn preview(
    conn: &Connection,
    request: &ReplaceRequest,
    surname_first: bool,
) -> Result<ReplacePlan, String> {
    let tree = db::load_family_tree(conn).map_err(|error| error.to_string())?;
    let alternates = list_all_alternate_names(conn)?;
    let places = list_places(conn)?;
    let notes = load_note_sources(conn)?;
    let media = db::media::list_media(conn)?;

    let mut names = Vec::new();
    for person in &tree.people {
        names.push(NameSource {
            person_id: person.id.clone(),
            alternate_id: None,
            owner: name_owner(
                &person.name_prefix,
                &person.given_name,
                &person.middle_name,
                &person.surname,
                &person.name_suffix,
                surname_first,
            ),
            parts: NameParts::from_person(person),
        });
    }
    for alternate in &alternates {
        names.push(NameSource {
            person_id: alternate.person_id.clone(),
            alternate_id: Some(alternate.id.clone()),
            owner: alternate_owner(alternate, surname_first),
            parts: NameParts::from_alternate(alternate),
        });
    }

    let places = places
        .into_iter()
        .map(|place: PlaceSummary| PlaceSource {
            id: place.id,
            name: place.name,
        })
        .collect::<Vec<_>>();
    let media = media
        .into_iter()
        .map(|media| MediaSource {
            id: media.id,
            owner: media_owner(&media.caption, &media.file_path),
            file_path: media.file_path,
        })
        .collect::<Vec<_>>();

    Ok(build_plan(request, &names, &places, &notes, &media))
}

/// Applies selected changes in one transaction.
pub fn apply_plan(
    conn: &mut Connection,
    plan: &ReplacePlan,
    selection: &ReplaceSelection,
) -> Result<ReplaceReport, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let mut report = ReplaceReport::default();

    for change in &plan.changes {
        if !selection.keys.contains(&change.key) {
            continue;
        }
        match &change.target {
            ReplaceTarget::Person { person_id, field } => {
                let column = field.column();
                let sql = format!(
                    "UPDATE people SET {column} = ?1, updated_at = datetime('now')
                     WHERE id = ?2 AND {column} = ?3"
                );
                let changed = tx
                    .execute(&sql, params![change.after, person_id, change.before])
                    .map_err(|error| error.to_string())?;
                if changed == 0 {
                    return Err(changed_since_preview("person"));
                }
                report.names += 1;
            }
            ReplaceTarget::AlternateName {
                alternate_id,
                field,
            } => {
                let column = field.column();
                let sql = format!(
                    "UPDATE alternate_names SET {column} = ?1
                     WHERE id = ?2 AND {column} = ?3"
                );
                let changed = tx
                    .execute(&sql, params![change.after, alternate_id, change.before])
                    .map_err(|error| error.to_string())?;
                if changed == 0 {
                    return Err(changed_since_preview("alternate name"));
                }
                report.alternate_names += 1;
            }
            ReplaceTarget::Place { place_id } => {
                let changed = tx
                    .execute(
                        "UPDATE places SET name = ?1, updated_at = datetime('now')
                         WHERE id = ?2 AND name = ?3",
                        params![change.after, place_id, change.before],
                    )
                    .map_err(|error| error.to_string())?;
                if changed == 0 {
                    return Err(changed_since_preview("place"));
                }
                report.places += 1;
            }
            ReplaceTarget::Note { note_id } => {
                let changed = tx
                    .execute(
                        "UPDATE notes SET body = ?1, updated_at = datetime('now')
                         WHERE id = ?2 AND body = ?3",
                        params![change.after, note_id, change.before],
                    )
                    .map_err(|error| error.to_string())?;
                if changed == 0 {
                    return Err(changed_since_preview("note"));
                }
                report.notes += 1;
            }
            ReplaceTarget::MediaPath { media_id } => {
                let current_format: Option<String> = tx
                    .query_row(
                        "SELECT coalesce(format, '') FROM media
                         WHERE id = ?1 AND file_path = ?2",
                        params![media_id, change.before],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                let Some(current_format) = current_format else {
                    return Err(changed_since_preview("media"));
                };
                let old_inferred = inferred_format(&change.before).unwrap_or_default();
                let new_inferred = inferred_format(&change.after).unwrap_or_default();
                let format = if current_format.is_empty() || current_format == old_inferred {
                    new_inferred
                } else {
                    current_format
                };
                let changed = tx
                    .execute(
                        "UPDATE media SET file_path = ?1, format = ?2
                         WHERE id = ?3 AND file_path = ?4",
                        params![
                            change.after,
                            (!format.is_empty()).then_some(format),
                            media_id,
                            change.before
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if changed == 0 {
                    return Err(changed_since_preview("media"));
                }
                report.media += 1;
            }
        }
    }

    tx.commit().map_err(|error| error.to_string())?;
    Ok(report)
}

fn changed_since_preview(kind: &str) -> String {
    format!("{kind} changed since the preview; run the preview again")
}

fn name_owner(
    prefix: &str,
    given: &str,
    middle: &str,
    surname: &str,
    suffix: &str,
    surname_first: bool,
) -> String {
    let name = format_name(prefix, given, middle, surname, suffix, surname_first);
    if name.trim().is_empty() {
        "(unnamed)".to_string()
    } else {
        name
    }
}

fn alternate_owner(alternate: &AlternateName, surname_first: bool) -> String {
    name_owner(
        &alternate.prefix,
        &alternate.given_name,
        &alternate.middle_name,
        &alternate.surname,
        &alternate.suffix,
        surname_first,
    )
}

fn media_owner(caption: &Option<String>, file_path: &str) -> String {
    caption
        .as_deref()
        .map(str::trim)
        .filter(|caption| !caption.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            Path::new(file_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(file_path)
                .to_string()
        })
}

fn load_note_sources(conn: &Connection) -> Result<Vec<NoteSource>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.body,
                    coalesce(
                        (SELECT trim(coalesce(p.name_prefix, '') || ' ' ||
                                     p.given_name || ' ' || coalesce(p.middle_name, '') || ' ' ||
                                     p.surname || ' ' || coalesce(p.name_suffix, ''))
                         FROM people p WHERE p.id = n.person_id),
                        (SELECT trim(coalesce(h.given_name, '') || ' ' || coalesce(h.surname, '') ||
                                     CASE WHEN h.id IS NOT NULL AND w.id IS NOT NULL
                                          THEN ' & ' ELSE '' END ||
                                     coalesce(w.given_name, '') || ' ' || coalesce(w.surname, ''))
                         FROM families f
                         LEFT JOIN people h ON h.id = f.husband_id
                         LEFT JOIN people w ON w.id = f.wife_id
                         WHERE f.id = n.family_id),
                        (SELECT coalesce(ft.label, e.event_type) || ' ' || coalesce(e.date, '')
                         FROM events e
                         LEFT JOIN fact_types ft ON ft.key = e.event_type
                         WHERE e.id = n.event_id),
                        (SELECT coalesce(s.title, '') || ' ' || coalesce(c.page_or_reference, '')
                         FROM citations c
                         LEFT JOIN sources s ON s.id = c.source_id
                         WHERE c.id = n.citation_id),
                        (SELECT p.name FROM places p WHERE p.id = n.place_id)
                    )
             FROM notes n ORDER BY n.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut notes = Vec::new();
    for row in rows {
        let (id, body, owner) = row.map_err(|error| error.to_string())?;
        let owner = owner
            .map(|owner| owner.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|owner| !owner.is_empty())
            .unwrap_or_else(|| "Note".to_string());
        notes.push(NoteSource { id, body, owner });
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("database");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragmas");
        init_db(&conn).expect("schema");
        conn
    }

    fn seed(conn: &Connection) {
        conn.execute_batch(
            r#"
            INSERT INTO people (id, given_name, middle_name, surname, nickname, name_prefix,
                                name_suffix, gender)
            VALUES ('p1', 'Ada Ann', '', 'King', 'Addy', 'Dr.', 'Jr.', 'F');
            INSERT INTO alternate_names (id, person_id, given_name, surname)
            VALUES ('a1', 'p1', 'Augusta Ann', 'King');
            INSERT INTO places (id, name) VALUES ('pl1', 'Ann Arbor');
            INSERT INTO notes (id, body, person_id) VALUES ('n1', 'Ann visited', 'p1');
            INSERT INTO media (id, file_path, format, caption)
            VALUES ('m1', '/photos/Ann.jpg', 'jpg', 'Ann portrait');
            "#,
        )
        .expect("seed");
    }

    fn request() -> ReplaceRequest {
        ReplaceRequest {
            find: "Ann".to_string(),
            replace: "Jane".to_string(),
            match_case: true,
            names: true,
            places: true,
            notes: true,
            filenames: true,
        }
    }

    #[test]
    fn preview_finds_names_places_notes_and_media() {
        let conn = connection();
        seed(&conn);
        let plan = preview(&conn, &request(), false).expect("plan");
        assert!(
            plan.changes
                .iter()
                .any(|change| change.key == "person:p1:given")
        );
        assert!(
            plan.changes
                .iter()
                .any(|change| change.key == "alternate:a1:given")
        );
        assert!(plan.changes.iter().any(|change| change.key == "place:pl1"));
        assert!(plan.changes.iter().any(|change| change.key == "note:n1"));
        assert!(plan.changes.iter().any(|change| change.key == "media:m1"));
    }

    #[test]
    fn apply_updates_only_selected_rows() {
        let mut conn = connection();
        seed(&conn);
        let plan = preview(&conn, &request(), false).expect("plan");
        let selection = ReplaceSelection {
            keys: HashSet::from(["person:p1:given".to_string(), "place:pl1".to_string()]),
        };
        let report = apply_plan(&mut conn, &plan, &selection).expect("apply");
        assert_eq!(report.names, 1);
        assert_eq!(report.places, 1);
        assert_eq!(report.notes, 0);

        let (given, nickname): (String, String) = conn
            .query_row(
                "SELECT given_name, nickname FROM people WHERE id = 'p1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("person");
        assert_eq!(given, "Ada Jane");
        assert_eq!(nickname, "Addy", "unselected field is untouched");
        let place: String = conn
            .query_row("SELECT name FROM places WHERE id = 'pl1'", [], |row| {
                row.get(0)
            })
            .expect("place");
        assert_eq!(place, "Jane Arbor");
        let note: String = conn
            .query_row("SELECT body FROM notes WHERE id = 'n1'", [], |row| {
                row.get(0)
            })
            .expect("note");
        assert_eq!(note, "Ann visited", "unselected note is untouched");
    }

    #[test]
    fn apply_updates_media_path_and_derived_format() {
        let mut conn = connection();
        seed(&conn);
        let plan = preview(
            &conn,
            &ReplaceRequest {
                find: "Ann.jpg".to_string(),
                replace: "Jane.png".to_string(),
                match_case: true,
                names: false,
                places: false,
                notes: false,
                filenames: true,
            },
            false,
        )
        .expect("plan");
        let report =
            apply_plan(&mut conn, &plan, &ReplaceSelection::select_all(&plan)).expect("apply");
        assert_eq!(report.media, 1);
        let (path, format): (String, Option<String>) = conn
            .query_row(
                "SELECT file_path, format FROM media WHERE id = 'm1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("media");
        assert_eq!(path, "/photos/Jane.png");
        assert_eq!(format.as_deref(), Some("png"));
    }

    #[test]
    fn stale_values_abort_the_transaction() {
        let mut conn = connection();
        seed(&conn);
        let plan = preview(&conn, &request(), false).expect("plan");
        conn.execute(
            "UPDATE notes SET body = 'changed elsewhere' WHERE id = 'n1'",
            [],
        )
        .expect("outside edit");
        let selection = ReplaceSelection::select_all(&plan);
        let error = apply_plan(&mut conn, &plan, &selection).expect_err("stale plan");
        assert!(error.contains("changed since the preview"));
        let note: String = conn
            .query_row("SELECT body FROM notes WHERE id = 'n1'", [], |row| {
                row.get(0)
            })
            .expect("note");
        assert_eq!(note, "changed elsewhere");
    }
}
