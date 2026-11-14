//! Modeless gazetteer search dialog.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;

use egui::{Context, RichText, Window};

use crate::geocode::{self, GazetteerHit};
use crate::i18n::tr;
use crate::model::geocode::normalize_query;

/// State owned by the gazetteer dialog.
#[derive(Debug, Default)]
pub struct GazetteerState {
    pub open: bool,
    pub query: String,
    pub results: Vec<GazetteerHit>,
    pub selected: Option<usize>,
    pub running: bool,
    pub error: Option<String>,
    pub from_cache: bool,
    cache: HashMap<String, Vec<GazetteerHit>>,
    rx: Option<Receiver<Result<Vec<GazetteerHit>, String>>>,
    active_query: Option<String>,
}

impl GazetteerState {
    pub fn open_with_query(&mut self, query: &str) {
        self.open = true;
        if !query.trim().is_empty() {
            self.query = query.trim().to_string();
        }
        if !self.running {
            self.results.clear();
            self.selected = None;
            self.from_cache = false;
        }
        self.error = None;
    }

    /// Searches immediately from the session cache or starts a worker request.
    pub fn start_search(&mut self, force: bool) {
        if self.running {
            return;
        }
        let query = self.query.trim().to_string();
        if query.is_empty() {
            return;
        }
        let key = normalize_query(&query);
        if !force && let Some(results) = self.cache.get(&key) {
            self.results = results.clone();
            self.selected = (!self.results.is_empty()).then_some(0);
            self.error = None;
            self.from_cache = true;
            return;
        }

        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.active_query = Some(key);
        self.running = true;
        self.error = None;
        self.from_cache = false;
        thread::spawn(move || {
            let agent = geocode::agent();
            let _ = tx.send(geocode::search(&agent, &query, 8));
        });
    }

    /// Consumes a completed background search.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(results)) => {
                self.running = false;
                self.rx = None;
                if let Some(key) = self.active_query.take() {
                    self.cache.insert(key, results.clone());
                }
                self.results = results;
                self.selected = (!self.results.is_empty()).then_some(0);
                self.error = None;
            }
            Ok(Err(message)) => {
                self.running = false;
                self.rx = None;
                self.active_query = None;
                self.results.clear();
                self.selected = None;
                self.error = Some(message);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.running = false;
                self.rx = None;
                self.active_query = None;
                self.error = Some(tr("gazetteer.search_failed"));
            }
        }
    }

    pub fn selected_hit(&self) -> Option<&GazetteerHit> {
        self.selected.and_then(|index| self.results.get(index))
    }
}

/// What the gazetteer dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq)]
pub enum GazetteerEvent {
    UsePlace(GazetteerHit),
    ShowOnMap(GazetteerHit),
    OpenSettings,
}

