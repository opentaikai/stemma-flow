//! Nuclear-family filtering for the Family graph view.

use std::collections::HashSet;

use crate::db::{Family, FamilyTree};

/// Families shown for a person: spouse families first, otherwise parent families.
pub fn candidate_families<'a>(tree: &'a FamilyTree, person_id: &str) -> Vec<&'a Family> {
    let is_spouse = |family: &&Family| {
        family.husband_id.as_deref() == Some(person_id)
            || family.wife_id.as_deref() == Some(person_id)
    };
    let spouse_families: Vec<&Family> = tree.families.iter().filter(is_spouse).collect();
    if !spouse_families.is_empty() {
        return spouse_families;
    }
    let parent_ids: HashSet<&str> = tree
        .child_links
        .iter()
        .filter(|(_, child_id)| child_id == person_id)
        .map(|(family_id, _)| family_id.as_str())
        .collect();
    tree.families
        .iter()
        .filter(|family| parent_ids.contains(family.id.as_str()))
        .collect()
}

/// Returns the chosen nuclear family and its direct children, or just the
/// person when no family is available.
pub fn family_tree(tree: &FamilyTree, person_id: &str, family_id: Option<&str>) -> FamilyTree {
    let Some(person) = tree.people.iter().find(|person| person.id == person_id) else {
        return FamilyTree::default();
    };
    let candidates = candidate_families(tree, person_id);
    let family = family_id
        .and_then(|id| candidates.iter().copied().find(|family| family.id == id))
        .or_else(|| candidates.first().copied());
    let Some(family) = family else {
        return FamilyTree {
            people: vec![person.clone()],
            families: Vec::new(),
            child_links: Vec::new(),
        };
    };
    let mut people: HashSet<&str> = HashSet::new();
    people.insert(person_id);
    for spouse in [family.husband_id.as_deref(), family.wife_id.as_deref()]
        .into_iter()
        .flatten()
    {
        people.insert(spouse);
    }
    let children: Vec<&str> = tree
        .child_links
        .iter()
        .filter(|(candidate_family, _)| candidate_family == &family.id)
        .map(|(_, child_id)| child_id.as_str())
        .collect();
    for child in &children {
        people.insert(child);
    }
    FamilyTree {
        people: tree
            .people
            .iter()
            .filter(|person| people.contains(person.id.as_str()))
            .cloned()
            .collect(),
        families: tree
            .families
            .iter()
            .filter(|candidate| candidate.id == family.id)
            .cloned()
            .collect(),
        child_links: tree
            .child_links
            .iter()
            .filter(|(candidate_family, child_id)| {
                candidate_family == &family.id && people.contains(child_id.as_str())
            })
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Person;

    fn person(id: &str) -> Person {
        let mut person = Person::new(id, "Person", "U");
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
            people: [
                "root",
                "spouse",
                "child",
                "child_spouse",
                "grandchild",
                "father",
                "mother",
                "sibling",
                "other",
            ]
            .into_iter()
            .map(person)
            .collect(),
            families: vec![
                family("f1", Some("root"), Some("spouse")),
                family("f2", Some("child"), Some("child_spouse")),
                family("f3", Some("father"), Some("mother")),
                family("f4", Some("other"), None),
            ],
            child_links: vec![
                ("f1".to_string(), "child".to_string()),
                ("f2".to_string(), "grandchild".to_string()),
                ("f3".to_string(), "root".to_string()),
                ("f3".to_string(), "sibling".to_string()),
            ],
        }
    }

    fn ids(tree: &FamilyTree) -> HashSet<&str> {
        tree.people
            .iter()
            .map(|person| person.id.as_str())
            .collect()
    }

    #[test]
    fn spouse_family_contains_couple_and_direct_children_only() {
        let result = family_tree(&tree(), "root", None);
        let ids = ids(&result);
        assert_eq!(ids, HashSet::from(["root", "spouse", "child"]));
        assert_eq!(result.families.len(), 1);
        assert_eq!(result.child_links.len(), 1);
    }

    #[test]
    fn parent_family_is_the_fallback_with_all_siblings() {
        let result = family_tree(&tree(), "sibling", None);
        let ids = ids(&result);
        assert_eq!(ids, HashSet::from(["father", "mother", "root", "sibling"]));
    }

    #[test]
    fn explicit_family_is_honoured_and_invalid_ids_fall_back() {
        let tree = tree();
        let selected = family_tree(&tree, "root", Some("f1"));
        assert!(ids(&selected).contains("spouse"));
        let fallback = family_tree(&tree, "root", Some("missing"));
        assert!(ids(&fallback).contains("spouse"));
    }

    #[test]
    fn person_without_a_family_is_a_single_node() {
        let result = family_tree(&tree(), "other", Some("f1"));
        assert_eq!(result.people.len(), 1);
        assert_eq!(result.people[0].id, "other");
    }

    #[test]
    fn single_parent_family_keeps_the_known_parent_and_children() {
        let mut tree = tree();
        tree.families.push(family("f5", Some("other"), None));
        tree.child_links
            .push(("f5".to_string(), "child".to_string()));
        let result = family_tree(&tree, "other", Some("f5"));
        let ids = ids(&result);
        assert_eq!(ids, HashSet::from(["other", "child"]));
    }
}
