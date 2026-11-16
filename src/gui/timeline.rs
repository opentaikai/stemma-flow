//! Timeline report export options.

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::db::groups::GroupSummary;
use crate::gui::inspector::person_label;
use crate::report::timeline::TimelineMode;

/// State owned by the timeline dialog.
#[derive(Debug, Default)]
pub struct TimelineState {
    pub open: bool,
    pub mode: TimelineMode,
    pub person_id: Option<String>,
    pub group_id: Option<String>,
    pub groups: Vec<GroupSummary>,
}

impl TimelineState {
    pub fn open_with(&mut self, person_id: Option<String>, groups: Vec<GroupSummary>) {
        self.open = true;
        self.person_id = person_id;
        self.group_id = groups.first().map(|group| group.id.clone());
        self.groups = groups;
    }
}

/// What the timeline dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEvent {
    pub mode: TimelineMode,
    pub person_id: Option<String>,
    pub group_id: Option<String>,
}

/// Draws the timeline options window.
pub fn show(
    ctx: &Context,
    state: &mut TimelineState,
    people: &[Person],
    surname_first: bool,
) -> Option<TimelineEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Timeline Report")
        .id(egui::Id::unique("timeline_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Export birth, death, marriage and fact events in chronological order.",
                )
                .weak()
                .small(),
            );
            ui.horizontal(|ui| {
                ui.label("Timeline");
                ui.selectable_value(&mut state.mode, TimelineMode::Person, "Person");
                ui.selectable_value(&mut state.mode, TimelineMode::Group, "Group");
            });
            match state.mode {
                TimelineMode::Person => {
                    if people.is_empty() {
                        ui.label(RichText::new("No people are available.").weak());
                    } else {
                        let current = state
                            .person_id
                            .as_deref()
                            .and_then(|id| people.iter().find(|person| person.id == id))
                            .map(|person| person_label(person, surname_first))
                            .unwrap_or_else(|| "Select a person".to_string());
                        egui::ComboBox::from_id_salt("timeline_person")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for person in people {
                                    let selected =
                                        state.person_id.as_deref() == Some(person.id.as_str());
                                    if ui
                                        .selectable_label(
                                            selected,
                                            person_label(person, surname_first),
                                        )
                                        .clicked()
                                    {
                                        state.person_id = Some(person.id.clone());
                                    }
                                }
                            });
                    }
                }
                TimelineMode::Group => {
                    if state.groups.is_empty() {
                        ui.label(RichText::new("No groups are available.").weak());
                    } else {
                        let current = state
                            .group_id
                            .as_deref()
                            .and_then(|id| state.groups.iter().find(|group| group.id == id))
                            .map(|group| format!("{} ({} people)", group.name, group.member_count))
                            .unwrap_or_else(|| "Select a group".to_string());
                        egui::ComboBox::from_id_salt("timeline_group")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for group in &state.groups {
                                    let selected =
                                        state.group_id.as_deref() == Some(group.id.as_str());
                                    if ui
                                        .selectable_label(
                                            selected,
                                            format!(
                                                "{} ({} people)",
                                                group.name, group.member_count
                                            ),
                                        )
                                        .clicked()
                                    {
                                        state.group_id = Some(group.id.clone());
                                    }
                                }
                            });
                    }
                }
            }
            ui.horizontal(|ui| {
                let ready = match state.mode {
                    TimelineMode::Person => state.person_id.is_some(),
                    TimelineMode::Group => state.group_id.is_some(),
                };
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(RichText::new("Export HTML\u{2026}").strong()),
                    )
                    .clicked()
                {
                    state.open = false;
                    event = Some(TimelineEvent {
                        mode: state.mode,
                        person_id: match state.mode {
                            TimelineMode::Person => state.person_id.clone(),
                            TimelineMode::Group => None,
                        },
                        group_id: match state.mode {
                            TimelineMode::Group => state.group_id.clone(),
                            TimelineMode::Person => None,
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

    fn person() -> Person {
        let mut person = Person::new("Ada", "King", "F");
        person.id = "p1".to_string();
        person
    }

    #[test]
    fn opens_with_a_person_and_renders() {
        let ctx = Context::default();
        let mut state = TimelineState::default();
        state.open_with(Some("p1".to_string()), Vec::new());
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person()], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert_eq!(state.mode, TimelineMode::Person);
    }
}
