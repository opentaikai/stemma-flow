//! Media albums and ordered album membership.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::db::media::{MediaSummary, summary};
use crate::model::name::format_name;

/// An album row with its owner name and media count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub person_id: Option<String>,
    pub person_name: Option<String>,
    pub media_count: i64,
}

/// A stored album loaded for editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub person_id: Option<String>,
    pub created_at: String,
}

/// Editable album fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlbumDraft {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub person_id: Option<String>,
}

fn clean(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn person_name(
    prefix: Option<String>,
    given: Option<String>,
    middle: Option<String>,
    surname: Option<String>,
    suffix: Option<String>,
    surname_first: bool,
) -> Option<String> {
    let name = format_name(
        prefix.as_deref().unwrap_or(""),
        given.as_deref().unwrap_or(""),
        middle.as_deref().unwrap_or(""),
        surname.as_deref().unwrap_or(""),
        suffix.as_deref().unwrap_or(""),
        surname_first,
    );
    if name.is_empty() { None } else { Some(name) }
}

/// Lists general and per-person albums with their media counts.
pub fn list_albums(conn: &Connection, surname_first: bool) -> Result<Vec<AlbumSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.name, a.description, a.person_id,
                    p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix,
                    (SELECT COUNT(*) FROM album_media am WHERE am.album_id = a.id)
             FROM albums a LEFT JOIN people p ON p.id = a.person_id
             ORDER BY a.person_id IS NOT NULL,
                      p.surname COLLATE NOCASE, p.given_name COLLATE NOCASE,
                      a.name COLLATE NOCASE, a.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let person_id: Option<String> = row.get(3)?;
            let person_name = if person_id.is_some() {
                person_name(
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    surname_first,
                )
            } else {
                None
            };
            Ok(AlbumSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                person_id,
                person_name,
                media_count: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads one album for editing.
pub fn load_album(conn: &Connection, id: &str) -> Result<Option<AlbumRecord>, String> {
    conn.query_row(
        "SELECT id, name, description, person_id, created_at FROM albums WHERE id = ?1",
        params![id],
        |row| {
            Ok(AlbumRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                person_id: row.get(3)?,
                created_at: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// Inserts or updates an album.
pub fn save_album(conn: &Connection, draft: &AlbumDraft) -> Result<String, String> {
    let name = clean(&draft.name).ok_or_else(|| "album name is required".to_string())?;
    let description = clean(&draft.description);
    if let Some(person_id) = &draft.person_id {
        ensure_exists(conn, "people", person_id)?;
    }
    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let duplicate: Option<String> = conn
        .query_row(
            "SELECT id FROM albums
             WHERE lower(name) = lower(?1)
               AND ((person_id IS NULL AND ?2 IS NULL) OR person_id = ?2)
               AND id <> ?3",
            params![name, draft.person_id, id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if duplicate.is_some() {
        return Err("an album with this name already exists".to_string());
    }

    let changed = match &draft.id {
        Some(existing) => conn
            .execute(
                "UPDATE albums
                 SET name = ?1, description = ?2, person_id = ?3,
                     updated_at = datetime('now')
                 WHERE id = ?4",
                params![name, description, draft.person_id, existing],
            )
            .map_err(|error| error.to_string())?,
        None => conn
            .execute(
                "INSERT INTO albums (id, name, description, person_id)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, name, description, draft.person_id],
            )
            .map_err(|error| error.to_string())?,
    };
    if changed == 0 {
        return Err(format!("no album with id {id}"));
    }
    Ok(id)
}

/// Deletes an album and its memberships.
pub fn delete_album(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM albums WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no album with id {id}"));
    }
    Ok(())
}

/// Lists the ordered media in one album.
pub fn list_album_media(conn: &Connection, album_id: &str) -> Result<Vec<MediaSummary>, String> {
    list_album_media_with_base(conn, album_id, None)
}

/// Lists the ordered media in one album, resolving paths against `base_dir`.
pub fn list_album_media_with_base(
    conn: &Connection,
    album_id: &str,
    base_dir: Option<&Path>,
) -> Result<Vec<MediaSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.file_path, m.format, m.caption,
                    (SELECT COUNT(*) FROM media_links l WHERE l.media_id = m.id)
             FROM album_media am JOIN media m ON m.id = am.media_id
             WHERE am.album_id = ?1
             ORDER BY am.sort_order, am.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![album_id], |row| {
            Ok(summary(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                base_dir,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Appends a media record to an album; existing membership is left unchanged.
pub fn add_to_album(conn: &Connection, album_id: &str, media_id: &str) -> Result<(), String> {
    ensure_exists(conn, "albums", album_id)?;
    ensure_exists(conn, "media", media_id)?;
    conn.execute(
        "INSERT OR IGNORE INTO album_media (album_id, media_id, sort_order)
         VALUES (?1, ?2,
                 COALESCE((SELECT MAX(sort_order) + 1 FROM album_media WHERE album_id = ?1), 0))",
        params![album_id, media_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Removes one media record from an album.
pub fn remove_from_album(conn: &Connection, album_id: &str, media_id: &str) -> Result<(), String> {
    let changed = conn
        .execute(
            "DELETE FROM album_media WHERE album_id = ?1 AND media_id = ?2",
            params![album_id, media_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("media is not in this album".to_string());
    }
    Ok(())
}

/// Moves an album entry before `before_id`, or to the end when `None`.
pub fn move_album_media(
    conn: &Connection,
    album_id: &str,
    media_id: &str,
    before_id: Option<&str>,
) -> Result<(), String> {
    if before_id == Some(media_id) {
        return Ok(());
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let mut ids: Vec<String> = {
        let mut stmt = tx
            .prepare(
                "SELECT media_id FROM album_media
                 WHERE album_id = ?1 ORDER BY sort_order, rowid",
            )
            .map_err(|error| error.to_string())?;
        stmt.query_map(params![album_id], |row| row.get(0))
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?
    };
    let Some(index) = ids.iter().position(|id| id == media_id) else {
        return Err("media is not in this album".to_string());
    };
    ids.remove(index);
    let target = match before_id {
        Some(before) => ids
            .iter()
            .position(|id| id == before)
            .ok_or_else(|| "target media is not in this album".to_string())?,
        None => ids.len(),
    };
    ids.insert(target, media_id.to_string());
    for (index, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE album_media SET sort_order = ?1
             WHERE album_id = ?2 AND media_id = ?3",
            params![index as i64, album_id, id],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

/// Lists the albums containing one media record.
pub fn albums_for_media(
    conn: &Connection,
    media_id: &str,
    surname_first: bool,
) -> Result<Vec<AlbumSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.name, a.description, a.person_id,
                    p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix,
                    (SELECT COUNT(*) FROM album_media am2 WHERE am2.album_id = a.id)
             FROM albums a
             JOIN album_media am ON am.album_id = a.id
             LEFT JOIN people p ON p.id = a.person_id
             WHERE am.media_id = ?1
             ORDER BY a.person_id IS NOT NULL,
                      p.surname COLLATE NOCASE, p.given_name COLLATE NOCASE,
                      a.name COLLATE NOCASE, a.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![media_id], |row| {
            let person_id: Option<String> = row.get(3)?;
            let person_name = if person_id.is_some() {
                person_name(
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    surname_first,
                )
            } else {
                None
            };
            Ok(AlbumSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                person_id,
                person_name,
                media_count: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
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
        let conn = Connection::open_in_memory().expect("in-memory database opens");
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("pragma applies");
        init_db(&conn).expect("schema initialises");
        conn.execute_batch(
            "INSERT INTO people (id, given_name, surname, gender)
             VALUES ('p1', 'Ada', 'King', 'F');
             INSERT INTO media (id, file_path) VALUES
                ('m1', 'one.jpg'), ('m2', 'two.jpg'), ('m3', 'three.jpg');",
        )
        .expect("fixtures");
        conn
    }

    fn draft(name: &str, person_id: Option<&str>) -> AlbumDraft {
        AlbumDraft {
            name: name.to_string(),
            person_id: person_id.map(str::to_string),
            ..AlbumDraft::default()
        }
    }

    #[test]
    fn saves_lists_updates_and_deletes_albums() {
        let conn = connection();
        let general = save_album(&conn, &draft("Trips", None)).expect("general album");
        let personal = save_album(&conn, &draft("Portraits", Some("p1"))).expect("person album");
        assert!(
            save_album(&conn, &draft("trips", None)).is_err(),
            "album names are unique per owner"
        );

        let albums = list_albums(&conn, false).expect("albums");
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].id, general);
        assert_eq!(albums[0].person_id, None);
        assert_eq!(albums[1].id, personal);
        assert_eq!(albums[1].person_name.as_deref(), Some("Ada King"));

        let loaded = load_album(&conn, &general).expect("load").expect("album");
        assert_eq!(loaded.name, "Trips");
        save_album(
            &conn,
            &AlbumDraft {
                id: Some(general.clone()),
                name: "Travel".to_string(),
                description: "Family trips".to_string(),
                ..AlbumDraft::default()
            },
        )
        .expect("update");
        assert_eq!(
            load_album(&conn, &general)
                .expect("load")
                .expect("album")
                .name,
            "Travel"
        );
        delete_album(&conn, &personal).expect("delete");
        assert_eq!(list_albums(&conn, false).expect("albums").len(), 1);
    }

    #[test]
    fn orders_moves_and_removes_album_media() {
        let conn = connection();
        let album = save_album(&conn, &draft("Trip", None)).expect("album");
        add_to_album(&conn, &album, "m1").expect("add one");
        add_to_album(&conn, &album, "m2").expect("add two");
        add_to_album(&conn, &album, "m3").expect("add three");
        add_to_album(&conn, &album, "m2").expect("add is idempotent");
        assert_eq!(
            list_album_media(&conn, &album)
                .expect("media")
                .iter()
                .map(|media| media.id.as_str())
                .collect::<Vec<_>>(),
            ["m1", "m2", "m3"]
        );

        move_album_media(&conn, &album, "m3", Some("m1")).expect("move to front");
        assert_eq!(
            list_album_media(&conn, &album)
                .expect("media")
                .iter()
                .map(|media| media.id.as_str())
                .collect::<Vec<_>>(),
            ["m3", "m1", "m2"]
        );
        move_album_media(&conn, &album, "m3", None).expect("move to end");
        assert_eq!(
            list_album_media(&conn, &album)
                .expect("media")
                .last()
                .expect("last")
                .id,
            "m3"
        );

        remove_from_album(&conn, &album, "m2").expect("remove");
        assert_eq!(list_album_media(&conn, &album).expect("media").len(), 2);
    }

    #[test]
    fn membership_cascades_and_reports_media_albums() {
        let conn = connection();
        let general = save_album(&conn, &draft("Trips", None)).expect("general");
        let personal = save_album(&conn, &draft("Portraits", Some("p1"))).expect("personal");
        add_to_album(&conn, &general, "m1").expect("general add");
        add_to_album(&conn, &personal, "m1").expect("personal add");
        assert_eq!(
            albums_for_media(&conn, "m1", false).expect("albums").len(),
            2
        );

        conn.execute("DELETE FROM media WHERE id = 'm1'", [])
            .expect("media delete");
        assert!(list_album_media(&conn, &general).expect("media").is_empty());
        assert!(
            list_album_media(&conn, &personal)
                .expect("media")
                .is_empty()
        );

        add_to_album(&conn, &personal, "m2").expect("add again");
        conn.execute("DELETE FROM people WHERE id = 'p1'", [])
            .expect("person delete");
        assert!(
            load_album(&conn, &personal).expect("load").is_none(),
            "person albums cascade"
        );
        assert_eq!(list_albums(&conn, false).expect("albums").len(), 1);
    }
}
