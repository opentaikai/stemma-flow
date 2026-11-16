//! Tree-wide record counts for administrative reports.

use rusqlite::Connection;

/// Counts of the main record tables plus event-type usage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeStatistics {
    pub people: i64,
    pub families: i64,
    pub children: i64,
    pub events: i64,
    pub places: i64,
    pub sources: i64,
    pub citations: i64,
    pub repositories: i64,
    pub media: i64,
    pub notes: i64,
    pub tasks: i64,
    pub groups: i64,
    pub labels: i64,
    pub albums: i64,
    pub associations: i64,
    pub bookmarks: i64,
    /// Event labels with their record counts, most frequent first.
    pub event_types: Vec<(String, i64)>,
}

/// Loads tree-wide statistics from one database connection.
pub fn load(conn: &Connection) -> Result<TreeStatistics, String> {
    let count = |table: &str| -> Result<i64, String> {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())
    };
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(f.label, e.event_type), COUNT(*)
             FROM events e
             LEFT JOIN fact_types f ON f.key = e.event_type
             GROUP BY e.event_type, COALESCE(f.label, e.event_type)
             ORDER BY COUNT(*) DESC, 1 COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let event_types = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<(String, i64)>>>()
        .map_err(|error| error.to_string())?;
    Ok(TreeStatistics {
        people: count("people")?,
        families: count("families")?,
        children: count("family_children")?,
        events: count("events")?,
        places: count("places")?,
        sources: count("sources")?,
        citations: count("citations")?,
        repositories: count("repositories")?,
        media: count("media")?,
        notes: count("notes")?,
        tasks: count("tasks")?,
        groups: count("groups")?,
        labels: count("labels")?,
        albums: count("albums")?,
        associations: count("associations")?,
        bookmarks: count("bookmarks")?,
        event_types,
    })
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
    fn empty_tree_has_zero_counts() {
        let stats = load(&connection()).expect("statistics load");
        assert_eq!(stats.people, 0);
        assert_eq!(stats.events, 0);
        assert!(stats.event_types.is_empty());
    }

    #[test]
    fn counts_records_and_groups_events_by_label() {
        let conn = connection();
        conn.execute_batch(
            "INSERT INTO places (id, name) VALUES ('pl1', 'London');
             INSERT INTO events (id, event_type, date, place_id)
                 VALUES ('e1', 'BIRTH', '1815', 'pl1');
             INSERT INTO events (id, event_type, date)
                 VALUES ('e2', 'BIRTH', '1816');
             INSERT INTO events (id, event_type, date)
                 VALUES ('e3', 'OCCUPATION', '1840');",
        )
        .expect("fixture inserts");
        let stats = load(&conn).expect("statistics load");
        assert_eq!(stats.events, 3);
        assert_eq!(stats.places, 1);
        assert_eq!(stats.people, 0);
        assert_eq!(stats.event_types.len(), 2);
        assert_eq!(stats.event_types[0].1, 2, "the most frequent type leads");
        assert!(
            stats
                .event_types
                .iter()
                .any(|(label, _)| label == "Occupation"),
            "{:?}",
            stats.event_types
        );
    }
}
