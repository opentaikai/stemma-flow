//! Modeless global text search across every top-level record type.

use egui::{Context, Key, RichText, ScrollArea, Window};

use crate::db::find::{FindResults, FindTarget};
use crate::i18n::{tr, tr_args};

/// State of the Find Everywhere window.
#[derive(Debug, Default)]
pub struct FindState {
    pub open: bool,
    pub query: String,
    pub submitted_query: String,
    pub results: FindResults,
    pub searched: bool,
    pub error: Option<String>,
}

/// What the Find Everywhere window asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindEvent {
    Search,
    Open(FindTarget),
}

/// Draws the search dialog.
pub fn show(ctx: &Context, state: &mut FindState) -> Option<FindEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("find.title"))
        .id(egui::Id::unique("find_everywhere_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .default_height(520.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("find.hint")).weak().small());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let button_width = 90.0;
                let field_width = (ui.available_width() - button_width).max(120.0);
                let response = ui.add_sized(
                    [field_width, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(&mut state.query).hint_text(tr("find.placeholder")),
                );
                let enter = response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                if ui.button(tr("common.search")).clicked() || enter {
                    event = Some(FindEvent::Search);
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.query.is_empty(),
                        egui::Button::new(tr("common.clear")),
                    )
                    .clicked()
                {
                    state.query.clear();
                    event = Some(FindEvent::Search);
                }
                if state.searched && !state.results.is_empty() {
                    ui.label(
                        RichText::new(tr_args(
                            "find.total",
                            &[("count", state.results.total().to_string())],
                        ))
                        .weak()
                        .small(),
                    );
                }
            });

            if let Some(error) = &state.error {
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }
            ui.separator();

            let available = ui.available_height().max(120.0);
            ScrollArea::vertical()
                .id_salt("find_everywhere_results")
                .auto_shrink([false, false])
                .max_height(available)
                .show(ui, |ui| {
                    if !state.searched {
                        ui.label(RichText::new(tr("find.prompt")).weak());
                        return;
                    }
                    if state.results.is_empty() {
                        ui.label(
                            RichText::new(tr_args(
                                "find.no_matches",
                                &[("query", state.submitted_query.clone())],
                            ))
                            .weak(),
                        );
                        return;
                    }
                    for group in &state.results.groups {
                        ui.label(
                            RichText::new(format!(
                                "{} ({})",
                                tr(group.kind.label_key()),
                                group.hits.len()
                            ))
                            .strong(),
                        );
                        for hit in &group.hits {
                            if ui
                                .selectable_label(false, RichText::new(&hit.title).strong())
                                .clicked()
                            {
                                event = Some(FindEvent::Open(hit.target.clone()));
                            }
                            if !hit.context.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label(RichText::new(&hit.context).weak().small());
                                });
                            }
                        }
                        if group.truncated {
                            ui.label(RichText::new(tr("find.truncated")).weak().small());
                        }
                        ui.add_space(4.0);
                    }
                });
        });
    if !open {
        state.open = false;
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::find::{FindGroup, FindHit, FindKind};
    use egui::{Context, RawInput};

    fn results() -> FindResults {
        FindResults {
            groups: vec![
                FindGroup {
                    kind: FindKind::Person,
                    hits: vec![FindHit {
                        title: "Ada King".to_string(),
                        context: "b. 1815".to_string(),
                        target: FindTarget::Person("p1".to_string()),
                    }],
                    truncated: false,
                },
                FindGroup {
                    kind: FindKind::Place,
                    hits: vec![FindHit {
                        title: "London".to_string(),
                        context: "12 events".to_string(),
                        target: FindTarget::Place("pl1".to_string()),
                    }],
                    truncated: true,
                },
            ],
        }
    }

    #[test]
    fn window_renders_headless() {
        let ctx = Context::default();
        let mut state = FindState {
            open: true,
            query: "london".to_string(),
            submitted_query: "london".to_string(),
            results: results(),
            searched: true,
            error: None,
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }

    #[test]
    fn empty_state_prompts_for_a_search() {
        let ctx = Context::default();
        let mut state = FindState {
            open: true,
            ..FindState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(!state.searched);
    }
}
