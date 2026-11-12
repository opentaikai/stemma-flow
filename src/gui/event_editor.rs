//! Modeless editor for an event opened from the place list.

use egui::{Context, RichText, Window};

use crate::db::events::{PlaceEventDraft, PlaceEventRecord};
use crate::gui::date_picker::date_edit;
use crate::i18n::{tr, tr_args};

/// State owned by the place event editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventEditor {
    pub draft: PlaceEventDraft,
    pub event_type: String,
    pub event_label: String,
    pub place_name: Option<String>,
    pub person_id: Option<String>,
    pub error: Option<String>,
    saved: PlaceEventDraft,
}

impl EventEditor {
    pub fn from_record(record: PlaceEventRecord) -> Self {
        let draft = PlaceEventDraft {
            id: record.id,
            date: record.date.unwrap_or_default(),
            description: record.description.unwrap_or_default(),
            private: record.private,
        };
        Self {
            saved: draft.clone(),
            draft,
            event_type: record.event_type,
            event_label: record.event_label,
            place_name: record.place_name,
            person_id: record.person_id,
            error: None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.draft != self.saved
    }

    /// Records the draft as persisted so the form stops reporting changes.
    pub fn mark_saved(&mut self) {
        self.saved = self.draft.clone();
    }
}

/// What the event editor asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventEditorOutcome {
    Save(PlaceEventDraft),
    Cancel,
}

/// Renders the event editor, returning an outcome when it closes.
pub fn show(ctx: &Context, editor: &mut EventEditor) -> Option<EventEditorOutcome> {
    let mut open = true;
    let mut outcome = None;
    Window::new(tr("event_editor.title"))
        .id(egui::Id::unique("place_event_editor_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(440.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.heading(&editor.event_label);
            ui.label(RichText::new(&editor.event_type).weak().small());
            if let Some(place) = &editor.place_name {
                ui.label(
                    RichText::new(tr_args("event_editor.place", &[("place", place.clone())]))
                        .small(),
                );
            }
            ui.separator();

            egui::Grid::new("place_event_fields")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("event_editor.date"));
                    date_edit(ui, &mut editor.draft.date, "place_event_date");
                    ui.end_row();

                    ui.label(tr("event_editor.description"));
                    ui.add(
                        egui::TextEdit::multiline(&mut editor.draft.description)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                });
            ui.checkbox(&mut editor.draft.private, tr("event_editor.private"));

            if let Some(error) = &editor.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        editor.is_dirty(),
                        egui::Button::new(RichText::new(tr("event_editor.save")).strong()),
                    )
                    .clicked()
                {
                    outcome = Some(EventEditorOutcome::Save(editor.draft.clone()));
                }
                if ui.button(tr("event_editor.cancel")).clicked() {
                    outcome = Some(EventEditorOutcome::Cancel);
                }
            });
        });
    if !open {
        outcome = Some(EventEditorOutcome::Cancel);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn record() -> PlaceEventRecord {
        PlaceEventRecord {
            id: "e1".to_string(),
            event_type: "CENSUS".to_string(),
            event_label: "Census".to_string(),
            date: Some("1850".to_string()),
            place_id: Some("pl1".to_string()),
            place_name: Some("Uppsala, Sweden".to_string()),
            description: Some("Household".to_string()),
            private: true,
            person_id: Some("p1".to_string()),
            family_id: None,
        }
    }

    #[test]
    fn builds_a_clean_draft_from_the_record() {
        let mut editor = EventEditor::from_record(record());
        assert_eq!(editor.draft.id, "e1");
        assert_eq!(editor.draft.date, "1850");
        assert_eq!(editor.draft.description, "Household");
        assert!(editor.draft.private);
        assert_eq!(editor.person_id.as_deref(), Some("p1"));
        assert!(!editor.is_dirty());

        editor.draft.date = "1851".to_string();
        assert!(editor.is_dirty());
        editor.mark_saved();
        assert!(!editor.is_dirty());
    }

    #[test]
    fn renders_headless_without_saving() {
        let ctx = Context::default();
        let mut editor = EventEditor::from_record(record());
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut editor).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(!editor.is_dirty());
    }
}
