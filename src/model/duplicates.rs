//! Scores people who may already exist before a new record is inserted.

use std::collections::HashMap;

use strsim::jaro_winkler;

use crate::db::people::{PersonDetails, VitalDates};
use crate::db::{FamilyTree, Person};
use crate::i18n::tr;

/// Lowest score that is still offered as a possible duplicate.
pub const MATCH_THRESHOLD: u8 = 55;

/// Maximum number of suggestions returned at once.
pub const MAX_MATCHES: usize = 5;

/// Fields used to look for an existing person.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DuplicateQuery {
    pub given_name: String,
    pub middle_name: String,
    pub surname: String,
    pub nickname: String,
    pub gender: String,
    pub birth_year: Option<i32>,
    pub birth_place: Option<String>,
    pub death_year: Option<i32>,
}

impl DuplicateQuery {
    /// Builds a query from buffered profile details.
    pub fn from_details(details: &PersonDetails) -> Self {
        Self {
            given_name: details.given_name.clone(),
            middle_name: details.middle_name.clone(),
            surname: details.surname.clone(),
            nickname: details.nickname.clone(),
            gender: details.gender.clone(),
            birth_year: year_of(details.birth_date.as_deref()),
            birth_place: details.birth_place.clone(),
            death_year: year_of(details.death_date.as_deref()),
        }
    }

    /// Builds a query from a loaded tree person and their vital dates.
    pub fn from_person(person: &Person, dates: Option<&VitalDates>) -> Self {
        Self {
            given_name: person.given_name.clone(),
            middle_name: person.middle_name.clone(),
            surname: person.surname.clone(),
            nickname: person.nickname.clone(),
            gender: person.gender.clone(),
            birth_year: dates.and_then(|dates| year_of(dates.birth.as_deref())),
            birth_place: dates.and_then(|dates| dates.birth_place.clone()),
            death_year: dates.and_then(|dates| year_of(dates.death.as_deref())),
        }
    }

    /// Whether there is enough of a name to compare.
    pub fn has_name(&self) -> bool {
        !normalize(&self.given_name).is_empty()
            || !normalize(&self.surname).is_empty()
            || !normalize(&self.nickname).is_empty()
    }
}

/// Why a person was suggested as a possible duplicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DuplicateReason {
    ExactName,
    SimilarName,
    SameBirthYear,
    CloseBirthYear,
    SameDeathYear,
    SameGender,
    SameBirthPlace,
    ConflictingBirthYear,
    ConflictingGender,
}

impl DuplicateReason {
    pub fn label(self) -> String {
        tr(match self {
            Self::ExactName => "duplicates.reason_exact_name",
            Self::SimilarName => "duplicates.reason_similar_name",
            Self::SameBirthYear => "duplicates.reason_same_birth_year",
            Self::CloseBirthYear => "duplicates.reason_close_birth_year",
            Self::SameDeathYear => "duplicates.reason_same_death_year",
            Self::SameGender => "duplicates.reason_same_gender",
            Self::SameBirthPlace => "duplicates.reason_same_birth_place",
            Self::ConflictingBirthYear => "duplicates.reason_conflicting_birth_year",
            Self::ConflictingGender => "duplicates.reason_conflicting_gender",
        })
    }
}

/// A scored person offered as a possible duplicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateMatch {
    pub person_id: String,
    pub score: u8,
    pub reasons: Vec<DuplicateReason>,
}

/// One possible duplicate pair found across the whole tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicatePair {
    pub first_id: String,
    pub second_id: String,
    pub score: u8,
    pub reasons: Vec<DuplicateReason>,
}

/// Finds possible duplicates in `tree`, skipping `exclude`.
pub fn find_duplicates(
    tree: &FamilyTree,
    dates: &HashMap<String, VitalDates>,
    query: &DuplicateQuery,
    exclude: &[&str],
) -> Vec<DuplicateMatch> {
    let candidates = tree
        .people
        .iter()
        .filter(|person| !exclude.contains(&person.id.as_str()));
    score_candidates(query, dates, candidates)
}

