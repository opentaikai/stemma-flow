//! Detect missing or contradictory genealogical data.

use std::collections::{HashMap, HashSet};

use crate::db::people::VitalDates;
use crate::db::{FamilyTree, MarriageInfo};
use crate::model::age::{self, Age};
use crate::model::date::{self, DateParts};

/// How serious a problem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProblemSeverity {
    Error,
    Warning,
}

impl ProblemSeverity {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Error => "problems.severity_error",
            Self::Warning => "problems.severity_warning",
        }
    }
}

/// One type of data problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProblemKind {
    Unnamed,
    MissingBirthDate,
    DeathBeforeBirth,
    ParentBornAfterChild,
    ParentDiedBeforeChildBirth,
    ParentTooYoung,
    ParentTooOld,
    MarriageBeforeBirth,
    MarriageAfterDeath,
    MultipleBirthEvents,
    MultipleDeathEvents,
}

impl ProblemKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Unnamed => "unnamed",
            Self::MissingBirthDate => "missing_birth_date",
            Self::DeathBeforeBirth => "death_before_birth",
            Self::ParentBornAfterChild => "parent_born_after_child",
            Self::ParentDiedBeforeChildBirth => "parent_died_before_child_birth",
            Self::ParentTooYoung => "parent_too_young",
            Self::ParentTooOld => "parent_too_old",
            Self::MarriageBeforeBirth => "marriage_before_birth",
            Self::MarriageAfterDeath => "marriage_after_death",
            Self::MultipleBirthEvents => "multiple_birth_events",
            Self::MultipleDeathEvents => "multiple_death_events",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Unnamed => "problems.kind_unnamed",
            Self::MissingBirthDate => "problems.kind_missing_birth_date",
            Self::DeathBeforeBirth => "problems.kind_death_before_birth",
            Self::ParentBornAfterChild => "problems.kind_parent_born_after_child",
            Self::ParentDiedBeforeChildBirth => "problems.kind_parent_died_before_child_birth",
            Self::ParentTooYoung => "problems.kind_parent_too_young",
            Self::ParentTooOld => "problems.kind_parent_too_old",
            Self::MarriageBeforeBirth => "problems.kind_marriage_before_birth",
            Self::MarriageAfterDeath => "problems.kind_marriage_after_death",
            Self::MultipleBirthEvents => "problems.kind_multiple_birth_events",
            Self::MultipleDeathEvents => "problems.kind_multiple_death_events",
        }
    }
}

/// One detected problem. The subject is always a person so selecting an alert
/// can focus the inspector consistently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemAlert {
    pub key: String,
    pub kind: ProblemKind,
    pub severity: ProblemSeverity,
    pub person_id: String,
    pub related_person_id: Option<String>,
    /// Relevant raw values in display order, such as dates or an event count.
    pub details: Vec<String>,
}

/// The minimum plausible age at which someone can become a parent.
const MIN_PARENT_AGE: u32 = 12;
/// The maximum plausible maternal age.
const MAX_MOTHER_AGE: u32 = 55;
/// The maximum plausible paternal age.
const MAX_FATHER_AGE: u32 = 90;

