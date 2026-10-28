//! Alternate-name editor shown in the person inspector.

use egui::{RichText, Ui};

use crate::db::names::{AlternateName, AlternateNameDraft};

/// Action requested by the names card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamesEvent {
    Save(Box<AlternateNameDraft>),
    Delete(String),
}

/// State owned by the window while the names card is visible.
#[derive(Debug, Default)]
pub struct NamesState {
    pub person_id: Option<String>,
    pub names: Vec<AlternateName>,
    pub draft: AlternateNameDraft,
    pub editing: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

/// Converts an alternate name into an editable draft.
pub fn draft_from_name(name: &AlternateName) -> AlternateNameDraft {
    AlternateNameDraft {
        id: Some(name.id.clone()),
        name_type: name.name_type.clone(),
        prefix: name.prefix.clone(),
        given_name: name.given_name.clone(),
        middle_name: name.middle_name.clone(),
        surname: name.surname.clone(),
        suffix: name.suffix.clone(),
        nickname: name.nickname.clone(),
    }
}

/// Draws the alternate-name card.
pub fn show(ui: &mut Ui, state: &mut NamesState, surname_first: bool) -> Option<NamesEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        if ui.small_button("+ Add name").clicked() {
            state.draft = AlternateNameDraft {
                name_type: "aka".to_string(),
                ..AlternateNameDraft::default()
            };
            state.editing = true;
            state.error = None;
        }
    });
    if state.names.is_empty() {
        ui.label(RichText::new("(none)").weak().small());
    }
    let mut delete = None;
    for name in &state.names {
        ui.horizontal(|ui| {
            ui.label(RichText::new(name.display_name(surname_first)).strong());
            ui.label(RichText::new(&name.name_type).weak().small());
            if ui.small_button("Edit").clicked() {
                state.draft = draft_from_name(name);
                state.editing = true;
                state.error = None;
            }
            if ui.small_button("Delete").clicked() {
                delete = Some(name.id.clone());
            }
        });
    }

    if state.editing {
        ui.separator();
        egui::Grid::new("alternate_name_fields")
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("Type:");
                ui.text_edit_singleline(&mut state.draft.name_type);
                ui.end_row();

                ui.label("Prefix:");
                ui.text_edit_singleline(&mut state.draft.prefix);
                ui.end_row();

                ui.label("Given:");
                ui.text_edit_singleline(&mut state.draft.given_name);
                ui.end_row();

                ui.label("Middle:");
                ui.text_edit_singleline(&mut state.draft.middle_name);
                ui.end_row();

                ui.label("Surname:");
                ui.text_edit_singleline(&mut state.draft.surname);
                ui.end_row();

                ui.label("Suffix:");
                ui.text_edit_singleline(&mut state.draft.suffix);
                ui.end_row();

                ui.label("Nickname:");
                ui.text_edit_singleline(&mut state.draft.nickname);
                ui.end_row();
            });
        ui.label(
            RichText::new("Type examples: aka, married, birth, religious.")
                .weak()
                .small(),
        );
        if let Some(error) = &state.error {
            ui.label(
                RichText::new(error)
                    .color(ui.visuals().error_fg_color)
                    .small(),
            );
        }
        let valid =
            !state.draft.given_name.trim().is_empty() || !state.draft.surname.trim().is_empty();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(valid, egui::Button::new("Save name"))
                .clicked()
            {
                event = Some(NamesEvent::Save(Box::new(state.draft.clone())));
            }
            if ui.button("Cancel").clicked() {
                state.editing = false;
                state.draft = AlternateNameDraft::default();
                state.error = None;
            }
        });
    }

    if let Some(id) = delete {
        event = Some(NamesEvent::Delete(id));
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn name() -> AlternateName {
        AlternateName {
            id: "n1".to_string(),
            person_id: "p1".to_string(),
            name_type: "married".to_string(),
            prefix: "Dr.".to_string(),
            given_name: "Ada".to_string(),
            middle_name: "Lynn".to_string(),
            surname: "Lovelace".to_string(),
            suffix: "Jr.".to_string(),
            nickname: "Addy".to_string(),
            sort_order: 0,
        }
    }

    #[test]
    fn drafts_round_trip_name_pieces() {
        let draft = draft_from_name(&name());
        assert_eq!(draft.id.as_deref(), Some("n1"));
        assert_eq!(draft.name_type, "married");
        assert_eq!(draft.prefix, "Dr.");
        assert_eq!(draft.suffix, "Jr.");
        assert_eq!(name().display_name(false), "Dr. Ada Lynn Lovelace Jr.");
    }

    #[test]
    fn names_card_renders_headless() {
        let ctx = Context::default();
        let mut state = NamesState {
            person_id: Some("p1".to_string()),
            names: vec![name()],
            editing: false,
            ..NamesState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the names card paints");
    }
}
