//! Permanent, field-by-field merging of duplicate people.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::db::people::{self, PersonDetails};

/// One person as shown in the merge comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePerson {
    pub id: String,
    pub details: PersonDetails,
    pub private: bool,
    pub living_override: Option<bool>,
    pub counts: MergeCounts,
}

/// Related records attached to one side of the comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeCounts {
    pub alternate_names: i64,
    pub events: i64,
    pub notes: i64,
    pub media: i64,
    pub tasks: i64,
    pub groups: i64,
    pub associations: i64,
    pub spouse_families: i64,
    pub parent_families: i64,
}

/// Family records affected by a merge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FamilyImpact {
    pub spouse_families: usize,
    pub parent_families: usize,
    pub child_links: usize,
}

/// Reasons a pair cannot be merged safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeBlocker {
    SamePerson,
    DirectParentChild,
    Spouses,
}

impl MergeBlocker {
    pub fn message(self) -> &'static str {
        match self {
            Self::SamePerson => "choose two different people",
            Self::DirectParentChild => {
                "these people are directly linked as parent and child; correct that link first"
            }
            Self::Spouses => "these people are recorded as spouses; correct that link first",
        }
    }
}

/// Both sides of a merge comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePreview {
    pub keep: MergePerson,
    pub merge: MergePerson,
    pub family_impact: FamilyImpact,
    pub blocked: Option<MergeBlocker>,
}

/// The composed values and options to merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeDraft {
    pub keep_id: String,
    pub merge_id: String,
    pub details: PersonDetails,
    pub private: bool,
    pub living_override: Option<bool>,
    pub preserve_primary_names: bool,
}

/// Counts of the records moved by a merge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub events_moved: usize,
    pub families_merged: usize,
    pub notes_moved: usize,
    pub media_moved: usize,
    pub tasks_moved: usize,
    pub groups_moved: usize,
    pub associations_moved: usize,
    pub alternate_names_moved: usize,
}

/// Loads both profiles and their related-record counts.
pub fn preview_merge(
    conn: &Connection,
    keep_id: &str,
    merge_id: &str,
) -> Result<MergePreview, String> {
    if keep_id == merge_id {
        return Err(MergeBlocker::SamePerson.message().to_string());
    }
    let keep =
        load_merge_person(conn, keep_id)?.ok_or_else(|| format!("no person with id {keep_id}"))?;
    let merge = load_merge_person(conn, merge_id)?
        .ok_or_else(|| format!("no person with id {merge_id}"))?;
    let family_impact = FamilyImpact {
        spouse_families: keep
            .counts
            .spouse_families
            .max(merge.counts.spouse_families) as usize,
        parent_families: keep
            .counts
            .parent_families
            .max(merge.counts.parent_families) as usize,
        child_links: merge.counts.parent_families as usize,
    };
    let blocked = direct_blocker(conn, keep_id, merge_id)?;
    Ok(MergePreview {
        keep,
        merge,
        family_impact,
        blocked,
    })
}

