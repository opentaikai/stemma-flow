//! Multi-criteria person search that filters the People grid.

use egui::{Context, RichText, Window};

use crate::gui::person_criteria::PersonCriteriaFields;
use crate::i18n::tr;
use crate::model::person_search::{PersonSearchCriteria, SavedPersonSearch};

/// State of the Advanced Search window.
#[derive(Debug, Default)]
pub struct AdvancedSearchState {
    pub open: bool,
    pub fields: PersonCriteriaFields,
    pub error: Option<String>,
    pub selected_saved: Option<String>,
    pub save_name: String,
}

impl AdvancedSearchState {
    /// Opens the dialog with the active criteria loaded for editing.
    pub fn open_with(&mut self, criteria: Option<&PersonSearchCriteria>) {
        self.open = true;
        self.error = None;
        self.selected_saved = None;
        self.save_name.clear();
        match criteria {
            Some(criteria) => self.fields.load(criteria),
            None => self.fields.clear(),
        }
    }

    /// Loads a saved rule into the fields without applying it.
    pub fn load_saved(&mut self, saved: &SavedPersonSearch) {
        self.fields.load(&saved.criteria);
        self.selected_saved = Some(saved.name.clone());
        self.save_name = saved.name.clone();
        self.error = None;
    }

    pub fn clear_fields(&mut self) {
        self.fields.clear();
        self.selected_saved = None;
        self.save_name.clear();
        self.error = None;
    }

    /// Validates the current fields and builds a named rule.
    pub fn saved_search(&mut self) -> Option<SavedPersonSearch> {
        let name = self.save_name.trim().to_string();
        if name.is_empty() {
            self.error = Some(tr("people.search_saved_name_required"));
            return None;
        }
        let criteria = self.fields.criteria()?;
        if criteria.is_empty() {
            self.error = Some(tr("people.search_saved_empty"));
            return None;
        }
        Some(SavedPersonSearch::new(name, criteria))
    }

    pub fn criteria(&mut self) -> Option<PersonSearchCriteria> {
        self.fields.criteria()
    }
}

/// What the Advanced Search window asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvancedSearchEvent {
    Apply(Box<PersonSearchCriteria>),
    Clear,
    Save(Box<SavedPersonSearch>),
    Delete(String),
}

/// Draws the criteria window.
pub fn show(
    ctx: &Context,
    state: &mut AdvancedSearchState,
    saved: &[SavedPersonSearch],
) -> Option<AdvancedSearchEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("people.search_title"))
        .id(egui::Id::unique("advanced_search_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(430.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("people.search_hint")).weak().small());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label(tr("people.search_saved"));
                let selected_text = state
                    .selected_saved
                    .clone()
                    .unwrap_or_else(|| tr("people.search_saved_none"));
                egui::ComboBox::from_id_salt("advanced_saved_search")
                    .selected_text(selected_text)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for saved in saved {
                            let selected =
                                state.selected_saved.as_deref() == Some(saved.name.as_str());
                            if ui.selectable_label(selected, &saved.name).clicked() {
                                state.load_saved(saved);
                            }
                        }
                    });
                let can_delete = state.selected_saved.as_deref().is_some_and(|name| {
                    let name = name.to_lowercase();
                    saved.iter().any(|item| item.name.to_lowercase() == name)
                });
                if ui
                    .add_enabled(can_delete, egui::Button::new(tr("common.delete")))
                    .clicked()
                    && let Some(name) = state.selected_saved.clone()
                {
                    event = Some(AdvancedSearchEvent::Delete(name));
                }
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut state.save_name)
                        .hint_text(tr("people.search_saved_name"))
                        .char_limit(80),
                );
                if ui.button(tr("people.search_saved_save")).clicked()
                    && let Some(saved) = state.saved_search()
                {
                    event = Some(AdvancedSearchEvent::Save(Box::new(saved)));
                }
            });
            ui.separator();

            state.fields.show(ui, "advanced_search");

            if let Some(error) = &state.error {
                ui.add_space(4.0);
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(
                        RichText::new(tr("people.search_apply")).strong(),
                    ))
                    .clicked()
                    && let Some(criteria) = state.criteria()
                {
                    event = Some(AdvancedSearchEvent::Apply(Box::new(criteria)));
                }
                if ui.button(tr("people.search_clear")).clicked() {
                    state.clear_fields();
                    event = Some(AdvancedSearchEvent::Clear);
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
    use crate::model::person_search::{GenderFilter, YearRange};
    use egui::{Context, RawInput};

    #[test]
    fn open_with_round_trips_criteria_and_clear_resets_fields() {
        let criteria = PersonSearchCriteria {
            name: "Ada".to_string(),
            gender: GenderFilter::Female,
            birth: YearRange {
                from: Some(1815),
                to: Some(1820),
            },
            place: "London".to_string(),
            notes: "parish".to_string(),
            ..PersonSearchCriteria::default()
        };
        let mut state = AdvancedSearchState::default();
        state.open_with(Some(&criteria));
        assert_eq!(state.fields.name, "Ada");
        assert_eq!(state.fields.birth_from, "1815");
        assert_eq!(state.fields.birth_to, "1820");
        assert_eq!(state.fields.gender, GenderFilter::Female);

        state.clear_fields();
        assert!(state.fields.name.is_empty());
        assert_eq!(state.fields.gender, GenderFilter::Any);
        assert!(state.fields.birth_from.is_empty());
    }

    #[test]
    fn window_renders_headless() {
        let ctx = Context::default();
        let mut state = AdvancedSearchState::default();
        state.open_with(None);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &[]).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }

    #[test]
    fn loading_a_saved_search_fills_the_fields_without_applying() {
        let saved = SavedPersonSearch::new(
            "London",
            PersonSearchCriteria {
                name: "Ada".to_string(),
                gender: GenderFilter::Female,
                birth: YearRange {
                    from: Some(1815),
                    to: Some(1820),
                },
                place: "London".to_string(),
                ..PersonSearchCriteria::default()
            },
        );
        let mut state = AdvancedSearchState::default();
        state.load_saved(&saved);
        assert_eq!(state.fields.name, "Ada");
        assert_eq!(state.fields.gender, GenderFilter::Female);
        assert_eq!(state.fields.birth_from, "1815");
        assert_eq!(state.fields.place, "London");
        assert_eq!(state.selected_saved.as_deref(), Some("London"));
        assert_eq!(state.save_name, "London");
        assert!(state.error.is_none());
    }

    #[test]
    fn saving_requires_a_name_and_at_least_one_criterion() {
        let mut state = AdvancedSearchState::default();
        assert!(state.saved_search().is_none());
        assert!(state.error.is_some());

        state.save_name = "Unnamed".to_string();
        assert!(state.saved_search().is_none());
        assert!(state.error.is_some());

        state.fields.name = "Ada".to_string();
        let saved = state.saved_search().expect("saved search");
        assert_eq!(saved.name, "Unnamed");
        assert_eq!(saved.criteria.name, "Ada");
    }
}
