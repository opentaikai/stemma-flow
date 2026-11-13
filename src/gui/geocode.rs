//! Batch place geocoding dialog.

use std::collections::HashSet;

use egui::{Context, RichText, Ui, Window};

use crate::db::places::PlaceSummary;
use crate::i18n::{tr, tr_args, trn};
use crate::model::geocode::{GeocodeOutcome, GeocodeReport, GeocodeRequest, GeocodeResult};

/// State owned by the geocoding dialog.
#[derive(Debug, Default)]
pub struct GeocodeState {
    pub open: bool,
    pub places: Vec<PlaceSummary>,
    pub selected: HashSet<String>,
    pub filter: String,
    pub results: Vec<GeocodeResult>,
    pub selected_results: HashSet<String>,
    pub running: bool,
    pub cancelling: bool,
    pub progress: Option<(usize, usize)>,
    pub confirm: bool,
    pub error: Option<String>,
}

impl GeocodeState {
    pub fn set_places(&mut self, places: Vec<PlaceSummary>) {
        self.open = true;
        self.places = places
            .into_iter()
            .filter(|place| place.latitude.is_none() || place.longitude.is_none())
            .collect();
        self.selected.clear();
        self.filter.clear();
        self.results.clear();
        self.selected_results.clear();
        self.running = false;
        self.cancelling = false;
        self.progress = None;
        self.confirm = false;
        self.error = None;
    }

    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    pub fn select_all(&mut self) {
        self.selected = self.places.iter().map(|place| place.id.clone()).collect();
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
    }

    pub fn requests(&self) -> Vec<GeocodeRequest> {
        self.places
            .iter()
            .filter(|place| self.selected.contains(&place.id))
            .map(|place| GeocodeRequest {
                place_id: place.id.clone(),
                place_name: place.name.clone(),
            })
            .collect()
    }

    pub fn start(&mut self) {
        self.running = true;
        self.cancelling = false;
        self.confirm = false;
        self.error = None;
        self.results.clear();
        self.selected_results.clear();
        self.progress = Some((0, self.selected.len()));
    }

    pub fn finish(&mut self, report: GeocodeReport) {
        self.running = false;
        self.cancelling = false;
        self.progress = None;
        self.error = report.error.clone();
        self.selected_results = report
            .results
            .iter()
            .filter(|result| result.is_found())
            .map(|result| result.place_id.clone())
            .collect();
        self.results = report.results;
        self.confirm = false;
    }

    pub fn chosen_results(&self) -> Vec<GeocodeResult> {
        self.results
            .iter()
            .filter(|result| result.is_found() && self.selected_results.contains(&result.place_id))
            .cloned()
            .collect()
    }
}

/// What the geocoding dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq)]
pub enum GeocodeEvent {
    Start(Vec<GeocodeRequest>),
    Cancel,
    Apply(Vec<GeocodeResult>),
    ClearCache,
    OpenSettings,
}

