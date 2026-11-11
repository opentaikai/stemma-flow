//! Multi-criteria person search that filters the People grid.

use egui::{Context, RichText, Window};

use crate::i18n::tr;
use crate::model::person_search::{
    GenderFilter, PersonSearchCriteria, SavedPersonSearch, YearRange,
};

/// State of the Advanced Search window.
#[derive(Debug, Default)]
pub struct AdvancedSearchState {
    pub open: bool,
    pub name: String,
    pub gender: GenderFilter,
    pub birth_from: String,
    pub birth_to: String,
    pub death_from: String,
    pub death_to: String,
    pub place: String,
    pub fact_type: String,
    pub fact_text: String,
    pub notes: String,
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
            Some(criteria) => self.load_criteria(criteria),
            None => self.clear_fields(),
        }
    }

    fn load_criteria(&mut self, criteria: &PersonSearchCriteria) {
        self.name = criteria.name.clone();
        self.gender = criteria.gender;
        self.birth_from = year_text(criteria.birth.from);
        self.birth_to = year_text(criteria.birth.to);
        self.death_from = year_text(criteria.death.from);
        self.death_to = year_text(criteria.death.to);
        self.place = criteria.place.clone();
        self.fact_type = criteria.fact_type.clone();
        self.fact_text = criteria.fact_text.clone();
        self.notes = criteria.notes.clone();
    }

    /// Loads a saved rule into the fields without applying it.
    pub fn load_saved(&mut self, saved: &SavedPersonSearch) {
        self.load_criteria(&saved.criteria);
        self.selected_saved = Some(saved.name.clone());
        self.save_name = saved.name.clone();
        self.error = None;
    }

    pub fn clear_fields(&mut self) {
        self.name.clear();
        self.gender = GenderFilter::Any;
        self.birth_from.clear();
        self.birth_to.clear();
        self.death_from.clear();
        self.death_to.clear();
        self.place.clear();
        self.fact_type.clear();
        self.fact_text.clear();
        self.notes.clear();
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
        let criteria = self.criteria()?;
        if criteria.is_empty() {
            self.error = Some(tr("people.search_saved_empty"));
            return None;
        }
        Some(SavedPersonSearch::new(name, criteria))
    }

    /// Converts the fields into criteria, reporting invalid years inline.
    pub fn criteria(&mut self) -> Option<PersonSearchCriteria> {
        self.error = None;
        let birth_from = match parse_year(&self.birth_from) {
            Ok(year) => year,
            Err(()) => return self.fail("people.search_invalid_year"),
        };
        let birth_to = match parse_year(&self.birth_to) {
            Ok(year) => year,
            Err(()) => return self.fail("people.search_invalid_year"),
        };
        let death_from = match parse_year(&self.death_from) {
            Ok(year) => year,
            Err(()) => return self.fail("people.search_invalid_year"),
        };
        let death_to = match parse_year(&self.death_to) {
            Ok(year) => year,
            Err(()) => return self.fail("people.search_invalid_year"),
        };
        if range_inverted(birth_from, birth_to) || range_inverted(death_from, death_to) {
            return self.fail("people.search_invalid_range");
        }
        Some(PersonSearchCriteria {
            name: self.name.trim().to_string(),
            gender: self.gender,
            birth: YearRange {
                from: birth_from,
                to: birth_to,
            },
            death: YearRange {
                from: death_from,
                to: death_to,
            },
            place: self.place.trim().to_string(),
            fact_type: self.fact_type.trim().to_string(),
            fact_text: self.fact_text.trim().to_string(),
            notes: self.notes.trim().to_string(),
        })
    }

    fn fail(&mut self, key: &str) -> Option<PersonSearchCriteria> {
        self.error = Some(tr(key));
        None
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

            egui::Grid::new("advanced_search_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("people.search_name"));
                    ui.text_edit_singleline(&mut state.name);
                    ui.end_row();

                    ui.label(tr("people.search_gender"));
                    egui::ComboBox::from_id_salt("advanced_search_gender")
                        .selected_text(gender_label(state.gender))
                        .show_ui(ui, |ui| {
                            for gender in [
                                GenderFilter::Any,
                                GenderFilter::Male,
                                GenderFilter::Female,
                                GenderFilter::Unknown,
                            ] {
                                ui.selectable_value(
                                    &mut state.gender,
                                    gender,
                                    gender_label(gender),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label(tr("people.search_birth"));
                    year_range_row(ui, &mut state.birth_from, &mut state.birth_to);
                    ui.end_row();

                    ui.label(tr("people.search_death"));
                    year_range_row(ui, &mut state.death_from, &mut state.death_to);
                    ui.end_row();

                    ui.label(tr("people.search_place"));
                    ui.text_edit_singleline(&mut state.place);
                    ui.end_row();

                    ui.label(tr("people.search_fact_type"));
                    ui.text_edit_singleline(&mut state.fact_type);
                    ui.end_row();

                    ui.label(tr("people.search_fact_text"));
                    ui.text_edit_singleline(&mut state.fact_text);
                    ui.end_row();

                    ui.label(tr("people.search_notes"));
                    ui.text_edit_singleline(&mut state.notes);
                    ui.end_row();
                });

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

fn year_range_row(ui: &mut egui::Ui, from: &mut String, to: &mut String) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(from)
                .hint_text(tr("people.search_year_from"))
                .desired_width(70.0),
        );
        ui.label(tr("people.search_to"));
        ui.add(
            egui::TextEdit::singleline(to)
                .hint_text(tr("people.search_year_to"))
                .desired_width(70.0),
        );
    });
}

