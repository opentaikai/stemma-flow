//! Applies reviewed DataClean plans transactionally.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::dataclean::{DataCleanPlan, NameCleanup, PlaceCleanup};

/// The cleanup rows selected in the preview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataCleanSelection {
    pub names: HashSet<String>,
    pub places: HashSet<String>,
}

impl DataCleanSelection {
    pub fn select_all(plan: &DataCleanPlan) -> Self {
        Self {
            names: plan.names.iter().map(|name| name.key.clone()).collect(),
            places: plan.places.iter().map(|place| place.key.clone()).collect(),
        }
    }
}

/// Counts of applied cleanup operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataCleanReport {
    pub names_changed: usize,
    pub alternate_names_changed: usize,
    pub places_renamed: usize,
    pub places_merged: usize,
}

/// Applies selected name and place cleanups in one transaction.
pub fn apply_plan(
    conn: &mut Connection,
    plan: &DataCleanPlan,
    selection: &DataCleanSelection,
) -> Result<DataCleanReport, String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let mut report = DataCleanReport::default();

    for cleanup in &plan.names {
        if !selection.names.contains(&cleanup.key) {
            continue;
        }
        if apply_name(&tx, cleanup)? {
            if cleanup.alternate_id.is_some() {
                report.alternate_names_changed += 1;
            } else {
                report.names_changed += 1;
            }
        }
    }

    for cleanup in &plan.places {
        if !selection.places.contains(&cleanup.key) {
            continue;
        }
        let temporary = format!("__place_clean__{}", Uuid::new_v4());
        let changed = tx
            .execute(
                "UPDATE places SET name = ?1 WHERE id = ?2",
                params![temporary, cleanup.keep_id],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err(format!("no place with id {}", cleanup.keep_id));
        }
    }

    for cleanup in &plan.places {
        if !selection.places.contains(&cleanup.key) {
            continue;
        }
        let merged = apply_place(&tx, cleanup)?;
        if merged {
            report.places_merged += 1;
        } else {
            report.places_renamed += 1;
        }
    }

    tx.commit().map_err(|error| error.to_string())?;
    Ok(report)
}

fn apply_name(conn: &Connection, cleanup: &NameCleanup) -> Result<bool, String> {
    let after = &cleanup.after;
    if let Some(alternate_id) = &cleanup.alternate_id {
        let changed = conn
            .execute(
                "UPDATE alternate_names
                 SET prefix = ?1, given_name = ?2, middle_name = ?3,
                     surname = ?4, suffix = ?5, nickname = ?6
                 WHERE id = ?7",
                params![
                    after.prefix,
                    after.given_name,
                    after.middle_name,
                    after.surname,
                    after.suffix,
                    after.nickname,
                    alternate_id
                ],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err(format!("no alternate name with id {alternate_id}"));
        }
    } else {
        let changed = conn
            .execute(
                "UPDATE people
                 SET given_name = ?1, middle_name = ?2, surname = ?3,
                     nickname = ?4, name_prefix = ?5, name_suffix = ?6,
                     updated_at = datetime('now')
                 WHERE id = ?7",
                params![
                    after.given_name,
                    after.middle_name,
                    after.surname,
                    after.nickname,
                    after.prefix,
                    after.suffix,
                    cleanup.person_id
                ],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err(format!("no person with id {}", cleanup.person_id));
        }
    }
    Ok(true)
}

