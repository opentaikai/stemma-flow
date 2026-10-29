//! New-person dialog that suggests existing records before inserting.

use std::collections::HashMap;

use egui::{Context, RichText, Window};

use crate::db::FamilyTree;
use crate::db::people::{PersonDetails, VitalDates};
use crate::i18n::tr;
use crate::model::duplicates::{DuplicateQuery, find_duplicates};

use super::ACCENT;
use super::duplicates::{self, DuplicateSuggestions};
use super::form::InspectorForm;
use super::panel::field_editors;

/// Buffered state of the New Person dialog.
#[derive(Debug, Clone, Default)]
pub struct NewPersonState {
    pub form: InspectorForm,
    pub suggestions: DuplicateSuggestions,
}

impl NewPersonState {
    /// Whether the buffered record can be inserted.
    pub fn can_submit(&self) -> bool {
        self.form.has_name() && self.form.dates_valid()
    }
}

/// What the user chose in the New Person dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewPersonOutcome {
    Submit(Box<PersonDetails>),
    CreateBlank,
    SelectExisting(String),
    Cancel,
}

/// Shows the New Person dialog; `None` keeps it open.
pub fn show(
    ctx: &Context,
    state: &mut NewPersonState,
    tree: &FamilyTree,
    dates: &HashMap<String, VitalDates>,
    surname_first: bool,
) -> Option<NewPersonOutcome> {
    let mut open = true;
    let mut outcome = None;
    Window::new(tr("new_person.title"))
        .id(egui::Id::new("new_person_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(460.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("new_person.hint")).weak().small());
            field_editors(ui, &mut state.form);
            ui.separator();

            let query = DuplicateQuery::from_details(&state.form.to_details());
            state
                .suggestions
                .update(&query, || find_duplicates(tree, dates, &query, &[]));
            if let Some(person_id) = duplicates::show(
                ui,
                state.suggestions.matches(),
                query.has_name(),
                tree,
                surname_first,
                &tr("duplicates.use_existing"),
            ) {
                outcome = Some(NewPersonOutcome::SelectExisting(person_id));
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tr("common.cancel")).clicked() {
                    outcome = Some(NewPersonOutcome::Cancel);
                }
                if ui.button(tr("new_person.create_blank")).clicked() {
                    outcome = Some(NewPersonOutcome::CreateBlank);
                }
                let can_submit = state.can_submit();
                if ui
                    .add_enabled(
                        can_submit,
                        egui::Button::new(RichText::new(tr("new_person.create")).strong())
                            .fill(ACCENT),
                    )
                    .clicked()
                {
                    outcome = Some(NewPersonOutcome::Submit(Box::new(state.form.to_details())));
                }
            });
        });
    if !open {
        return Some(NewPersonOutcome::Cancel);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Person;
    use egui::{Context, RawInput};

    fn person(id: &str, given: &str, surname: &str) -> Person {
        let mut person = Person::new(given, surname, "U");
        person.id = id.to_string();
        person
    }

    #[test]
    fn blank_forms_cannot_be_submitted() {
        let mut state = NewPersonState::default();
        assert!(!state.can_submit());
        state.form.given_name = "Ada".to_string();
        assert!(state.can_submit());
        state.form.birth = "not a date".to_string();
        assert!(!state.can_submit());
    }

    #[test]
    fn dialog_suggests_an_existing_person() {
        let ctx = Context::default();
        let mut state = NewPersonState::default();
        state.form.given_name = "Ada".to_string();
        state.form.surname = "King".to_string();
        let tree = FamilyTree {
            people: vec![
                person("existing", "Ada", "King"),
                person("other", "Grace", "Hopper"),
            ],
            families: Vec::new(),
            child_links: Vec::new(),
        };
        let mut outcome = Some(NewPersonOutcome::Cancel);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            outcome = show(ui.ctx(), &mut state, &tree, &HashMap::new(), false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the dialog paints");
        assert_eq!(outcome, None, "no input keeps the dialog open");
        assert_eq!(state.suggestions.matches().len(), 1);
        assert_eq!(state.suggestions.matches()[0].person_id, "existing");
    }
}