/// Detects focused missing-data and contradiction problems.
pub fn detect(
    tree: &FamilyTree,
    dates: &HashMap<String, VitalDates>,
    marriages: &HashMap<String, MarriageInfo>,
) -> Vec<ProblemAlert> {
    let mut alerts = Vec::new();
    let mut seen = HashSet::new();
    let people: HashMap<&str, _> = tree
        .people
        .iter()
        .map(|person| (person.id.as_str(), person))
        .collect();

    for person in &tree.people {
        let vitals = dates.get(&person.id);
        let unnamed = person.given_name.trim().is_empty()
            && person.middle_name.trim().is_empty()
            && person.surname.trim().is_empty();
        if unnamed {
            add(
                &mut alerts,
                &mut seen,
                alert(
                    ProblemKind::Unnamed,
                    ProblemSeverity::Error,
                    &person.id,
                    None,
                    Vec::new(),
                ),
            );
        } else if vitals.and_then(|v| v.birth.as_deref()).is_none() {
            add(
                &mut alerts,
                &mut seen,
                alert(
                    ProblemKind::MissingBirthDate,
                    ProblemSeverity::Warning,
                    &person.id,
                    None,
                    Vec::new(),
                ),
            );
        }

        let Some(vitals) = vitals else {
            continue;
        };
        let birth = parts(vitals.birth.as_deref());
        let death = parts(vitals.death.as_deref());
        if let (Some(birth), Some(death)) = (birth, death)
            && before(death, birth)
        {
            add(
                &mut alerts,
                &mut seen,
                alert(
                    ProblemKind::DeathBeforeBirth,
                    ProblemSeverity::Error,
                    &person.id,
                    None,
                    vec![
                        vitals.birth.clone().unwrap_or_default(),
                        vitals.death.clone().unwrap_or_default(),
                    ],
                ),
            );
        }
        if vitals.birth_events > 1 {
            add(
                &mut alerts,
                &mut seen,
                alert(
                    ProblemKind::MultipleBirthEvents,
                    ProblemSeverity::Warning,
                    &person.id,
                    None,
                    vec![vitals.birth_events.to_string()],
                ),
            );
        }
        if vitals.death_events > 1 {
            add(
                &mut alerts,
                &mut seen,
                alert(
                    ProblemKind::MultipleDeathEvents,
                    ProblemSeverity::Warning,
                    &person.id,
                    None,
                    vec![vitals.death_events.to_string()],
                ),
            );
        }
    }

    for family in &tree.families {
        let family_dates = marriages.get(&family.id);
        for (role, parent_id) in [
            ("father", family.husband_id.as_deref()),
            ("mother", family.wife_id.as_deref()),
        ] {
            let Some(parent_id) = parent_id else {
                continue;
            };
            if !people.contains_key(parent_id) {
                continue;
            }
            let parent_vitals = dates.get(parent_id);
            let parent_birth = parent_vitals.and_then(|v| parts(v.birth.as_deref()));
            let parent_death = parent_vitals.and_then(|v| parts(v.death.as_deref()));

            if let (Some(marriage), Some(birth)) = (
                family_dates.and_then(|m| parts(m.date.as_deref())),
                parent_birth,
            ) && before(marriage, birth)
            {
                add(
                    &mut alerts,
                    &mut seen,
                    alert(
                        ProblemKind::MarriageBeforeBirth,
                        ProblemSeverity::Warning,
                        parent_id,
                        other_spouse(family, parent_id),
                        vec![
                            family_dates
                                .and_then(|m| m.date.clone())
                                .unwrap_or_default(),
                            parent_vitals
                                .and_then(|v| v.birth.clone())
                                .unwrap_or_default(),
                        ],
                    ),
                );
            }
            if let (Some(marriage), Some(death)) = (
                family_dates.and_then(|m| parts(m.date.as_deref())),
                parent_death,
            ) && before(death, marriage)
            {
                add(
                    &mut alerts,
                    &mut seen,
                    alert(
                        ProblemKind::MarriageAfterDeath,
                        ProblemSeverity::Warning,
                        parent_id,
                        other_spouse(family, parent_id),
                        vec![
                            family_dates
                                .and_then(|m| m.date.clone())
                                .unwrap_or_default(),
                            parent_vitals
                                .and_then(|v| v.death.clone())
                                .unwrap_or_default(),
                        ],
                    ),
                );
            }

            for (family_id, child_id) in &tree.child_links {
                if family_id != &family.id {
                    continue;
                }
                let Some(child) = people.get(child_id.as_str()) else {
                    continue;
                };
                let child_vitals = dates.get(child_id);
                let child_birth = child_vitals.and_then(|v| parts(v.birth.as_deref()));
                let Some(child_birth) = child_birth else {
                    continue;
                };
                if let Some(parent_birth) = parent_birth {
                    if before(child_birth, parent_birth) {
                        add(
                            &mut alerts,
                            &mut seen,
                            alert(
                                ProblemKind::ParentBornAfterChild,
                                ProblemSeverity::Error,
                                &child.id,
                                Some(parent_id.to_string()),
                                vec![
                                    parent_vitals
                                        .and_then(|v| v.birth.clone())
                                        .unwrap_or_default(),
                                    child_vitals
                                        .and_then(|v| v.birth.clone())
                                        .unwrap_or_default(),
                                ],
                            ),
                        );
                    } else if let Some((low, high)) = age_at_birth(
                        parent_vitals.and_then(|v| v.birth.as_deref()),
                        child_vitals.and_then(|v| v.birth.as_deref()),
                    ) {
                        if high < MIN_PARENT_AGE {
                            add(
                                &mut alerts,
                                &mut seen,
                                alert(
                                    ProblemKind::ParentTooYoung,
                                    ProblemSeverity::Warning,
                                    &child.id,
                                    Some(parent_id.to_string()),
                                    vec![
                                        parent_birth.year.to_string(),
                                        child_birth.year.to_string(),
                                        high.to_string(),
                                    ],
                                ),
                            );
                        }
                        let maximum = if role == "mother" {
                            MAX_MOTHER_AGE
                        } else {
                            MAX_FATHER_AGE
                        };
                        if low > maximum {
                            add(
                                &mut alerts,
                                &mut seen,
                                alert(
                                    ProblemKind::ParentTooOld,
                                    ProblemSeverity::Warning,
                                    &child.id,
                                    Some(parent_id.to_string()),
                                    vec![
                                        parent_birth.year.to_string(),
                                        child_birth.year.to_string(),
                                        low.to_string(),
                                    ],
                                ),
                            );
                        }
                    }
                }
                if let Some(parent_death) = parent_death
                    && before(parent_death, child_birth)
                {
                    add(
                        &mut alerts,
                        &mut seen,
                        alert(
                            ProblemKind::ParentDiedBeforeChildBirth,
                            ProblemSeverity::Error,
                            &child.id,
                            Some(parent_id.to_string()),
                            vec![
                                parent_vitals
                                    .and_then(|v| v.death.clone())
                                    .unwrap_or_default(),
                                child_vitals
                                    .and_then(|v| v.birth.clone())
                                    .unwrap_or_default(),
                            ],
                        ),
                    );
                }
            }
        }
    }

    alerts.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.person_id.cmp(&right.person_id))
            .then_with(|| left.key.cmp(&right.key))
    });
    alerts
}