fn apply_place(conn: &Connection, cleanup: &PlaceCleanup) -> Result<bool, String> {
    let keep_exists: Option<String> = conn
        .query_row(
            "SELECT id FROM places WHERE id = ?1",
            params![cleanup.keep_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if keep_exists.is_none() {
        return Err(format!("no place with id {}", cleanup.keep_id));
    }

    for duplicate_id in &cleanup.merged_ids {
        let duplicate: Option<(Option<f64>, Option<f64>)> = conn
            .query_row(
                "SELECT latitude, longitude FROM places WHERE id = ?1",
                params![duplicate_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((latitude, longitude)) = duplicate else {
            return Err(format!("no place with id {duplicate_id}"));
        };
        conn.execute(
            "UPDATE places
             SET latitude = COALESCE(latitude, ?1),
                 longitude = COALESCE(longitude, ?2)
             WHERE id = ?3",
            params![latitude, longitude, cleanup.keep_id],
        )
        .map_err(|error| error.to_string())?;

        conn.execute(
            "UPDATE events SET place_id = ?1 WHERE place_id = ?2",
            params![cleanup.keep_id, duplicate_id],
        )
        .map_err(|error| error.to_string())?;
        conn.execute(
            "UPDATE notes SET place_id = ?1 WHERE place_id = ?2",
            params![cleanup.keep_id, duplicate_id],
        )
        .map_err(|error| error.to_string())?;

        conn.execute(
            "DELETE FROM media_links WHERE place_id = ?1 AND media_id IN
                 (SELECT media_id FROM media_links WHERE place_id = ?2)",
            params![duplicate_id, cleanup.keep_id],
        )
        .map_err(|error| error.to_string())?;
        conn.execute(
            "UPDATE media_links SET place_id = ?1 WHERE place_id = ?2",
            params![cleanup.keep_id, duplicate_id],
        )
        .map_err(|error| error.to_string())?;

        conn.execute(
            "DELETE FROM task_links WHERE place_id = ?1 AND task_id IN
                 (SELECT task_id FROM task_links WHERE place_id = ?2)",
            params![duplicate_id, cleanup.keep_id],
        )
        .map_err(|error| error.to_string())?;
        conn.execute(
            "UPDATE task_links SET place_id = ?1 WHERE place_id = ?2",
            params![cleanup.keep_id, duplicate_id],
        )
        .map_err(|error| error.to_string())?;

        conn.execute("DELETE FROM places WHERE id = ?1", params![duplicate_id])
            .map_err(|error| error.to_string())?;
    }

    let changed = conn
        .execute(
            "UPDATE places SET name = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![cleanup.after, cleanup.keep_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no place with id {}", cleanup.keep_id));
    }
    Ok(!cleanup.merged_ids.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::init_db;
    use crate::db::names::{AlternateNameDraft, list_all_alternate_names, save_alternate_name};
    use crate::db::places::{PlaceDraft, list_places, load_place, save_place};
    use crate::model::dataclean::{build_plan, normalize_name_text};

    fn connection() -> Connection {
        let conn = Connection::open_in_memory().expect("database");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragmas");
        init_db(&conn).expect("schema");
        conn
    }

    fn add_person(conn: &Connection, id: &str, given: &str, surname: &str) {
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES (?1, ?2, ?3, 'U')",
            params![id, given, surname],
        )
        .expect("person");
    }

    #[test]
    fn applies_selected_names_and_alternate_names_only() {
        let mut conn = connection();
        add_person(&conn, "p1", "ADA", "KING");
        add_person(&conn, "p2", "GRACE", "HOPPER");
        let alternate_id = save_alternate_name(
            &mut conn,
            "p1",
            &AlternateNameDraft {
                name_type: "aka".to_string(),
                given_name: "AUGUSTA".to_string(),
                surname: "KING".to_string(),
                ..AlternateNameDraft::default()
            },
        )
        .expect("alternate");
        let tree = db::load_family_tree(&conn).expect("tree");
        let alternates = list_all_alternate_names(&conn).expect("alternates");
        let plan = build_plan(&tree.people, &alternates, &[]);
        let selection = DataCleanSelection {
            names: HashSet::from(["person:p1".to_string(), format!("alternate:{alternate_id}")]),
            places: HashSet::new(),
        };
        let report = apply_plan(&mut conn, &plan, &selection).expect("apply");
        assert_eq!(report.names_changed, 1);
        assert_eq!(report.alternate_names_changed, 1);
        let names: (String, String) = conn
            .query_row(
                "SELECT given_name, surname FROM people WHERE id='p1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("person");
        assert_eq!(names, ("Ada".to_string(), "King".to_string()));
        let other: String = conn
            .query_row("SELECT given_name FROM people WHERE id='p2'", [], |row| {
                row.get(0)
            })
            .expect("other");
        assert_eq!(other, "GRACE", "unselected person is untouched");
        let alternate: (String, String) = conn
            .query_row(
                "SELECT given_name, surname FROM alternate_names WHERE id=?1",
                params![alternate_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("alternate");
        assert_eq!(alternate, ("Augusta".to_string(), "King".to_string()));
    }

    #[test]
    fn merges_place_variants_and_moves_all_links() {
        let mut conn = connection();
        add_person(&conn, "p1", "Ada", "King");
        let _keep = save_place(
            &mut conn,
            &PlaceDraft {
                name: "PARIS".to_string(),
                latitude: Some(48.8),
                longitude: Some(2.3),
                ..PlaceDraft::default()
            },
        )
        .expect("keep place");
        let duplicate = save_place(
            &mut conn,
            &PlaceDraft {
                name: "Paris ".to_string(),
                ..PlaceDraft::default()
            },
        )
        .expect("duplicate place");
        conn.execute(
            "INSERT INTO events (id, event_type, place_id, person_id)
             VALUES ('e1', 'BIRTH', ?1, 'p1')",
            params![duplicate],
        )
        .expect("event");
        conn.execute(
            "INSERT INTO notes (id, body, place_id) VALUES ('n1', 'Note', ?1)",
            params![duplicate],
        )
        .expect("note");
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m1', 'photo.jpg')",
            [],
        )
        .expect("media");
        conn.execute(
            "INSERT INTO media_links (media_id, place_id) VALUES ('m1', ?1)",
            params![duplicate],
        )
        .expect("media link");
        conn.execute(
            "INSERT INTO tasks (id, title) VALUES ('t1', 'Research')",
            [],
        )
        .expect("task");
        conn.execute(
            "INSERT INTO task_links (task_id, place_id) VALUES ('t1', ?1)",
            params![duplicate],
        )
        .expect("task link");

        let tree = db::load_family_tree(&conn).expect("tree");
        let places = list_places(&conn).expect("places");
        let plan = build_plan(&tree.people, &[], &places);
        assert_eq!(plan.places.len(), 1);
        assert_eq!(plan.places[0].event_count, 1, "group event count");
        let target = plan.places[0].keep_id.clone();
        let report = apply_plan(
            &mut conn,
            &plan,
            &DataCleanSelection {
                names: HashSet::new(),
                places: HashSet::from([plan.places[0].key.clone()]),
            },
        )
        .expect("apply");
        assert_eq!(report.places_merged, 1);
        let places = list_places(&conn).expect("places");
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].id, target);
        assert_eq!(places[0].name, "Paris");
        assert_eq!(places[0].event_count, 1);
        let event_place: String = conn
            .query_row("SELECT place_id FROM events WHERE id='e1'", [], |row| {
                row.get(0)
            })
            .expect("event");
        assert_eq!(event_place, target);
        let note_place: String = conn
            .query_row("SELECT place_id FROM notes WHERE id='n1'", [], |row| {
                row.get(0)
            })
            .expect("note");
        assert_eq!(note_place, target);
        let media_place: String = conn
            .query_row(
                "SELECT place_id FROM media_links WHERE media_id='m1'",
                [],
                |row| row.get(0),
            )
            .expect("media");
        assert_eq!(media_place, target);
        let task_place: String = conn
            .query_row(
                "SELECT place_id FROM task_links WHERE task_id='t1'",
                [],
                |row| row.get(0),
            )
            .expect("task");
        assert_eq!(task_place, target);
        let (lat, lon): (Option<f64>, Option<f64>) = conn
            .query_row(
                "SELECT latitude, longitude FROM places WHERE id=?1",
                params![target],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("coords");
        assert_eq!((lat, lon), (Some(48.8), Some(2.3)));
    }

    #[test]
    fn renames_cross_groups_without_collisions() {
        let mut conn = connection();
        let alpha = save_place(
            &mut conn,
            &PlaceDraft {
                name: "Alpha".to_string(),
                ..PlaceDraft::default()
            },
        )
        .expect("alpha saves");
        let beta = save_place(
            &mut conn,
            &PlaceDraft {
                name: "Beta".to_string(),
                ..PlaceDraft::default()
            },
        )
        .expect("beta saves");

        let plan = DataCleanPlan {
            names: Vec::new(),
            places: vec![
                PlaceCleanup {
                    key: "alpha-to-beta".to_string(),
                    keep_id: alpha.clone(),
                    merged_ids: Vec::new(),
                    before_names: vec!["Alpha".to_string()],
                    after: "Beta".to_string(),
                    event_count: 0,
                },
                PlaceCleanup {
                    key: "beta-to-gamma".to_string(),
                    keep_id: beta.clone(),
                    merged_ids: Vec::new(),
                    before_names: vec!["Beta".to_string()],
                    after: "Gamma".to_string(),
                    event_count: 0,
                },
            ],
        };
        let report = apply_plan(&mut conn, &plan, &DataCleanSelection::select_all(&plan))
            .expect("cross-group renames apply");
        assert_eq!(report.places_renamed, 2);
        assert_eq!(
            load_place(&conn, &alpha)
                .expect("load")
                .expect("alpha exists")
                .name,
            "Beta"
        );
        assert_eq!(
            load_place(&conn, &beta)
                .expect("load")
                .expect("beta exists")
                .name,
            "Gamma"
        );
    }

    #[test]
    fn normalization_and_report_counts_are_stable() {
        assert_eq!(normalize_name_text(" ADA  KING "), "Ada King");
        let mut conn = connection();
        add_person(&conn, "p1", "Ada", "King");
        let tree = db::load_family_tree(&conn).expect("tree");
        let plan = build_plan(&tree.people, &[], &[]);
        assert!(plan.is_empty());
        let report =
            apply_plan(&mut conn, &plan, &DataCleanSelection::select_all(&plan)).expect("noop");
        assert_eq!(report, DataCleanReport::default());
    }
}
