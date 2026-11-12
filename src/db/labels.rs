//! Coloured person labels for visual grouping.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::name::format_name;

/// One stored label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelRecord {
    pub id: String,
    pub name: String,
    pub color: u32,
    pub created_at: String,
}

/// A label row with its member count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelSummary {
    pub id: String,
    pub name: String,
    pub color: u32,
    pub member_count: i64,
}

/// Editable label fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabelDraft {
    pub id: Option<String>,
    pub name: String,
    pub color: u32,
}

impl LabelDraft {
    pub fn from_record(record: &LabelRecord) -> Self {
        Self {
            id: Some(record.id.clone()),
            name: record.name.clone(),
            color: record.color,
        }
    }
}

/// One person carrying a label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelMember {
    pub person_id: String,
    pub name: String,
}

/// One person-label assignment for badge building.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonLabel {
    pub person_id: String,
    pub label: LabelSummary,
}

fn clean(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Lists every label with its member count.
pub fn list_labels(conn: &Connection) -> Result<Vec<LabelSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT l.id, l.name, l.color,
                    (SELECT COUNT(*) FROM person_labels p WHERE p.label_id = l.id)
             FROM labels l ORDER BY l.name COLLATE NOCASE, l.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], label_summary)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn label_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<LabelSummary> {
    Ok(LabelSummary {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get::<_, i64>(2)? as u32,
        member_count: row.get(3)?,
    })
}

