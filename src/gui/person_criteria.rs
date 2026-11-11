//! Reusable person-search criteria fields for dialogs and group rules.

use egui::Ui;

use crate::i18n::tr;
use crate::model::person_search::{GenderFilter, PersonSearchCriteria, YearRange};

/// Editable text state for one set of person-search criteria.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersonCriteriaFields {
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
}

impl PersonCriteriaFields {
    pub fn load(&mut self, criteria: &PersonSearchCriteria) {
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
        self.error = None;
    }

    pub fn clear(&mut self) {
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
        self.error = None;
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

    /// Draws the shared criteria form.
    pub fn show(&mut self, ui: &mut Ui, id_salt: &str) {
        egui::Grid::new((id_salt, "criteria_grid"))
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label(tr("people.search_name"));
                ui.text_edit_singleline(&mut self.name);
                ui.end_row();

                ui.label(tr("people.search_gender"));
                egui::ComboBox::from_id_salt((id_salt, "gender"))
                    .selected_text(gender_label(self.gender))
                    .show_ui(ui, |ui| {
                        for gender in [
                            GenderFilter::Any,
                            GenderFilter::Male,
                            GenderFilter::Female,
                            GenderFilter::Unknown,
                        ] {
                            ui.selectable_value(&mut self.gender, gender, gender_label(gender));
                        }
                    });
                ui.end_row();

                ui.label(tr("people.search_birth"));
                year_range_row(ui, &mut self.birth_from, &mut self.birth_to);
                ui.end_row();

                ui.label(tr("people.search_death"));
                year_range_row(ui, &mut self.death_from, &mut self.death_to);
                ui.end_row();

                ui.label(tr("people.search_place"));
                ui.text_edit_singleline(&mut self.place);
                ui.end_row();

                ui.label(tr("people.search_fact_type"));
                ui.text_edit_singleline(&mut self.fact_type);
                ui.end_row();

                ui.label(tr("people.search_fact_text"));
                ui.text_edit_singleline(&mut self.fact_text);
                ui.end_row();

                ui.label(tr("people.search_notes"));
                ui.text_edit_singleline(&mut self.notes);
                ui.end_row();
            });
        if let Some(error) = &self.error {
            ui.label(egui::RichText::new(error).color(ui.visuals().error_fg_color));
        }
    }

    fn fail(&mut self, key: &str) -> Option<PersonSearchCriteria> {
        self.error = Some(tr(key));
        None
    }
}

fn year_range_row(ui: &mut Ui, from: &mut String, to: &mut String) {
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

    #[test]
    fn criteria_parse_years_and_reject_invalid_input() {
        let mut fields = PersonCriteriaFields {
            name: "ada".to_string(),
            birth_from: "1800".to_string(),
            birth_to: "1820".to_string(),
            death_to: "1900".to_string(),
            ..PersonCriteriaFields::default()
        };
        let criteria = fields.criteria().expect("criteria");
        assert_eq!(criteria.name, "ada");
        assert_eq!(criteria.birth.from, Some(1800));
        assert_eq!(criteria.birth.to, Some(1820));
        assert_eq!(criteria.death.from, None);
        assert_eq!(criteria.death.to, Some(1900));

        fields.birth_from = "about 1800".to_string();
        assert!(fields.criteria().is_none());
        assert!(fields.error.is_some());

        fields.birth_from = "1821".to_string();
        fields.error = None;
        assert!(fields.criteria().is_none());
        assert!(fields.error.is_some());
    }
}
