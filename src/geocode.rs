//! Opt-in OpenStreetMap Nominatim lookups and a cache-aware batch runner.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use ureq::Agent;

use crate::db::geocode_cache::{self, CachedGeocode};
use crate::model::geocode::{
    GeocodeOutcome, GeocodeReport, GeocodeRequest, GeocodeResult, normalize_query,
};

const SEARCH_URL: &str = "https://nominatim.openstreetmap.org/search";
const USER_AGENT: &str =
    "stemma-flow/0.1 (genealogy desktop app; https://github.com/opentaikai/stemma-flow)";

/// One successful Nominatim match.
#[derive(Debug, Clone, PartialEq)]
pub struct GeocodeHit {
    pub display_name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(serde::Deserialize)]
struct NominatimHit {
    lat: String,
    lon: String,
    display_name: String,
}

/// Builds the shared HTTP agent used by a batch lookup.
pub fn agent() -> Agent {
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .user_agent(USER_AGENT)
        .build()
        .into();
    agent
}

/// Looks up one place, returning the first provider match.
pub fn lookup(agent: &Agent, query: &str) -> Result<Option<GeocodeHit>, String> {
    let mut response = agent
        .get(SEARCH_URL)
        .query("q", query)
        .query("format", "jsonv2")
        .query("limit", "1")
        .query("addressdetails", "0")
        .call()
        .map_err(|error| error.to_string())?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    parse_response(&body)
}

/// Parses a Nominatim search response into at most one validated hit.
pub fn parse_response(body: &str) -> Result<Option<GeocodeHit>, String> {
    let hits: Vec<NominatimHit> = serde_json::from_str(body)
        .map_err(|error| format!("invalid geocoding response: {error}"))?;
    let Some(hit) = hits.into_iter().next() else {
        return Ok(None);
    };
    let latitude = hit
        .lat
        .parse::<f64>()
        .map_err(|_| format!("invalid latitude \"{}\"", hit.lat))?;
    let longitude = hit
        .lon
        .parse::<f64>()
        .map_err(|_| format!("invalid longitude \"{}\"", hit.lon))?;
    if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
        return Err(format!("latitude {} is out of range", hit.lat));
    }
    if !longitude.is_finite() || !(-180.0..=180.0).contains(&longitude) {
        return Err(format!("longitude {} is out of range", hit.lon));
    }
    Ok(Some(GeocodeHit {
        display_name: hit.display_name,
        latitude,
        longitude,
    }))
}

/// Runs a cache-first batch lookup, stopping on cancellation or the first error.
pub fn run_batch(
    conn: &Connection,
    requests: &[GeocodeRequest],
    cancel: &AtomicBool,
    mut lookup: impl FnMut(&str) -> Result<Option<GeocodeHit>, String>,
    mut progress: impl FnMut(usize, usize),
) -> GeocodeReport {
    let total = requests.len();
    let mut report = GeocodeReport::default();
    progress(0, total);
    for (index, request) in requests.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let query = normalize_query(&request.place_name);
        let (outcome, cached) = match geocode_cache::get(conn, &query) {
            Ok(Some(entry)) => (cached_outcome(entry), true),
            _ => match lookup(&query) {
                Ok(Some(hit)) => {
                    let _ = geocode_cache::put(
                        conn,
                        &query,
                        &CachedGeocode {
                            display_name: Some(hit.display_name.clone()),
                            latitude: Some(hit.latitude),
                            longitude: Some(hit.longitude),
                            found: true,
                        },
                    );
                    (
                        GeocodeOutcome::Found {
                            display_name: hit.display_name,
                            latitude: hit.latitude,
                            longitude: hit.longitude,
                        },
                        false,
                    )
                }
                Ok(None) => {
                    let _ = geocode_cache::put(
                        conn,
                        &query,
                        &CachedGeocode {
                            display_name: None,
                            latitude: None,
                            longitude: None,
                            found: false,
                        },
                    );
                    (GeocodeOutcome::NotFound, false)
                }
                Err(message) => {
                    report.results.push(GeocodeResult {
                        place_id: request.place_id.clone(),
                        place_name: request.place_name.clone(),
                        query,
                        outcome: GeocodeOutcome::Failed(message.clone()),
                        cached: false,
                    });
                    report.error = Some(message);
                    break;
                }
            },
        };
        report.results.push(GeocodeResult {
            place_id: request.place_id.clone(),
            place_name: request.place_name.clone(),
            query,
            outcome,
            cached,
        });
        progress(index + 1, total);
    }
    report
}

