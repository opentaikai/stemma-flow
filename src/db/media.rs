//! Media record persistence and record attachments.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::name::format_name;

/// A record that can own a media link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaOwner {
    Person(String),
    Family(String),
    Event(String),
    Source(String),
    Place(String),
}

impl MediaOwner {
    pub(crate) fn column(&self) -> &'static str {
        match self {
            Self::Person(_) => "person_id",
            Self::Family(_) => "family_id",
            Self::Event(_) => "event_id",
            Self::Source(_) => "source_id",
            Self::Place(_) => "place_id",
        }
    }

    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Person(id)
            | Self::Family(id)
            | Self::Event(id)
            | Self::Source(id)
            | Self::Place(id) => id,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Person(_) => "person",
            Self::Family(_) => "family",
            Self::Event(_) => "fact",
            Self::Source(_) => "source",
            Self::Place(_) => "place",
        }
    }
}

/// One stored media record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRecord {
    pub id: String,
    pub file_path: String,
    pub format: Option<String>,
    pub caption: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

/// A media row with its attachment count and local-file state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSummary {
    pub id: String,
    pub file_path: String,
    pub format: Option<String>,
    pub caption: Option<String>,
    pub link_count: i64,
    pub missing: bool,
}

/// An editable media record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaDraft {
    pub id: Option<String>,
    pub file_path: String,
    pub format: String,
    pub caption: String,
    pub notes: String,
}

/// One record linked to a media item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLinkSummary {
    pub owner_type: String,
    pub owner_id: String,
    pub label: String,
}

