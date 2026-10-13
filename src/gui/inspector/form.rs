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
        details.birth_place = blank_to_none(&self.birth_place);
        details.death_place = blank_to_none(&self.death_place);
        details.notes = self.notes.trim().to_string();
        details
    }
}

fn blank_to_none(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
