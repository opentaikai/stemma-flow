//! Directed person-to-person associations such as Friend or Godparent.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::name::format_name;

/// One stored association, with the associate's display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationRecord {
    pub id: String,
    pub person_id: String,
    pub associate_id: String,
    pub associate_name: String,
    pub relationship: String,
}

/// One association attached to an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventAssociationRecord {
    pub id: String,
    pub event_id: String,
    pub associate_id: String,
    pub associate_name: String,
    pub relationship: String,
}

/// An editable event association.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventAssociationDraft {
    pub id: Option<String>,
    pub event_id: String,
    pub associate_id: String,
    pub relationship: String,
}

/// An editable association.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationDraft {
    pub id: Option<String>,
    pub person_id: String,
    pub associate_id: String,
    pub relationship: String,
}

impl AssociationDraft {
    pub fn new(person_id: impl Into<String>) -> Self {
        Self {
            id: None,
            person_id: person_id.into(),
            associate_id: String::new(),
            relationship: "Associate".to_string(),
        }
    }
}

fn clean_relationship(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "Associate".to_string()
    } else {
        value.to_string()
    }
}

/// Lists the associations this person owns, ordered by label and associate.
pub fn list_associations(
    conn: &Connection,
    person_id: &str,
    surname_first: bool,
) -> Result<Vec<AssociationRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.person_id, a.associate_id, a.relationship,
                    p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix
             FROM associations a
             JOIN people p ON p.id = a.associate_id
             WHERE a.person_id = ?1
             ORDER BY a.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| {
            let associate_name = format_name(
                &row.get::<_, String>(4)?,
                &row.get::<_, String>(5)?,
                &row.get::<_, String>(6)?,
                &row.get::<_, String>(7)?,
                &row.get::<_, String>(8)?,
                surname_first,
            );
            Ok(AssociationRecord {
                id: row.get(0)?,
                person_id: row.get(1)?,
                associate_id: row.get(2)?,
                relationship: row.get(3)?,
                associate_name: if associate_name.is_empty() {
                    "(unnamed)".to_string()
                } else {
                    associate_name
                },
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let mut rows = rows;
    rows.sort_by(|left, right| {
        left.relationship
            .to_lowercase()
            .cmp(&right.relationship.to_lowercase())
            .then_with(|| {
                left.associate_name
                    .to_lowercase()
                    .cmp(&right.associate_name.to_lowercase())
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(rows)
}

/// Lists the associations attached to an event.
pub fn list_event_associations(
    conn: &Connection,
    event_id: &str,
    surname_first: bool,
) -> Result<Vec<EventAssociationRecord>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.event_id, a.associate_id, a.relationship,
                    p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix
             FROM event_associations a
             JOIN people p ON p.id = a.associate_id
             WHERE a.event_id = ?1
             ORDER BY a.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![event_id], |row| {
            let name = format_name(
                &row.get::<_, String>(4)?,
                &row.get::<_, String>(5)?,
                &row.get::<_, String>(6)?,
                &row.get::<_, String>(7)?,
                &row.get::<_, String>(8)?,
                surname_first,
            );
            Ok(EventAssociationRecord {
                id: row.get(0)?,
                event_id: row.get(1)?,
                associate_id: row.get(2)?,
                relationship: row.get(3)?,
                associate_name: if name.is_empty() {
                    "(unnamed)".to_string()
                } else {
                    name
                },
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Inserts or updates an event association.
pub fn save_event_association(
    conn: &Connection,
    draft: &EventAssociationDraft,
) -> Result<String, String> {
    if draft.event_id.trim().is_empty() || draft.associate_id.trim().is_empty() {
        return Err("choose an event and person".to_string());
    }
    let relationship = clean_relationship(&draft.relationship);
    for (table, id) in [("events", &draft.event_id), ("people", &draft.associate_id)] {
        let sql = format!("SELECT 1 FROM {table} WHERE id = ?1");
        let found: Option<i64> = conn
            .query_row(&sql, params![id], |row| row.get(0))
            .optional()
            .map_err(|error| error.to_string())?;
        if found.is_none() {
            return Err(format!("no {table} record with id {id}"));
        }
    }
    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let result = match &draft.id {
        Some(existing) => conn.execute(
            "UPDATE event_associations SET associate_id = ?1, relationship = ?2,
                updated_at = datetime('now') WHERE id = ?3 AND event_id = ?4",
            params![draft.associate_id, relationship, existing, draft.event_id],
        ),
        None => conn.execute(
            "INSERT INTO event_associations (id, event_id, associate_id, relationship)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, draft.event_id, draft.associate_id, relationship],
        ),
    };
    match result {
        Ok(0) => return Err("no event association with that id".to_string()),
        Ok(_) => {}
        Err(error) if error.to_string().contains("UNIQUE") => {
            return Err("that event association already exists".to_string());
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(id)
}

/// Inserts or updates an association.
pub fn save_association(conn: &mut Connection, draft: &AssociationDraft) -> Result<String, String> {
    if draft.person_id == draft.associate_id {
        return Err("a person cannot be associated with themselves".to_string());
    }
    if draft.associate_id.trim().is_empty() {
        return Err("choose a person to associate".to_string());
    }
    let relationship = clean_relationship(&draft.relationship);
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    for id in [&draft.person_id, &draft.associate_id] {
        let exists: Option<i64> = tx
            .query_row("SELECT 1 FROM people WHERE id = ?1", params![id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|error| error.to_string())?;
        if exists.is_none() {
            return Err(format!("no person with id {id}"));
        }
    }

    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let result = match &draft.id {
        Some(existing) => tx.execute(
            "UPDATE associations SET associate_id = ?1, relationship = ?2,
                updated_at = datetime('now')
             WHERE id = ?3 AND person_id = ?4",
            params![draft.associate_id, relationship, existing, draft.person_id],
        ),
        None => tx.execute(
            "INSERT INTO associations (id, person_id, associate_id, relationship)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, draft.person_id, draft.associate_id, relationship],
        ),
    };
    match result {
        Ok(0) => return Err("no association with that id".to_string()),
        Ok(_) => {}
        Err(error) if error.to_string().contains("UNIQUE") => {
            return Err("that association already exists".to_string());
        }
        Err(error) => return Err(error.to_string()),
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(id)
}

/// Deletes one association.
pub fn delete_association(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM associations WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no association with id {id}"));
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
             VALUES ('p1', 'Ada', 'King', 'F'),
                    ('p2', 'Charles', 'Babbage', 'M'),
                    ('p3', 'Mary', 'Somerville', 'F');",
        )
        .expect("people");
        conn
    }

    #[test]
    fn saves_lists_updates_and_deletes_associations() {
        let mut conn = connection();
        let mut draft = AssociationDraft::new("p1");
        draft.associate_id = "p2".to_string();
        draft.relationship = "  Friend  ".to_string();
        let id = save_association(&mut conn, &draft).expect("association saves");

        let associations = list_associations(&conn, "p1", false).expect("list");
        assert_eq!(associations.len(), 1);
        assert_eq!(associations[0].relationship, "Friend");
        assert_eq!(associations[0].associate_name, "Charles Babbage");
        assert!(
            list_associations(&conn, "p2", false)
                .expect("reverse")
                .is_empty(),
            "associations are directed"
        );

        let edit = AssociationDraft {
            id: Some(id.clone()),
            relationship: "Colleague".to_string(),
            ..draft
        };
        save_association(&mut conn, &edit).expect("association updates");
        let associations = list_associations(&conn, "p1", true).expect("list");
        assert_eq!(associations[0].relationship, "Colleague");
        assert_eq!(associations[0].associate_name, "Babbage, Charles");

        delete_association(&conn, &id).expect("delete");
        assert!(
            list_associations(&conn, "p1", false)
                .expect("list")
                .is_empty()
        );
    }

    #[test]
    fn rejects_self_duplicate_and_blank_targets() {
        let mut conn = connection();
        let mut draft = AssociationDraft::new("p1");
        draft.associate_id = "p1".to_string();
        assert!(save_association(&mut conn, &draft).is_err());

        draft.associate_id = String::new();
        assert!(save_association(&mut conn, &draft).is_err());

        draft.associate_id = "p2".to_string();
        draft.relationship = "Friend".to_string();
        save_association(&mut conn, &draft).expect("first");
        let mut duplicate = AssociationDraft::new("p1");
        duplicate.associate_id = "p2".to_string();
        duplicate.relationship = "Friend".to_string();
        assert_eq!(
            save_association(&mut conn, &duplicate).expect_err("duplicate"),
            "that association already exists"
        );
    }

    #[test]
    fn deleting_either_person_cascades() {
        let mut conn = connection();
        let mut draft = AssociationDraft::new("p1");
        draft.associate_id = "p2".to_string();
        save_association(&mut conn, &draft).expect("association saves");
        conn.execute("DELETE FROM people WHERE id = 'p2'", [])
            .expect("associate delete");
        assert!(
            list_associations(&conn, "p1", false)
                .expect("list")
                .is_empty()
        );
    }
}
