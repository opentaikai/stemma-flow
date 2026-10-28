//! Static named groups of people.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::name::format_name;

/// One stored group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
}

/// A group row with its member count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub member_count: i64,
}

/// Editable group fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupDraft {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
}

impl GroupDraft {
    pub fn from_record(record: &GroupRecord) -> Self {
        Self {
            id: Some(record.id.clone()),
            name: record.name.clone(),
            description: record.description.clone().unwrap_or_default(),
        }
    }
}

/// One person in a group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMember {
    pub person_id: String,
    pub name: String,
}

fn clean(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Lists every group with its member count.
pub fn list_groups(conn: &Connection) -> Result<Vec<GroupSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT g.id, g.name, g.description,
                    (SELECT COUNT(*) FROM group_members m WHERE m.group_id = g.id)
             FROM groups g ORDER BY g.name COLLATE NOCASE, g.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(GroupSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                member_count: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads one group for editing.
pub fn load_group(conn: &Connection, id: &str) -> Result<Option<GroupRecord>, String> {
    conn.query_row(
        "SELECT id, name, description, created_at FROM groups WHERE id = ?1",
        params![id],
        |row| {
            Ok(GroupRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// Inserts or updates a group.
pub fn save_group(conn: &Connection, draft: &GroupDraft) -> Result<String, String> {
    let name = clean(&draft.name).ok_or_else(|| "group name is required".to_string())?;
    let description = clean(&draft.description);
    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let result = match &draft.id {
        Some(existing) => conn.execute(
            "UPDATE groups SET name = ?1, description = ?2 WHERE id = ?3",
            params![name, description, existing],
        ),
        None => conn.execute(
            "INSERT INTO groups (id, name, description) VALUES (?1, ?2, ?3)",
            params![id, name, description],
        ),
    };
    match result {
        Ok(0) => return Err("no group with that id".to_string()),
        Ok(_) => {}
        Err(error) if error.to_string().contains("UNIQUE") => {
            return Err("a group with that name already exists".to_string());
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(id)
}

/// Deletes a group and its memberships.
pub fn delete_group(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM groups WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no group with id {id}"));
    }
    Ok(())
}

/// Lists a group's members by display name.
pub fn list_members(
    conn: &Connection,
    group_id: &str,
    surname_first: bool,
) -> Result<Vec<GroupMember>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix
             FROM group_members m JOIN people p ON p.id = m.person_id
             WHERE m.group_id = ?1 ORDER BY p.surname COLLATE NOCASE, p.given_name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![group_id], |row| {
            let name = format_name(
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
                &row.get::<_, String>(4)?,
                &row.get::<_, String>(5)?,
                surname_first,
            );
            Ok(GroupMember {
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

/// Adds a person to a group.
pub fn add_member(conn: &Connection, group_id: &str, person_id: &str) -> Result<(), String> {
    ensure_exists(conn, "groups", group_id)?;
    ensure_exists(conn, "people", person_id)?;
    conn.execute(
        "INSERT OR IGNORE INTO group_members (group_id, person_id) VALUES (?1, ?2)",
        params![group_id, person_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Removes a person from a group.
pub fn remove_member(conn: &Connection, group_id: &str, person_id: &str) -> Result<(), String> {
    let changed = conn
        .execute(
            "DELETE FROM group_members WHERE group_id = ?1 AND person_id = ?2",
            params![group_id, person_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("group member no longer exists".to_string());
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
        let conn = Connection::open_in_memory().expect("database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma");
        init_db(&conn).expect("schema");
        conn.execute_batch(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F'), ('p2', 'Charles', 'Babbage', 'M');",
        )
        .expect("people");
        conn
    }

    #[test]
    fn saves_lists_updates_and_deletes_groups() {
        let conn = connection();
        let mut draft = GroupDraft {
            name: "  Mathematicians  ".to_string(),
            description: "  Friends and colleagues  ".to_string(),
            ..GroupDraft::default()
        };
        let id = save_group(&conn, &draft).expect("save");
        let loaded = load_group(&conn, &id).expect("load").expect("group");
        assert_eq!(loaded.name, "Mathematicians");
        assert_eq!(
            loaded.description.as_deref(),
            Some("Friends and colleagues")
        );
        assert_eq!(list_groups(&conn).expect("list")[0].member_count, 0);

        draft.id = Some(id.clone());
        draft.name = "Computing pioneers".to_string();
        save_group(&conn, &draft).expect("update");
        assert_eq!(
            list_groups(&conn).expect("list")[0].name,
            "Computing pioneers"
        );
        assert!(save_group(&conn, &GroupDraft::default()).is_err());

        delete_group(&conn, &id).expect("delete");
        assert!(list_groups(&conn).expect("list").is_empty());
    }

    #[test]
    fn rejects_duplicate_names_and_tracks_members() {
        let conn = connection();
        let first = save_group(
            &conn,
            &GroupDraft {
                name: "Friends".to_string(),
                ..GroupDraft::default()
            },
        )
        .expect("first");
        let duplicate = save_group(
            &conn,
            &GroupDraft {
                name: "Friends".to_string(),
                ..GroupDraft::default()
            },
        )
        .expect_err("duplicate");
        assert_eq!(duplicate, "a group with that name already exists");

        add_member(&conn, &first, "p1").expect("add");
        add_member(&conn, &first, "p2").expect("add");
        add_member(&conn, &first, "p1").expect("idempotent");
        let members = list_members(&conn, &first, false).expect("members");
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name, "Ada King");
        assert_eq!(members[1].name, "Charles Babbage");
        assert_eq!(list_groups(&conn).expect("list")[0].member_count, 2);

        remove_member(&conn, &first, "p1").expect("remove");
        assert_eq!(
            list_members(&conn, &first, false).expect("members").len(),
            1
        );

        conn.execute("DELETE FROM people WHERE id = 'p2'", [])
            .expect("delete person");
        assert!(
            list_members(&conn, &first, false)
                .expect("members")
                .is_empty()
        );
    }
}