/// Finds every possible duplicate pair in the tree.
///
/// People are blocked by surname Soundex (or given-name Soundex when the
/// surname is empty), so only likely candidates are compared. Each pair is
/// scored in both directions and returned once, best scores first.
pub fn find_all_duplicates(
    tree: &FamilyTree,
    dates: &HashMap<String, VitalDates>,
) -> Vec<DuplicatePair> {
    let mut blocks: HashMap<String, Vec<usize>> = HashMap::new();
    let mut people: Vec<(&Person, DuplicateQuery)> = Vec::new();
    for person in &tree.people {
        let query = DuplicateQuery::from_person(person, dates.get(&person.id));
        if !query.has_name() {
            continue;
        }
        blocks
            .entry(duplicate_block(person))
            .or_default()
            .push(people.len());
        people.push((person, query));
    }
    let mut pairs = Vec::new();
    for indices in blocks.values() {
        for (position, &left) in indices.iter().enumerate() {
            for &right in &indices[position + 1..] {
                let (left_person, left_query) = &people[left];
                let (right_person, right_query) = &people[right];
                let forward = score_person(left_query, dates.get(&right_person.id), right_person);
                let backward = score_person(right_query, dates.get(&left_person.id), left_person);
                let best = match (forward, backward) {
                    (Some(forward), Some(backward)) => {
                        if forward.score >= backward.score {
                            forward
                        } else {
                            backward
                        }
                    }
                    (Some(forward), None) => forward,
                    (None, Some(backward)) => backward,
                    (None, None) => continue,
                };
                let (first_id, second_id) = if left_person.id <= right_person.id {
                    (left_person.id.clone(), right_person.id.clone())
                } else {
                    (right_person.id.clone(), left_person.id.clone())
                };
                pairs.push(DuplicatePair {
                    first_id,
                    second_id,
                    score: best.score,
                    reasons: best.reasons,
                });
            }
        }
    }
    pairs.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.first_id.cmp(&right.first_id))
            .then_with(|| left.second_id.cmp(&right.second_id))
    });
    pairs
}

fn duplicate_block(person: &Person) -> String {
    let surname = crate::model::phonetics::american_soundex(&person.surname);
    if !person.surname.trim().is_empty() && !surname.is_empty() {
        format!("surname:{surname}")
    } else {
        format!(
            "given:{}",
            crate::model::phonetics::american_soundex(&person.given_name)
        )
    }
}

/// Scores an arbitrary set of candidates, best matches first.
pub fn score_candidates<'a>(
    query: &DuplicateQuery,
    dates: &HashMap<String, VitalDates>,
    candidates: impl IntoIterator<Item = &'a Person>,
) -> Vec<DuplicateMatch> {
    if !query.has_name() {
        return Vec::new();
    }
    let mut matches: Vec<DuplicateMatch> = candidates
        .into_iter()
        .filter_map(|person| score_person(query, dates.get(&person.id), person))
        .collect();
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.person_id.cmp(&right.person_id))
    });
    matches.truncate(MAX_MATCHES);
    matches
}

