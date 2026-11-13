//! Tree-local cache of geocoding lookups.

use rusqlite::{Connection, OptionalExtension, params};

/// A cached positive or negative geocoding result.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedGeocode {
    pub display_name: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub found: bool,
}

/// Loads a cached result for a normalized query.
pub fn get(conn: &Connection, query: &str) -> Result<Option<CachedGeocode>, String> {
    conn.query_row(
        "SELECT display_name, latitude, longitude, found
         FROM geocode_cache WHERE query = ?1",
        params![query],
        |row| {
            Ok(CachedGeocode {
                display_name: row.get(0)?,
                latitude: row.get(1)?,
                longitude: row.get(2)?,
                found: row.get::<_, i64>(3)? != 0,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// Inserts or replaces a cached result and refreshes its timestamp.
pub fn put(conn: &Connection, query: &str, entry: &CachedGeocode) -> Result<(), String> {
    conn.execute(
        "INSERT INTO geocode_cache (query, display_name, latitude, longitude, found)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(query) DO UPDATE SET
             display_name = excluded.display_name,
             latitude = excluded.latitude,
             longitude = excluded.longitude,
             found = excluded.found,
             fetched_at = datetime('now')",
        params![
            query,
            entry.display_name,
            entry.latitude,
            entry.longitude,
            entry.found as i64
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Removes every cached lookup for the tree, returning the row count.
pub fn clear(conn: &Connection) -> Result<usize, String> {
    conn.execute("DELETE FROM geocode_cache", [])
        .map_err(|error| error.to_string())
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
    fn stores_and_replaces_found_results() {
        let conn = connection();
        put(
            &conn,
            "paris, france",
            &CachedGeocode {
                display_name: Some("Paris, France".to_string()),
                latitude: Some(48.8566),
                longitude: Some(2.3522),
                found: true,
            },
        )
        .expect("cache write");
        let cached = get(&conn, "paris, france")
            .expect("cache read")
            .expect("entry exists");
        assert_eq!(cached.display_name.as_deref(), Some("Paris, France"));
        assert_eq!(cached.latitude, Some(48.8566));
        assert_eq!(cached.longitude, Some(2.3522));
        assert!(cached.found);

        put(
            &conn,
            "paris, france",
            &CachedGeocode {
                display_name: None,
                latitude: None,
                longitude: None,
                found: false,
            },
        )
        .expect("cache replacement");
        let cached = get(&conn, "paris, france")
            .expect("cache read")
            .expect("entry exists");
        assert!(!cached.found);
        assert_eq!(cached.latitude, None);
    }

    #[test]
    fn missing_queries_and_cleared_tables_return_nothing() {
        let conn = connection();
        assert!(get(&conn, "nowhere").expect("cache read").is_none());
        put(
            &conn,
            "nowhere",
            &CachedGeocode {
                display_name: None,
                latitude: None,
                longitude: None,
                found: false,
            },
        )
        .expect("cache write");
        assert_eq!(clear(&conn).expect("cache clear"), 1);
        assert!(get(&conn, "nowhere").expect("cache read").is_none());
    }
}