/// Loads one label for editing.
pub fn load_label(conn: &Connection, id: &str) -> Result<Option<LabelRecord>, String> {
    conn.query_row(
        "SELECT id, name, color, created_at FROM labels WHERE id = ?1",
        params![id],
        |row| {
            Ok(LabelRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get::<_, i64>(2)? as u32,
                created_at: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// Inserts or updates a label.
pub fn save_label(conn: &Connection, draft: &LabelDraft) -> Result<String, String> {
    let name = clean(&draft.name).ok_or_else(|| "label name is required".to_string())?;
    let color = (draft.color & 0x00ff_ffff) as i64;
    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let result = match &draft.id {
        Some(existing) => conn.execute(
            "UPDATE labels SET name = ?1, color = ?2 WHERE id = ?3",
            params![name, color, existing],
        ),
        None => conn.execute(
            "INSERT INTO labels (id, name, color) VALUES (?1, ?2, ?3)",
            params![id, name, color],
        ),
    };
    match result {
        Ok(0) => return Err("no label with that id".to_string()),
        Ok(_) => {}
        Err(error) if error.to_string().contains("UNIQUE") => {
            return Err("a label with that name already exists".to_string());
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(id)
}

/// Deletes a label and its person assignments.
pub fn delete_label(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM labels WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no label with id {id}"));
    }
    Ok(())
}

/// Lists a label's members by display name.
pub fn list_members(
    conn: &Connection,
    label_id: &str,
    surname_first: bool,
) -> Result<Vec<LabelMember>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix
             FROM person_labels l JOIN people p ON p.id = l.person_id
             WHERE l.label_id = ?1 ORDER BY p.surname COLLATE NOCASE, p.given_name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![label_id], |row| {
            let name = format_name(
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
                &row.get::<_, String>(4)?,
                &row.get::<_, String>(5)?,
                surname_first,
            );
            Ok(LabelMember {
                person_id: row.get(0)?,
                name: if name.is_empty() {
                    "(unnamed)".to_string()
                } else {
                    name
                },
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut rows = rows;
    rows.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.person_id.cmp(&right.person_id))
    });
    Ok(rows)
}

/// Lists labels carried by one person.
pub fn list_labels_for_person(
    conn: &Connection,
    person_id: &str,
) -> Result<Vec<LabelSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT l.id, l.name, l.color,
                    (SELECT COUNT(*) FROM person_labels c WHERE c.label_id = l.id)
             FROM labels l JOIN person_labels p ON p.label_id = l.id
             WHERE p.person_id = ?1
             ORDER BY l.name COLLATE NOCASE, l.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], label_summary)
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads every person-label assignment for badge building.
pub fn list_all_person_labels(conn: &Connection) -> Result<Vec<PersonLabel>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.person_id, l.id, l.name, l.color,
                    (SELECT COUNT(*) FROM person_labels c WHERE c.label_id = l.id)
             FROM person_labels p JOIN labels l ON l.id = p.label_id
             ORDER BY p.person_id, l.name COLLATE NOCASE, l.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PersonLabel {
                person_id: row.get(0)?,
                label: LabelSummary {
                    id: row.get(1)?,
                    name: row.get(2)?,
                    color: row.get::<_, i64>(3)? as u32,
                    member_count: row.get(4)?,
                },
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Adds a person to a label.
pub fn add_member(conn: &Connection, label_id: &str, person_id: &str) -> Result<(), String> {
    ensure_exists(conn, "labels", label_id)?;
    ensure_exists(conn, "people", person_id)?;
    conn.execute(
        "INSERT OR IGNORE INTO person_labels (person_id, label_id) VALUES (?1, ?2)",
        params![person_id, label_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Removes a person from a label.
pub fn remove_member(conn: &Connection, label_id: &str, person_id: &str) -> Result<(), String> {
    let changed = conn
        .execute(
            "DELETE FROM person_labels WHERE label_id = ?1 AND person_id = ?2",
            params![label_id, person_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("label member no longer exists".to_string());
    }
    Ok(())
}

fn ensure_exists(conn: &Connection, table: &str, id: &str) -> Result<(), String> {
    let sql = format!("SELECT 1 FROM {table} WHERE id = ?1");
    let found: Option<i64> = conn
        .query_row(&sql, params![id], |row| row.get(0))
        .optional()
        .map_err(|error| error.to_string())?;
    if found.is_none() {
        return Err(format!("no {table} record with id {id}"));
    }
    Ok(())
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
        conn.execute_batch(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F'), ('p2', 'Charles', 'Babbage', 'M');",
        )
        .expect("people");
        conn
    }

    #[test]
    fn saves_lists_and_deletes_labels() {
        let conn = connection();
        let id = save_label(
            &conn,
            &LabelDraft {
                name: "  Researchers  ".to_string(),
                color: 0x3355ff,
                ..LabelDraft::default()
            },
        )
        .expect("save");
        let label = load_label(&conn, &id).expect("load").expect("label");
        assert_eq!(label.name, "Researchers");
        assert_eq!(label.color, 0x3355ff);
        assert_eq!(list_labels(&conn).expect("list")[0].member_count, 0);

        let duplicate = save_label(
            &conn,
            &LabelDraft {
                name: "Researchers".to_string(),
                color: 0xffffff,
                ..LabelDraft::default()
            },
        )
        .expect_err("duplicate");
        assert!(duplicate.contains("already exists"));

        delete_label(&conn, &id).expect("delete");
        assert!(list_labels(&conn).expect("list").is_empty());
    }

    #[test]
    fn tracks_label_members_and_per_person_labels() {
        let conn = connection();
        let first = save_label(
            &conn,
            &LabelDraft {
                name: "Maternal".to_string(),
                color: 0xff3366,
                ..LabelDraft::default()
            },
        )
        .expect("first");
        let second = save_label(
            &conn,
            &LabelDraft {
                name: "Brick wall".to_string(),
                color: 0x33aa66,
                ..LabelDraft::default()
            },
        )
        .expect("second");
        add_member(&conn, &first, "p1").expect("add");
        add_member(&conn, &second, "p1").expect("add");
        add_member(&conn, &first, "p1").expect("idempotent");
        assert_eq!(
            list_members(&conn, &first, false).expect("members").len(),
            1
        );
        assert_eq!(
            list_labels_for_person(&conn, "p1")
                .expect("person labels")
                .len(),
            2
        );
        assert_eq!(
            list_all_person_labels(&conn)
                .expect("all assignments")
                .len(),
            2
        );

        remove_member(&conn, &first, "p1").expect("remove");
        assert_eq!(
            list_labels_for_person(&conn, "p1")
                .expect("person labels")
                .len(),
            1
        );
        conn.execute("DELETE FROM people WHERE id = 'p1'", [])
            .expect("delete person");
        assert!(
            list_all_person_labels(&conn)
                .expect("assignments")
                .is_empty()
        );
    }
}
