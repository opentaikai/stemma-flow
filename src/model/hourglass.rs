//! Combined ancestor and descendant filtering for hourglass charts.

use std::collections::HashSet;

use crate::db::FamilyTree;
use crate::model::descendant_view::descendant_tree;
use crate::model::pedigree::pedigree_tree;

/// Builds the union of a root's ancestors and descendants.
///
/// Ancestor and descendant depths are independent; the root itself is kept
/// once. People and family records preserve their original tree order.
pub fn combined_tree(
    tree: &FamilyTree,
    root_id: &str,
    ancestor_generations: usize,
    descendant_generations: usize,
) -> FamilyTree {
    if !tree.people.iter().any(|person| person.id == root_id) {
        return FamilyTree::default();
    }
    let ancestors = pedigree_tree(tree, root_id, ancestor_generations);
    let descendants = descendant_tree(tree, root_id, descendant_generations);
    let people: HashSet<&str> = ancestors
        .people
        .iter()
        .chain(descendants.people.iter())
        .map(|person| person.id.as_str())
        .collect();
    let families: HashSet<&str> = ancestors
        .families
        .iter()
        .chain(descendants.families.iter())
        .map(|family| family.id.as_str())
        .collect();
    let links: HashSet<(&str, &str)> = ancestors
        .child_links
        .iter()
        .chain(descendants.child_links.iter())
        .map(|(family, child)| (family.as_str(), child.as_str()))
        .collect();
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
            .filter(|family| families.contains(family.id.as_str()))
            .cloned()
            .collect(),
        child_links: tree
            .child_links
            .iter()
            .filter(|(family, child)| links.contains(&(family.as_str(), child.as_str())))
            .cloned()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Family, Person};

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
                "father",
                "mother",
                "gf",
                "spouse",
                "child",
                "grandchild",
                "sibling",
                "other",
            ]
            .into_iter()
            .map(person)
            .collect(),
            families: vec![
                family("f1", Some("father"), Some("mother")),
                family("f2", Some("gf"), None),
                family("f3", Some("root"), Some("spouse")),
                family("f4", Some("child"), None),
            ],
            child_links: vec![
                ("f1".to_string(), "root".to_string()),
                ("f1".to_string(), "sibling".to_string()),
                ("f2".to_string(), "father".to_string()),
                ("f3".to_string(), "child".to_string()),
                ("f4".to_string(), "grandchild".to_string()),
            ],
        }
    }

    fn person_ids(tree: &FamilyTree) -> HashSet<&str> {
        tree.people
            .iter()
            .map(|person| person.id.as_str())
            .collect()
    }

    #[test]
    fn combines_ancestors_and_descendants() {
        let result = combined_tree(&tree(), "root", 3, 3);
        let ids = person_ids(&result);
        for expected in [
            "root",
            "father",
            "mother",
            "gf",
            "spouse",
            "child",
            "grandchild",
        ] {
            assert!(ids.contains(expected), "missing {expected}");
        }
        assert!(!ids.contains("sibling"), "siblings are not combined");
        assert!(!ids.contains("other"));
        assert_eq!(
            result
                .people
                .iter()
                .filter(|person| person.id == "root")
                .count(),
            1,
            "the shared root appears once"
        );
    }

    #[test]
    fn generation_limits_are_independent() {
        let shallow_ancestors = combined_tree(&tree(), "root", 2, 4);
        let ids = person_ids(&shallow_ancestors);
        assert!(ids.contains("father"));
        assert!(ids.contains("mother"));
        assert!(!ids.contains("gf"), "third ancestor generation excluded");
        assert!(ids.contains("grandchild"));

        let shallow_descendants = combined_tree(&tree(), "root", 4, 2);
        let ids = person_ids(&shallow_descendants);
        assert!(ids.contains("gf"));
        assert!(ids.contains("child"));
        assert!(
            !ids.contains("grandchild"),
            "third descendant generation excluded"
        );
    }

    #[test]
    fn zero_generations_leave_only_the_other_side() {
        let ancestors_only = combined_tree(&tree(), "root", 3, 0);
        let ids = person_ids(&ancestors_only);
        assert!(ids.contains("gf"));
        assert!(!ids.contains("child"));

        let descendants_only = combined_tree(&tree(), "root", 0, 3);
        let ids = person_ids(&descendants_only);
        assert!(ids.contains("grandchild"));
        assert!(!ids.contains("father"));
    }

    #[test]
    fn missing_root_is_empty() {
        assert!(combined_tree(&tree(), "missing", 3, 3).people.is_empty());
    }

    #[test]
    fn source_order_and_links_are_preserved() {
        let source = tree();
        let result = combined_tree(&source, "root", 3, 3);
        let order: Vec<&str> = result.people.iter().map(|p| p.id.as_str()).collect();
        let expected: Vec<&str> = source
            .people
            .iter()
            .map(|p| p.id.as_str())
            .filter(|id| order.contains(id))
            .collect();
        assert_eq!(order, expected);
        for (family, child) in &result.child_links {
            assert!(
                source
                    .child_links
                    .iter()
                    .any(|(candidate_family, candidate_child)| {
                        candidate_family == family && candidate_child == child
                    })
            );
        }
    }
}