fn cached_outcome(entry: CachedGeocode) -> GeocodeOutcome {
    if entry.found
        && let (Some(display_name), Some(latitude), Some(longitude)) =
            (entry.display_name, entry.latitude, entry.longitude)
    {
        return GeocodeOutcome::Found {
            display_name,
            latitude,
            longitude,
        };
    }
    GeocodeOutcome::NotFound
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

    fn request(id: &str, name: &str) -> GeocodeRequest {
        GeocodeRequest {
            place_id: id.to_string(),
            place_name: name.to_string(),
        }
    }

    fn hit(name: &str, latitude: f64, longitude: f64) -> GeocodeHit {
        GeocodeHit {
            display_name: name.to_string(),
            latitude,
            longitude,
        }
    }

    #[test]
    fn parses_found_empty_and_malformed_responses() {
        let found = parse_response(
            r#"[{"lat":"59.8586","lon":"17.6389","display_name":"Uppsala, Sweden"}]"#,
        )
        .expect("response parses")
        .expect("one hit");
        assert_eq!(found.display_name, "Uppsala, Sweden");
        assert_eq!(found.latitude, 59.8586);
        assert_eq!(found.longitude, 17.6389);

        assert_eq!(parse_response("[]").expect("empty parses"), None);
        assert!(parse_response("not json").is_err());
        assert!(parse_response(r#"[{"lat":"91","lon":"17","display_name":"Bad"}]"#).is_err());
    }

    #[test]
    fn caches_hits_and_negative_results() {
        let conn = connection();
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        let report = run_batch(
            &conn,
            &[request("p1", "Uppsala, Sweden"), request("p2", "Nowhere")],
            &cancel,
            |query| {
                calls += 1;
                if query == "uppsala, sweden" {
                    Ok(Some(hit("Uppsala, Sweden", 59.8586, 17.6389)))
                } else {
                    Ok(None)
                }
            },
            |_, _| {},
        );
        assert_eq!(calls, 2);
        assert_eq!(report.results.len(), 2);
        assert_eq!(report.found_count(), 1);
        assert_eq!(report.not_found_count(), 1);
        assert!(!report.results[0].cached);

        let mut calls = 0;
        let report = run_batch(
            &conn,
            &[request("p1", "Uppsala, Sweden"), request("p2", "Nowhere")],
            &cancel,
            |_| {
                calls += 1;
                panic!("cache hits must not call the provider");
            },
            |_, _| {},
        );
        assert_eq!(calls, 0);
        assert!(report.results.iter().all(|result| result.cached));
        assert_eq!(report.results[0].coordinates(), Some((59.8586, 17.6389)));
    }

    #[test]
    fn cancellation_stops_before_the_next_lookup() {
        let conn = connection();
        let cancel = AtomicBool::new(true);
        let report = run_batch(
            &conn,
            &[request("p1", "Uppsala")],
            &cancel,
            |_| panic!("cancelled batches must not look up"),
            |_, _| {},
        );
        assert!(report.cancelled);
        assert!(report.results.is_empty());
    }

    #[test]
    fn errors_keep_partial_results_and_stop() {
        let conn = connection();
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        let report = run_batch(
            &conn,
            &[request("p1", "One"), request("p2", "Two")],
            &cancel,
            |query| {
                calls += 1;
                if query == "one" {
                    Ok(Some(hit("One", 1.0, 2.0)))
                } else {
                    Err("rate limited".to_string())
                }
            },
            |_, _| {},
        );
        assert_eq!(calls, 2);
        assert_eq!(report.found_count(), 1);
        assert_eq!(report.failed_count(), 1);
        assert_eq!(report.error.as_deref(), Some("rate limited"));
        assert_eq!(report.results.len(), 2);
    }

    #[test]
    fn progress_reports_each_completed_lookup() {
        let conn = connection();
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        run_batch(
            &conn,
            &[request("p1", "One"), request("p2", "Two")],
            &cancel,
            |_| Ok(None),
            |done, total| seen.push((done, total)),
        );
        assert_eq!(seen, [(0, 2), (1, 2), (2, 2)]);
    }
}