/// Draws the geocoding dialog.
pub fn show(ctx: &Context, state: &mut GeocodeState, allow_online: bool) -> Option<GeocodeEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("geocode.title"))
        .id(egui::Id::unique("geocode_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(760.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("geocode.hint")).weak().small());
            if state.places.is_empty() {
                ui.separator();
                ui.label(RichText::new(tr("geocode.empty")).weak());
                if ui.button(tr("common.close")).clicked() {
                    state.open = false;
                }
                return;
            }

            ui.horizontal(|ui| {
                ui.label(tr("geocode.filter"));
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .hint_text(tr("geocode.filter_hint")),
                );
                if ui.button(tr("geocode.select_all")).clicked() {
                    state.select_all();
                }
                if ui.button(tr("geocode.select_none")).clicked() {
                    state.clear_selection();
                }
                ui.label(
                    RichText::new(tr_args(
                        "geocode.selected",
                        &[("count", state.selected_count().to_string())],
                    ))
                    .weak(),
                );
            });
            ui.separator();

            let needle = state.filter.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("geocode_places")
                .max_height(220.0)
                .show(ui, |ui| {
                    for place in &state.places {
                        if !needle.is_empty() && !place.name.to_lowercase().contains(&needle) {
                            continue;
                        }
                        place_row(ui, place, &mut state.selected);
                    }
                });

            if !allow_online {
                ui.separator();
                ui.label(
                    RichText::new(tr("geocode.online_disabled"))
                        .color(ui.visuals().warn_fg_color)
                        .strong(),
                );
                ui.label(RichText::new(tr("geocode.enable_hint")).weak().small());
                if ui.button(tr("geocode.open_settings")).clicked() {
                    event = Some(GeocodeEvent::OpenSettings);
                }
            } else {
                ui.label(RichText::new(tr("geocode.attribution")).weak().small());
            }

            ui.separator();
            if state.running {
                ui.horizontal(|ui| {
                    ui.spinner();
                    let progress = state.progress.unwrap_or((0, state.selected_count()));
                    ui.label(tr_args(
                        "geocode.progress",
                        &[
                            ("done", progress.0.to_string()),
                            ("total", progress.1.to_string()),
                        ],
                    ));
                    if ui
                        .add_enabled(!state.cancelling, egui::Button::new(tr("geocode.cancel")))
                        .clicked()
                    {
                        state.cancelling = true;
                        event = Some(GeocodeEvent::Cancel);
                    }
                    if state.cancelling {
                        ui.label(RichText::new(tr("geocode.cancelling")).weak());
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    let count = state.selected_count();
                    let label = trn("geocode.lookup", count as i64);
                    if ui
                        .add_enabled(
                            allow_online && count > 0,
                            egui::Button::new(RichText::new(label).strong()),
                        )
                        .clicked()
                    {
                        let requests = state.requests();
                        state.start();
                        event = Some(GeocodeEvent::Start(requests));
                    }
                    if ui.button(tr("geocode.clear_cache")).clicked() {
                        event = Some(GeocodeEvent::ClearCache);
                    }
                });
            }

            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }

            if !state.results.is_empty() {
                ui.separator();
                ui.label(RichText::new(tr("geocode.results")).strong());
                let found = state
                    .results
                    .iter()
                    .filter(|result| result.is_found())
                    .count();
                let not_found = state
                    .results
                    .iter()
                    .filter(|result| matches!(result.outcome, GeocodeOutcome::NotFound))
                    .count();
                let failed = state
                    .results
                    .iter()
                    .filter(|result| matches!(result.outcome, GeocodeOutcome::Failed(_)))
                    .count();
                ui.label(
                    RichText::new(tr_args(
                        "geocode.summary",
                        &[
                            ("found", found.to_string()),
                            ("not_found", not_found.to_string()),
                            ("failed", failed.to_string()),
                        ],
                    ))
                    .weak()
                    .small(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("geocode_results")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for result in &state.results {
                            result_row(ui, result, &mut state.selected_results);
                        }
                    });

                let chosen = state.chosen_results();
                if state.confirm {
                    ui.label(
                        RichText::new(tr_args(
                            "geocode.confirm",
                            &[("count", chosen.len().to_string())],
                        ))
                        .color(ui.visuals().error_fg_color)
                        .strong(),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !chosen.is_empty(),
                                egui::Button::new(RichText::new(tr("geocode.apply")).strong()),
                            )
                            .clicked()
                        {
                            event = Some(GeocodeEvent::Apply(chosen));
                        }
                        if ui.button(tr("common.cancel")).clicked() {
                            state.confirm = false;
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !chosen.is_empty(),
                                egui::Button::new(RichText::new(tr("geocode.review")).strong()),
                            )
                            .clicked()
                        {
                            state.confirm = true;
                        }
                    });
                }
            }
        });
    if !open {
        state.open = false;
    }
    event
}

