//! Research tasks, folders and links to genealogical records.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::model::name::format_name;

/// Whether a research task is still open or finished.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    Open,
    Done,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Done => "done",
        }
    }

    pub fn from_db(value: &str) -> Self {
        if value.eq_ignore_ascii_case("done") {
            Self::Done
        } else {
            Self::Open
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Done => "Done",
        }
    }
}

/// A record a task can be linked to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOwner {
    Person(String),
    Family(String),
    Event(String),
    Source(String),
    Citation(String),
    Place(String),
    Media(String),
}

impl TaskOwner {
    pub(crate) fn column(&self) -> &'static str {
        match self {
            Self::Person(_) => "person_id",
            Self::Family(_) => "family_id",
            Self::Event(_) => "event_id",
            Self::Source(_) => "source_id",
            Self::Citation(_) => "citation_id",
            Self::Place(_) => "place_id",
            Self::Media(_) => "media_id",
        }
    }

    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Person(id)
            | Self::Family(id)
            | Self::Event(id)
            | Self::Source(id)
            | Self::Citation(id)
            | Self::Place(id)
            | Self::Media(id) => id,
        }
    }

    fn table(&self) -> &'static str {
        match self {
            Self::Person(_) => "people",
            Self::Family(_) => "families",
            Self::Event(_) => "events",
            Self::Source(_) => "sources",
            Self::Citation(_) => "citations",
            Self::Place(_) => "places",
            Self::Media(_) => "media",
        }
    }
}

/// One stored task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub priority: Option<i64>,
    pub folder: Option<String>,
    pub due_date: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A task row with its linked-record count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub priority: Option<i64>,
    pub folder: Option<String>,
    pub due_date: Option<String>,
    pub completed_at: Option<String>,
    pub link_count: i64,
}

/// Editable task fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskDraft {
    pub id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: Option<i64>,
    pub folder: String,
    pub due_date: String,
}

impl TaskDraft {
    pub fn from_record(record: &TaskRecord) -> Self {
        Self {
            id: Some(record.id.clone()),
            title: record.title.clone(),
            description: record.description.clone().unwrap_or_default(),
            status: record.status,
            priority: record.priority,
            folder: record.folder.clone().unwrap_or_default(),
            due_date: record.due_date.clone().unwrap_or_default(),
        }
    }
}

/// One record linked to a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLinkSummary {
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

fn row_status(value: String) -> TaskStatus {
    TaskStatus::from_db(&value)
}