fn gender_label(gender: GenderFilter) -> String {
    match gender {
        GenderFilter::Any => tr("people.search_gender_any"),
        GenderFilter::Male => tr("inspector.gender_male"),
        GenderFilter::Female => tr("inspector.gender_female"),
        GenderFilter::Unknown => tr("inspector.gender_unknown"),
    }
}

fn year_text(year: Option<i32>) -> String {
    year.map(|year| year.to_string()).unwrap_or_default()
}

fn parse_year(value: &str) -> Result<Option<i32>, ()> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value.parse::<i32>().map(Some).map_err(|_| ())
}

fn range_inverted(from: Option<i32>, to: Option<i32>) -> bool {
    matches!((from, to), (Some(from), Some(to)) if from > to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn criteria_parse_years_and_reject_invalid_input() {
        let mut state = AdvancedSearchState {
            name: "ada".to_string(),
            birth_from: "1800".to_string(),
            birth_to: "1820".to_string(),
            death_from: "".to_string(),
            death_to: "1900".to_string(),
            ..AdvancedSearchState::default()
        };
        let criteria = state.criteria().expect("criteria");
        assert_eq!(criteria.name, "ada");
        assert_eq!(criteria.birth.from, Some(1800));
        assert_eq!(criteria.birth.to, Some(1820));
        assert_eq!(criteria.death.from, None);
        assert_eq!(criteria.death.to, Some(1900));

        state.birth_from = "about 1800".to_string();
        assert!(state.criteria().is_none());
        assert!(state.error.is_some());

        state.birth_from = "1821".to_string();
        state.error = None;
        assert!(state.criteria().is_none());
        assert!(state.error.is_some());
    }

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
        assert_eq!(state.name, "Ada");
        assert_eq!(state.birth_from, "1815");
        assert_eq!(state.birth_to, "1820");
        assert_eq!(state.gender, GenderFilter::Female);

        state.clear_fields();
        assert!(state.name.is_empty());
        assert_eq!(state.gender, GenderFilter::Any);
        assert!(state.birth_from.is_empty());
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
        assert_eq!(state.name, "Ada");
        assert_eq!(state.gender, GenderFilter::Female);
        assert_eq!(state.birth_from, "1815");
        assert_eq!(state.place, "London");
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

        state.name = "Ada".to_string();
        let saved = state.saved_search().expect("saved search");
        assert_eq!(saved.name, "Unnamed");
        assert_eq!(saved.criteria.name, "Ada");
    }
}
