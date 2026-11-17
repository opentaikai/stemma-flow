//! Descendant list export options.

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;
use crate::model::descendant_list::DescendantNumbering;
use crate::report::descendant_list::DescendantListMode;

/// State owned by the descendant list dialog.
#[derive(Debug, Clone)]
pub struct DescendantListState {
    pub open: bool,
    pub root_id: Option<String>,
    pub generations: usize,
    pub mode: DescendantListMode,
    pub numbering: DescendantNumbering,
    pub target_id: Option<String>,
}

impl Default for DescendantListState {
    fn default() -> Self {
        Self {
            open: false,
            root_id: None,
            generations: 4,
            mode: DescendantListMode::Indented,
            numbering: DescendantNumbering::DAboville,
            target_id: None,
        }
    }
}

impl DescendantListState {
    pub fn open_with(&mut self, root_id: Option<String>) {
        self.open = true;
        self.root_id = root_id;
        self.generations = self.generations.clamp(2, 7);
        self.target_id = None;
    }
}

/// What the descendant list dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescendantListEvent {
    pub root_id: String,
    pub generations: usize,
    pub mode: DescendantListMode,
    pub numbering: DescendantNumbering,
    pub target_id: Option<String>,
}

/// Draws the descendant list options window.
///
/// `targets` holds the current root's descendants for direct-line mode.
pub fn show(
    ctx: &Context,
    state: &mut DescendantListState,
    people: &[Person],
    targets: &[Person],
    surname_first: bool,
) -> Option<DescendantListEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Descendant List")
        .id(egui::Id::unique("descendant_list_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Export descendants as an indented list or a direct line to one person.",
                )
                .weak()
                .small(),
            );
            if people.is_empty() {
                ui.label(RichText::new("No people are available.").weak());
                if ui.button("Close").clicked() {
                    state.open = false;
                }
                return;
            }
            let current = state
                .root_id
                .as_deref()
                .and_then(|id| people.iter().find(|person| person.id == id))
                .map(|person| person_label(person, surname_first))
                .unwrap_or_else(|| "Select a person".to_string());
            egui::ComboBox::from_id_salt("descendant_list_root")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for person in people {
                        let selected = state.root_id.as_deref() == Some(person.id.as_str());
                        if ui
                            .selectable_label(selected, person_label(person, surname_first))
                            .clicked()
                        {
                            state.root_id = Some(person.id.clone());
                            state.target_id = None;
                        }
                    }
                });

            ui.horizontal(|ui| {
                ui.label("Layout");
                ui.selectable_value(&mut state.mode, DescendantListMode::Indented, "Indented");
                ui.selectable_value(&mut state.mode, DescendantListMode::Direct, "Direct line");
            });
            ui.horizontal(|ui| {
                ui.label("Numbering");
                ui.selectable_value(
                    &mut state.numbering,
                    DescendantNumbering::DAboville,
                    "d'Aboville",
                );
                ui.selectable_value(&mut state.numbering, DescendantNumbering::Henry, "Henry");
            });

            match state.mode {
                DescendantListMode::Indented => {
                    ui.add(egui::Slider::new(&mut state.generations, 2..=7).text("Generations"));
                }
                DescendantListMode::Direct => {
                    if targets.is_empty() {
                        state.target_id = None;
                        ui.label(RichText::new("This person has no recorded descendants.").weak());
                    } else {
                        let valid = state
                            .target_id
                            .as_deref()
                            .is_some_and(|id| targets.iter().any(|person| person.id == id));
                        if !valid {
                            state.target_id = targets.first().map(|person| person.id.clone());
                        }
                        let target_label = state
                            .target_id
                            .as_deref()
                            .and_then(|id| targets.iter().find(|person| person.id == id))
                            .map(|person| person_label(person, surname_first))
                            .unwrap_or_else(|| "Select a descendant".to_string());
                        egui::ComboBox::from_id_salt("descendant_list_target")
                            .selected_text(target_label)
                            .show_ui(ui, |ui| {
                                for person in targets {
                                    let selected =
                                        state.target_id.as_deref() == Some(person.id.as_str());
                                    if ui
                                        .selectable_label(
                                            selected,
                                            person_label(person, surname_first),
                                        )
                                        .clicked()
                                    {
                                        state.target_id = Some(person.id.clone());
                                    }
                                }
                            });
                    }
                }
            }

            ui.horizontal(|ui| {
                let ready = state.root_id.is_some()
                    && (state.mode == DescendantListMode::Indented || state.target_id.is_some());
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(RichText::new("Export\u{2026}").strong()),
                    )
                    .clicked()
                    && let Some(root_id) = state.root_id.clone()
                {
                    state.open = false;
                    event = Some(DescendantListEvent {
                        root_id,
                        generations: state.generations,
                        mode: state.mode,
                        numbering: state.numbering,
                        target_id: match state.mode {
                            DescendantListMode::Direct => state.target_id.clone(),
                            DescendantListMode::Indented => None,
                        },
                    });
                }
                if ui.button("Cancel").clicked() {
                    state.open = false;
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
    use egui::{Context, RawInput};

    fn person(id: &str) -> Person {
        let mut person = Person::new(id, "Stone", "U");
        person.id = id.to_string();
        person
    }

    #[test]
    fn indented_mode_renders_without_targets() {
        let ctx = Context::default();
        let mut state = DescendantListState::default();
        state.open_with(Some("p1".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person("p1")], &[], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }

    #[test]
    fn direct_mode_selects_the_first_target() {
        let ctx = Context::default();
        let mut state = DescendantListState {
            mode: DescendantListMode::Direct,
            ..DescendantListState::default()
        };
        state.open_with(Some("p1".to_string()));
        let people = [person("p1"), person("p2")];
        let targets = [person("p2")];
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &people, &targets, false).is_none());
        });
        output.textures_delta.clear();
        assert_eq!(state.target_id.as_deref(), Some("p2"));
    }
}
