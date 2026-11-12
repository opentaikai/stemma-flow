//! Component-level batch standardisation of place names.

use std::collections::HashSet;

use egui::{Context, RichText, Ui, Window};

use crate::db::places::PlaceSummary;
use crate::i18n::{tr, tr_args};
use crate::model::dataclean::DataCleanPlan;
use crate::model::place_clean::{PlaceComponent, build_components, preview_plan};

/// State owned by the PlaceClean window.
#[derive(Debug, Default)]
pub struct PlaceCleanState {
    pub open: bool,
    pub places: Vec<PlaceSummary>,
    pub components: Vec<PlaceComponent>,
    pub selected: HashSet<String>,
    pub preview: DataCleanPlan,
    pub filter: String,
    pub confirm: bool,
    pub error: Option<String>,
}

impl PlaceCleanState {
    pub fn set_places(&mut self, places: Vec<PlaceSummary>) {
        self.open = true;
        self.components = build_components(&places);
        self.places = places;
        self.selected.clear();
        self.filter.clear();
        self.confirm = false;
        self.error = None;
        self.refresh_preview();
    }

    pub fn refresh_preview(&mut self) {
        self.preview = preview_plan(&self.places, &self.components, &self.selected);
    }

    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    pub fn select_all(&mut self) {
        self.selected = self
            .components
            .iter()
            .map(|component| component.key.clone())
            .collect();
        self.refresh_preview();
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.refresh_preview();
    }
}

/// Draws the PlaceClean window, returning the preview to apply.
pub fn show(ctx: &Context, state: &mut PlaceCleanState) -> Option<DataCleanPlan> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("placeclean.title"))
        .id(egui::Id::unique("place_clean_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(760.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("placeclean.hint")).weak().small());
            if state.components.is_empty() {
                ui.separator();
                ui.label(RichText::new(tr("placeclean.empty")).weak());
                if ui.button(tr("common.close")).clicked() {
                    state.open = false;
                }
                return;
            }

            ui.horizontal(|ui| {
                ui.label(tr("placeclean.filter"));
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .hint_text(tr("placeclean.filter_hint")),
                );
                if ui.button(tr("placeclean.select_all")).clicked() {
                    state.select_all();
                }
                if ui.button(tr("placeclean.select_none")).clicked() {
                    state.clear_selection();
                }
                ui.label(
                    RichText::new(tr_args(
                        "placeclean.selected",
                        &[("count", state.selected_count().to_string())],
                    ))
                    .weak(),
                );
            });
            ui.separator();

            let needle = state.filter.trim().to_lowercase();
            let mut changed = false;
            egui::ScrollArea::vertical()
                .id_salt("place_clean_components")
                .max_height(280.0)
                .show(ui, |ui| {
                    for component in &mut state.components {
                        let matches = needle.is_empty()
                            || component.key.contains(&needle)
                            || component
                                .variants
                                .iter()
                                .any(|variant| variant.text.to_lowercase().contains(&needle));
                        if !matches {
                            continue;
                        }
                        changed |= component_row(ui, component, &mut state.selected);
                    }
                });
            if changed {
                state.refresh_preview();
            }

            ui.separator();
            ui.label(RichText::new(tr("placeclean.preview")).strong());
            if state.preview.places.is_empty() {
                ui.label(RichText::new(tr("placeclean.no_changes")).weak().small());
            } else {
                let renamed = state
                    .preview
                    .places
                    .iter()
                    .filter(|place| place.merged_ids.is_empty())
                    .count();
                let merged = state
                    .preview
                    .places
                    .iter()
                    .filter(|place| !place.merged_ids.is_empty())
                    .count();
                ui.label(
                    RichText::new(tr_args(
                        "placeclean.summary",
                        &[
                            ("renamed", renamed.to_string()),
                            ("merged", merged.to_string()),
                        ],
                    ))
                    .strong(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("place_clean_preview")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for cleanup in &state.preview.places {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(cleanup.before_names.join(" / ")).weak());
                                ui.label(crate::gui::symbols::ARROW_RIGHT);
                                ui.label(RichText::new(&cleanup.after).strong());
                                ui.label(
                                    RichText::new(tr_args(
                                        "placeclean.events",
                                        &[("events", cleanup.event_count.to_string())],
                                    ))
                                    .weak()
                                    .small(),
                                );
                                if !cleanup.merged_ids.is_empty() {
                                    ui.label(
                                        RichText::new(tr_args(
                                            "placeclean.duplicates",
                                            &[("count", cleanup.merged_ids.len().to_string())],
                                        ))
                                        .weak()
                                        .small(),
                                    );
                                }
                            });
                        }
                    });
            }

            if let Some(error) = &state.error {
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }

            ui.separator();
            if state.confirm {
                ui.label(
                    RichText::new(tr_args(
                        "placeclean.confirm",
                        &[("count", state.selected_count().to_string())],
                    ))
                    .color(ui.visuals().error_fg_color)
                    .strong(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !state.preview.places.is_empty(),
                            egui::Button::new(RichText::new(tr("placeclean.apply")).strong()),
                        )
                        .clicked()
                    {
                        event = Some(state.preview.clone());
                        state.open = false;
                    }
                    if ui.button(tr("common.cancel")).clicked() {
                        state.confirm = false;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !state.preview.places.is_empty(),
                            egui::Button::new(RichText::new(tr("placeclean.start")).strong()),
                        )
                        .clicked()
                    {
                        state.confirm = true;
                    }
                    if ui.button(tr("common.cancel")).clicked() {
                        state.open = false;
                    }
                });
            }
        });
    if !open {
        state.open = false;
    }
    event
}

