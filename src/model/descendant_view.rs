//! Descendant-only tree filtering for descendant charts.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::db::FamilyTree;

/// Builds the descendant tree rooted at `root_id`, including `generations`
/// levels where the root counts as level one.
pub fn descendant_tree(tree: &FamilyTree, root_id: &str, generations: usize) -> FamilyTree {
    if generations == 0 || !tree.people.iter().any(|person| person.id == root_id) {
        return FamilyTree::default();
    }
    let mut spouse_families: HashMap<&str, Vec<&crate::db::Family>> = HashMap::new();
    for family in &tree.families {
        for spouse in [family.husband_id.as_deref(), family.wife_id.as_deref()]
            .into_iter()
            .flatten()
        {
            spouse_families.entry(spouse).or_default().push(family);
        }
    }

    let max_depth = generations.saturating_sub(1);
    let mut people: HashSet<&str> = HashSet::from([root_id]);
    let mut families: HashSet<&str> = HashSet::new();
    let mut links: HashSet<(&str, &str)> = HashSet::new();
    let mut best_depth: HashMap<&str, usize> = HashMap::new();
    let mut queue = VecDeque::from([(root_id, 0usize)]);

    while let Some((person_id, depth)) = queue.pop_front() {
        let known = best_depth.entry(person_id).or_insert(depth);
        if *known < depth {
            continue;
        }
        *known = depth;
        let Some(person_families) = spouse_families.get(person_id) else {
            continue;
        };
        for family in person_families {
            families.insert(family.id.as_str());
            for spouse in [family.husband_id.as_deref(), family.wife_id.as_deref()]
                .into_iter()
                .flatten()
            {
                if tree.people.iter().any(|person| person.id == spouse) {
                    people.insert(spouse);
                }
            }
            if depth >= max_depth {
                continue;
            }
            for (family_id, child_id) in &tree.child_links {
                if family_id != &family.id
                    || !tree.people.iter().any(|person| person.id == *child_id)
                {
                    continue;
                }
                links.insert((family.id.as_str(), child_id.as_str()));
                people.insert(child_id);
                if best_depth
                    .get(child_id.as_str())
                    .is_none_or(|known| *known > depth + 1)
                {
                    queue.push_back((child_id, depth + 1));
                }
            }
        }
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
            .filter(|family| families.contains(family.id.as_str()))
            .cloned()
            .collect(),
        child_links: tree
            .child_links
            .iter()
            .filter(|(family_id, child_id)| {
                links.contains(&(family_id.as_str(), child_id.as_str()))
            })
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
                "spouse",
                "child",
                "child_spouse",
                "grandchild",
                "great_grandchild",
                "root_father",
                "root_sibling",
                "other_spouse",
                "other",
                "other_child",
            ]
            .into_iter()
            .map(person)
            .collect(),
            families: vec![
                family("f1", Some("root"), Some("spouse")),
                family("f2", Some("child"), Some("child_spouse")),
                family("f3", Some("child"), Some("other_spouse")),
                family("f4", Some("grandchild"), None),
                family("f5", Some("root_father"), None),
                family("f6", Some("other"), Some("other_spouse")),
            ],
            child_links: vec![
                ("f1".to_string(), "child".to_string()),
                ("f2".to_string(), "grandchild".to_string()),
                ("f3".to_string(), "other_child".to_string()),
                ("f4".to_string(), "great_grandchild".to_string()),
                ("f5".to_string(), "root".to_string()),
                ("f5".to_string(), "root_sibling".to_string()),
                ("f6".to_string(), "other".to_string()),
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
    fn two_generations_include_root_spouses_and_children() {
        let result = descendant_tree(&tree(), "root", 2);
        let ids = ids(&result);
        assert_eq!(
            ids,
            HashSet::from(["root", "spouse", "child", "child_spouse", "other_spouse"])
        );
        assert_eq!(result.child_links.len(), 1);
        assert!(!ids.contains("grandchild"));
    }

    #[test]
    fn deeper_generations_include_all_descendants_and_their_spouses() {
        let result = descendant_tree(&tree(), "root", 4);
        let ids = ids(&result);
        for expected in [
            "root",
            "spouse",
            "child",
            "child_spouse",
            "grandchild",
            "great_grandchild",
        ] {
            assert!(ids.contains(expected), "missing {expected}");
        }
        assert!(
            ids.contains("other_spouse"),
            "remarried descendant spouse kept"
        );
        assert!(ids.contains("other_child"), "all descendant families kept");
        assert!(!ids.contains("other"), "spouse's unrelated family excluded");
    }

    #[test]
    fn final_generation_spouses_are_shown_without_their_children() {
        let result = descendant_tree(&tree(), "root", 3);
        let ids = ids(&result);
        assert!(ids.contains("grandchild"));
        assert!(!ids.contains("great_grandchild"));
        assert!(
            result.families.iter().any(|family| family.id == "f4"),
            "final family is kept"
        );
    }

    #[test]
    fn ancestors_and_root_siblings_are_excluded() {
        let result = descendant_tree(&tree(), "root", 6);
        let ids = ids(&result);
        assert!(!ids.contains("root_father"));
        assert!(!ids.contains("root_sibling"));
    }

    #[test]
    fn missing_root_yields_an_empty_tree() {
        assert!(descendant_tree(&tree(), "missing", 4).people.is_empty());
        assert!(descendant_tree(&tree(), "root", 0).people.is_empty());
    }

    #[test]
    fn output_preserves_source_order_and_deduplicates_cycles() {
        let mut tree = tree();
        tree.child_links
            .push(("f4".to_string(), "child".to_string()));
        let result = descendant_tree(&tree, "root", 6);
        let order: Vec<&str> = result
            .people
            .iter()
            .map(|person| person.id.as_str())
            .collect();
        let expected: Vec<&str> = tree
            .people
            .iter()
            .map(|person| person.id.as_str())
            .filter(|id| order.contains(id))
            .collect();
        assert_eq!(order, expected);
        assert_eq!(order.iter().filter(|id| **id == "child").count(), 1);
    }
}
