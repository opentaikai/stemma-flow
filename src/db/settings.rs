//! Per-tree display preferences.

use rusqlite::{Connection, OptionalExtension, params};

/// Preferences stored in the tree itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeSettings {
    pub surname_first: bool,
    pub root_person_id: Option<String>,
}

/// Loads the preferences, defaulting to normal name order.
pub fn load_tree_settings(conn: &Connection) -> Result<TreeSettings, String> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM tree_settings WHERE key = 'surname_first'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let root_person_id: Option<String> = conn
        .query_row(
            "SELECT value FROM tree_settings WHERE key = 'root_person_id'",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten()
        .filter(|value| !value.trim().is_empty());
    Ok(TreeSettings {
        surname_first: matches!(value.as_deref(), Some("1" | "true" | "TRUE")),
        root_person_id,
    })
}

/// Stores the surname-first preference in the tree.
pub fn set_surname_first(conn: &Connection, surname_first: bool) -> Result<(), String> {
    conn.execute(
        "INSERT INTO tree_settings (key, value) VALUES ('surname_first', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![if surname_first { "1" } else { "0" }],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Stores or clears the tree's root person.
pub fn set_root_person(conn: &Connection, person_id: Option<&str>) -> Result<(), String> {
    match person_id {
        Some(id) => conn.execute(
            "INSERT INTO tree_settings (key, value) VALUES ('root_person_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![id],
        ),
        None => conn.execute("DELETE FROM tree_settings WHERE key = 'root_person_id'", []),
    }
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

    #[test]
    fn defaults_to_normal_order_and_persists() {
        let conn = connection();
        assert!(!load_tree_settings(&conn).expect("settings").surname_first);
        set_surname_first(&conn, true).expect("set");
        assert!(load_tree_settings(&conn).expect("settings").surname_first);
        set_surname_first(&conn, false).expect("set");
        assert!(!load_tree_settings(&conn).expect("settings").surname_first);
    }

    #[test]
    fn settings_are_per_tree() {
        let first = connection();
        let second = connection();
        set_surname_first(&first, true).expect("set first");
        assert!(load_tree_settings(&first).expect("first").surname_first);
        assert!(!load_tree_settings(&second).expect("second").surname_first);
    }

    #[test]
    fn root_person_persists_and_clears_per_tree() {
        let first = connection();
        let second = connection();
        assert!(
            load_tree_settings(&first)
                .expect("default")
                .root_person_id
                .is_none()
        );
        set_root_person(&first, Some("p1")).expect("set");
        assert_eq!(
            load_tree_settings(&first)
                .expect("load")
                .root_person_id
                .as_deref(),
            Some("p1")
        );
        assert!(
            load_tree_settings(&second)
                .expect("second")
                .root_person_id
                .is_none()
        );
        set_root_person(&first, None).expect("clear");
        assert!(
            load_tree_settings(&first)
                .expect("clear load")
                .root_person_id
                .is_none()
        );
    }
}