fn component_row(
    ui: &mut Ui,
    component: &mut PlaceComponent,
    selected: &mut HashSet<String>,
) -> bool {
    let mut changed = false;
    ui.push_id(&component.key, |ui| {
        ui.horizontal_wrapped(|ui| {
            let mut checked = selected.contains(&component.key);
            if ui.checkbox(&mut checked, "").changed() {
                if checked {
                    selected.insert(component.key.clone());
                } else {
                    selected.remove(&component.key);
                }
                changed = true;
            }
            let variants = component
                .variants
                .iter()
                .map(|variant| format!("{} ({})", variant.text, variant.count))
                .collect::<Vec<_>>()
                .join(" / ");
            ui.label(RichText::new(variants).weak());
            ui.label(crate::gui::symbols::ARROW_RIGHT);
            if ui
                .add(egui::TextEdit::singleline(&mut component.after).desired_width(180.0))
                .changed()
            {
                changed = true;
            }
            ui.label(
                RichText::new(tr_args(
                    "placeclean.usage",
                    &[("places", component.place_count.to_string())],
                ))
                .weak()
                .small(),
            );
        });
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn place(id: &str, name: &str, event_count: i64) -> PlaceSummary {
        PlaceSummary {
            id: id.to_string(),
            name: name.to_string(),
            notes: None,
            latitude: None,
            longitude: None,
            event_count,
        }
    }

    #[test]
    fn set_places_starts_without_a_selection() {
        let mut state = PlaceCleanState::default();
        state.set_places(vec![place("p1", "PARIS", 1), place("p2", "paris ", 2)]);
        assert!(state.open);
        assert_eq!(state.components.len(), 1);
        assert_eq!(state.selected_count(), 0);
        assert!(state.preview.places.is_empty());
    }

    #[test]
    fn selecting_a_component_previews_a_merge() {
        let mut state = PlaceCleanState::default();
        state.set_places(vec![place("p1", "PARIS", 1), place("p2", "paris ", 2)]);
        let key = state.components[0].key.clone();
        state.selected.insert(key);
        state.refresh_preview();
        assert_eq!(state.preview.places.len(), 1);
        assert_eq!(state.preview.places[0].keep_id, "p2");
        assert_eq!(state.preview.places[0].after, "Paris");
    }

    #[test]
    fn window_renders_headless() {
        let ctx = Context::default();
        let mut state = PlaceCleanState::default();
        state.set_places(vec![place("p1", "PARIS", 1), place("p2", "paris ", 2)]);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }
}