/// Lists every task with its link count.
pub fn list_tasks(conn: &Connection) -> Result<Vec<TaskSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.title, t.status, t.priority, t.folder, t.due_date,
                    t.completed_at,
                    (SELECT COUNT(*) FROM task_links l WHERE l.task_id = t.id)
             FROM tasks t
             ORDER BY CASE t.status WHEN 'open' THEN 0 ELSE 1 END,
                      t.priority IS NULL, t.priority DESC,
                      t.due_date IS NULL, t.due_date, t.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(TaskSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                status: row_status(row.get(2)?),
                priority: row.get(3)?,
                folder: row.get(4)?,
                due_date: row.get(5)?,
                completed_at: row.get(6)?,
                link_count: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Loads one task for editing.
pub fn load_task(conn: &Connection, id: &str) -> Result<Option<TaskRecord>, String> {
    conn.query_row(
        "SELECT id, title, description, status, priority, folder, due_date,
                completed_at, created_at, updated_at
         FROM tasks WHERE id = ?1",
        params![id],
        |row| {
            let status: String = row.get(3)?;
            Ok(TaskRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: TaskStatus::from_db(&status),
                priority: row.get(4)?,
                folder: row.get(5)?,
                due_date: row.get(6)?,
                completed_at: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// Inserts or updates a task, maintaining the completion time from status.
pub fn save_task(conn: &Connection, draft: &TaskDraft) -> Result<String, String> {
    let title = clean(&draft.title).ok_or_else(|| "task title is required".to_string())?;
    let description = clean(&draft.description);
    let folder = clean(&draft.folder);
    let due_date = clean(&draft.due_date);
    let status = draft.status.as_str();
    let existing_completed: Option<String> = match &draft.id {
        Some(id) => conn
            .query_row(
                "SELECT completed_at FROM tasks WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .flatten(),
        None => None,
    };
    let completed_at = if draft.status == TaskStatus::Done {
        match existing_completed {
            Some(value) => Some(value),
            None => Some(
                conn.query_row("SELECT datetime('now')", [], |row| row.get(0))
                    .map_err(|error| error.to_string())?,
            ),
        }
    } else {
        None
    };
    let id = draft
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let changed = match &draft.id {
        Some(existing) => conn
            .execute(
                "UPDATE tasks SET title = ?1, description = ?2, status = ?3,
                    priority = ?4, folder = ?5, due_date = ?6, completed_at = ?7,
                    updated_at = datetime('now')
                 WHERE id = ?8",
                params![
                    title,
                    description,
                    status,
                    draft.priority,
                    folder,
                    due_date,
                    completed_at,
                    existing
                ],
            )
            .map_err(|error| error.to_string())?,
        None => conn
            .execute(
                "INSERT INTO tasks
                 (id, title, description, status, priority, folder, due_date, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    title,
                    description,
                    status,
                    draft.priority,
                    folder,
                    due_date,
                    completed_at
                ],
            )
            .map_err(|error| error.to_string())?,
    };
    if changed == 0 {
        return Err(format!("no task with id {id}"));
    }
    Ok(id)
}

/// Deletes one task and its links.
pub fn delete_task(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err(format!("no task with id {id}"));
    }
    Ok(())
}

/// Lists distinct non-empty folders in task order.
pub fn list_folders(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT folder FROM tasks
             WHERE folder IS NOT NULL AND trim(folder) <> ''
             ORDER BY folder COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Lists tasks linked to one record.
pub fn list_owner_tasks(conn: &Connection, owner: &TaskOwner) -> Result<Vec<TaskSummary>, String> {
    let sql = format!(
        "SELECT t.id, t.title, t.status, t.priority, t.folder, t.due_date,
                t.completed_at,
                (SELECT COUNT(*) FROM task_links l WHERE l.task_id = t.id)
         FROM tasks t
         JOIN task_links l ON l.task_id = t.id
         WHERE l.{} = ?1
         ORDER BY CASE t.status WHEN 'open' THEN 0 ELSE 1 END,
                  t.priority IS NULL, t.priority DESC,
                  t.due_date IS NULL, t.due_date, t.rowid",
        owner.column()
    );
    let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![owner.id()], |row| {
            Ok(TaskSummary {
                id: row.get(0)?,
                title: row.get(1)?,
                status: row_status(row.get(2)?),
                priority: row.get(3)?,
                folder: row.get(4)?,
                due_date: row.get(5)?,
                completed_at: row.get(6)?,
                link_count: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

/// Attaches a task to one record, ignoring an existing duplicate.
pub fn attach_task(conn: &Connection, task_id: &str, owner: &TaskOwner) -> Result<(), String> {
    ensure_exists(conn, "tasks", task_id)?;
    ensure_exists(conn, owner.table(), owner.id())?;
    if is_attached(conn, task_id, owner)? {
        return Ok(());
    }
    let sql = format!(
        "INSERT INTO task_links (task_id, {}) VALUES (?1, ?2)",
        owner.column()
    );
    conn.execute(&sql, params![task_id, owner.id()])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Detaches one task from one record.
pub fn detach_task(conn: &Connection, task_id: &str, owner: &TaskOwner) -> Result<(), String> {
    let sql = format!(
        "DELETE FROM task_links WHERE task_id = ?1 AND {} = ?2",
        owner.column()
    );
    let changed = conn
        .execute(&sql, params![task_id, owner.id()])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("task link no longer exists".to_string());
    }
    Ok(())
}

/// Lists the records linked to one task.
pub fn list_task_links(
    conn: &Connection,
    task_id: &str,
    surname_first: bool,
) -> Result<Vec<TaskLinkSummary>, String> {
    let mut links = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name_prefix, p.given_name, p.middle_name, p.surname, p.name_suffix
             FROM task_links l JOIN people p ON p.id = l.person_id
             WHERE l.task_id = ?1 AND l.person_id IS NOT NULL ORDER BY p.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            let name = format_name(
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
                &row.get::<_, String>(4)?,
                &row.get::<_, String>(5)?,
                surname_first,
            );
            Ok(TaskLinkSummary {
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
            "SELECT f.id FROM task_links l JOIN families f ON f.id = l.family_id
             WHERE l.task_id = ?1 AND l.family_id IS NOT NULL ORDER BY f.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok(TaskLinkSummary {
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
             FROM task_links l JOIN events e ON e.id = l.event_id
             LEFT JOIN fact_types ft ON ft.key = e.event_type
             WHERE l.task_id = ?1 AND l.event_id IS NOT NULL ORDER BY e.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            let label: String = row.get(1)?;
            let date: Option<String> = row.get(2)?;
            Ok(TaskLinkSummary {
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
            "SELECT s.id, s.title FROM task_links l JOIN sources s ON s.id = l.source_id
             WHERE l.task_id = ?1 AND l.source_id IS NOT NULL ORDER BY s.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok(TaskLinkSummary {
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
            "SELECT c.id, s.title, c.page_or_reference
             FROM task_links l JOIN citations c ON c.id = l.citation_id
             JOIN sources s ON s.id = c.source_id
             WHERE l.task_id = ?1 AND l.citation_id IS NOT NULL ORDER BY c.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            let title: String = row.get(1)?;
            let page: Option<String> = row.get(2)?;
            Ok(TaskLinkSummary {
                owner_type: "citation".to_string(),
                owner_id: row.get(0)?,
                label: match page {
                    Some(page) => format!("{title} — {page}"),
                    None => title,
                },
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    links.extend(rows);

    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name FROM task_links l JOIN places p ON p.id = l.place_id
             WHERE l.task_id = ?1 AND l.place_id IS NOT NULL ORDER BY p.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok(TaskLinkSummary {
                owner_type: "place".to_string(),
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
            "SELECT m.id, COALESCE(m.caption, m.file_path)
             FROM task_links l JOIN media m ON m.id = l.media_id
             WHERE l.task_id = ?1 AND l.media_id IS NOT NULL ORDER BY m.rowid",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok(TaskLinkSummary {
                owner_type: "media".to_string(),
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

fn is_attached(conn: &Connection, task_id: &str, owner: &TaskOwner) -> Result<bool, String> {
    let sql = format!(
        "SELECT 1 FROM task_links WHERE task_id = ?1 AND {} = ?2",
        owner.column()
    );
    conn.query_row(&sql, params![task_id, owner.id()], |row| {
        row.get::<_, i64>(0)
    })
    .optional()
    .map(|found| found.is_some())
    .map_err(|error| error.to_string())
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
             INSERT INTO citations (id, source_id) VALUES ('c1', 's1');
             INSERT INTO places (id, name) VALUES ('pl1', 'Uppsala');
             INSERT INTO media (id, file_path) VALUES ('m1', 'photo.jpg');",
        )
        .expect("records");
        conn
    }

    fn draft(title: &str) -> TaskDraft {
        TaskDraft {
            title: title.to_string(),
            ..TaskDraft::default()
        }
    }

    #[test]
    fn saves_updates_completion_and_folders() {
        let conn = connection();
        let mut task = draft("Find the birth record");
        task.description = "Search the parish books".to_string();
        task.priority = Some(3);
        task.folder = "Parish records".to_string();
        task.due_date = "2026-01-01".to_string();
        let id = save_task(&conn, &task).expect("save");
        let loaded = load_task(&conn, &id).expect("load").expect("task");
        assert_eq!(loaded.status, TaskStatus::Open);
        assert!(loaded.completed_at.is_none());
        assert_eq!(loaded.priority, Some(3));

        let mut done = TaskDraft::from_record(&loaded);
        done.status = TaskStatus::Done;
        save_task(&conn, &done).expect("complete");
        let loaded = load_task(&conn, &id).expect("load").expect("task");
        assert!(loaded.completed_at.is_some());
        let completed = loaded.completed_at.clone();

        save_task(&conn, &done).expect("re-save done");
        let loaded = load_task(&conn, &id).expect("load").expect("task");
        assert_eq!(loaded.completed_at, completed, "completion time is kept");

        done.status = TaskStatus::Open;
        save_task(&conn, &done).expect("reopen");
        let loaded = load_task(&conn, &id).expect("load").expect("task");
        assert!(loaded.completed_at.is_none());
        assert_eq!(
            list_folders(&conn).expect("folders"),
            ["Parish records".to_string()]
        );
        assert!(save_task(&conn, &draft("  ")).is_err());
    }

    #[test]
    fn links_every_owner_type_without_duplicates() {
        let conn = connection();
        let id = save_task(&conn, &draft("Research")).expect("save");
        for owner in [
            TaskOwner::Person("p1".to_string()),
            TaskOwner::Family("f1".to_string()),
            TaskOwner::Event("e1".to_string()),
            TaskOwner::Source("s1".to_string()),
            TaskOwner::Citation("c1".to_string()),
            TaskOwner::Place("pl1".to_string()),
            TaskOwner::Media("m1".to_string()),
        ] {
            attach_task(&conn, &id, &owner).expect("attach");
            attach_task(&conn, &id, &owner).expect("idempotent attach");
            assert_eq!(
                list_owner_tasks(&conn, &owner).expect("owner tasks").len(),
                1
            );
        }
        let links = list_task_links(&conn, &id, false).expect("links");
        assert_eq!(links.len(), 7);
        assert!(links.iter().any(|link| link.label == "Ada King"));
        assert!(links.iter().any(|link| link.label == "Parish Book"));
        assert!(links.iter().any(|link| link.label == "photo.jpg"));
        assert_eq!(list_tasks(&conn).expect("list")[0].link_count, 7);

        detach_task(&conn, &id, &TaskOwner::Person("p1".to_string())).expect("detach");
        assert_eq!(list_task_links(&conn, &id, false).expect("links").len(), 6);
    }

    #[test]
    fn deleting_task_or_owner_cascades_links() {
        let conn = connection();
        let id = save_task(&conn, &draft("Research")).expect("save");
        attach_task(&conn, &id, &TaskOwner::Person("p1".to_string())).expect("attach");
        conn.execute("DELETE FROM people WHERE id = 'p1'", [])
            .expect("delete person");
        assert!(
            list_task_links(&conn, &id, false)
                .expect("links")
                .is_empty()
        );

        attach_task(&conn, &id, &TaskOwner::Place("pl1".to_string())).expect("attach");
        delete_task(&conn, &id).expect("delete task");
        assert!(
            list_owner_tasks(&conn, &TaskOwner::Place("pl1".to_string()))
                .expect("owner tasks")
                .is_empty()
        );
    }
}
