use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A person in the family tree.
///
/// `created_at` and `updated_at` are written by the database defaults on
/// insert; re-read the stored row to pick them up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: String,
    pub given_name: String,
    pub surname: String,
    /// One of `"M"`, `"F"`, `"U"` (enforced by a CHECK constraint).
    pub gender: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A marriage or partnership between two people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub id: String,
    pub husband_id: Option<String>,
    pub wife_id: Option<String>,
    pub created_at: String,
}

/// An event attached to a person or a family (birth, death, marriage, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event_type: String,
    pub date: Option<String>,
    pub place: Option<String>,
    pub description: Option<String>,
    pub person_id: Option<String>,
    pub family_id: Option<String>,
}

/// A source citation backing an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub id: String,
    pub source_title: String,
    pub page_or_reference: Option<String>,
    pub notes: Option<String>,
    pub event_id: Option<String>,
}

impl Person {
    pub fn new(
        given_name: impl Into<String>,
        surname: impl Into<String>,
        gender: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            given_name: given_name.into(),
            surname: surname.into(),
            gender: gender.into(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

impl Family {
    pub fn new(husband_id: Option<String>, wife_id: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            husband_id,
            wife_id,
            created_at: String::new(),
        }
    }
}

impl Event {
    pub fn new(
        event_type: impl Into<String>,
        person_id: Option<String>,
        family_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            date: None,
            place: None,
            description: None,
            person_id,
            family_id,
        }
    }
}

impl Citation {
    pub fn new(source_title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source_title: source_title.into(),
            page_or_reference: None,
            notes: None,
            event_id: None,
        }
    }
}
