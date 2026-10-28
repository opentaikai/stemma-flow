//! Buffered profile form shared by the inspector panel and relation pop-up.

use std::collections::HashMap;

use crate::db::Person;
use crate::db::people::{PersonDetails, VitalDates};

/// Buffered profile fields, one snapshot per selected person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorForm {
    pub given_name: String,
    pub middle_name: String,
    pub surname: String,
    pub nickname: String,
    pub name_prefix: String,
    pub name_suffix: String,
    pub gender: String,
    pub birth: String,
    pub birth_place: String,
    pub death: String,
    pub death_place: String,
    pub notes: String,
}

impl Default for InspectorForm {
    fn default() -> Self {
        Self {
            given_name: String::new(),
            middle_name: String::new(),
            surname: String::new(),
            nickname: String::new(),
            name_prefix: String::new(),
            name_suffix: String::new(),
            gender: "U".to_string(),
            birth: String::new(),
            birth_place: String::new(),
            death: String::new(),
            death_place: String::new(),
            notes: String::new(),
        }
    }
}

impl InspectorForm {
    /// Buffers the stored person plus its vital dates and places.
    pub fn from_person(person: &Person, dates: &HashMap<String, VitalDates>) -> Self {
        let vitals = dates.get(&person.id);
        Self {
            given_name: person.given_name.clone(),
            middle_name: person.middle_name.clone(),
            surname: person.surname.clone(),
            nickname: person.nickname.clone(),
            name_prefix: person.name_prefix.clone(),
            name_suffix: person.name_suffix.clone(),
            gender: person.gender.clone(),
            birth: vitals
                .and_then(|vitals| vitals.birth.clone())
                .unwrap_or_default(),
            birth_place: vitals
                .and_then(|vitals| vitals.birth_place.clone())
                .unwrap_or_default(),
            death: vitals
                .and_then(|vitals| vitals.death.clone())
                .unwrap_or_default(),
            death_place: vitals
                .and_then(|vitals| vitals.death_place.clone())
                .unwrap_or_default(),
            notes: person.notes.clone(),
        }
    }

    /// Converts the buffer into write-ready details; blank fields clear.
    pub fn to_details(&self) -> PersonDetails {
        let mut details = PersonDetails::new(
            self.given_name.clone(),
            self.middle_name.clone(),
            self.surname.clone(),
            self.nickname.clone(),
            self.gender.clone(),
            blank_to_none(&self.birth),
            blank_to_none(&self.death),
        );
        details.name_prefix = self.name_prefix.trim().to_string();
        details.name_suffix = self.name_suffix.trim().to_string();
        details.birth_place = blank_to_none(&self.birth_place);
        details.death_place = blank_to_none(&self.death_place);
        details.notes = self.notes.trim().to_string();
        details
    }

    /// Validation messages for the optional birth and death dates.
    pub(super) fn date_errors(&self) -> DateErrors {
        DateErrors {
            birth: date_error(&self.birth),
            death: date_error(&self.death),
        }
    }

    /// Whether both optional vital dates are blank or valid.
    pub(super) fn dates_valid(&self) -> bool {
        self.date_errors().is_empty()
    }

    /// Whether a given name or surname is present.
    pub(super) fn has_name(&self) -> bool {
        !self.given_name.trim().is_empty() || !self.surname.trim().is_empty()
    }
}

/// Validation messages for the optional vital dates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct DateErrors {
    pub birth: Option<String>,
    pub death: Option<String>,
}

impl DateErrors {
    fn is_empty(&self) -> bool {
        self.birth.is_none() && self.death.is_none()
    }
}

fn date_error(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    crate::model::date::canonicalize(trimmed)
        .err()
        .map(|error| error.to_string())
}

fn blank_to_none(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
