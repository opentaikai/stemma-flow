//! Preview and apply literal replacements across names, places, notes and paths.

use std::collections::HashSet;

use egui::{Context, RichText, ScrollArea, Window};

use crate::i18n::{tr, tr_args};
use crate::model::search_replace::{ReplacePlan, ReplaceRequest};

/// State of the Search and Replace window.
pub struct SearchReplaceState {
    pub open: bool,
    pub find: String,
    pub replace: String,
    pub match_case: bool,
    pub names: bool,
    pub places: bool,
    pub notes: bool,
    pub filenames: bool,
    pub plan: ReplacePlan,
    pub selected: HashSet<String>,
    pub previewed: bool,
    pub confirm: bool,
    pub error: Option<String>,
}

impl Default for SearchReplaceState {
    fn default() -> Self {
        Self {
            open: false,
            find: String::new(),
            replace: String::new(),
            match_case: true,
            names: true,
            places: true,
            notes: true,
            filenames: true,
            plan: ReplacePlan::default(),
            selected: HashSet::new(),
            previewed: false,
            confirm: false,
            error: None,
        }
    }
}

impl SearchReplaceState {
    pub fn invalidate(&mut self) {
        self.plan = ReplacePlan::default();
        self.selected.clear();
        self.previewed = false;
        self.confirm = false;
        self.error = None;
    }

    pub fn set_plan(&mut self, plan: ReplacePlan) {
        self.selected = plan
            .changes
            .iter()
            .map(|change| change.key.clone())
            .collect();
        self.plan = plan;
        self.previewed = true;
        self.confirm = false;
        self.error = None;
    }

    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    pub fn request(&self) -> ReplaceRequest {
        ReplaceRequest {
            find: self.find.clone(),
            replace: self.replace.clone(),
            match_case: self.match_case,
            names: self.names,
            places: self.places,
            notes: self.notes,
            filenames: self.filenames,
        }
    }

    fn has_scope(&self) -> bool {
        self.names || self.places || self.notes || self.filenames
    }
}

/// What the Search and Replace window asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchReplaceEvent {
    Preview(Box<ReplaceRequest>),
    Apply(HashSet<String>),
}

