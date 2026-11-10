//! Batch normalization of person names and place names.

use std::collections::BTreeMap;

use crate::db::Person;
use crate::db::names::AlternateName;
use crate::db::places::PlaceSummary;

/// The editable name pieces shared by primary and alternate names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameParts {
    pub prefix: String,
    pub given_name: String,
    pub middle_name: String,
    pub surname: String,
    pub suffix: String,
    pub nickname: String,
}

impl NameParts {
    pub fn from_person(person: &Person) -> Self {
        Self {
            prefix: person.name_prefix.clone(),
            given_name: person.given_name.clone(),
            middle_name: person.middle_name.clone(),
            surname: person.surname.clone(),
            suffix: person.name_suffix.clone(),
            nickname: person.nickname.clone(),
        }
    }

    pub fn from_alternate(name: &AlternateName) -> Self {
        Self {
            prefix: name.prefix.clone(),
            given_name: name.given_name.clone(),
            middle_name: name.middle_name.clone(),
            surname: name.surname.clone(),
            suffix: name.suffix.clone(),
            nickname: name.nickname.clone(),
        }
    }

    pub fn display_name(&self) -> String {
        crate::model::name::full_name(
            &self.prefix,
            &self.given_name,
            &self.middle_name,
            &self.surname,
            &self.suffix,
        )
    }

    pub fn is_empty(&self) -> bool {
        [
            &self.prefix,
            &self.given_name,
            &self.middle_name,
            &self.surname,
            &self.suffix,
            &self.nickname,
        ]
        .iter()
        .all(|value| value.trim().is_empty())
    }
}

/// One primary or alternate-name cleanup row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameCleanup {
    pub key: String,
    pub person_id: String,
    pub alternate_id: Option<String>,
    pub before: NameParts,
    pub after: NameParts,
}

/// One place rename or merge group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceCleanup {
    pub key: String,
    pub keep_id: String,
    pub merged_ids: Vec<String>,
    pub before_names: Vec<String>,
    pub after: String,
    pub event_count: i64,
}

/// The complete proposed cleanup, before the user selects rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataCleanPlan {
    pub names: Vec<NameCleanup>,
    pub places: Vec<PlaceCleanup>,
}

impl DataCleanPlan {
    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.places.is_empty()
    }
}

/// Builds a deterministic cleanup plan from loaded names and places.
pub fn build_plan(
    people: &[Person],
    alternate_names: &[AlternateName],
    places: &[PlaceSummary],
) -> DataCleanPlan {
    let mut names = Vec::new();
    for person in people {
        let before = NameParts::from_person(person);
        let after = normalize_parts(&before);
        if before != after {
            names.push(NameCleanup {
                key: format!("person:{}", person.id),
                person_id: person.id.clone(),
                alternate_id: None,
                before,
                after,
            });
        }
    }
    for alternate in alternate_names {
        let before = NameParts::from_alternate(alternate);
        let after = normalize_parts(&before);
        if before != after {
            names.push(NameCleanup {
                key: format!("alternate:{}", alternate.id),
                person_id: alternate.person_id.clone(),
                alternate_id: Some(alternate.id.clone()),
                before,
                after,
            });
        }
    }
    names.sort_by(|left, right| left.key.cmp(&right.key));

    let mut groups: BTreeMap<String, Vec<&PlaceSummary>> = BTreeMap::new();
    for place in places {
        groups
            .entry(normalize_place_name(&place.name).to_lowercase())
            .or_default()
            .push(place);
    }
    let mut place_cleanups = Vec::new();
    for (key, mut group) in groups {
        group.sort_by(|left, right| {
            right
                .event_count
                .cmp(&left.event_count)
                .then_with(|| left.id.cmp(&right.id))
        });
        let Some(keep) = group.first().copied() else {
            continue;
        };
        let after = normalize_place_name(&keep.name);
        let merged_ids: Vec<String> = group.iter().skip(1).map(|place| place.id.clone()).collect();
        if after == keep.name && merged_ids.is_empty() {
            continue;
        }
        let event_count = group.iter().map(|place| place.event_count).sum();
        let before_names = group.iter().map(|place| place.name.clone()).collect();
        place_cleanups.push(PlaceCleanup {
            key,
            keep_id: keep.id.clone(),
            merged_ids,
            before_names,
            after,
            event_count,
        });
    }
    place_cleanups.sort_by(|left, right| left.key.cmp(&right.key));

    DataCleanPlan {
        names,
        places: place_cleanups,
    }
}

/// Normalizes all name pieces.
pub fn normalize_parts(parts: &NameParts) -> NameParts {
    NameParts {
        prefix: normalize_name_text(&parts.prefix),
        given_name: normalize_name_text(&parts.given_name),
        middle_name: normalize_name_text(&parts.middle_name),
        surname: normalize_name_text(&parts.surname),
        suffix: normalize_name_text(&parts.suffix),
        nickname: normalize_name_text(&parts.nickname),
    }
}

/// Normalizes a person-name component without changing meaningful mixed case.
pub fn normalize_name_text(value: &str) -> String {
    let text = normalize_spacing(value);
    if text.is_empty() {
        return text;
    }
    let mut result = String::with_capacity(text.len());
    for (index, word) in text.split(' ').enumerate() {
        if index > 0 {
            result.push(' ');
        }
        result.push_str(&repair_word(word, index == 0));
    }
    result
}

/// Normalizes a place name, including comma-separated components.
pub fn normalize_place_name(value: &str) -> String {
    let parts: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(normalize_name_text)
        .filter(|part| !part.is_empty())
        .collect();
    parts.join(", ")
}