fn place_row(ui: &mut Ui, place: &PlaceSummary, selected: &mut HashSet<String>) {
    let mut checked = selected.contains(&place.id);
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut checked, "").changed() {
            if checked {
                selected.insert(place.id.clone());
            } else {
                selected.remove(&place.id);
            }
        }
        ui.label(&place.name);
        let missing = match (place.latitude.is_none(), place.longitude.is_none()) {
            (true, true) => tr("geocode.missing_both"),
            (true, false) => tr("geocode.missing_latitude"),
            (false, true) => tr("geocode.missing_longitude"),
            (false, false) => String::new(),
        };
        if !missing.is_empty() {
            ui.label(RichText::new(missing).weak().small());
        }
        ui.label(
            RichText::new(tr_args(
                "geocode.events",
                &[("events", place.event_count.to_string())],
            ))
            .weak()
            .small(),
        );
    });
}

fn result_row(ui: &mut Ui, result: &GeocodeResult, selected: &mut HashSet<String>) {
    ui.horizontal_wrapped(|ui| {
        if result.is_found() {
            let mut checked = selected.contains(&result.place_id);
            if ui.checkbox(&mut checked, "").changed() {
                if checked {
                    selected.insert(result.place_id.clone());
                } else {
                    selected.remove(&result.place_id);
                }
            }
        } else {
            ui.add_space(18.0);
        }
        ui.label(RichText::new(&result.place_name).strong());
        match &result.outcome {
            GeocodeOutcome::Found {
                display_name,
                latitude,
                longitude,
            } => {
                ui.label(RichText::new(tr_args(
                    "geocode.found",
                    &[("name", display_name.clone())],
                )));
                ui.label(
                    RichText::new(format!("{latitude:.5}, {longitude:.5}"))
                        .weak()
                        .small(),
                );
            }
            GeocodeOutcome::NotFound => {
                ui.label(RichText::new(tr("geocode.not_found")).weak());
            }
            GeocodeOutcome::Failed(error) => {
                ui.label(
                    RichText::new(tr_args("geocode.failed", &[("error", error.clone())]))
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
        }
        if result.cached {
            ui.label(RichText::new(tr("geocode.cached")).weak().small());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn place(id: &str, name: &str, latitude: Option<f64>, longitude: Option<f64>) -> PlaceSummary {
        PlaceSummary {
            id: id.to_string(),
            name: name.to_string(),
            notes: None,
            latitude,
            longitude,
            event_count: 0,
        }
    }

    #[test]
    fn only_missing_coordinates_are_listed() {
        let mut state = GeocodeState::default();
        state.set_places(vec![
            place("p1", "Uppsala", None, None),
            place("p2", "Paris", Some(48.8), Some(2.3)),
            place("p3", "Lund", Some(55.7), None),
        ]);
        assert!(state.open);
        assert_eq!(state.places.len(), 2);
        assert_eq!(state.selected_count(), 0);
    }

    #[test]
    fn requests_and_finished_results_follow_the_selection() {
        let mut state = GeocodeState::default();
        state.set_places(vec![
            place("p1", "Uppsala", None, None),
            place("p2", "Lund", None, None),
        ]);
        state.selected.insert("p2".to_string());
        let requests = state.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].place_id, "p2");

        state.finish(GeocodeReport {
            results: vec![
                GeocodeResult {
                    place_id: "p1".to_string(),
                    place_name: "Uppsala".to_string(),
                    query: "uppsala".to_string(),
                    outcome: GeocodeOutcome::Found {
                        display_name: "Uppsala, Sweden".to_string(),
                        latitude: 59.8586,
                        longitude: 17.6389,
                    },
                    cached: true,
                },
                GeocodeResult {
                    place_id: "p2".to_string(),
                    place_name: "Lund".to_string(),
                    query: "lund".to_string(),
                    outcome: GeocodeOutcome::NotFound,
                    cached: false,
                },
            ],
            cancelled: false,
            error: None,
        });
        assert!(!state.running);
        assert_eq!(state.selected_results, HashSet::from(["p1".to_string()]));
        assert_eq!(state.chosen_results().len(), 1);
    }

    #[test]
    fn window_renders_headless() {
        let ctx = Context::default();
        let mut state = GeocodeState::default();
        state.set_places(vec![place("p1", "Uppsala", None, None)]);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
