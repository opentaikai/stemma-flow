//! Photo tree export options.

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;

/// State owned by the photo tree dialog.
#[derive(Debug, Clone)]
pub struct PhotoTreeState {
    pub open: bool,
    pub root_id: Option<String>,
    pub generations: usize,
}

impl Default for PhotoTreeState {
    fn default() -> Self {
        Self {
            open: false,
            root_id: None,
            generations: 4,
        }
    }
}

impl PhotoTreeState {
    pub fn open_with(&mut self, root_id: Option<String>) {
        self.open = true;
        self.root_id = root_id;
        self.generations = self.generations.clamp(2, 6);
    }
}

/// What the photo tree dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoTreeEvent {
    pub root_id: String,
    pub generations: usize,
}

/// Draws the photo tree options window.
pub fn show(
    ctx: &Context,
    state: &mut PhotoTreeState,
    people: &[Person],
    surname_first: bool,
) -> Option<PhotoTreeEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Photo Tree")
        .id(egui::Id::unique("photo_tree_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Export a printable ancestor chart with portraits.")
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
            egui::ComboBox::from_id_salt("photo_tree_root")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for person in people {
                        let selected = state.root_id.as_deref() == Some(person.id.as_str());
                        if ui
                            .selectable_label(selected, person_label(person, surname_first))
                            .clicked()
                        {
                            state.root_id = Some(person.id.clone());
                        }
                    }
                });
            ui.add(egui::Slider::new(&mut state.generations, 2..=6).text("Generations"));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        state.root_id.is_some(),
                        egui::Button::new(RichText::new("Export\u{2026}").strong()),
                    )
                    .clicked()
                    && let Some(root_id) = state.root_id.clone()
                {
                    state.open = false;
                    event = Some(PhotoTreeEvent {
                        root_id,
                        generations: state.generations,
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
    fn opens_with_a_root_and_renders() {
        let ctx = Context::default();
        let mut state = PhotoTreeState::default();
        state.open_with(Some("p1".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person()], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }
}
