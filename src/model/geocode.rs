//! Geocoding requests, results and query normalization.

use crate::model::dataclean::normalize_place_name;

/// One place to look up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeocodeRequest {
    pub place_id: String,
    pub place_name: String,
}

/// The result of a single place lookup.
#[derive(Debug, Clone, PartialEq)]
pub enum GeocodeOutcome {
    Found {
        display_name: String,
        latitude: f64,
        longitude: f64,
    },
    NotFound,
    Failed(String),
}

/// One completed lookup, cached or fresh.
#[derive(Debug, Clone, PartialEq)]
pub struct GeocodeResult {
    pub place_id: String,
    pub place_name: String,
    pub query: String,
    pub outcome: GeocodeOutcome,
    pub cached: bool,
}

impl GeocodeResult {
    pub fn is_found(&self) -> bool {
        matches!(self.outcome, GeocodeOutcome::Found { .. })
    }

    pub fn coordinates(&self) -> Option<(f64, f64)> {
        match self.outcome {
            GeocodeOutcome::Found {
                latitude,
                longitude,
                ..
            } => Some((latitude, longitude)),
            GeocodeOutcome::NotFound | GeocodeOutcome::Failed(_) => None,
        }
    }
}

/// The complete result of one batch lookup.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GeocodeReport {
    pub results: Vec<GeocodeResult>,
    pub cancelled: bool,
    pub error: Option<String>,
}

impl GeocodeReport {
    pub fn found_count(&self) -> usize {
        self.results
            .iter()
            .filter(|result| result.is_found())
            .count()
    }

    pub fn not_found_count(&self) -> usize {
        self.results
            .iter()
            .filter(|result| matches!(result.outcome, GeocodeOutcome::NotFound))
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.results
            .iter()
            .filter(|result| matches!(result.outcome, GeocodeOutcome::Failed(_)))
            .count()
    }
}

/// Normalizes a place name into the cache and provider query key.
pub fn normalize_query(name: &str) -> String {
    normalize_place_name(name).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(id: &str, outcome: GeocodeOutcome) -> GeocodeResult {
        GeocodeResult {
            place_id: id.to_string(),
            place_name: format!("Place {id}"),
            query: format!("place {id}"),
            outcome,
            cached: false,
        }
    }

    #[test]
    fn normalizes_place_queries() {
        assert_eq!(normalize_query("  Uppsala ,  SWEDEN "), "uppsala, sweden");
        assert_eq!(normalize_query("New York,, USA"), "new york, usa");
    }

    #[test]
    fn counts_outcomes_and_exposes_coordinates() {
        let report = GeocodeReport {
            results: vec![
                result(
                    "1",
                    GeocodeOutcome::Found {
                        display_name: "Uppsala, Sweden".to_string(),
                        latitude: 59.8586,
                        longitude: 17.6389,
                    },
                ),
                result("2", GeocodeOutcome::NotFound),
                result("3", GeocodeOutcome::Failed("timeout".to_string())),
            ],
            cancelled: false,
            error: None,
        };
        assert_eq!(report.found_count(), 1);
        assert_eq!(report.not_found_count(), 1);
        assert_eq!(report.failed_count(), 1);
        assert_eq!(report.results[0].coordinates(), Some((59.8586, 17.6389)));
        assert_eq!(report.results[1].coordinates(), None);
    }
}
