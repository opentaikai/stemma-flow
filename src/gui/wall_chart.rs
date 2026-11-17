//! Wall chart export options.

use std::path::PathBuf;

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;
use crate::report::wall_chart::{WallChartMode, WallOrientation, WallPageSize};

/// One image available as a wall chart background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallMediaChoice {
    pub label: String,
    pub path: PathBuf,
}

/// State owned by the wall chart dialog.
#[derive(Debug, Clone)]
pub struct WallChartState {
    pub open: bool,
    pub mode: WallChartMode,
    pub root_id: Option<String>,
    pub ancestor_generations: usize,
    pub descendant_generations: usize,
    pub page_size: WallPageSize,
    pub orientation: WallOrientation,
    pub media: Vec<WallMediaChoice>,
    pub background: Option<PathBuf>,
    /// Background opacity in percent.
    pub background_opacity: f64,
}

impl Default for WallChartState {
    fn default() -> Self {
        Self {
            open: false,
            mode: WallChartMode::Ancestors,
            root_id: None,
            ancestor_generations: 5,
            descendant_generations: 4,
            page_size: WallPageSize::A2,
            orientation: WallOrientation::Landscape,
            media: Vec::new(),
            background: None,
            background_opacity: 20.0,
        }
    }
}

impl WallChartState {
    pub fn open_with(&mut self, root_id: Option<String>, media: Vec<WallMediaChoice>) {
        self.open = true;
        self.root_id = root_id;
        self.media = media;
        self.background = None;
        self.ancestor_generations = self.ancestor_generations.clamp(3, 8);
        self.descendant_generations = self.descendant_generations.clamp(2, 7);
        self.background_opacity = self.background_opacity.clamp(0.0, 100.0);
    }
}

/// What the wall chart dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq)]
pub struct WallChartEvent {
    pub mode: WallChartMode,
    pub root_id: String,
    pub ancestor_generations: usize,
    pub descendant_generations: usize,
    pub page_size: WallPageSize,
    pub orientation: WallOrientation,
    pub background: Option<PathBuf>,
    pub background_opacity: f64,
}

/// Draws the wall chart options window.
pub fn show(
    ctx: &Context,
    state: &mut WallChartState,
    people: &[Person],
    surname_first: bool,
) -> Option<WallChartEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Wall Charts")
        .id(egui::Id::unique("wall_chart_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Export a large-format ancestor or descendant chart with an optional media background.",
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
            ui.horizontal(|ui| {
                ui.label("Chart");
                ui.selectable_value(&mut state.mode, WallChartMode::Ancestors, "Ancestors");
                ui.selectable_value(&mut state.mode, WallChartMode::Descendants, "Descendants");
            });
            let current = state
                .root_id
                .as_deref()
                .and_then(|id| people.iter().find(|person| person.id == id))
                .map(|person| person_label(person, surname_first))
                .unwrap_or_else(|| "Select a person".to_string());
            egui::ComboBox::from_id_salt("wall_chart_root")
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
            match state.mode {
                WallChartMode::Ancestors => {
                    ui.add(
                        egui::Slider::new(&mut state.ancestor_generations, 3..=8)
                            .text("Generations"),
                    );
                }
                WallChartMode::Descendants => {
                    ui.add(
                        egui::Slider::new(&mut state.descendant_generations, 2..=7)
                            .text("Generations"),
                    );
                }
            }
            ui.horizontal(|ui| {
                ui.label("Page");
                egui::ComboBox::from_id_salt("wall_chart_page")
                    .selected_text(state.page_size.label())
                    .show_ui(ui, |ui| {
                        for size in [
                            WallPageSize::A3,
                            WallPageSize::A2,
                            WallPageSize::A1,
                            WallPageSize::A0,
                        ] {
                            ui.selectable_value(&mut state.page_size, size, size.label());
                        }
                    });
                ui.selectable_value(
                    &mut state.orientation,
                    WallOrientation::Landscape,
                    "Landscape",
                );
                ui.selectable_value(
                    &mut state.orientation,
                    WallOrientation::Portrait,
                    "Portrait",
                );
            });
            ui.horizontal(|ui| {
                ui.label("Background");
                let current = state
                    .background
                    .as_deref()
                    .and_then(|path| {
                        state
                            .media
                            .iter()
                            .find(|choice| choice.path == path)
                            .map(|choice| choice.label.clone())
                    })
                    .unwrap_or_else(|| "None".to_string());
                egui::ComboBox::from_id_salt("wall_chart_background")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(state.background.is_none(), "None")
                            .clicked()
                        {
                            state.background = None;
                        }
                        for choice in &state.media {
                            let selected = state.background.as_deref() == Some(choice.path.as_path());
                            if ui.selectable_label(selected, &choice.label).clicked() {
                                state.background = Some(choice.path.clone());
                            }
                        }
                    });
                if state.media.is_empty() {
                    ui.label(RichText::new("No image media available.").weak().small());
                }
            });
            if state.background.is_some() {
                ui.add(
                    egui::Slider::new(&mut state.background_opacity, 0.0..=100.0)
                        .text("Opacity %"),
                );
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        state.root_id.is_some(),
                        egui::Button::new(RichText::new("Export HTML\u{2026}").strong()),
                    )
                    .clicked()
                    && let Some(root_id) = state.root_id.clone()
                {
                    state.open = false;
                    event = Some(WallChartEvent {
                        mode: state.mode,
                        root_id,
                        ancestor_generations: state.ancestor_generations,
                        descendant_generations: state.descendant_generations,
                        page_size: state.page_size,
                        orientation: state.orientation,
                        background: state.background.clone(),
                        background_opacity: state.background_opacity / 100.0,
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
        let mut state = WallChartState::default();
        state.open_with(
            Some("p1".to_string()),
            vec![WallMediaChoice {
                label: "Garden".to_string(),
                path: PathBuf::from("/tmp/garden.jpg"),
            }],
        );
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person()], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert_eq!(state.mode, WallChartMode::Ancestors);
        assert_eq!(state.media.len(), 1);
    }
}