/// Draws the replacement dialog.
pub fn show(ctx: &Context, state: &mut SearchReplaceState) -> Option<SearchReplaceEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("replace.title"))
        .id(egui::Id::unique("search_replace_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(700.0)
        .default_height(560.0)
        .open(&mut open)
        .show(ctx, |ui| {
            let mut dirty = false;
            ui.label(RichText::new(tr("replace.hint")).weak().small());
            ui.add_space(4.0);
            egui::Grid::new("search_replace_fields")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("replace.find"));
                    dirty |= ui.text_edit_singleline(&mut state.find).changed();
                    ui.end_row();

                    ui.label(tr("replace.replace_with"));
                    dirty |= ui.text_edit_singleline(&mut state.replace).changed();
                    ui.end_row();
                });
            if ui
                .checkbox(&mut state.match_case, tr("replace.match_case"))
                .changed()
            {
                dirty = true;
            }
            ui.horizontal(|ui| {
                dirty |= ui
                    .checkbox(&mut state.names, tr("replace.scope_names"))
                    .changed();
                dirty |= ui
                    .checkbox(&mut state.places, tr("replace.scope_places"))
                    .changed();
                dirty |= ui
                    .checkbox(&mut state.notes, tr("replace.scope_notes"))
                    .changed();
                dirty |= ui
                    .checkbox(&mut state.filenames, tr("replace.scope_filenames"))
                    .changed();
            });
            if dirty {
                state.invalidate();
            }

            if let Some(error) = &state.error {
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }
            ui.horizontal(|ui| {
                if ui.button(tr("replace.preview")).clicked() {
                    if state.find.is_empty() {
                        state.error = Some(tr("replace.empty_find"));
                    } else if !state.has_scope() {
                        state.error = Some(tr("replace.no_scopes"));
                    } else {
                        event = Some(SearchReplaceEvent::Preview(Box::new(state.request())));
                    }
                }
                if state.previewed {
                    ui.label(
                        RichText::new(tr_args(
                            "replace.change_count",
                            &[("count", state.plan.changes.len().to_string())],
                        ))
                        .weak(),
                    );
                }
            });
            ui.separator();

            if state.previewed && state.plan.is_empty() {
                ui.label(RichText::new(tr("replace.no_matches")).weak());
            } else if state.previewed {
                ui.horizontal(|ui| {
                    if ui.button(tr("replace.select_all")).clicked() {
                        state.selected = state
                            .plan
                            .changes
                            .iter()
                            .map(|change| change.key.clone())
                            .collect();
                    }
                    if ui.button(tr("replace.select_none")).clicked() {
                        state.selected.clear();
                    }
                    ui.label(
                        RichText::new(tr_args(
                            "replace.selected_count",
                            &[("count", state.selected_count().to_string())],
                        ))
                        .weak(),
                    );
                });
                let height = ui.available_height().max(160.0);
                ScrollArea::vertical()
                    .id_salt("search_replace_preview")
                    .auto_shrink([false, false])
                    .max_height(height)
                    .show(ui, |ui| {
                        let mut current = None;
                        for change in &state.plan.changes {
                            let kind = change.kind();
                            if current != Some(kind) {
                                if current.is_some() {
                                    ui.add_space(4.0);
                                }
                                ui.label(RichText::new(tr(kind.label_key())).strong());
                                current = Some(kind);
                            }
                            let mut checked = state.selected.contains(&change.key);
                            ui.horizontal_wrapped(|ui| {
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        state.selected.insert(change.key.clone());
                                    } else {
                                        state.selected.remove(&change.key);
                                    }
                                }
                                ui.label(
                                    RichText::new(format!(
                                        "{} — {}",
                                        change.owner,
                                        tr(&change.field)
                                    ))
                                    .strong(),
                                );
                            });
                            let text = format!("{} → {}", change.before, change.after);
                            ui.horizontal(|ui| {
                                ui.add_space(24.0);
                                ui.add(
                                    egui::Label::new(RichText::new(&text).weak().small())
                                        .truncate(),
                                )
                                .on_hover_text(text);
                            });
                        }
                    });
            } else {
                ui.label(RichText::new(tr("replace.prompt")).weak());
            }

            ui.separator();
            if state.confirm {
                ui.label(
                    RichText::new(tr_args(
                        "replace.confirm",
                        &[("count", state.selected_count().to_string())],
                    ))
                    .color(ui.visuals().error_fg_color)
                    .strong(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.selected_count() > 0,
                            egui::Button::new(RichText::new(tr("replace.apply")).strong())
                                .fill(ui.visuals().error_fg_color),
                        )
                        .clicked()
                    {
                        event = Some(SearchReplaceEvent::Apply(state.selected.clone()));
                    }
                    if ui.button(tr("replace.back")).clicked() {
                        state.confirm = false;
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            state.previewed && state.selected_count() > 0,
                            egui::Button::new(RichText::new(tr("replace.apply_selected")).strong()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::search_replace::{ReplaceChange, ReplaceTarget};
    use egui::{Context, RawInput};

    fn plan() -> ReplacePlan {
        ReplacePlan {
            changes: vec![ReplaceChange {
                key: "person:p1:given".to_string(),
                target: ReplaceTarget::Person {
                    person_id: "p1".to_string(),
                    field: crate::model::search_replace::NameField::Given,
                },
                owner: "Ada King".to_string(),
                field: "replace.field_given".to_string(),
                before: "Ada".to_string(),
                after: "Grace".to_string(),
            }],
        }
    }

    #[test]
    fn setting_a_plan_selects_every_change() {
        let mut state = SearchReplaceState::default();
        state.set_plan(plan());
        assert!(state.previewed);
        assert_eq!(state.selected_count(), 1);
        assert!(!state.confirm);
    }

    #[test]
    fn editing_inputs_invalidates_the_preview() {
        let mut state = SearchReplaceState::default();
        state.set_plan(plan());
        state.find = "changed".to_string();
        state.invalidate();
        assert!(!state.previewed);
        assert!(state.plan.is_empty());
        assert_eq!(state.selected_count(), 0);
    }

    #[test]
    fn window_renders_headless() {
        let ctx = Context::default();
        let mut state = SearchReplaceState {
            open: true,
            find: "Ada".to_string(),
            replace: "Grace".to_string(),
            ..SearchReplaceState::default()
        };
        state.set_plan(plan());
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }
}