fn clean(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn inferred_format(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
}

fn summary(
    id: String,
    file_path: String,
    format: Option<String>,
    caption: Option<String>,
    link_count: i64,
) -> MediaSummary {
    MediaSummary {
        missing: !Path::new(&file_path).is_file(),
        id,
        file_path,
        format,
        caption,
        link_count,
    }
}

/// Lists every media record with its link count.
pub fn list_media(conn: &Connection) -> Result<Vec<MediaSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.file_path, m.format, m.caption,
                    (SELECT COUNT(*) FROM media_links l WHERE l.media_id = m.id)
             FROM media m ORDER BY m.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(summary(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads one media record for editing.
pub fn load_media(conn: &Connection, id: &str) -> Result<Option<MediaRecord>, String> {
    conn.query_row(
        "SELECT id, file_path, format, caption, notes, created_at
         FROM media WHERE id = ?1",
        params![id],
        |row| {
            Ok(MediaRecord {
                id: row.get(0)?,
                file_path: row.get(1)?,
                format: row.get(2)?,
                caption: row.get(3)?,
                notes: row.get(4)?,
                created_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// Inserts or updates a media record.
pub fn save_media(conn: &Connection, draft: &MediaDraft) -> Result<String, String> {
    let file_path = clean(&draft.file_path).ok_or_else(|| "choose a media file".to_string())?;
    let format = clean(&draft.format).or_else(|| inferred_format(&file_path));
    let caption = clean(&draft.caption);
    let notes = clean(&draft.notes);
    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let changed = match &draft.id {
        Some(existing) => conn
            .execute(
                "UPDATE media SET file_path = ?1, format = ?2, caption = ?3, notes = ?4
                 WHERE id = ?5",
                params![file_path, format, caption, notes, existing],
            )
            .map_err(|error| error.to_string())?,
        None => conn
            .execute(
                "INSERT INTO media (id, file_path, format, caption, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, file_path, format, caption, notes],
            )
            .map_err(|error| error.to_string())?,
    };
    if changed == 0 {
        return Err(format!("no media with id {id}"));
    }
    Ok(id)
}

/// Deletes one media record and its links.
pub fn delete_media(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM media WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no media with id {id}"));
    }
    Ok(())
}

/// Lists media attached to one record.
pub fn list_owner_media(
    conn: &Connection,
    owner: &MediaOwner,
) -> Result<Vec<MediaSummary>, String> {
    let sql = format!(
        "SELECT m.id, m.file_path, m.format, m.caption,
                (SELECT COUNT(*) FROM media_links l WHERE l.media_id = m.id)
         FROM media m
         JOIN media_links l ON l.media_id = m.id
         WHERE l.{} = ?1
         ORDER BY m.rowid",
        owner.column()
    );
    let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![owner.id()], |row| {
            Ok(summary(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Attaches a media record to one owner, ignoring an existing duplicate.
pub fn attach_media(conn: &Connection, media_id: &str, owner: &MediaOwner) -> Result<(), String> {
    ensure_exists(conn, "media", media_id)?;
    ensure_exists(conn, owner_table(owner), owner.id())?;
    if is_attached(conn, media_id, owner)? {
        return Ok(());
    }
    let sql = format!(
        "INSERT INTO media_links (media_id, {}) VALUES (?1, ?2)",
        owner.column()
    );
    conn.execute(&sql, params![media_id, owner.id()])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Detaches one media record from one owner.
pub fn detach_media(conn: &Connection, media_id: &str, owner: &MediaOwner) -> Result<(), String> {
    let sql = format!(
        "DELETE FROM media_links WHERE media_id = ?1 AND {} = ?2",
        owner.column()
    );
    let changed = conn
        .execute(&sql, params![media_id, owner.id()])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("media link no longer exists".to_string());
    }
    Ok(())
}

/// Lists the records using one media item.
pub fn list_media_links(
    conn: &Connection,
    media_id: &str,
    surname_first: bool,
) -> Result<Vec<MediaLinkSummary>, String> {
    let mut links = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix
             FROM media_links l JOIN people p ON p.id = l.person_id
             WHERE l.media_id = ?1 AND l.person_id IS NOT NULL ORDER BY p.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![media_id], |row| {
            let name = format_name(
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
                &row.get::<_, String>(4)?,
                &row.get::<_, String>(5)?,
                surname_first,
            );
            Ok(MediaLinkSummary {
                owner_type: "person".to_string(),
                owner_id: row.get(0)?,
                label: if name.is_empty() {
                    "(unnamed)".to_string()
                } else {
                    name
                },
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    links.extend(rows);

    let mut stmt = conn
        .prepare(
            "SELECT f.id FROM media_links l JOIN families f ON f.id = l.family_id
             WHERE l.media_id = ?1 AND l.family_id IS NOT NULL ORDER BY f.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![media_id], |row| {
            Ok(MediaLinkSummary {
                owner_type: "family".to_string(),
                owner_id: row.get(0)?,
                label: "Family".to_string(),
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    links.extend(rows);

    let mut stmt = conn
        .prepare(
            "SELECT e.id, COALESCE(ft.label, e.event_type), e.date
             FROM media_links l JOIN events e ON e.id = l.event_id
             LEFT JOIN fact_types ft ON ft.key = e.event_type
             WHERE l.media_id = ?1 AND l.event_id IS NOT NULL ORDER BY e.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![media_id], |row| {
            let label: String = row.get(1)?;
            let date: Option<String> = row.get(2)?;
            Ok(MediaLinkSummary {
                owner_type: "fact".to_string(),
                owner_id: row.get(0)?,
                label: match date {
                    Some(date) => format!("{label} ({date})"),
                    None => label,
                },
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    links.extend(rows);

    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.title FROM media_links l JOIN sources s ON s.id = l.source_id
             WHERE l.media_id = ?1 AND l.source_id IS NOT NULL ORDER BY s.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![media_id], |row| {
            Ok(MediaLinkSummary {
                owner_type: "source".to_string(),
                owner_id: row.get(0)?,
                label: row.get(1)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    links.extend(rows);

    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name FROM media_links l JOIN places p ON p.id = l.place_id
             WHERE l.media_id = ?1 AND l.place_id IS NOT NULL ORDER BY p.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![media_id], |row| {
            Ok(MediaLinkSummary {
                owner_type: "place".to_string(),
                owner_id: row.get(0)?,
                label: row.get(1)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    links.extend(rows);
    Ok(links)
}

fn owner_table(owner: &MediaOwner) -> &'static str {
    match owner {
        MediaOwner::Person(_) => "people",
        MediaOwner::Family(_) => "families",
        MediaOwner::Event(_) => "events",
        MediaOwner::Source(_) => "sources",
        MediaOwner::Place(_) => "places",
    }
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

fn is_attached(conn: &Connection, media_id: &str, owner: &MediaOwner) -> Result<bool, String> {
    let sql = format!(
        "SELECT 1 FROM media_links WHERE media_id = ?1 AND {} = ?2",
        owner.column()
    );
    conn.query_row(&sql, params![media_id, owner.id()], |row| {
        row.get::<_, i64>(0)
    })
    .optional()
    .map(|found| found.is_some())
    .map_err(|error| error.to_string())
}

/// Human-readable owner type for the dialog.
pub fn owner_label(owner: &MediaOwner) -> &'static str {
    owner.label()
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
             VALUES ('p1', 'Ada', 'King', 'F');
             INSERT INTO families (id) VALUES ('f1');
             INSERT INTO events (id, event_type, person_id) VALUES ('e1', 'BIRTH', 'p1');
             INSERT INTO sources (id, title) VALUES ('s1', 'Parish Book');
             INSERT INTO places (id, name) VALUES ('pl1', 'Uppsala');",
        )
        .expect("records");
        conn
    }

    fn draft(path: &str) -> MediaDraft {
        MediaDraft {
            file_path: path.to_string(),
            ..MediaDraft::default()
        }
    }

    #[test]
    fn saves_loads_and_infers_format() {
        let conn = connection();
        let id = save_media(&conn, &draft(" /tmp/photo.JPG ")).expect("save");
        let record = load_media(&conn, &id).expect("load").expect("record");
        assert_eq!(record.file_path, "/tmp/photo.JPG");
        assert_eq!(record.format.as_deref(), Some("jpg"));
        assert!(list_media(&conn).expect("list")[0].missing);
        assert!(save_media(&conn, &draft("   ")).is_err());
    }

    #[test]
    fn attaches_all_owner_types_without_duplicates() {
        let conn = connection();
        let id = save_media(&conn, &draft("/tmp/photo.jpg")).expect("save");
        for owner in [
            MediaOwner::Person("p1".to_string()),
            MediaOwner::Family("f1".to_string()),
            MediaOwner::Event("e1".to_string()),
            MediaOwner::Source("s1".to_string()),
            MediaOwner::Place("pl1".to_string()),
        ] {
            attach_media(&conn, &id, &owner).expect("attach");
            attach_media(&conn, &id, &owner).expect("idempotent attach");
            assert_eq!(
                list_owner_media(&conn, &owner).expect("owner media").len(),
                1
            );
        }
        let links = list_media_links(&conn, &id, false).expect("links");
        assert_eq!(links.len(), 5);
        assert!(links.iter().any(|link| link.label == "Ada King"));
        assert!(links.iter().any(|link| link.label == "Parish Book"));
        assert!(links.iter().any(|link| link.label == "Uppsala"));
        assert_eq!(list_media(&conn).expect("list")[0].link_count, 5);
    }

    #[test]
    fn detach_and_cascades_remove_links() {
        let conn = connection();
        let id = save_media(&conn, &draft("/tmp/photo.jpg")).expect("save");
        attach_media(&conn, &id, &MediaOwner::Person("p1".to_string())).expect("attach");
        detach_media(&conn, &id, &MediaOwner::Person("p1".to_string())).expect("detach");
        assert!(
            list_owner_media(&conn, &MediaOwner::Person("p1".to_string()))
                .expect("list")
                .is_empty()
        );

        attach_media(&conn, &id, &MediaOwner::Person("p1".to_string())).expect("attach");
        conn.execute("DELETE FROM people WHERE id = 'p1'", [])
            .expect("delete person");
        assert!(list_media(&conn).expect("list")[0].link_count == 0);

        attach_media(&conn, &id, &MediaOwner::Place("pl1".to_string())).expect("attach");
        delete_media(&conn, &id).expect("delete media");
        assert!(
            list_owner_media(&conn, &MediaOwner::Place("pl1".to_string()))
                .expect("list")
                .is_empty()
        );
    }
}
