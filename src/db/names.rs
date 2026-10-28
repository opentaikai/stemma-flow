//! Alternate names for people.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::name::format_name;

/// One stored alternate name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternateName {
    pub id: String,
    pub person_id: String,
    pub name_type: String,
    pub prefix: String,
    pub given_name: String,
    pub middle_name: String,
    pub surname: String,
    pub suffix: String,
    pub nickname: String,
    pub sort_order: i64,
}

impl AlternateName {
    pub fn display_name(&self, surname_first: bool) -> String {
        format_name(
            &self.prefix,
            &self.given_name,
            &self.middle_name,
            &self.surname,
            &self.suffix,
            surname_first,
        )
    }
}

/// An editable alternate name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlternateNameDraft {
    pub id: Option<String>,
    pub name_type: String,
    pub prefix: String,
    pub given_name: String,
    pub middle_name: String,
    pub surname: String,
    pub suffix: String,
    pub nickname: String,
}

fn clean(value: &str) -> String {
    value.trim().to_string()
}

/// Lists alternate names for a person in display order.
pub fn list_alternate_names(
    conn: &Connection,
    person_id: &str,
) -> Result<Vec<AlternateName>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, person_id, name_type, prefix, given_name, middle_name,
                    surname, suffix, nickname, sort_order
             FROM alternate_names
             WHERE person_id = ?1
             ORDER BY sort_order, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![person_id], |row| {
            Ok(AlternateName {
                id: row.get(0)?,
                person_id: row.get(1)?,
                name_type: row.get(2)?,
                prefix: row.get(3)?,
                given_name: row.get(4)?,
                middle_name: row.get(5)?,
                surname: row.get(6)?,
                suffix: row.get(7)?,
                nickname: row.get(8)?,
                sort_order: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Inserts or updates an alternate name.
pub fn save_alternate_name(
    conn: &mut Connection,
    person_id: &str,
    draft: &AlternateNameDraft,
) -> Result<String, String> {
    let given_name = clean(&draft.given_name);
    let surname = clean(&draft.surname);
    if given_name.is_empty() && surname.is_empty() {
        return Err("Alternate names need a given name or surname".to_string());
    }
    let name_type = {
        let cleaned = clean(&draft.name_type);
        if cleaned.is_empty() {
            "aka".to_string()
        } else {
            cleaned
        }
    };
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let person_exists: Option<i64> = tx
        .query_row(
            "SELECT 1 FROM people WHERE id = ?1",
            params![person_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if person_exists.is_none() {
        return Err(format!("no person with id {person_id}"));
    }

    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let changed = tx
        .execute(
            "UPDATE alternate_names
             SET name_type = ?1, prefix = ?2, given_name = ?3, middle_name = ?4,
                 surname = ?5, suffix = ?6, nickname = ?7
             WHERE id = ?8 AND person_id = ?9",
            params![
                name_type,
                clean(&draft.prefix),
                given_name,
                clean(&draft.middle_name),
                surname,
                clean(&draft.suffix),
                clean(&draft.nickname),
                id,
                person_id
            ],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        let sort_order: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM alternate_names
                 WHERE person_id = ?1",
                params![person_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO alternate_names
             (id, person_id, name_type, prefix, given_name, middle_name,
              surname, suffix, nickname, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                person_id,
                name_type,
                clean(&draft.prefix),
                given_name,
                clean(&draft.middle_name),
                surname,
                clean(&draft.suffix),
                clean(&draft.nickname),
                sort_order
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(id)
}

/// Deletes an alternate name.
pub fn delete_alternate_name(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM alternate_names WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no alternate name with id {id}"));
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
        conn.execute(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F')",
            [],
        )
        .expect("person insert");
        conn
    }

    #[test]
    fn saves_and_lists_alternate_names() {
        let mut conn = connection();
        let id = save_alternate_name(
            &mut conn,
            "p1",
            &AlternateNameDraft {
                name_type: "married".to_string(),
                given_name: "Ada".to_string(),
                surname: "Lovelace".to_string(),
                ..AlternateNameDraft::default()
            },
        )
        .expect("name saves");
        let names = list_alternate_names(&conn, "p1").expect("names");
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].id, id);
        assert_eq!(names[0].name_type, "married");
        assert_eq!(names[0].display_name(false), "Ada Lovelace");
    }

    #[test]
    fn updates_defaults_and_deletes() {
        let mut conn = connection();
        let id = save_alternate_name(
            &mut conn,
            "p1",
            &AlternateNameDraft {
                given_name: "Ada".to_string(),
                ..AlternateNameDraft::default()
            },
        )
        .expect("name saves");
        let mut draft = AlternateNameDraft {
            id: Some(id.clone()),
            prefix: "Dr.".to_string(),
            given_name: "Ada".to_string(),
            surname: "Byron".to_string(),
            suffix: "Jr.".to_string(),
            ..AlternateNameDraft::default()
        };
        save_alternate_name(&mut conn, "p1", &draft).expect("name updates");
        let names = list_alternate_names(&conn, "p1").expect("names");
        assert_eq!(names[0].name_type, "aka");
        assert_eq!(names[0].display_name(false), "Dr. Ada Byron Jr.");
        assert_eq!(names[0].display_name(true), "Byron, Dr. Ada Jr.");
        delete_alternate_name(&conn, &id).expect("name deletes");
        assert!(list_alternate_names(&conn, "p1").expect("names").is_empty());
        draft.given_name.clear();
        draft.surname.clear();
        assert!(save_alternate_name(&mut conn, "p1", &draft).is_err());
    }
}
