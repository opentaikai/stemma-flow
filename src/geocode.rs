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

/// One gazetteer match with its Nominatim classification.
#[derive(Debug, Clone, PartialEq)]
pub struct GazetteerHit {
    pub display_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub kind: Option<String>,
    pub class: Option<String>,
}

#[derive(serde::Deserialize)]
struct NominatimHit {
    lat: String,
    lon: String,
    display_name: String,
    #[serde(default)]
    class: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
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
    Ok(parse_search_response(&request(agent, query, 1)?)?
        .into_iter()
        .next()
        .map(|hit| GeocodeHit {
            display_name: hit.display_name,
            latitude: hit.latitude,
            longitude: hit.longitude,
        }))
}

/// Searches the gazetteer, returning up to `limit` validated matches.
pub fn search(agent: &Agent, query: &str, limit: usize) -> Result<Vec<GazetteerHit>, String> {
    parse_search_response(&request(agent, query, limit)?)
}

fn request(agent: &Agent, query: &str, limit: usize) -> Result<String, String> {
    let mut response = agent
        .get(SEARCH_URL)
        .query("q", query)
        .query("format", "jsonv2")
        .query("limit", limit.to_string())
        .query("addressdetails", "0")
        .call()
        .map_err(|error| error.to_string())?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())
}

/// Parses a Nominatim search response into at most one validated hit.
pub fn parse_response(body: &str) -> Result<Option<GeocodeHit>, String> {
    Ok(parse_search_response(body)?
        .into_iter()
        .next()
        .map(|hit| GeocodeHit {
            display_name: hit.display_name,
            latitude: hit.latitude,
            longitude: hit.longitude,
        }))
}

/// Parses a Nominatim search response into every valid match.
pub fn parse_search_response(body: &str) -> Result<Vec<GazetteerHit>, String> {
    let hits: Vec<NominatimHit> = serde_json::from_str(body)
        .map_err(|error| format!("invalid geocoding response: {error}"))?;
    Ok(hits.into_iter().filter_map(valid_hit).collect())
}

fn valid_hit(hit: NominatimHit) -> Option<GazetteerHit> {
    let latitude = hit.lat.parse::<f64>().ok()?;
    let longitude = hit.lon.parse::<f64>().ok()?;
    if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
        return None;
    }
    if !longitude.is_finite() || !(-180.0..=180.0).contains(&longitude) {
        return None;
    }
    Some(GazetteerHit {
        display_name: hit.display_name,
        latitude,
        longitude,
        kind: hit.kind,
        class: hit.class,
    })
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
        assert_eq!(
            parse_response(r#"[{"lat":"91","lon":"17","display_name":"Bad"}]"#)
                .expect("invalid hits are skipped"),
            None
        );
    }

    #[test]
    fn parses_multiple_gazetteer_hits_with_types() {
        let hits = parse_search_response(
            r#"[
                {"lat":"59.8586","lon":"17.6389","display_name":"Uppsala, Sweden","class":"place","type":"city"},
                {"lat":"59.8000","lon":"17.6000","display_name":"Uppsala County","class":"boundary","type":"administrative"}
            ]"#,
        )
        .expect("search response parses");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].display_name, "Uppsala, Sweden");
        assert_eq!(hits[0].kind.as_deref(), Some("city"));
        assert_eq!(hits[0].class.as_deref(), Some("place"));
        assert_eq!(hits[1].kind.as_deref(), Some("administrative"));
    }

    #[test]
    fn skips_invalid_gazetteer_hits_but_keeps_valid_ones() {
        let hits = parse_search_response(
            r#"[
                {"lat":"not-a-number","lon":"17","display_name":"Broken"},
                {"lat":"59.8586","lon":"17.6389","display_name":"Uppsala, Sweden"},
                {"lat":"59.0","lon":"200","display_name":"Out of range"}
            ]"#,
        )
        .expect("search response parses");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].display_name, "Uppsala, Sweden");
        assert_eq!(hits[0].kind, None);
        assert_eq!(hits[0].class, None);

        assert!(
            parse_search_response("[]")
                .expect("empty search")
                .is_empty()
        );
        assert!(parse_search_response("not json").is_err());
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
