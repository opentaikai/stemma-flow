//! Shared dialog for editing notes on any record.

use egui::{Context, RichText, Ui};

use crate::db::notes::{Note, NoteOwner};

/// What the notes dialog asks the shell to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotesOutcome {
    Save { id: Option<String>, body: String },
    Delete(String),
}

/// State owned by the window while the notes dialog is open.
#[derive(Debug, Default)]
pub struct NotesState {
    pub target: Option<NoteOwner>,
    pub title: String,
    pub notes: Vec<Note>,
    pub draft: String,
    pub editing: Option<String>,
    pub adding: bool,
    pub error: Option<String>,
}

impl NotesState {
    pub fn open(&mut self, target: NoteOwner, title: String, notes: Vec<Note>) {
        self.target = Some(target);
        self.title = title;
        self.notes = notes;
        self.draft.clear();
        self.editing = None;
        self.adding = false;
        self.error = None;
    }

    pub fn replace_notes(&mut self, notes: Vec<Note>) {
        self.notes = notes;
    }

    pub fn begin_add(&mut self) {
        self.adding = true;
        self.editing = None;
        self.draft.clear();
        self.error = None;
    }

    pub fn begin_edit(&mut self, note: &Note) {
        self.adding = false;
        self.editing = Some(note.id.clone());
        self.draft = note.body.clone();
        self.error = None;
    }

    pub fn finish_edit(&mut self) {
        self.adding = false;
        self.editing = None;
        self.draft.clear();
    }
}

/// Draws the notes window; `None` keeps it open.
pub fn show(ctx: &Context, state: &mut NotesState) -> Option<NotesOutcome> {
    state.target.as_ref()?;
    let mut outcome = None;
    let mut open = true;
    egui::Window::new(&state.title)
        .id(egui::Id::new("record_notes_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(440.0)
        .open(&mut open)
        .show(ctx, |ui| {
            notes_list(ui, state, &mut outcome);
            ui.separator();
            if !state.adding && state.editing.is_none() && ui.button("+ Add note").clicked() {
                state.begin_add();
            }
            if state.adding || state.editing.is_some() {
                note_editor(ui, state, &mut outcome);
            }
            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
        });
    if !open {
        state.target = None;
        state.finish_edit();
        return None;
    }
    outcome
}

fn notes_list(ui: &mut Ui, state: &mut NotesState, outcome: &mut Option<NotesOutcome>) {
    if state.notes.is_empty() {
        ui.label(RichText::new("No notes yet.").weak().small());
        return;
    }
    let mut edit = None;
    let mut delete = None;
    for note in &state.notes {
        ui.push_id(&note.id, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&note.body).small());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if ui.small_button("Delete").clicked() {
                        delete = Some(note.id.clone());
                    }
                    if ui.small_button("Edit").clicked() {
                        edit = Some(note.id.clone());
                    }
                });
            });
        });
        ui.separator();
    }
    if let Some(id) = delete {
        *outcome = Some(NotesOutcome::Delete(id));
    } else if let Some(id) = edit
        && let Some(note) = state.notes.iter().find(|note| note.id == id).cloned()
    {
        state.begin_edit(&note);
    }
}

fn note_editor(ui: &mut Ui, state: &mut NotesState, outcome: &mut Option<NotesOutcome>) {
    ui.add(
        egui::TextEdit::multiline(&mut state.draft)
            .desired_rows(6)
            .desired_width(f32::INFINITY)
            .hint_text("Note text…"),
    );
    let valid = !state.draft.trim().is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                valid,
                egui::Button::new(RichText::new("Save note").strong()),
            )
            .clicked()
        {
            *outcome = Some(NotesOutcome::Save {
                id: state.editing.clone(),
                body: state.draft.trim().to_string(),
            });
        }
        if ui.button("Cancel").clicked() {
            state.finish_edit();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::RawInput;

    fn note(id: &str, body: &str) -> Note {
        Note {
            id: id.to_string(),
            body: body.to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn dialog_renders_with_notes() {
        let ctx = Context::default();
        let mut state = NotesState::default();
        state.open(
            NoteOwner::Person("p1".to_string()),
            "Notes — Ada King".to_string(),
            vec![note("n1", "First note")],
        );
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui.ctx(), &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the dialog paints");
    }
}