fn score_person(
    query: &DuplicateQuery,
    dates: Option<&VitalDates>,
    person: &Person,
) -> Option<DuplicateMatch> {
    let given = best_similarity(
        [&query.given_name, &query.nickname],
        [&person.given_name, &person.nickname],
    );
    let surname = similarity(&query.surname, &person.surname);
    let name = match (given, surname) {
        (Some(given), Some(surname)) => given * 0.55 + surname * 0.45,
        (Some(given), None) => given,
        (None, Some(surname)) => surname,
        (None, None) => return None,
    };

    let mut score = (name * 70.0).round() as i32;
    let mut reasons = Vec::new();
    if exact_name(query, person) {
        score = score.max(85);
        reasons.push(DuplicateReason::ExactName);
    } else if name >= 0.55 {
        reasons.push(DuplicateReason::SimilarName);
    }

    let birth_year = dates
        .and_then(|dates| dates.birth.as_deref())
        .and_then(|value| year_of(Some(value)));
    if let (Some(query_year), Some(person_year)) = (query.birth_year, birth_year) {
        match query_year.abs_diff(person_year) {
            0 => {
                score += 10;
                reasons.push(DuplicateReason::SameBirthYear);
            }
            1 => {
                score += 5;
                reasons.push(DuplicateReason::CloseBirthYear);
            }
            _ => {
                score -= 12;
                reasons.push(DuplicateReason::ConflictingBirthYear);
            }
        }
    }

    let death_year = dates
        .and_then(|dates| dates.death.as_deref())
        .and_then(|value| year_of(Some(value)));
    if query.death_year.is_some() && query.death_year == death_year {
        score += 8;
        reasons.push(DuplicateReason::SameDeathYear);
    }

    if matches!(query.gender.as_str(), "M" | "F") && matches!(person.gender.as_str(), "M" | "F") {
        if query.gender == person.gender {
            score += 4;
            reasons.push(DuplicateReason::SameGender);
        } else {
            score -= 15;
            reasons.push(DuplicateReason::ConflictingGender);
        }
    }

    let query_place = query
        .birth_place
        .as_deref()
        .map(normalize)
        .unwrap_or_default();
    let person_place = dates
        .and_then(|dates| dates.birth_place.as_deref())
        .map(normalize)
        .unwrap_or_default();
    if !query_place.is_empty() && !person_place.is_empty() {
        let place = jaro_winkler(&query_place, &person_place);
        if place >= 0.85 {
            score += 4;
            reasons.push(DuplicateReason::SameBirthPlace);
        }
    }

    let score = score.clamp(0, 100) as u8;
    if score < MATCH_THRESHOLD {
        return None;
    }
    reasons.sort();
    reasons.dedup();
    Some(DuplicateMatch {
        person_id: person.id.clone(),
        score,
        reasons,
    })
}

fn exact_name(query: &DuplicateQuery, person: &Person) -> bool {
    let query_name = normalize_full(&query.given_name, &query.surname);
    let person_name = normalize_full(&person.given_name, &person.surname);
    !query_name.is_empty() && query_name == person_name
}