fn normalize_spacing(value: &str) -> String {
    let text = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return text;
    }
    text.replace(" ,", ",")
        .replace(" -", "-")
        .replace("- ", "-")
        .replace(" '", "'")
        .replace("' ", "'")
}

fn repair_word(word: &str, first_word: bool) -> String {
    let mut result = String::with_capacity(word.len());
    let mut segment = String::new();
    let mut first_segment = first_word;
    for character in word.chars() {
        if matches!(character, '-' | '\'' | '.') {
            result.push_str(&repair_segment(&segment, first_segment));
            result.push(character);
            segment.clear();
            first_segment = false;
        } else {
            segment.push(character);
        }
    }
    result.push_str(&repair_segment(&segment, first_segment));
    result
}

fn repair_segment(segment: &str, first: bool) -> String {
    if segment.is_empty() {
        return String::new();
    }
    let letters: Vec<char> = segment
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect();
    if letters.is_empty() {
        return segment.to_string();
    }
    let all_upper = letters.len() > 1 && letters.iter().all(|character| character.is_uppercase());
    let all_lower = letters.iter().all(|character| character.is_lowercase());
    let has_upper = letters.iter().any(|character| character.is_uppercase());
    let has_lower = letters.iter().any(|character| character.is_lowercase());
    if has_upper && has_lower {
        return segment.to_string();
    }
    let lower = segment.to_lowercase();
    if is_particle(&lower) {
        return if first { capitalize(&lower) } else { lower };
    }
    if all_upper && (letters.len() <= 2 || is_known_acronym(&lower)) {
        return segment.to_string();
    }
    if !all_upper && !all_lower {
        return segment.to_string();
    }
    capitalize(&lower)
}

fn capitalize(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut first_letter = true;
    for character in value.chars() {
        if first_letter && character.is_alphabetic() {
            result.extend(character.to_uppercase());
            first_letter = false;
        } else {
            result.push(character);
        }
    }
    result
}

fn is_known_acronym(value: &str) -> bool {
    matches!(value, "usa" | "uk" | "ussr" | "nyc" | "dc" | "gb")
}

fn is_particle(value: &str) -> bool {
    matches!(
        value,
        "van"
            | "von"
            | "de"
            | "der"
            | "den"
            | "da"
            | "di"
            | "du"
            | "la"
            | "le"
            | "del"
            | "dos"
            | "das"
            | "ter"
            | "ten"
            | "of"
            | "the"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::places::PlaceSummary;

    fn person(id: &str, given: &str, surname: &str) -> Person {
        let mut person = Person::new(given, surname, "U");
        person.id = id.to_string();
        person
    }

    fn place(id: &str, name: &str, event_count: i64) -> PlaceSummary {
        PlaceSummary {
            id: id.to_string(),
            name: name.to_string(),
            notes: None,
            latitude: None,
            longitude: None,
            event_count,
        }
    }

    #[test]
    fn normalizes_whitespace_and_conservative_case() {
        assert_eq!(normalize_name_text("  ADA   lynn  "), "Ada Lynn");
        assert_eq!(normalize_name_text("McDonald"), "McDonald");
        assert_eq!(normalize_name_text("McDONALD"), "McDONALD");
        assert_eq!(normalize_name_text("VAN DER BERG"), "Van der Berg");
        assert_eq!(normalize_name_text("van der berg"), "Van der Berg");
        assert_eq!(normalize_name_text("O'CONNOR"), "O'Connor");
        assert_eq!(normalize_name_text("SMITH - JONES"), "Smith-Jones");
        assert_eq!(normalize_name_text("USA"), "USA");
        assert_eq!(normalize_name_text("J."), "J.");
    }

    #[test]
    fn normalizes_place_commas_and_case() {
        assert_eq!(
            normalize_place_name("  london ,  ENGLAND "),
            "London, England"
        );
        assert_eq!(normalize_place_name("New York,, USA"), "New York, USA");
    }

    #[test]
    fn plan_includes_primary_and_alternate_names() {
        let mut person = person("p1", "ADA", "KING");
        person.nickname = "addy".to_string();
        let mut alternate = AlternateName {
            id: "a1".to_string(),
            person_id: "p1".to_string(),
            name_type: "aka".to_string(),
            prefix: String::new(),
            given_name: "AUGUSTA".to_string(),
            middle_name: String::new(),
            surname: "KING".to_string(),
            suffix: String::new(),
            nickname: String::new(),
            sort_order: 0,
        };
        let plan = build_plan(&[person.clone()], &[alternate.clone()], &[]);
        assert_eq!(plan.names.len(), 2);
        assert!(plan.names.iter().any(|name| name.key == "person:p1"));
        assert!(plan.names.iter().any(|name| name.key == "alternate:a1"));
        assert_eq!(plan.names[0].after.display_name(), "Augusta King");

        alternate.given_name = "Augusta".to_string();
        alternate.surname = "King".to_string();
        person.given_name = "Ada".to_string();
        person.surname = "King".to_string();
        person.nickname = "Addy".to_string();
        assert!(build_plan(&[person], &[alternate], &[]).is_empty());
    }

    #[test]
    fn place_plan_groups_case_and_space_variants() {
        let plan = build_plan(
            &[],
            &[],
            &[
                place("p2", "PARIS", 2),
                place("p1", "Paris ", 5),
                place("p3", "London", 1),
            ],
        );
        assert_eq!(plan.places.len(), 1);
        assert_eq!(plan.places[0].keep_id, "p1");
        assert_eq!(plan.places[0].merged_ids, ["p2"]);
        assert_eq!(plan.places[0].after, "Paris");
        assert_eq!(plan.places[0].event_count, 7);
    }
}
