//! Fan chart export options.

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;

/// State owned by the fan chart dialog.
#[derive(Debug, Clone)]
pub struct FanChartState {
    pub open: bool,
    pub root_id: Option<String>,
    pub generations: usize,
    pub span_degrees: f64,
    pub start_angle: f64,
}

impl Default for FanChartState {
    fn default() -> Self {
        Self {
            open: false,
            root_id: None,
            generations: 5,
            span_degrees: 180.0,
            start_angle: 180.0,
        }
    }
}

impl FanChartState {
    pub fn open_with(&mut self, root_id: Option<String>) {
        self.open = true;
        self.root_id = root_id;
        self.generations = self.generations.clamp(3, 6);
        self.span_degrees = self.span_degrees.clamp(90.0, 360.0);
        self.start_angle = self.start_angle.rem_euclid(360.0);
    }
}

/// What the fan chart dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq)]
pub struct FanChartEvent {
    pub root_id: String,
    pub generations: usize,
    pub span_degrees: f64,
    pub start_angle: f64,
}

/// Draws the fan chart options window.
pub fn show(
    ctx: &Context,
    state: &mut FanChartState,
    people: &[Person],
    surname_first: bool,
) -> Option<FanChartEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Fan Chart")
        .id(egui::Id::unique("fan_chart_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Export a radial ancestor chart with selectable span and rotation.")
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
            egui::ComboBox::from_id_salt("fan_chart_root")
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
            ui.add(egui::Slider::new(&mut state.generations, 3..=6).text("Generations"));
            ui.horizontal(|ui| {
                ui.label("Span");
                for (degrees, label) in [
                    (90.0, "Quarter (90\u{00b0})"),
                    (180.0, "Half (180\u{00b0})"),
                    (270.0, "Three-quarter (270\u{00b0})"),
                    (360.0, "Full circle (360\u{00b0})"),
                ] {
                    ui.selectable_value(&mut state.span_degrees, degrees, label);
                }
            });
            let full_circle = state.span_degrees >= 360.0;
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!full_circle, |ui| {
                    ui.label("Start");
                    ui.add(
                        egui::DragValue::new(&mut state.start_angle)
                            .range(0.0..=359.0)
                            .speed(1.0)
                            .suffix("\u{00b0}"),
                    );
                    if ui.small_button("Top").clicked() {
                        state.start_angle = 180.0;
                    }
                    if ui.small_button("Right").clicked() {
                        state.start_angle = 270.0;
                    }
                    if ui.small_button("Bottom").clicked() {
                        state.start_angle = 0.0;
                    }
                    if ui.small_button("Left").clicked() {
                        state.start_angle = 90.0;
                    }
                });
                if full_circle {
                    ui.label(
                        RichText::new("A full circle ignores the start angle.")
                            .weak()
                            .small(),
                    );
                }
            });
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
                    event = Some(FanChartEvent {
                        root_id,
                        generations: state.generations,
                        span_degrees: state.span_degrees,
                        start_angle: if state.span_degrees >= 360.0 {
                            0.0
                        } else {
                            state.start_angle
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
    fn opens_with_a_root_and_renders() {
        let ctx = Context::default();
        let mut state = FanChartState::default();
        state.open_with(Some("p1".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person()], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert_eq!(state.span_degrees, 180.0);
    }
}
