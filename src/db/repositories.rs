//! Repository persistence and their source links.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

/// One row in the repositories grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySummary {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub source_count: i64,
}

/// A source linked to a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySourceLink {
    pub source_id: String,
    pub source_title: String,
    pub call_number: Option<String>,
    pub media_type: Option<String>,
}

/// A stored repository loaded for editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRecord {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub links: Vec<RepositorySourceLink>,
}

/// An editable repository draft.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryDraft {
    pub id: Option<String>,
    pub name: String,
    pub address: String,
    pub notes: String,
    pub links: Vec<RepositoryLinkDraft>,
}

/// A source link inside a repository draft.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryLinkDraft {
    pub source_id: String,
    pub call_number: String,
    pub media_type: String,
}

fn clean(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Lists repositories with their linked-source counts.
pub fn list_repositories(conn: &Connection) -> Result<Vec<RepositorySummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.name, r.address,
                    (SELECT COUNT(*) FROM source_repositories sr
                     WHERE sr.repository_id = r.id)
             FROM repositories r
             ORDER BY r.name COLLATE NOCASE, r.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RepositorySummary {
                id: row.get(0)?,
                name: row.get(1)?,
                address: row.get(2)?,
                source_count: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads one repository and its linked sources.
pub fn load_repository(conn: &Connection, id: &str) -> Result<Option<RepositoryRecord>, String> {
    let record = conn
        .query_row(
            "SELECT id, name, address, notes FROM repositories WHERE id = ?1",
            params![id],
            |row| {
                Ok(RepositoryRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    address: row.get(2)?,
                    notes: row.get(3)?,
                    links: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(mut record) = record else {
        return Ok(None);
    };

    let mut stmt = conn
        .prepare(
            "SELECT sr.source_id, s.title, sr.call_number, sr.media_type
             FROM source_repositories sr
             JOIN sources s ON s.id = sr.source_id
             WHERE sr.repository_id = ?1
             ORDER BY s.title COLLATE NOCASE, s.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![id], |row| {
            Ok(RepositorySourceLink {
                source_id: row.get(0)?,
                source_title: row.get(1)?,
                call_number: row.get(2)?,
                media_type: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    record.links = rows;
    Ok(Some(record))
}

/// Inserts or updates a repository and replaces its source links.
pub fn save_repository(conn: &mut Connection, draft: &RepositoryDraft) -> Result<String, String> {
    let name = clean(&draft.name).ok_or_else(|| "Repository name is required".to_string())?;
    let address = clean(&draft.address);
    let notes = clean(&draft.notes);
    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut seen = HashSet::new();
    for link in &draft.links {
        if !seen.insert(link.source_id.as_str()) {
            return Err(format!("source {} is linked twice", link.source_id));
        }
    }

    let tx = conn.transaction().map_err(|error| error.to_string())?;
    for link in &draft.links {
        let exists = tx
            .query_row(
                "SELECT 1 FROM sources WHERE id = ?1",
                params![link.source_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if exists.is_none() {
            return Err(format!("no source with id {}", link.source_id));
        }
    }

    let changed = tx
        .execute(
            "UPDATE repositories
             SET name = ?1, address = ?2, notes = ?3, updated_at = datetime('now')
             WHERE id = ?4",
            params![name, address, notes, id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        tx.execute(
            "INSERT INTO repositories (id, name, address, notes) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, address, notes],
        )
        .map_err(|error| error.to_string())?;
    }

    tx.execute(
        "DELETE FROM source_repositories WHERE repository_id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    for link in &draft.links {
        tx.execute(
            "INSERT INTO source_repositories
             (source_id, repository_id, call_number, media_type)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                link.source_id,
                id,
                clean(&link.call_number),
                clean(&link.media_type)
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    tx.commit().map_err(|error| error.to_string())?;
    Ok(id)
}

/// Deletes a repository; its source links cascade.
pub fn delete_repository(conn: &mut Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM repositories WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no repository with id {id}"));
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
        conn
    }

    fn source(conn: &Connection, id: &str, title: &str) {
        conn.execute(
            "INSERT INTO sources (id, title) VALUES (?1, ?2)",
            params![id, title],
        )
        .expect("source insert");
    }

    #[test]
    fn saves_and_loads_repository_links() {
        let mut conn = connection();
        source(&conn, "s1", "Parish Book");
        source(&conn, "s2", "Census");
        let draft = RepositoryDraft {
            name: "  Uppsala Archive  ".to_string(),
            address: "Main Street\nUppsala".to_string(),
            notes: "Reading room".to_string(),
            links: vec![
                RepositoryLinkDraft {
                    source_id: "s1".to_string(),
                    call_number: "12/34".to_string(),
                    media_type: "film".to_string(),
                },
                RepositoryLinkDraft {
                    source_id: "s2".to_string(),
                    call_number: String::new(),
                    media_type: "microfilm".to_string(),
                },
            ],
            ..RepositoryDraft::default()
        };

        let id = save_repository(&mut conn, &draft).expect("repository saves");
        let loaded = load_repository(&conn, &id)
            .expect("load")
            .expect("repository exists");
        assert_eq!(loaded.name, "Uppsala Archive");
        assert_eq!(loaded.address.as_deref(), Some("Main Street\nUppsala"));
        assert_eq!(loaded.notes.as_deref(), Some("Reading room"));
        assert_eq!(loaded.links.len(), 2);
        assert_eq!(loaded.links[0].source_title, "Census");
        assert_eq!(loaded.links[0].call_number, None);
        assert_eq!(loaded.links[0].media_type.as_deref(), Some("microfilm"));
        assert_eq!(loaded.links[1].source_title, "Parish Book");
        assert_eq!(loaded.links[1].call_number.as_deref(), Some("12/34"));

        let list = list_repositories(&conn).expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].source_count, 2);
    }

    #[test]
    fn updating_replaces_the_links() {
        let mut conn = connection();
        source(&conn, "s1", "Parish Book");
        source(&conn, "s2", "Census");
        let mut draft = RepositoryDraft {
            name: "Archive".to_string(),
            links: vec![RepositoryLinkDraft {
                source_id: "s1".to_string(),
                call_number: "1".to_string(),
                media_type: String::new(),
            }],
            ..RepositoryDraft::default()
        };
        let id = save_repository(&mut conn, &draft).expect("repository saves");
        draft.id = Some(id.clone());
        draft.links = vec![RepositoryLinkDraft {
            source_id: "s2".to_string(),
            call_number: "2".to_string(),
            media_type: "film".to_string(),
        }];
        save_repository(&mut conn, &draft).expect("repository updates");

        let loaded = load_repository(&conn, &id)
            .expect("load")
            .expect("repository exists");
        assert_eq!(loaded.links.len(), 1);
        assert_eq!(loaded.links[0].source_id, "s2");
        assert_eq!(loaded.links[0].call_number.as_deref(), Some("2"));
        let count = list_repositories(&conn).expect("list")[0].source_count;
        assert_eq!(count, 1);
    }

    #[test]
    fn requires_a_name_and_known_sources() {
        let mut conn = connection();
        source(&conn, "s1", "Parish Book");
        assert!(
            save_repository(
                &mut conn,
                &RepositoryDraft {
                    name: "   ".to_string(),
                    ..RepositoryDraft::default()
                }
            )
            .is_err()
        );
        assert!(
            save_repository(
                &mut conn,
                &RepositoryDraft {
                    name: "Archive".to_string(),
                    links: vec![RepositoryLinkDraft {
                        source_id: "missing".to_string(),
                        ..RepositoryLinkDraft::default()
                    }],
                    ..RepositoryDraft::default()
                }
            )
            .is_err()
        );
        assert!(
            save_repository(
                &mut conn,
                &RepositoryDraft {
                    name: "Archive".to_string(),
                    links: vec![
                        RepositoryLinkDraft {
                            source_id: "s1".to_string(),
                            ..RepositoryLinkDraft::default()
                        },
                        RepositoryLinkDraft {
                            source_id: "s1".to_string(),
                            ..RepositoryLinkDraft::default()
                        },
                    ],
                    ..RepositoryDraft::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn deleting_cascades_the_source_links() {
        let mut conn = connection();
        source(&conn, "s1", "Parish Book");
        let id = save_repository(
            &mut conn,
            &RepositoryDraft {
                name: "Archive".to_string(),
                links: vec![RepositoryLinkDraft {
                    source_id: "s1".to_string(),
                    ..RepositoryLinkDraft::default()
                }],
                ..RepositoryDraft::default()
            },
        )
        .expect("repository saves");

        delete_repository(&mut conn, &id).expect("repository deletes");

        assert!(load_repository(&conn, &id).expect("load").is_none());
        let links: i64 = conn
            .query_row("SELECT COUNT(*) FROM source_repositories", [], |row| {
                row.get(0)
            })
            .expect("link count");
        assert_eq!(links, 0);
    }
}
