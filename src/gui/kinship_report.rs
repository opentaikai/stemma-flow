//! Kinship report export options.

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;

/// State owned by the kinship report dialog.
#[derive(Debug, Clone, Default)]
pub struct KinshipReportState {
    pub open: bool,
    pub subject_id: Option<String>,
    pub only_relatives: bool,
}

impl KinshipReportState {
    pub fn open_with(&mut self, subject_id: Option<String>) {
        self.open = true;
        self.subject_id = subject_id;
    }
}

/// What the kinship report dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KinshipReportEvent {
    pub subject_id: String,
    pub only_relatives: bool,
}

/// Draws the kinship report options window.
pub fn show(
    ctx: &Context,
    state: &mut KinshipReportState,
    people: &[Person],
    surname_first: bool,
) -> Option<KinshipReportEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Kinship Report")
        .id(egui::Id::unique("kinship_report_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Export every person's relationship to a chosen subject, with vital data.",
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
                .subject_id
                .as_deref()
                .and_then(|id| people.iter().find(|person| person.id == id))
                .map(|person| person_label(person, surname_first))
                .unwrap_or_else(|| "Select a person".to_string());
            egui::ComboBox::from_id_salt("kinship_report_subject")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for person in people {
                        let selected = state.subject_id.as_deref() == Some(person.id.as_str());
                        if ui
                            .selectable_label(selected, person_label(person, surname_first))
                            .clicked()
                        {
                            state.subject_id = Some(person.id.clone());
                        }
                    }
                });
            ui.checkbox(&mut state.only_relatives, "Only relatives");
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        state.subject_id.is_some(),
                        egui::Button::new(RichText::new("Export\u{2026}").strong()),
                    )
                    .clicked()
                    && let Some(subject_id) = state.subject_id.clone()
                {
                    state.open = false;
                    event = Some(KinshipReportEvent {
                        subject_id,
                        only_relatives: state.only_relatives,
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
    fn opens_with_a_subject_and_renders() {
        let ctx = Context::default();
        let mut state = KinshipReportState::default();
        state.open_with(Some("p1".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person()], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert!(!state.only_relatives);
    }
}
