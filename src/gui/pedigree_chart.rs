//! Pedigree chart export options.

use crate::i18n::tr;

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;

/// State owned by the pedigree chart dialog.
#[derive(Debug, Clone)]
pub struct PedigreeChartState {
    pub open: bool,
    pub root_id: Option<String>,
    pub generations: usize,
    pub include_photos: bool,
}

impl Default for PedigreeChartState {
    fn default() -> Self {
        Self {
            open: false,
            root_id: None,
            generations: 5,
            include_photos: true,
        }
    }
}

impl PedigreeChartState {
    pub fn open_with(&mut self, root_id: Option<String>) {
        self.open = true;
        self.root_id = root_id;
        self.generations = self.generations.clamp(4, 8);
    }
}

/// What the pedigree chart dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedigreeChartEvent {
    pub root_id: String,
    pub generations: usize,
    pub include_photos: bool,
    pub svg: bool,
}

/// Draws the pedigree chart options window.
pub fn show(
    ctx: &Context,
    state: &mut PedigreeChartState,
    people: &[Person],
    surname_first: bool,
) -> Option<PedigreeChartEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("pedigree_chart.title"))
        .id(egui::Id::unique("pedigree_chart_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(tr("pedigree_chart.description"))
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
                .root_id
                .as_deref()
                .and_then(|id| people.iter().find(|person| person.id == id))
                .map(|person| person_label(person, surname_first))
                .unwrap_or_else(|| tr("report_dialog.select_person").to_string());
            egui::ComboBox::from_id_salt("pedigree_chart_root")
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
            ui.add(egui::Slider::new(&mut state.generations, 4..=8).text(tr("common.generations")));
            ui.checkbox(&mut state.include_photos, tr("common.include_portraits"));
            ui.label(
                RichText::new(tr("pedigree_chart.numbering_hint"))
                    .weak()
                    .small(),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        state.root_id.is_some(),
                        egui::Button::new(RichText::new(tr("common.export")).strong()),
                    )
                    .clicked()
                    && let Some(root_id) = state.root_id.clone()
                {
                    state.open = false;
                    event = Some(PedigreeChartEvent {
                        root_id,
                        generations: state.generations,
                        include_photos: state.include_photos,
                        svg: false,
                    });
                }
                if ui
                    .add_enabled(state.root_id.is_some(), egui::Button::new(tr("common.svg")))
                    .clicked()
                    && let Some(root_id) = state.root_id.clone()
                {
                    state.open = false;
                    event = Some(PedigreeChartEvent {
                        root_id,
                        generations: state.generations,
                        include_photos: state.include_photos,
                        svg: true,
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
    fn opens_with_a_root_and_renders() {
        let ctx = Context::default();
        let mut state = PedigreeChartState::default();
        state.open_with(Some("p1".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[person()], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert!(state.include_photos);
    }

    #[test]
    fn generation_clamps_to_supported_range() {
        let mut state = PedigreeChartState {
            generations: 20,
            ..PedigreeChartState::default()
        };
        state.open_with(Some("p1".to_string()));
        assert_eq!(state.generations, 8);
    }
}