fn alert(
    kind: ProblemKind,
    severity: ProblemSeverity,
    person_id: &str,
    related_person_id: Option<String>,
    details: Vec<String>,
) -> ProblemAlert {
    let key = format!(
        "{}|{}|{}|{}",
        kind.key(),
        person_id,
        related_person_id.as_deref().unwrap_or(""),
        details.join("|")
    );
    ProblemAlert {
        key,
        kind,
        severity,
        person_id: person_id.to_string(),
        related_person_id,
        details,
    }
}

fn add(alerts: &mut Vec<ProblemAlert>, seen: &mut HashSet<String>, alert: ProblemAlert) {
    if seen.insert(alert.key.clone()) {
        alerts.push(alert);
    }
}

fn other_spouse(family: &crate::db::Family, person_id: &str) -> Option<String> {
    if family.husband_id.as_deref() == Some(person_id) {
        family.wife_id.clone()
    } else {
        family.husband_id.clone()
    }
}

fn parts(value: Option<&str>) -> Option<DateParts> {
    value.and_then(date::exact_parts)
}

fn before(left: DateParts, right: DateParts) -> bool {
    if left.year != right.year {
        return left.year < right.year;
    }
    match (left.month, left.day, right.month, right.day) {
        (Some(left_month), Some(left_day), Some(right_month), Some(right_day)) => {
            (left_month, left_day) < (right_month, right_day)
        }
        _ => false,
    }
}