fn normalize_full(given: &str, surname: &str) -> String {
    [given, surname]
        .into_iter()
        .map(normalize)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn best_similarity<const N: usize>(left: [&str; N], right: [&str; N]) -> Option<f64> {
    let mut best: Option<f64> = None;
    for left in left {
        for right in right {
            if let Some(value) = similarity(left, right) {
                best = Some(best.map_or(value, |known| known.max(value)));
            }
        }
    }
    best
}

fn similarity(left: &str, right: &str) -> Option<f64> {
    let left = normalize(left);
    let right = normalize(right);
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some(jaro_winkler(&left, &right))
}

/// Lowercases, folds common diacritics and drops punctuation.
fn normalize(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut last_space = false;
    for character in value.trim().chars().flat_map(fold_char) {
        if character.is_whitespace() || character == '-' || character == ',' || character == '.' {
            if !normalized.is_empty() && !last_space {
                normalized.push(' ');
                last_space = true;
            }
            continue;
        }
        if character.is_alphanumeric() {
            normalized.extend(character.to_lowercase());
            last_space = false;
        }
    }
    normalized.trim().to_string()
}

fn fold_char(character: char) -> Vec<char> {
    let text = match character {
        'ä' | 'Ä' => "ae",
        'ö' | 'Ö' => "oe",
        'ü' | 'Ü' => "ue",
        'ß' => "ss",
        'é' | 'É' | 'è' | 'È' | 'ê' | 'Ê' => "e",
        'á' | 'Á' | 'à' | 'À' | 'â' | 'Â' => "a",
        'í' | 'Í' | 'ì' | 'Ì' | 'î' | 'Î' => "i",
        'ó' | 'Ó' | 'ò' | 'Ò' | 'ô' | 'Ô' => "o",
        'ú' | 'Ú' | 'ù' | 'Ù' | 'û' | 'Û' => "u",
        'ñ' | 'Ñ' => "n",
        'ç' | 'Ç' => "c",
        _ => return vec![character],
    };
    text.chars().collect()
}

fn year_of(value: Option<&str>) -> Option<i32> {
    let value = value?;
    if let Some(parts) = crate::model::date::exact_parts(value) {
        return Some(parts.year);
    }
    value
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| part.len() == 4)
        .find_map(|part| part.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Family;

    fn person(id: &str, given: &str, surname: &str, gender: &str) -> Person {
        let mut person = Person::new(given, surname, gender);
        person.id = id.to_string();
        person
    }

    fn query(given: &str, surname: &str) -> DuplicateQuery {
        DuplicateQuery {
            given_name: given.to_string(),
            surname: surname.to_string(),
            ..DuplicateQuery::default()
        }
    }

    fn family_tree(people: Vec<Person>) -> FamilyTree {
        FamilyTree {
            people,
            families: Vec::<Family>::new(),
            child_links: Vec::new(),
        }
    }

    fn match_ids(matches: &[DuplicateMatch]) -> Vec<&str> {
        matches
            .iter()
            .map(|matched| matched.person_id.as_str())
            .collect()
    }

    #[test]
    fn exact_names_score_highest() {
        let tree = family_tree(vec![
            person("exact", "Ada", "King", "F"),
            person("other", "Grace", "Hopper", "F"),
        ]);
        let matches = find_duplicates(&tree, &HashMap::new(), &query("ada", "king"), &[]);
        assert_eq!(match_ids(&matches), vec!["exact"]);
        assert_eq!(matches[0].score, 85);
        assert!(matches[0].reasons.contains(&DuplicateReason::ExactName));
    }

    #[test]
    fn spelling_variants_and_diacritics_are_matched() {
        let tree = family_tree(vec![
            person("mueller", "Jürgen", "Müller", "M"),
            person("john", "John", "Smith", "M"),
        ]);
        let folded = find_duplicates(&tree, &HashMap::new(), &query("Juergen", "Mueller"), &[]);
        assert_eq!(match_ids(&folded), vec!["mueller"]);

        let typo = find_duplicates(&tree, &HashMap::new(), &query("Jon", "Smith"), &[]);
        assert_eq!(match_ids(&typo), vec!["john"]);
    }

    #[test]
    fn unrelated_names_are_not_suggested() {
        let tree = family_tree(vec![person("other", "Grace", "Hopper", "F")]);
        let matches = find_duplicates(&tree, &HashMap::new(), &query("Ada", "King"), &[]);
        assert!(matches.is_empty());
    }

    #[test]
    fn vitals_and_gender_adjust_the_score() {
        let tree = family_tree(vec![
            person("matching", "Ada", "King", "F"),
            person("conflict", "Ada", "King", "M"),
        ]);
        let mut dates = HashMap::new();
        dates.insert(
            "matching".to_string(),
            VitalDates {
                birth: Some("10 DEC 1815".to_string()),
                birth_place: Some("London".to_string()),
                ..VitalDates::default()
            },
        );
        dates.insert(
            "conflict".to_string(),
            VitalDates {
                birth: Some("1900".to_string()),
                ..VitalDates::default()
            },
        );
        let mut details = query("Ada", "King");
        details.gender = "F".to_string();
        details.birth_year = Some(1815);
        details.birth_place = Some("London".to_string());

        let matches = find_duplicates(&tree, &dates, &details, &[]);
        assert_eq!(match_ids(&matches), vec!["matching", "conflict"]);
        assert!(matches[0].score > matches[1].score);
        assert!(matches[0].reasons.contains(&DuplicateReason::SameBirthYear));
        assert!(
            matches[1]
                .reasons
                .contains(&DuplicateReason::ConflictingBirthYear)
        );
        assert!(
            matches[1]
                .reasons
                .contains(&DuplicateReason::ConflictingGender)
        );
    }

    #[test]
    fn exclusions_blank_queries_and_unnamed_people_are_ignored() {
        let tree = family_tree(vec![
            person("excluded", "Ada", "King", "F"),
            person("unnamed", "", "", "U"),
        ]);
        assert!(
            find_duplicates(&tree, &HashMap::new(), &query("Ada", "King"), &["excluded"])
                .is_empty()
        );
        assert!(find_duplicates(&tree, &HashMap::new(), &query("", ""), &[]).is_empty());
    }

    fn vitals(birth: &str, death: Option<&str>) -> VitalDates {
        VitalDates {
            birth: Some(birth.to_string()),
            death: death.map(str::to_string),
            ..VitalDates::default()
        }
    }

    #[test]
    fn finds_duplicate_pairs_once_with_vitals_and_reasons() {
        let tree = family_tree(vec![
            person("a", "Ada", "King", "F"),
            person("b", "Ada", "King", "F"),
            person("c", "Bo", "Tester", "M"),
        ]);
        let mut dates = HashMap::new();
        dates.insert("a".to_string(), vitals("1815", Some("1852")));
        dates.insert("b".to_string(), vitals("1815", Some("1852")));
        let pairs = find_all_duplicates(&tree, &dates);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].first_id, "a");
        assert_eq!(pairs[0].second_id, "b");
        assert!(pairs[0].score >= 85);
        assert!(pairs[0].reasons.contains(&DuplicateReason::ExactName));
    }

    #[test]
    fn soundex_blocks_match_spelling_variants() {
        let tree = family_tree(vec![
            person("smith", "John", "Smith", "M"),
            person("smyth", "John", "Smyth", "M"),
            person("other", "Ada", "King", "F"),
        ]);
        let pairs = find_all_duplicates(&tree, &HashMap::new());
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].first_id, "smith");
        assert_eq!(pairs[0].second_id, "smyth");
        assert!(pairs[0].reasons.contains(&DuplicateReason::SimilarName));
    }

    #[test]
    fn conflicting_vitals_stay_in_the_pair_with_reasons() {
        let mut tree = family_tree(vec![
            person("one", "Ada", "King", "F"),
            person("two", "Ada", "King", "M"),
        ]);
        tree.people[1].gender = "M".to_string();
        let mut dates = HashMap::new();
        dates.insert("one".to_string(), vitals("1815", None));
        dates.insert("two".to_string(), vitals("1820", None));
        let pairs = find_all_duplicates(&tree, &dates);
        assert_eq!(pairs.len(), 1);
        assert!(
            pairs[0]
                .reasons
                .contains(&DuplicateReason::ConflictingBirthYear)
        );
        assert!(
            pairs[0]
                .reasons
                .contains(&DuplicateReason::ConflictingGender)
        );
    }

    #[test]
    fn queries_from_people_copy_names_and_vitals() {
        let mut person = person("p1", "Ada", "King", "F");
        person.nickname = "Addie".to_string();
        let query = DuplicateQuery::from_person(&person, Some(&vitals("12 JAN 1815", None)));
        assert_eq!(query.given_name, "Ada");
        assert_eq!(query.surname, "King");
        assert_eq!(query.nickname, "Addie");
        assert_eq!(query.birth_year, Some(1815));
        assert_eq!(query.gender, "F");
    }

    #[test]
    fn all_duplicates_skip_unnamed_people() {
        let tree = family_tree(vec![person("one", "", "", "U"), person("two", "", "", "U")]);
        assert!(find_all_duplicates(&tree, &HashMap::new()).is_empty());
    }

    #[test]
    fn results_are_sorted_limited_and_include_nicknames() {
        let people = (0..7)
            .map(|index| person(&format!("p{index}"), "Ada", "King", "F"))
            .collect();
        let tree = family_tree(people);
        let matches = find_duplicates(&tree, &HashMap::new(), &query("Ada", "King"), &[]);
        assert_eq!(matches.len(), MAX_MATCHES);
        assert!(
            matches
                .windows(2)
                .all(|pair| pair[0].score >= pair[1].score)
        );

        let mut nicknamed = person("nick", "Augusta", "King", "F");
        nicknamed.nickname = "Ada".to_string();
        let tree = family_tree(vec![nicknamed]);
        let matches = find_duplicates(&tree, &HashMap::new(), &query("Ada", ""), &[]);
        assert_eq!(match_ids(&matches), vec!["nick"]);
    }
}
