//! Multi-criteria person search over loaded tree data.

use crate::model::date;

/// Gender restriction for a person search.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GenderFilter {
    #[default]
    Any,
    Male,
    Female,
    Unknown,
}

impl GenderFilter {
    pub fn accepts(self, gender: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Male => gender == "M",
            Self::Female => gender == "F",
            Self::Unknown => gender == "U",
        }
    }
}

/// Inclusive optional year range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct YearRange {
    pub from: Option<i32>,
    pub to: Option<i32>,
}

impl YearRange {
    pub fn is_empty(self) -> bool {
        self.from.is_none() && self.to.is_none()
    }

    pub fn accepts(self, year: Option<i32>) -> bool {
        if self.is_empty() {
            return true;
        }
        let Some(year) = year else {
            return false;
        };
        self.from.is_none_or(|from| year >= from) && self.to.is_none_or(|to| year <= to)
    }
}

/// Criteria entered in the Advanced Search dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersonSearchCriteria {
    pub name: String,
    pub gender: GenderFilter,
    pub birth: YearRange,
    pub death: YearRange,
    pub place: String,
    pub fact_type: String,
    pub fact_text: String,
    pub notes: String,
}

impl PersonSearchCriteria {
    pub fn is_empty(&self) -> bool {
        self.name.trim().is_empty()
            && self.gender == GenderFilter::Any
            && self.birth.is_empty()
            && self.death.is_empty()
            && self.place.trim().is_empty()
            && self.fact_type.trim().is_empty()
            && self.fact_text.trim().is_empty()
            && self.notes.trim().is_empty()
    }
}

/// Searchable text and years for one person.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersonSearchDocument {
    pub names: String,
    pub alternate_names: String,
    pub gender: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub places: String,
    pub fact_types: String,
    pub fact_text: String,
    pub notes: String,
}

impl PersonSearchDocument {
    pub fn matches(&self, criteria: &PersonSearchCriteria) -> bool {
        if !criteria.name.trim().is_empty()
            && !contains_ci(
                &format!("{} {}", self.names, self.alternate_names),
                &criteria.name,
            )
        {
            return false;
        }
        if !criteria.gender.accepts(&self.gender) {
            return false;
        }
        if !criteria.birth.accepts(self.birth_year) {
            return false;
        }
        if !criteria.death.accepts(self.death_year) {
            return false;
        }
        if !criteria.place.trim().is_empty() && !contains_ci(&self.places, &criteria.place) {
            return false;
        }
        if !criteria.fact_type.trim().is_empty()
            && !contains_ci(&self.fact_types, &criteria.fact_type)
        {
            return false;
        }
        if !criteria.fact_text.trim().is_empty()
            && !contains_ci(&self.fact_text, &criteria.fact_text)
        {
            return false;
        }
        if !criteria.notes.trim().is_empty() && !contains_ci(&self.notes, &criteria.notes) {
            return false;
        }
        true
    }
}

/// Best-effort year extraction from a canonical or year-only GEDCOM date.
pub fn year_of(value: Option<&str>) -> Option<i32> {
    let value = value?;
    if let Some(parts) = date::exact_parts(value) {
        return Some(parts.year);
    }
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .find_map(|part| part.parse().ok())
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack
        .to_lowercase()
        .contains(&needle.trim().to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> PersonSearchDocument {
        PersonSearchDocument {
            names: "Ada Lynn King".to_string(),
            alternate_names: "Augusta King".to_string(),
            gender: "F".to_string(),
            birth_year: Some(1815),
            death_year: Some(1852),
            places: "London, England".to_string(),
            fact_types: "OCCUPATION BIRTH".to_string(),
            fact_text: "Teacher at Cambridge".to_string(),
            notes: "First line\nSecond line about the parish register".to_string(),
        }
    }

    #[test]
    fn empty_criteria_match_everything() {
        let criteria = PersonSearchCriteria::default();
        assert!(criteria.is_empty());
        assert!(document().matches(&criteria));
    }

    #[test]
    fn names_and_alternate_names_match_case_insensitively() {
        let mut criteria = PersonSearchCriteria {
            name: "aDa".to_string(),
            ..PersonSearchCriteria::default()
        };
        assert!(document().matches(&criteria));
        criteria.name = "augusta".to_string();
        assert!(document().matches(&criteria));
        criteria.name = "missing".to_string();
        assert!(!document().matches(&criteria));
    }

    #[test]
    fn gender_and_year_ranges_are_inclusive() {
        let mut criteria = PersonSearchCriteria {
            gender: GenderFilter::Female,
            ..PersonSearchCriteria::default()
        };
        assert!(document().matches(&criteria));
        criteria.gender = GenderFilter::Male;
        assert!(!document().matches(&criteria));

        criteria.gender = GenderFilter::Any;
        criteria.birth = YearRange {
            from: Some(1815),
            to: Some(1815),
        };
        assert!(document().matches(&criteria));
        criteria.birth.from = Some(1816);
        assert!(!document().matches(&criteria));

        let mut no_date = document();
        no_date.birth_year = None;
        criteria.birth = YearRange {
            from: Some(1800),
            to: Some(1900),
        };
        assert!(!no_date.matches(&criteria));
    }

    #[test]
    fn place_fact_and_note_criteria_match() {
        let mut criteria = PersonSearchCriteria {
            place: "england".to_string(),
            ..PersonSearchCriteria::default()
        };
        assert!(document().matches(&criteria));
        criteria.place.clear();
        criteria.fact_type = "occupation".to_string();
        assert!(document().matches(&criteria));
        criteria.fact_type.clear();
        criteria.fact_text = "cambridge".to_string();
        assert!(document().matches(&criteria));
        criteria.fact_text.clear();
        criteria.notes = "parish".to_string();
        assert!(document().matches(&criteria));
    }

    #[test]
    fn combined_criteria_use_and_semantics() {
        let mut criteria = PersonSearchCriteria {
            name: "ada".to_string(),
            birth: YearRange {
                from: Some(1800),
                to: Some(1820),
            },
            ..PersonSearchCriteria::default()
        };
        assert!(document().matches(&criteria));
        criteria.place = "paris".to_string();
        assert!(!document().matches(&criteria));
    }

    #[test]
    fn year_extraction_handles_exact_and_plain_years() {
        assert_eq!(year_of(Some("12 JAN 1815")), Some(1815));
        assert_eq!(year_of(Some("1815")), Some(1815));
        assert_eq!(year_of(Some("ABT 1815")), Some(1815));
        assert_eq!(year_of(Some("not a date")), None);
    }
}
