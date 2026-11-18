//! Individual summary export options.

use crate::i18n::tr;

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;

/// State owned by the individual summary dialog.
#[derive(Debug, Clone)]
pub struct IndividualSummaryState {
    pub open: bool,
    pub person_id: Option<String>,
    pub include_notes: bool,
}

impl Default for IndividualSummaryState {
    fn default() -> Self {
        Self {
            open: false,
            person_id: None,
            include_notes: true,
        }
    }
}

impl IndividualSummaryState {
    pub fn open_with(&mut self, person_id: Option<String>) {
        self.open = true;
        self.person_id = person_id;
    }
}

/// What the individual summary dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndividualSummaryEvent {
    pub person_id: String,
    pub include_notes: bool,
}

/// Draws the individual summary options window.
pub fn show(
    ctx: &Context,
    state: &mut IndividualSummaryState,
    people: &[Person],
    surname_first: bool,
) -> Option<IndividualSummaryEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("individual_summary.title"))
        .id(egui::Id::unique("individual_summary_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(tr("individual_summary.description"))
                    .weak()
                    .small(),
            );
            if people.is_empty() {
                ui.label(RichText::new(tr("common.no_people")).weak());
                if ui.button(tr("common.close")).clicked() {
                    state.open = false;
                }
                return;
            }
            let current = state
                .person_id
                .as_deref()
                .and_then(|id| people.iter().find(|person| person.id == id))
                .map(|person| person_label(person, surname_first))
                .unwrap_or_else(|| tr("report_dialog.select_person").to_string());
            egui::ComboBox::from_id_salt("individual_summary_person")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for person in people {
                        let selected = state.person_id.as_deref() == Some(person.id.as_str());
                        if ui
                            .selectable_label(selected, person_label(person, surname_first))
                            .clicked()
                        {
                            state.person_id = Some(person.id.clone());
                        }
                    }
                });
            ui.checkbox(&mut state.include_notes, tr("common.include_notes"));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        state.person_id.is_some(),
                        egui::Button::new(RichText::new(tr("common.export")).strong()),
                    )
                    .clicked()
                    && let Some(person_id) = state.person_id.clone()
                {
                    state.open = false;
                    event = Some(IndividualSummaryEvent {
                        person_id,
                        include_notes: state.include_notes,
                    });
                }
                if ui.button(tr("common.cancel")).clicked() {
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
        let mut state = IndividualSummaryState::default();
        state.open_with(Some("p1".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person()], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert!(state.include_notes);
    }
}
