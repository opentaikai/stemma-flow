//! Hourglass and bow-tie chart options.

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;
use crate::report::hourglass::CombinedChartMode;

/// State owned by the combined chart dialog.
#[derive(Debug, Clone)]
pub struct HourglassChartState {
    pub open: bool,
    pub mode: CombinedChartMode,
    pub root_id: Option<String>,
    pub ancestor_generations: usize,
    pub descendant_generations: usize,
}

impl Default for HourglassChartState {
    fn default() -> Self {
        Self {
            open: false,
            mode: CombinedChartMode::Hourglass,
            root_id: None,
            ancestor_generations: 4,
            descendant_generations: 4,
        }
    }
}

impl HourglassChartState {
    pub fn open_with(&mut self, root_id: Option<String>) {
        self.open = true;
        self.root_id = root_id;
        self.ancestor_generations = self.ancestor_generations.clamp(2, 6);
        self.descendant_generations = self.descendant_generations.clamp(2, 6);
    }
}

/// What the combined chart dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HourglassChartEvent {
    pub mode: CombinedChartMode,
    pub root_id: String,
    pub ancestor_generations: usize,
    pub descendant_generations: usize,
    pub svg: bool,
}

/// Draws the hourglass/bow-tie options window.
pub fn show(
    ctx: &Context,
    state: &mut HourglassChartState,
    people: &[Person],
    surname_first: bool,
) -> Option<HourglassChartEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Hourglass & Bow-tie")
        .id(egui::Id::unique("hourglass_chart_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Export a combined ancestor and descendant chart around one person.")
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
            ui.horizontal(|ui| {
                ui.label("Layout");
                ui.selectable_value(&mut state.mode, CombinedChartMode::Hourglass, "Hourglass");
                ui.selectable_value(&mut state.mode, CombinedChartMode::BowTie, "Bow-tie");
            });
            let current = state
                .root_id
                .as_deref()
                .and_then(|id| people.iter().find(|person| person.id == id))
                .map(|person| person_label(person, surname_first))
                .unwrap_or_else(|| "Select a person".to_string());
            egui::ComboBox::from_id_salt("hourglass_chart_root")
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
            ui.add(
                egui::Slider::new(&mut state.ancestor_generations, 2..=6)
                    .text("Ancestor generations"),
            );
            ui.add(
                egui::Slider::new(&mut state.descendant_generations, 2..=6)
                    .text("Descendant generations"),
            );
            let description = match state.mode {
                CombinedChartMode::Hourglass => "Ancestors above the root and descendants below.",
                CombinedChartMode::BowTie => {
                    "Ancestors to the left of the root and descendants to the right."
                }
            };
            ui.label(RichText::new(description).weak().small());
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
                    event = Some(HourglassChartEvent {
                        mode: state.mode,
                        root_id,
                        ancestor_generations: state.ancestor_generations,
                        descendant_generations: state.descendant_generations,
                        svg: false,
                    });
                }
                if ui
                    .add_enabled(state.root_id.is_some(), egui::Button::new("SVG\u{2026}"))
                    .clicked()
                    && let Some(root_id) = state.root_id.clone()
                {
                    state.open = false;
                    event = Some(HourglassChartEvent {
                        mode: state.mode,
                        root_id,
                        ancestor_generations: state.ancestor_generations,
                        descendant_generations: state.descendant_generations,
                        svg: true,
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
        let mut state = HourglassChartState::default();
        state.open_with(Some("p1".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person()], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert_eq!(state.mode, CombinedChartMode::Hourglass);
    }
}