/// Draws the gazetteer window.
pub fn show(
    ctx: &Context,
    state: &mut GazetteerState,
    allow_online: bool,
) -> Option<GazetteerEvent> {
    state.poll();
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("gazetteer.title"))
        .id(egui::Id::unique("gazetteer_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(680.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("gazetteer.hint")).weak().small());
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .hint_text(tr("gazetteer.query_hint")),
                );
                let enter =
                    response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                let can_search = allow_online && !state.running && !state.query.trim().is_empty();
                let search = ui
                    .add_enabled(
                        can_search,
                        egui::Button::new(RichText::new(tr("gazetteer.search")).strong()),
                    )
                    .clicked();
                let refresh = ui
                    .add_enabled(can_search, egui::Button::new(tr("gazetteer.refresh")))
                    .clicked();
                if search || enter {
                    state.start_search(false);
                }
                if refresh {
                    state.start_search(true);
                }
                if state.from_cache {
                    ui.label(RichText::new(tr("gazetteer.cached")).weak().small());
                }
            });

            if !allow_online {
                ui.separator();
                ui.label(
                    RichText::new(tr("gazetteer.online_disabled"))
                        .color(ui.visuals().warn_fg_color)
                        .strong(),
                );
                ui.label(RichText::new(tr("gazetteer.enable_hint")).weak().small());
                if ui.button(tr("gazetteer.open_settings")).clicked() {
                    event = Some(GazetteerEvent::OpenSettings);
                }
            }

            if state.running {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr("gazetteer.searching"));
                });
            }

            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }

            ui.separator();
            ui.label(RichText::new(tr("gazetteer.results")).strong());
            if state.results.is_empty() && !state.running && !state.query.trim().is_empty() {
                ui.label(RichText::new(tr("gazetteer.no_results")).weak().small());
            }
            let mut chosen = state.selected;
            egui::ScrollArea::vertical()
                .id_salt("gazetteer_result_list")
                .max_height(320.0)
                .show(ui, |ui| {
                    for (index, hit) in state.results.iter().enumerate() {
                        let selected = chosen == Some(index);
                        if ui
                            .selectable_label(selected, RichText::new(&hit.display_name).strong())
                            .clicked()
                        {
                            chosen = Some(index);
                        }
                        ui.label(RichText::new(result_details(hit)).weak().small());
                    }
                });
            state.selected = chosen;

            if let Some(hit) = state.selected_hit().cloned() {
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(
                            RichText::new(tr("gazetteer.use_place")).strong(),
                        ))
                        .clicked()
                    {
                        state.open = false;
                        event = Some(GazetteerEvent::UsePlace(hit.clone()));
                    }
                    if ui.button(tr("gazetteer.show_map")).clicked() {
                        state.open = false;
                        event = Some(GazetteerEvent::ShowOnMap(hit));
                    }
                });
            }

            if allow_online {
                ui.separator();
                ui.hyperlink_to(
                    tr("gazetteer.attribution"),
                    "https://www.openstreetmap.org/copyright",
                );
            }
        });
    if !open {
        state.open = false;
    }
    event
}

fn result_details(hit: &GazetteerHit) -> String {
    let mut parts = Vec::new();
    if let Some(kind) = hit.kind.as_deref().filter(|kind| !kind.trim().is_empty()) {
        parts.push(kind.to_string());
    }
    if let Some(class) = hit
        .class
        .as_deref()
        .filter(|class| !class.trim().is_empty())
    {
        parts.push(class.to_string());
    }
    parts.push(format!("{:.5}, {:.5}", hit.latitude, hit.longitude));
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn hit(name: &str) -> GazetteerHit {
        GazetteerHit {
            display_name: name.to_string(),
            latitude: 59.8586,
            longitude: 17.6389,
            kind: Some("city".to_string()),
            class: Some("place".to_string()),
        }
    }

    #[test]
    fn opens_with_a_query_and_clears_stale_results() {
        let mut state = GazetteerState {
            results: vec![hit("Old")],
            selected: Some(0),
            ..GazetteerState::default()
        };
        state.open_with_query("Uppsala");
        assert!(state.open);
        assert_eq!(state.query, "Uppsala");
        assert!(state.results.is_empty());
        assert_eq!(state.selected, None);
    }

    #[test]
    fn session_cache_answers_without_network() {
        let mut state = GazetteerState {
            query: "Uppsala".to_string(),
            ..GazetteerState::default()
        };
        state
            .cache
            .insert(normalize_query("Uppsala"), vec![hit("Uppsala, Sweden")]);
        state.start_search(false);
        assert!(!state.running);
        assert!(state.from_cache);
        assert_eq!(state.results.len(), 1);
        assert_eq!(state.selected, Some(0));
    }

    #[test]
    fn completed_worker_results_are_selected() {
        let (tx, rx) = channel();
        let mut state = GazetteerState {
            rx: Some(rx),
            running: true,
            active_query: Some("uppsala".to_string()),
            ..GazetteerState::default()
        };
        tx.send(Ok(vec![hit("Uppsala, Sweden")])).expect("send");
        state.poll();
        assert!(!state.running);
        assert_eq!(state.results.len(), 1);
        assert_eq!(state.selected, Some(0));
        assert_eq!(state.results[0].kind.as_deref(), Some("city"));
    }

    #[test]
    fn window_renders_offline() {
        let ctx = Context::default();
        let mut state = GazetteerState {
            open: true,
            query: "Uppsala".to_string(),
            results: vec![hit("Uppsala, Sweden")],
            selected: Some(0),
            ..GazetteerState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
