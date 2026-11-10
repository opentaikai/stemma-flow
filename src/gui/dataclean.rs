//! Preview and apply batch name and place normalization.

use std::collections::HashSet;

use egui::{Context, RichText, Window};

use crate::i18n::{tr, tr_args};
use crate::model::dataclean::{DataCleanPlan, NameCleanup, PlaceCleanup};

/// State of the DataClean window.
#[derive(Debug, Default)]
pub struct DataCleanState {
    pub open: bool,
    pub plan: DataCleanPlan,
    pub selected_names: HashSet<String>,
    pub selected_places: HashSet<String>,
    pub confirm: bool,
    pub error: Option<String>,
}

impl DataCleanState {
    pub fn set_plan(&mut self, plan: DataCleanPlan) {
        self.open = true;
        self.selected_names = plan.names.iter().map(|name| name.key.clone()).collect();
        self.selected_places = plan.places.iter().map(|place| place.key.clone()).collect();
        self.plan = plan;
        self.confirm = false;
        self.error = None;
    }

    pub fn selected_count(&self) -> usize {
        self.selected_names.len() + self.selected_places.len()
    }

    pub fn clear_selection(&mut self) {
        self.selected_names.clear();
        self.selected_places.clear();
    }
}

/// What the DataClean window asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataCleanEvent {
    pub names: HashSet<String>,
    pub places: HashSet<String>,
}

/// Draws the preview window.
pub fn show(ctx: &Context, state: &mut DataCleanState) -> Option<DataCleanEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("dataclean.title"))
        .id(egui::Id::new("dataclean_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(680.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("dataclean.hint")).weak().small());
            ui.label(
                RichText::new(tr_args(
                    "dataclean.summary",
                    &[
                        ("names", state.plan.names.len().to_string()),
                        ("places", state.plan.places.len().to_string()),
                    ],
                ))
                .strong(),
            );
            if state.plan.is_empty() {
                ui.separator();
                ui.label(RichText::new(tr("dataclean.empty")).weak());
                if ui.button(tr("common.close")).clicked() {
                    state.open = false;
                }
                return;
            }

            ui.horizontal(|ui| {
                if ui.button(tr("dataclean.select_all")).clicked() {
                    state.selected_names = state
                        .plan
                        .names
                        .iter()
                        .map(|name| name.key.clone())
                        .collect();
                    state.selected_places = state
                        .plan
                        .places
                        .iter()
                        .map(|place| place.key.clone())
                        .collect();
                }
                if ui.button(tr("dataclean.select_none")).clicked() {
                    state.clear_selection();
                }
                ui.label(
                    RichText::new(tr_args(
                        "dataclean.selected",
                        &[("count", state.selected_count().to_string())],
                    ))
                    .weak(),
                );
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("dataclean_preview")
                .max_height(420.0)
                .show(ui, |ui| {
                    if !state.plan.names.is_empty() {
                        ui.label(RichText::new(tr("dataclean.names")).strong());
                        for name in &state.plan.names {
                            name_row(ui, name, &mut state.selected_names);
                        }
                    }
                    if !state.plan.places.is_empty() {
                        ui.separator();
                        ui.label(RichText::new(tr("dataclean.places")).strong());
                        for place in &state.plan.places {
                            place_row(ui, place, &mut state.selected_places);
                        }
                    }
                });

            if let Some(error) = &state.error {
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }
            ui.separator();
            if state.confirm {
                ui.label(
                    RichText::new(tr_args(
                        "dataclean.confirm",
                        &[("count", state.selected_count().to_string())],
                    ))
                    .color(ui.visuals().error_fg_color)
                    .strong(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.selected_count() > 0,
                            egui::Button::new(RichText::new(tr("dataclean.apply")).strong()),
                        )
                        .clicked()
                    {
                        event = Some(DataCleanEvent {
                            names: state.selected_names.clone(),
                            places: state.selected_places.clone(),
                        });
                    }
                    if ui.button(tr("common.cancel")).clicked() {
                        state.confirm = false;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.selected_count() > 0,
                            egui::Button::new(RichText::new(tr("dataclean.start")).strong()),
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

fn name_row(ui: &mut egui::Ui, name: &NameCleanup, selected: &mut HashSet<String>) {
    let mut checked = selected.contains(&name.key);
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut checked, "").changed() {
            if checked {
                selected.insert(name.key.clone());
            } else {
                selected.remove(&name.key);
            }
        }
        ui.label(RichText::new(before_name(&name.before)).weak());
        ui.label("→");
        ui.label(RichText::new(before_name(&name.after)).strong());
        if name.alternate_id.is_some() {
            ui.label(RichText::new(tr("dataclean.alternate")).weak().small());
        }
    });
}

fn place_row(ui: &mut egui::Ui, place: &PlaceCleanup, selected: &mut HashSet<String>) {
    let mut checked = selected.contains(&place.key);
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut checked, "").changed() {
            if checked {
                selected.insert(place.key.clone());
            } else {
                selected.remove(&place.key);
            }
        }
        ui.label(RichText::new(place.before_names.join(" / ")).weak());
        ui.label("→");
        ui.label(RichText::new(&place.after).strong());
        ui.label(
            RichText::new(tr_args(
                "dataclean.place_usage",
                &[
                    ("events", place.event_count.to_string()),
                    ("duplicates", place.merged_ids.len().to_string()),
                ],
            ))
            .weak()
            .small(),
        );
    });
}

fn before_name(parts: &crate::model::dataclean::NameParts) -> String {
    let name = parts.display_name();
    if name.is_empty() {
        tr("dataclean.unnamed")
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::dataclean::{NameParts, PlaceCleanup};
    use egui::{Context, RawInput};

    fn plan() -> DataCleanPlan {
        DataCleanPlan {
            names: vec![NameCleanup {
                key: "person:p1".to_string(),
                person_id: "p1".to_string(),
                alternate_id: None,
                before: NameParts {
                    given_name: "ADA".to_string(),
                    surname: "KING".to_string(),
                    ..NameParts::default()
                },
                after: NameParts {
                    given_name: "Ada".to_string(),
                    surname: "King".to_string(),
                    ..NameParts::default()
                },
            }],
            places: vec![PlaceCleanup {
                key: "paris".to_string(),
                keep_id: "p1".to_string(),
                merged_ids: vec!["p2".to_string()],
                before_names: vec!["PARIS".to_string(), "Paris ".to_string()],
                after: "Paris".to_string(),
                event_count: 2,
            }],
        }
    }

    #[test]
    fn window_renders_headless_and_selects_all_by_default() {
        let ctx = Context::default();
        let mut state = DataCleanState::default();
        state.set_plan(plan());
        assert_eq!(state.selected_count(), 2);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let event = show(ui.ctx(), &mut state);
            assert!(event.is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }

    #[test]
    fn empty_plan_renders_without_apply() {
        let ctx = Context::default();
        let mut state = DataCleanState::default();
        state.set_plan(DataCleanPlan::default());
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