fn age_at_birth(parent_birth: Option<&str>, child_birth: Option<&str>) -> Option<(u32, u32)> {
    let age = age::age_at(parent_birth, child_birth)?;
    Some(match age {
        Age::Exact(years) | Age::Approximate(years) => (years, years),
        Age::AtMost(years) => (0, years),
        Age::AtLeast(years) => (years, u32::MAX),
        Age::Range(low, high) => (low, high),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Family, FamilyTree, Person};

    fn person(id: &str, given: &str, surname: &str) -> Person {
        let mut person = Person::new(given, surname, "U");
        person.id = id.to_string();
        person
    }

    fn family(id: &str, husband: Option<&str>, wife: Option<&str>) -> Family {
        Family {
            id: id.to_string(),
            husband_id: husband.map(str::to_string),
            wife_id: wife.map(str::to_string),
            created_at: String::new(),
        }
    }

    fn dates(birth: Option<&str>, death: Option<&str>) -> VitalDates {
        VitalDates {
            birth: birth.map(str::to_string),
            death: death.map(str::to_string),
            birth_events: usize::from(birth.is_some()),
            death_events: usize::from(death.is_some()),
            ..VitalDates::default()
        }
    }

    fn tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("parent", "Ada", "King"),
                person("child", "Elsa", "King"),
            ],
            families: vec![family("f1", Some("parent"), None)],
            child_links: vec![("f1".to_string(), "child".to_string())],
        }
    }

    fn kinds(alerts: &[ProblemAlert]) -> Vec<ProblemKind> {
        alerts.iter().map(|alert| alert.kind).collect()
    }

    #[test]
    fn missing_and_contradictory_person_data_is_detected() {
        let mut tree = tree();
        tree.people.push(person("unnamed", "", ""));
        let mut data = HashMap::new();
        data.insert("parent".to_string(), dates(Some("1900"), Some("1890")));
        data.insert("child".to_string(), dates(None, None));
        let alerts = detect(&tree, &data, &HashMap::new());
        assert!(kinds(&alerts).contains(&ProblemKind::DeathBeforeBirth));
        assert!(kinds(&alerts).contains(&ProblemKind::MissingBirthDate));
        assert!(kinds(&alerts).contains(&ProblemKind::Unnamed));
        assert!(
            alerts
                .windows(2)
                .all(|pair| pair[0].severity <= pair[1].severity)
        );
    }

    #[test]
    fn parent_chronology_and_age_are_detected() {
        let tree = tree();
        let mut data = HashMap::new();
        data.insert("parent".to_string(), dates(Some("1900"), Some("1950")));
        data.insert("child".to_string(), dates(Some("1890"), None));
        let alerts = detect(&tree, &data, &HashMap::new());
        assert!(kinds(&alerts).contains(&ProblemKind::ParentBornAfterChild));

        let mut died = HashMap::new();
        died.insert("parent".to_string(), dates(Some("1850"), Some("1880")));
        died.insert("child".to_string(), dates(Some("1890"), None));
        let alerts = detect(&tree, &died, &HashMap::new());
        assert!(kinds(&alerts).contains(&ProblemKind::ParentDiedBeforeChildBirth));

        let mut data = HashMap::new();
        data.insert("parent".to_string(), dates(Some("1900"), None));
        data.insert("child".to_string(), dates(Some("1905"), None));
        let alerts = detect(&tree, &data, &HashMap::new());
        assert!(kinds(&alerts).contains(&ProblemKind::ParentTooYoung));
    }

    #[test]
    fn marriage_dates_are_checked_against_both_spouses() {
        let tree = FamilyTree {
            people: vec![person("a", "Ada", "King"), person("b", "Ben", "King")],
            families: vec![family("f1", Some("a"), Some("b"))],
            child_links: Vec::new(),
        };
        let mut data = HashMap::new();
        data.insert("a".to_string(), dates(Some("1900"), None));
        data.insert("b".to_string(), dates(Some("1900"), None));
        let mut marriages = HashMap::new();
        marriages.insert(
            "f1".to_string(),
            MarriageInfo {
                date: Some("1880".to_string()),
                ..MarriageInfo::default()
            },
        );
        let alerts = detect(&tree, &data, &marriages);
        assert_eq!(
            kinds(&alerts)
                .iter()
                .filter(|kind| **kind == ProblemKind::MarriageBeforeBirth)
                .count(),
            2
        );
    }

    #[test]
    fn multiple_vital_events_and_partial_dates_are_handled() {
        let tree = FamilyTree {
            people: vec![person("a", "Ada", "King")],
            families: Vec::new(),
            child_links: Vec::new(),
        };
        let mut data = HashMap::new();
        let mut vitals = dates(Some("1900"), Some("1950"));
        vitals.birth_events = 2;
        vitals.death_events = 2;
        data.insert("a".to_string(), vitals);
        let alerts = detect(&tree, &data, &HashMap::new());
        assert!(kinds(&alerts).contains(&ProblemKind::MultipleBirthEvents));
        assert!(kinds(&alerts).contains(&ProblemKind::MultipleDeathEvents));

        let mut data = HashMap::new();
        data.insert("a".to_string(), dates(Some("1900"), Some("1890")));
        // Partial/ambiguous dates must not create a false death-before-birth alert.
        data.get_mut("a").unwrap().birth = Some("1900".to_string());
        data.get_mut("a").unwrap().death = Some("1890".to_string());
        let alerts = detect(&tree, &data, &HashMap::new());
        assert!(kinds(&alerts).contains(&ProblemKind::DeathBeforeBirth));
    }

    #[test]
    fn keys_change_when_the_underlying_value_changes() {
        let tree = FamilyTree {
            people: vec![person("a", "Ada", "King")],
            families: Vec::new(),
            child_links: Vec::new(),
        };
        let mut first = HashMap::new();
        first.insert("a".to_string(), dates(Some("1900"), Some("1890")));
        let first_key = detect(&tree, &first, &HashMap::new())[0].key.clone();
        let mut second = HashMap::new();
        second.insert("a".to_string(), dates(Some("1901"), Some("1890")));
        let second_key = detect(&tree, &second, &HashMap::new())[0].key.clone();
        assert_ne!(first_key, second_key);
    }
}