/// Merges `merge_id` into `keep_id` in one transaction.
pub fn merge_people(conn: &mut Connection, draft: &MergeDraft) -> Result<MergeReport, String> {
    if draft.keep_id == draft.merge_id {
        return Err(MergeBlocker::SamePerson.message().to_string());
    }
    let mut report = MergeReport::default();
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    if !person_exists(&tx, &draft.keep_id)? {
        return Err(format!("no person with id {}", draft.keep_id));
    }
    if !person_exists(&tx, &draft.merge_id)? {
        return Err(format!("no person with id {}", draft.merge_id));
    }
    if let Some(blocked) = direct_blocker(&tx, &draft.keep_id, &draft.merge_id)? {
        return Err(blocked.message().to_string());
    }
    let details = people::normalize_named(&draft.details)?;
    let keep_original = people::load_person_details(&tx, &draft.keep_id)?
        .ok_or_else(|| format!("no person with id {}", draft.keep_id))?;
    let merge_original = people::load_person_details(&tx, &draft.merge_id)?
        .ok_or_else(|| format!("no person with id {}", draft.merge_id))?;

    tx.execute(
        "UPDATE people
         SET given_name = ?1, middle_name = ?2, surname = ?3, nickname = ?4,
             name_prefix = ?5, name_suffix = ?6, gender = ?7,
             private = ?8, living_override = ?9, updated_at = datetime('now')
         WHERE id = ?10",
        params![
            details.given_name,
            details.middle_name,
            details.surname,
            details.nickname,
            details.name_prefix,
            details.name_suffix,
            details.gender,
            if draft.private { 1 } else { 0 },
            draft.living_override.map(|value| if value { 1 } else { 0 }),
            draft.keep_id,
        ],
    )
    .map_err(|error| error.to_string())?;

    report.events_moved += merge_vital(
        &tx,
        &draft.keep_id,
        &draft.merge_id,
        "BIRTH",
        details.birth_date.as_deref(),
        details.birth_place.as_deref(),
    )?;
    report.events_moved += merge_vital(
        &tx,
        &draft.keep_id,
        &draft.merge_id,
        "DEATH",
        details.death_date.as_deref(),
        details.death_place.as_deref(),
    )?;
    report.events_moved += move_other_events(&tx, &draft.keep_id, &draft.merge_id)?;

    report.families_merged = merge_families(&tx, &draft.keep_id, &draft.merge_id)?;

    report.notes_moved = moved_count(
        &tx,
        "UPDATE notes SET person_id = ?1 WHERE person_id = ?2",
        &draft.keep_id,
        &draft.merge_id,
    )?;
    report.media_moved = move_media_links(&tx, &draft.keep_id, &draft.merge_id)?;
    report.tasks_moved = move_task_links(&tx, &draft.keep_id, &draft.merge_id)?;
    report.groups_moved = move_groups(&tx, &draft.keep_id, &draft.merge_id)?;
    report.associations_moved = move_associations(&tx, &draft.keep_id, &draft.merge_id)?;
    move_event_associations(&tx, &draft.keep_id, &draft.merge_id)?;
    move_event_participants(&tx, &draft.keep_id, &draft.merge_id)?;

    report.alternate_names_moved = move_alternate_names(&tx, &draft.keep_id, &draft.merge_id)?;
    if draft.preserve_primary_names {
        add_primary_as_alternate(&tx, &draft.keep_id, &keep_original, &details)?;
        add_primary_as_alternate(&tx, &draft.keep_id, &merge_original, &details)?;
    }

    tx.execute(
        "UPDATE tree_settings SET value = ?1
         WHERE key = 'root_person_id' AND value = ?2",
        params![draft.keep_id, draft.merge_id],
    )
    .map_err(|error| error.to_string())?;

    let deleted = tx
        .execute("DELETE FROM people WHERE id = ?1", params![draft.merge_id])
        .map_err(|error| error.to_string())?;
    if deleted == 0 {
        return Err(format!("no person with id {}", draft.merge_id));
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(report)
}

fn load_merge_person(conn: &Connection, person_id: &str) -> Result<Option<MergePerson>, String> {
    let flags: Option<(i64, Option<i64>)> = conn
        .query_row(
            "SELECT private, living_override FROM people WHERE id = ?1",
            params![person_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((private, living_override)) = flags else {
        return Ok(None);
    };
    let details = people::load_person_details(conn, person_id)?
        .ok_or_else(|| format!("no person with id {person_id}"))?;
    let counts = MergeCounts {
        alternate_names: count(
            conn,
            "SELECT COUNT(*) FROM alternate_names WHERE person_id = ?1",
            person_id,
        )?,
        events: count(
            conn,
            "SELECT COUNT(*) FROM events WHERE person_id = ?1",
            person_id,
        )?,
        notes: count(
            conn,
            "SELECT COUNT(*) FROM notes WHERE person_id = ?1",
            person_id,
        )?,
        media: count(
            conn,
            "SELECT COUNT(*) FROM media_links WHERE person_id = ?1",
            person_id,
        )?,
        tasks: count(
            conn,
            "SELECT COUNT(*) FROM task_links WHERE person_id = ?1",
            person_id,
        )?,
        groups: count(
            conn,
            "SELECT COUNT(*) FROM group_members WHERE person_id = ?1",
            person_id,
        )?,
        associations: count(
            conn,
            "SELECT COUNT(*) FROM associations WHERE person_id = ?1 OR associate_id = ?1",
            person_id,
        )?,
        spouse_families: count(
            conn,
            "SELECT COUNT(*) FROM families WHERE husband_id = ?1 OR wife_id = ?1",
            person_id,
        )?,
        parent_families: count(
            conn,
            "SELECT COUNT(*) FROM family_children WHERE child_id = ?1",
            person_id,
        )?,
    };
    Ok(Some(MergePerson {
        id: person_id.to_string(),
        details,
        private: private != 0,
        living_override: living_override.map(|value| value != 0),
        counts,
    }))
}

fn count(conn: &Connection, sql: &str, person_id: &str) -> Result<i64, String> {
    conn.query_row(sql, params![person_id], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn person_exists(conn: &Connection, person_id: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM people WHERE id = ?1",
        params![person_id],
        |_| Ok(()),
    )
    .optional()
    .map(|found| found.is_some())
    .map_err(|error| error.to_string())
}

fn direct_blocker(
    conn: &Connection,
    keep_id: &str,
    merge_id: &str,
) -> Result<Option<MergeBlocker>, String> {
    let parent_child: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM family_children fc
             JOIN families f ON f.id = fc.family_id
             WHERE ((f.husband_id = ?1 OR f.wife_id = ?1) AND fc.child_id = ?2)
                OR ((f.husband_id = ?2 OR f.wife_id = ?2) AND fc.child_id = ?1)
             LIMIT 1",
            params![keep_id, merge_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if parent_child.is_some() {
        return Ok(Some(MergeBlocker::DirectParentChild));
    }
    let spouses: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM families
             WHERE (husband_id = ?1 AND wife_id = ?2)
                OR (husband_id = ?2 AND wife_id = ?1)
             LIMIT 1",
            params![keep_id, merge_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(spouses.map(|_| MergeBlocker::Spouses))
}

type FamilyKey = (Option<String>, Option<String>);

#[derive(Debug)]
struct FamilyRow {
    id: String,
    husband_id: Option<String>,
    wife_id: Option<String>,
}

fn load_family(conn: &Connection, family_id: &str) -> Result<FamilyRow, String> {
    conn.query_row(
        "SELECT id, husband_id, wife_id FROM families WHERE id = ?1",
        params![family_id],
        |row| {
            Ok(FamilyRow {
                id: row.get(0)?,
                husband_id: row.get(1)?,
                wife_id: row.get(2)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

fn merge_vital(
    conn: &Connection,
    keep_id: &str,
    merge_id: &str,
    event_type: &str,
    date: Option<&str>,
    place: Option<&str>,
) -> Result<usize, String> {
    let keep_events = person_events_of_type(conn, keep_id, event_type)?;
    let merge_events = person_events_of_type(conn, merge_id, event_type)?;
    let mut moved = 0;
    let canonical = if let Some(first) = keep_events.first().cloned() {
        Some(first)
    } else if let Some(first) = merge_events.first().cloned() {
        conn.execute(
            "UPDATE events SET person_id = ?1 WHERE id = ?2",
            params![keep_id, first],
        )
        .map_err(|error| error.to_string())?;
        moved += 1;
        Some(first)
    } else {
        None
    };
    if let Some(canonical) = canonical.as_deref() {
        for event_id in keep_events
            .iter()
            .chain(merge_events.iter())
            .filter(|event_id| event_id.as_str() != canonical)
        {
            merge_event_links(conn, canonical, event_id)?;
            moved += 1;
        }
        let place_id = people::resolve_place(conn, place).map_err(|error| error.to_string())?;
        conn.execute(
            "UPDATE events SET date = ?1, place_id = ?2 WHERE id = ?3",
            params![date, place_id.as_deref(), canonical],
        )
        .map_err(|error| error.to_string())?;
    } else if date.is_some() || place.is_some() {
        let event_id = Uuid::new_v4().to_string();
        let place_id = people::resolve_place(conn, place).map_err(|error| error.to_string())?;
        conn.execute(
            "INSERT INTO events (id, event_type, date, place_id, person_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![event_id, event_type, date, place_id.as_deref(), keep_id],
        )
        .map_err(|error| error.to_string())?;
        moved += 1;
    }
    Ok(moved)
}

fn person_events_of_type(
    conn: &Connection,
    person_id: &str,
    event_type: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM events
             WHERE person_id = ?1 AND event_type = ?2
             ORDER BY rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id, event_type], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<String>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn move_other_events(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<usize, String> {
    let events = event_rows_for_person(conn, merge_id)?;
    let mut moved = 0;
    for event in events {
        if event.event_type == "BIRTH" || event.event_type == "DEATH" {
            continue;
        }
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM events
                 WHERE person_id = ?1 AND event_type = ?2
                   AND date IS ?3 AND place_id IS ?4 AND description IS ?5
                 ORDER BY rowid LIMIT 1",
                params![
                    keep_id,
                    event.event_type,
                    event.date,
                    event.place_id,
                    event.description
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        match existing {
            Some(existing) => merge_event_links(conn, &existing, &event.id)?,
            None => {
                conn.execute(
                    "UPDATE events SET person_id = ?1 WHERE id = ?2",
                    params![keep_id, event.id],
                )
                .map_err(|error| error.to_string())?;
            }
        }
        moved += 1;
    }
    Ok(moved)
}

#[derive(Debug)]
struct EventRow {
    id: String,
    event_type: String,
    date: Option<String>,
    place_id: Option<String>,
    description: Option<String>,
}

fn event_rows_for_person(conn: &Connection, person_id: &str) -> Result<Vec<EventRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, event_type, date, place_id, description
             FROM events WHERE person_id = ?1 ORDER BY rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| {
            Ok(EventRow {
                id: row.get(0)?,
                event_type: row.get(1)?,
                date: row.get(2)?,
                place_id: row.get(3)?,
                description: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Moves every link from `discard_event` onto `keep_event`, then deletes it.
fn merge_event_links(
    conn: &Connection,
    keep_event: &str,
    discard_event: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE citations SET event_id = ?1 WHERE event_id = ?2",
        params![keep_event, discard_event],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO citation_links (citation_id, event_id)
         SELECT citation_id, ?1 FROM citation_links WHERE event_id = ?2",
        params![keep_event, discard_event],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM citation_links WHERE event_id = ?1",
        params![discard_event],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "DELETE FROM media_links WHERE event_id = ?1 AND media_id IN
             (SELECT media_id FROM media_links WHERE event_id = ?2)",
        params![discard_event, keep_event],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE media_links SET event_id = ?1 WHERE event_id = ?2",
        params![keep_event, discard_event],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "DELETE FROM task_links WHERE event_id = ?1 AND task_id IN
             (SELECT task_id FROM task_links WHERE event_id = ?2)",
        params![discard_event, keep_event],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE task_links SET event_id = ?1 WHERE event_id = ?2",
        params![keep_event, discard_event],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "UPDATE notes SET event_id = ?1 WHERE event_id = ?2",
        params![keep_event, discard_event],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "INSERT OR IGNORE INTO event_associations
         (id, event_id, associate_id, relationship, created_at, updated_at)
         SELECT id, ?1, associate_id, relationship, created_at, updated_at
         FROM event_associations WHERE event_id = ?2",
        params![keep_event, discard_event],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM event_associations WHERE event_id = ?1",
        params![discard_event],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "INSERT OR IGNORE INTO event_participants
         (event_id, person_id, role_key, sort_order)
         SELECT ?1, person_id, role_key, sort_order
         FROM event_participants WHERE event_id = ?2",
        params![keep_event, discard_event],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM event_participants WHERE event_id = ?1",
        params![discard_event],
    )
    .map_err(|error| error.to_string())?;

    conn.execute("DELETE FROM events WHERE id = ?1", params![discard_event])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn merge_families(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<usize, String> {
    let mut merged = 0;
    let families = families_with_spouse(conn, merge_id)?;
    for family in families {
        let husband_id = if family.husband_id.as_deref() == Some(merge_id) {
            if family.wife_id.as_deref() == Some(keep_id) {
                None
            } else {
                Some(keep_id.to_string())
            }
        } else {
            family.husband_id.clone()
        };
        let wife_id = if family.wife_id.as_deref() == Some(merge_id) {
            if family.husband_id.as_deref() == Some(keep_id) {
                None
            } else {
                Some(keep_id.to_string())
            }
        } else {
            family.wife_id.clone()
        };
        conn.execute(
            "UPDATE families SET husband_id = ?1, wife_id = ?2 WHERE id = ?3",
            params![husband_id, wife_id, family.id],
        )
        .map_err(|error| error.to_string())?;
    }

    merged += consolidate_duplicate_families(conn, keep_id, FamilyRole::Spouse)?;

    let child_links = child_families(conn, merge_id)?;
    for family_id in child_links {
        let already_child: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM family_children WHERE family_id = ?1 AND child_id = ?2",
                params![family_id, keep_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if already_child.is_none() {
            conn.execute(
                "UPDATE family_children SET child_id = ?1
                 WHERE family_id = ?2 AND child_id = ?3",
                params![keep_id, family_id, merge_id],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    merged += consolidate_duplicate_families(conn, keep_id, FamilyRole::Child)?;

    conn.execute(
        "DELETE FROM families
         WHERE husband_id IS NULL AND wife_id IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM family_children WHERE family_id = families.id
           )",
        [],
    )
    .map_err(|error| error.to_string())?;
    Ok(merged)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FamilyRole {
    Spouse,
    Child,
}

fn families_with_spouse(conn: &Connection, person_id: &str) -> Result<Vec<FamilyRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, husband_id, wife_id FROM families
             WHERE husband_id = ?1 OR wife_id = ?1 ORDER BY rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| {
            Ok(FamilyRow {
                id: row.get(0)?,
                husband_id: row.get(1)?,
                wife_id: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn child_families(conn: &Connection, person_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT family_id FROM family_children
             WHERE child_id = ?1 ORDER BY rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<String>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn consolidate_duplicate_families(
    conn: &Connection,
    person_id: &str,
    role: FamilyRole,
) -> Result<usize, String> {
    let families = match role {
        FamilyRole::Spouse => families_with_spouse(conn, person_id)?,
        FamilyRole::Child => {
            let mut families = Vec::new();
            for family_id in child_families(conn, person_id)? {
                families.push(load_family(conn, &family_id)?);
            }
            families
        }
    };
    let mut groups: Vec<(FamilyKey, Vec<FamilyRow>)> = Vec::new();
    for family in families {
        if role == FamilyRole::Spouse && (family.husband_id.is_none() || family.wife_id.is_none()) {
            continue;
        }
        let mut key = (family.husband_id.clone(), family.wife_id.clone());
        if key.0 > key.1 {
            std::mem::swap(&mut key.0, &mut key.1);
        }
        match groups.iter_mut().find(|(known, _)| *known == key) {
            Some((_, members)) => members.push(family),
            None => groups.push((key, vec![family])),
        }
    }
    let mut merged = 0;
    for (_, members) in groups {
        let Some((keep, rest)) = members.split_first() else {
            continue;
        };
        for discard in rest {
            merge_family(conn, &keep.id, &discard.id)?;
            merged += 1;
        }
    }
    Ok(merged)
}

fn merge_family(conn: &Connection, keep_id: &str, discard_id: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO family_children (family_id, child_id)
         SELECT ?1, child_id FROM family_children WHERE family_id = ?2",
        params![keep_id, discard_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM family_children WHERE family_id = ?1",
        params![discard_id],
    )
    .map_err(|error| error.to_string())?;

    let events = family_events(conn, discard_id)?;
    for event in events {
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM events
                 WHERE family_id = ?1 AND event_type = ?2
                   AND date IS ?3 AND place_id IS ?4 AND description IS ?5
                 ORDER BY rowid LIMIT 1",
                params![
                    keep_id,
                    event.event_type,
                    event.date,
                    event.place_id,
                    event.description
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        match existing {
            Some(existing) => merge_event_links(conn, &existing, &event.id)?,
            None => {
                conn.execute(
                    "UPDATE events SET family_id = ?1 WHERE id = ?2",
                    params![keep_id, event.id],
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }

    conn.execute(
        "UPDATE notes SET family_id = ?1 WHERE family_id = ?2",
        params![keep_id, discard_id],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "DELETE FROM media_links WHERE family_id = ?1 AND media_id IN
             (SELECT media_id FROM media_links WHERE family_id = ?2)",
        params![discard_id, keep_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE media_links SET family_id = ?1 WHERE family_id = ?2",
        params![keep_id, discard_id],
    )
    .map_err(|error| error.to_string())?;

    conn.execute(
        "DELETE FROM task_links WHERE family_id = ?1 AND task_id IN
             (SELECT task_id FROM task_links WHERE family_id = ?2)",
        params![discard_id, keep_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "UPDATE task_links SET family_id = ?1 WHERE family_id = ?2",
        params![keep_id, discard_id],
    )
    .map_err(|error| error.to_string())?;

    conn.execute("DELETE FROM families WHERE id = ?1", params![discard_id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn family_events(conn: &Connection, family_id: &str) -> Result<Vec<EventRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, event_type, date, place_id, description
             FROM events WHERE family_id = ?1 ORDER BY rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![family_id], |row| {
            Ok(EventRow {
                id: row.get(0)?,
                event_type: row.get(1)?,
                date: row.get(2)?,
                place_id: row.get(3)?,
                description: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn moved_count(
    conn: &Connection,
    sql: &str,
    keep_id: &str,
    merge_id: &str,
) -> Result<usize, String> {
    conn.execute(sql, params![keep_id, merge_id])
        .map_err(|error| error.to_string())
}

fn move_media_links(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM media_links WHERE person_id = ?1 AND media_id IN
             (SELECT media_id FROM media_links WHERE person_id = ?2)",
        params![merge_id, keep_id],
    )
    .map_err(|error| error.to_string())?;
    moved_count(
        conn,
        "UPDATE media_links SET person_id = ?1 WHERE person_id = ?2",
        keep_id,
        merge_id,
    )
}

fn move_task_links(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM task_links WHERE person_id = ?1 AND task_id IN
             (SELECT task_id FROM task_links WHERE person_id = ?2)",
        params![merge_id, keep_id],
    )
    .map_err(|error| error.to_string())?;
    moved_count(
        conn,
        "UPDATE task_links SET person_id = ?1 WHERE person_id = ?2",
        keep_id,
        merge_id,
    )
}

fn move_groups(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<usize, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM group_members WHERE person_id = ?1",
            params![merge_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO group_members (group_id, person_id)
         SELECT group_id, ?1 FROM group_members WHERE person_id = ?2",
        params![keep_id, merge_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM group_members WHERE person_id = ?1",
        params![merge_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(count as usize)
}

fn move_associations(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<usize, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM associations
             WHERE person_id = ?1 OR associate_id = ?1",
            params![merge_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT OR IGNORE INTO associations
         (id, person_id, associate_id, relationship, created_at, updated_at)
         SELECT lower(hex(randomblob(16))),
                CASE WHEN person_id = ?2 THEN ?1 ELSE person_id END,
                CASE WHEN associate_id = ?2 THEN ?1 ELSE associate_id END,
                relationship, created_at, updated_at
         FROM associations
         WHERE (person_id = ?2 OR associate_id = ?2)
           AND (CASE WHEN person_id = ?2 THEN ?1 ELSE person_id END)
            <> (CASE WHEN associate_id = ?2 THEN ?1 ELSE associate_id END)",
        params![keep_id, merge_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM associations WHERE person_id = ?1 OR associate_id = ?1",
        params![merge_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(count as usize)
}

fn move_event_associations(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO event_associations
         (id, event_id, associate_id, relationship, created_at, updated_at)
         SELECT lower(hex(randomblob(16))), event_id, ?1, relationship,
                created_at, updated_at
         FROM event_associations WHERE associate_id = ?2",
        params![keep_id, merge_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM event_associations WHERE associate_id = ?1",
        params![merge_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM event_associations
         WHERE associate_id = ?1 AND event_id IN
             (SELECT id FROM events WHERE person_id = ?1)",
        params![keep_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn move_event_participants(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO event_participants
         (event_id, person_id, role_key, sort_order)
         SELECT event_id, ?1, role_key, sort_order
         FROM event_participants WHERE person_id = ?2",
        params![keep_id, merge_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM event_participants WHERE person_id = ?1",
        params![merge_id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM event_participants
         WHERE person_id = ?1 AND event_id IN
             (SELECT id FROM events WHERE person_id = ?1)",
        params![keep_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn move_alternate_names(conn: &Connection, keep_id: &str, merge_id: &str) -> Result<usize, String> {
    let moved = moved_count(
        conn,
        "UPDATE alternate_names SET person_id = ?1 WHERE person_id = ?2",
        keep_id,
        merge_id,
    )?;
    conn.execute(
        "DELETE FROM alternate_names
         WHERE person_id = ?1 AND rowid NOT IN (
             SELECT MIN(rowid) FROM alternate_names
             WHERE person_id = ?1
             GROUP BY name_type, prefix, given_name, middle_name,
                      surname, suffix, nickname
         )",
        params![keep_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(moved)
}

fn add_primary_as_alternate(
    conn: &Connection,
    person_id: &str,
    original: &PersonDetails,
    final_details: &PersonDetails,
) -> Result<(), String> {
    let original_name = crate::model::name::full_name(
        &original.name_prefix,
        &original.given_name,
        &original.middle_name,
        &original.surname,
        &original.name_suffix,
    );
    let final_name = crate::model::name::full_name(
        &final_details.name_prefix,
        &final_details.given_name,
        &final_details.middle_name,
        &final_details.surname,
        &final_details.name_suffix,
    );
    if original_name.is_empty() || original_name == final_name {
        return Ok(());
    }
    let exists: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM alternate_names
             WHERE person_id = ?1 AND name_type = 'aka'
               AND prefix = ?2 AND given_name = ?3 AND middle_name = ?4
               AND surname = ?5 AND suffix = ?6 AND nickname = ?7
             LIMIT 1",
            params![
                person_id,
                original.name_prefix,
                original.given_name,
                original.middle_name,
                original.surname,
                original.name_suffix,
                original.nickname,
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if exists.is_some() {
        return Ok(());
    }
    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM alternate_names
             WHERE person_id = ?1",
            params![person_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO alternate_names
         (id, person_id, name_type, prefix, given_name, middle_name,
          surname, suffix, nickname, sort_order)
         VALUES (?1, ?2, 'aka', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            Uuid::new_v4().to_string(),
            person_id,
            original.name_prefix,
            original.given_name,
            original.middle_name,
            original.surname,
            original.name_suffix,
            original.nickname,
            sort_order,
        ],
    )
    .map_err(|error| error.to_string())?;
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

    fn person(conn: &Connection, id: &str, given: &str, surname: &str) {
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES (?1, ?2, ?3, 'U')",
            params![id, given, surname],
        )
        .expect("person inserts");
    }

    fn family(conn: &Connection, id: &str, husband: Option<&str>, wife: Option<&str>) {
        conn.execute(
            "INSERT INTO families (id, husband_id, wife_id) VALUES (?1, ?2, ?3)",
            params![id, husband, wife],
        )
        .expect("family inserts");
    }

    fn child(conn: &Connection, family_id: &str, child_id: &str) {
        conn.execute(
            "INSERT OR IGNORE INTO family_children (family_id, child_id) VALUES (?1, ?2)",
            params![family_id, child_id],
        )
        .expect("child link inserts");
    }

    fn event(
        conn: &Connection,
        id: &str,
        event_type: &str,
        person_id: Option<&str>,
        family_id: Option<&str>,
        date: Option<&str>,
        description: Option<&str>,
    ) {
        conn.execute(
            "INSERT INTO events
             (id, event_type, date, place_id, description, person_id, family_id)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6)",
            params![id, event_type, date, description, person_id, family_id],
        )
        .expect("event inserts");
    }

    fn note(conn: &Connection, id: &str, person_id: &str, body: &str) {
        conn.execute(
            "INSERT INTO notes (id, body, person_id) VALUES (?1, ?2, ?3)",
            params![id, body, person_id],
        )
        .expect("note inserts");
    }

    fn count_for(conn: &Connection, sql: &str, id: &str) -> i64 {
        conn.query_row(sql, params![id], |row| row.get(0))
            .expect("count queries")
    }

    fn draft(keep: &str, merge: &str) -> MergeDraft {
        MergeDraft {
            keep_id: keep.to_string(),
            merge_id: merge.to_string(),
            details: PersonDetails::new("Ada", "", "King", "", "F", None, None),
            private: false,
            living_override: None,
            preserve_primary_names: true,
        }
    }

    #[test]
    fn preview_loads_profiles_counts_and_blockers() {
        let conn = connection();
        person(&conn, "keep", "Ada", "King");
        person(&conn, "merge", "Augusta", "King");
        person(&conn, "child", "Byron", "King");
        person(&conn, "spouse", "William", "King");
        person(&conn, "dad", "George", "Byron");
        family(&conn, "f1", Some("keep"), Some("spouse"));
        family(&conn, "f2", Some("merge"), None);
        family(&conn, "f3", Some("dad"), None);
        child(&conn, "f3", "merge");
        note(&conn, "n1", "merge", "A note");
        conn.execute(
            "INSERT INTO alternate_names (id, person_id, name_type, given_name, surname)
             VALUES ('a1', 'merge', 'aka', 'Augusta', 'King')",
            [],
        )
        .unwrap();

        let preview = preview_merge(&conn, "keep", "merge").expect("preview loads");
        assert_eq!(preview.keep.details.given_name, "Ada");
        assert_eq!(preview.merge.details.given_name, "Augusta");
        assert_eq!(preview.merge.counts.notes, 1);
        assert_eq!(preview.merge.counts.alternate_names, 1);
        assert_eq!(preview.merge.counts.spouse_families, 1);
        assert_eq!(preview.merge.counts.parent_families, 1);
        assert_eq!(preview.blocked, None);

        assert!(preview_merge(&conn, "keep", "keep").is_err());
        assert!(preview_merge(&conn, "keep", "missing").is_err());

        family(&conn, "f4", Some("keep"), Some("merge"));
        assert_eq!(
            preview_merge(&conn, "keep", "merge")
                .expect("spouse preview")
                .blocked,
            Some(MergeBlocker::Spouses)
        );
    }

    #[test]
    fn direct_parent_child_pairs_are_blocked() {
        let conn = connection();
        person(&conn, "parent", "Ada", "King");
        person(&conn, "child", "Ada", "King");
        family(&conn, "f1", Some("parent"), None);
        child(&conn, "f1", "child");

        assert_eq!(
            preview_merge(&conn, "parent", "child")
                .expect("preview")
                .blocked,
            Some(MergeBlocker::DirectParentChild)
        );
        let mut conn = conn;
        let error = merge_people(&mut conn, &draft("parent", "child"))
            .expect_err("direct relation rejected");
        assert!(error.contains("parent and child"), "{error}");
    }

    #[test]
    fn merge_moves_profile_links_and_root() {
        let mut conn = connection();
        person(&conn, "keep", "Ada", "King");
        person(&conn, "merge", "Augusta", "King");
        person(&conn, "friend", "Mary", "Shelley");
        conn.execute(
            "UPDATE people SET private = 1, living_override = 1 WHERE id = 'merge'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tree_settings (key, value) VALUES ('root_person_id', 'merge')",
            [],
        )
        .unwrap();
        note(&conn, "n1", "merge", "Merged note");
        conn.execute(
            "INSERT INTO media (id, file_path) VALUES ('m1', '/tmp/portrait.jpg')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO media_links (media_id, person_id) VALUES ('m1', 'merge')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks (id, title) VALUES ('t1', 'Research')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO task_links (task_id, person_id) VALUES ('t1', 'merge')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO groups (id, name) VALUES ('g1', 'Family')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO group_members (group_id, person_id) VALUES ('g1', 'merge')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO associations (id, person_id, associate_id, relationship)
             VALUES ('as1', 'merge', 'friend', 'Friend')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO alternate_names (id, person_id, name_type, given_name, surname)
             VALUES ('a1', 'merge', 'aka', 'Augusta', 'King')",
            [],
        )
        .unwrap();
        event(
            &conn,
            "e1",
            "OCCUPATION",
            Some("merge"),
            None,
            Some("1840"),
            Some("Writer"),
        );

        let mut details = PersonDetails::new("Ada", "", "Lovelace", "", "F", None, None);
        details.birth_date = Some("1815".to_string());
        details.birth_place = Some("London".to_string());
        let report = merge_people(
            &mut conn,
            &MergeDraft {
                keep_id: "keep".to_string(),
                merge_id: "merge".to_string(),
                details,
                private: true,
                living_override: Some(false),
                preserve_primary_names: true,
            },
        )
        .expect("merge succeeds");

        assert!(report.notes_moved >= 1);
        assert!(report.media_moved >= 1);
        assert!(report.tasks_moved >= 1);
        assert!(report.groups_moved >= 1);
        assert!(report.associations_moved >= 1);
        assert_eq!(
            count_for(&conn, "SELECT COUNT(*) FROM people WHERE id = ?1", "merge"),
            0
        );
        let (given, surname, private, living): (String, String, i64, Option<i64>) = conn
            .query_row(
                "SELECT given_name, surname, private, living_override FROM people WHERE id = 'keep'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!((given.as_str(), surname.as_str()), ("Ada", "Lovelace"));
        assert_eq!(private, 1);
        assert_eq!(living, Some(0));
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM notes WHERE person_id = ?1",
                "keep"
            ),
            1
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM media_links WHERE person_id = ?1",
                "keep"
            ),
            1
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM task_links WHERE person_id = ?1",
                "keep"
            ),
            1
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM group_members WHERE person_id = ?1",
                "keep"
            ),
            1
        );
        let association: String = conn
            .query_row(
                "SELECT associate_id FROM associations WHERE person_id = 'keep'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(association, "friend");
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM events WHERE person_id = ?1",
                "keep"
            ),
            2,
            "occupation and birth events moved"
        );
        let root: String = conn
            .query_row(
                "SELECT value FROM tree_settings WHERE key = 'root_person_id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root, "keep");
        let names: Vec<String> = conn
            .prepare("SELECT given_name || ' ' || surname FROM alternate_names WHERE person_id = 'keep' ORDER BY given_name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(names.contains(&"Ada King".to_string()), "{names:?}");
        assert!(names.contains(&"Augusta King".to_string()), "{names:?}");
    }

    #[test]
    fn duplicate_spouse_and_parent_families_are_consolidated() {
        let mut conn = connection();
        person(&conn, "keep", "Ada", "King");
        person(&conn, "merge", "Augusta", "King");
        person(&conn, "spouse", "William", "King");
        person(&conn, "dad", "George", "Byron");
        person(&conn, "mom", "Anne", "Byron");
        person(&conn, "c1", "Child", "One");
        person(&conn, "c2", "Child", "Two");
        family(&conn, "f_keep", Some("keep"), Some("spouse"));
        family(&conn, "f_merge", Some("merge"), Some("spouse"));
        child(&conn, "f_keep", "c1");
        child(&conn, "f_merge", "c2");
        event(
            &conn,
            "fe1",
            "MARRIAGE",
            None,
            Some("f_merge"),
            Some("1835"),
            None,
        );
        note(&conn, "fn1", "merge", "family note");
        family(&conn, "f_parent_keep", Some("dad"), Some("mom"));
        family(&conn, "f_parent_merge", Some("dad"), Some("mom"));
        child(&conn, "f_parent_keep", "keep");
        child(&conn, "f_parent_merge", "merge");

        let report =
            merge_people(&mut conn, &draft("keep", "merge")).expect("family merge succeeds");

        assert!(report.families_merged >= 2);
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM families WHERE id = ?1",
                "f_merge"
            ),
            0
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM family_children WHERE family_id = ?1",
                "f_keep"
            ),
            2,
            "both children live in the surviving spouse family"
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM events WHERE family_id = ?1",
                "f_keep"
            ),
            1,
            "family event moved"
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM families WHERE id = ?1",
                "f_parent_merge"
            ),
            0
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM family_children WHERE family_id = ?1",
                "f_parent_keep"
            ),
            1,
            "duplicate parent family keeps one child row"
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM family_children WHERE child_id = ?1",
                "keep"
            ),
            1,
            "kept person remains a child in the parent family"
        );
    }

    #[test]
    fn vital_and_duplicate_events_keep_their_evidence() {
        let mut conn = connection();
        person(&conn, "keep", "Ada", "King");
        person(&conn, "merge", "Augusta", "King");
        conn.execute(
            "INSERT INTO places (id, name) VALUES ('london', 'London')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sources (id, title) VALUES ('s1', 'Register')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO citations (id, source_id) VALUES ('c1', 's1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO citations (id, source_id) VALUES ('c2', 's1')",
            [],
        )
        .unwrap();
        for event_id in ["birth1", "birth2", "job1", "job2", "job3"] {
            let (event_type, date) = match event_id {
                "birth1" | "birth2" => ("BIRTH", "1815"),
                "job1" | "job2" => ("OCCUPATION", "1840"),
                _ => ("OCCUPATION", "1845"),
            };
            conn.execute(
                "INSERT INTO events (id, event_type, date, place_id)
                 VALUES (?1, ?2, ?3, 'london')",
                params![event_id, event_type, date],
            )
            .unwrap();
        }
        conn.execute(
            "UPDATE events SET person_id = 'keep' WHERE id IN ('birth1', 'job1')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE events SET person_id = 'merge' WHERE id IN ('birth2', 'job2', 'job3')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE events SET description = 'Writer' WHERE id IN ('job2', 'job1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO citation_links (citation_id, event_id) VALUES ('c1', 'birth1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO citation_links (citation_id, event_id) VALUES ('c2', 'birth2')",
            [],
        )
        .unwrap();

        let mut details = PersonDetails::new("Ada", "", "King", "", "F", None, None);
        details.birth_date = Some("1815".to_string());
        details.birth_place = Some("London".to_string());
        merge_people(
            &mut conn,
            &MergeDraft {
                keep_id: "keep".to_string(),
                merge_id: "merge".to_string(),
                details,
                private: false,
                living_override: None,
                preserve_primary_names: false,
            },
        )
        .expect("merge succeeds");

        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM events WHERE person_id = ?1 AND event_type = 'BIRTH'",
                "keep"
            ),
            1
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM citation_links WHERE event_id = ?1",
                "birth1"
            ),
            2,
            "both birth citations move to the canonical event"
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM events WHERE person_id = ?1 AND event_type = 'OCCUPATION'",
                "keep"
            ),
            2,
            "identical occupations merge, distinct ones remain"
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM events WHERE person_id = ?1 AND event_type = 'BIRTH'",
                "merge"
            ),
            0
        );
    }

    #[test]
    fn invalid_details_roll_back_the_merge() {
        let mut conn = connection();
        person(&conn, "keep", "Ada", "King");
        person(&conn, "merge", "Augusta", "King");
        note(&conn, "n1", "merge", "Keep me");

        let mut details = PersonDetails::new("", "", "", "", "U", None, None);
        details.birth_date = Some("not a date".to_string());
        let error = merge_people(
            &mut conn,
            &MergeDraft {
                keep_id: "keep".to_string(),
                merge_id: "merge".to_string(),
                details,
                private: false,
                living_override: None,
                preserve_primary_names: true,
            },
        )
        .expect_err("invalid details rejected");
        assert!(!error.is_empty());
        assert_eq!(
            count_for(&conn, "SELECT COUNT(*) FROM people WHERE id = ?1", "merge"),
            1,
            "source survives"
        );
        assert_eq!(
            count_for(
                &conn,
                "SELECT COUNT(*) FROM notes WHERE person_id = ?1",
                "merge"
            ),
            1,
            "links were untouched"
        );
    }
}
