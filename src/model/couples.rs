//! Derived couple rows for the sortable couple list.

use std::collections::HashMap;

use crate::db::{FamilyTree, MarriageInfo};
use crate::model::name::format_name;

/// One family displayed as a couple row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Couple {
    pub family_id: String,
    pub partner1_id: Option<String>,
    pub partner1_name: String,
    pub partner2_id: Option<String>,
    pub partner2_name: String,
    pub marriage_date: Option<String>,
    pub marriage_place: Option<String>,
    pub child_count: i64,
    pub marriage_event_id: Option<String>,
}

/// Builds one row per family that has at least one recorded spouse.
pub fn couples(
    tree: &FamilyTree,
    marriages: &HashMap<String, MarriageInfo>,
    surname_first: bool,
) -> Vec<Couple> {
    let person_name = |id: Option<&str>| {
        let Some(id) = id else {
            return String::new();
        };
        let Some(person) = tree.people.iter().find(|person| person.id == id) else {
            return String::new();
        };
        let name = format_name(
            &person.name_prefix,
            &person.given_name,
            &person.middle_name,
            &person.surname,
            &person.name_suffix,
            surname_first,
        );
        if name.is_empty() {
            "(unnamed)".to_string()
        } else {
            name
        }
    };
    tree.families
        .iter()
        .filter(|family| family.husband_id.is_some() || family.wife_id.is_some())
        .map(|family| {
            let marriage = marriages.get(&family.id);
            let child_count = tree
                .child_links
                .iter()
                .filter(|(family_id, _)| family_id == &family.id)
                .count() as i64;
            Couple {
                family_id: family.id.clone(),
                partner1_id: family.husband_id.clone(),
                partner1_name: person_name(family.husband_id.as_deref()),
                partner2_id: family.wife_id.clone(),
                partner2_name: person_name(family.wife_id.as_deref()),
                marriage_date: marriage.and_then(|marriage| marriage.date.clone()),
                marriage_place: marriage.and_then(|marriage| marriage.place.clone()),
                child_count,
                marriage_event_id: marriage.and_then(|marriage| marriage.event_id.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Family, Person};

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

    fn tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("p1", "Ada", "King"),
                person("p2", "Charles", "Babbage"),
                person("p3", "Mary", "Somerville"),
            ],
            families: vec![
                family("f1", Some("p1"), Some("p2")),
                family("f2", Some("p3"), None),
                family("f3", None, None),
            ],
            child_links: vec![
                ("f1".to_string(), "p3".to_string()),
                ("f1".to_string(), "p1".to_string()),
                ("f2".to_string(), "p2".to_string()),
            ],
        }
    }

    #[test]
    fn derives_partners_marriage_and_child_counts() {
        let mut marriages = HashMap::new();
        marriages.insert(
            "f1".to_string(),
            MarriageInfo {
                event_id: Some("e1".to_string()),
                date: Some("1850".to_string()),
                place: Some("London".to_string()),
            },
        );
        let rows = couples(&tree(), &marriages, false);
        assert_eq!(rows.len(), 2, "empty family excluded");
        assert_eq!(rows[0].partner1_name, "Ada King");
        assert_eq!(rows[0].partner2_name, "Charles Babbage");
        assert_eq!(rows[0].marriage_date.as_deref(), Some("1850"));
        assert_eq!(rows[0].marriage_place.as_deref(), Some("London"));
        assert_eq!(rows[0].child_count, 2);
        assert_eq!(rows[0].marriage_event_id.as_deref(), Some("e1"));
        assert_eq!(rows[1].partner2_name, "");
        assert_eq!(rows[1].child_count, 1);
    }

    #[test]
    fn surname_first_changes_display_only() {
        let rows = couples(&tree(), &HashMap::new(), true);
        assert_eq!(rows[0].partner1_name, "King, Ada");
        assert_eq!(rows[0].partner1_id.as_deref(), Some("p1"));
    }

    #[test]
    fn missing_marriage_leaves_optional_fields_empty() {
        let rows = couples(&tree(), &HashMap::new(), false);
        assert!(rows[0].marriage_date.is_none());
        assert!(rows[0].marriage_place.is_none());
        assert!(rows[0].marriage_event_id.is_none());
    }
}
