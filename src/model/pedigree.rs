//! Ancestor-only tree filtering for pedigree charts.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::db::FamilyTree;

/// Builds the ancestor tree rooted at `root_id`, including `generations`
/// levels where the root counts as level one.
pub fn pedigree_tree(tree: &FamilyTree, root_id: &str, generations: usize) -> FamilyTree {
    if generations == 0 || !tree.people.iter().any(|person| person.id == root_id) {
        return FamilyTree::default();
    }
    let family_by_id: HashMap<&str, &crate::db::Family> = tree
        .families
        .iter()
        .map(|family| (family.id.as_str(), family))
        .collect();
    let mut parent_families: HashMap<&str, Vec<&crate::db::Family>> = HashMap::new();
    for (family_id, child_id) in &tree.child_links {
        if let Some(family) = family_by_id.get(family_id.as_str()) {
            parent_families
                .entry(child_id.as_str())
                .or_default()
                .push(family);
        }
    }

    let max_depth = generations.saturating_sub(1);
    let mut people: HashSet<&str> = HashSet::new();
    let mut families: HashSet<&str> = HashSet::new();
    let mut links: HashSet<(&str, &str)> = HashSet::new();
    let mut best_depth: HashMap<&str, usize> = HashMap::new();
    let mut queue = VecDeque::from([(root_id, 0usize)]);
    people.insert(root_id);

    while let Some((person_id, depth)) = queue.pop_front() {
        let known = best_depth.entry(person_id).or_insert(depth);
        if *known < depth {
            continue;
        }
        *known = depth;
        if depth >= max_depth {
            continue;
        }
        let Some(parents) = parent_families.get(person_id) else {
            continue;
        };
        for family in parents {
            families.insert(family.id.as_str());
            links.insert((family.id.as_str(), person_id));
            for parent in [family.husband_id.as_deref(), family.wife_id.as_deref()]
                .into_iter()
                .flatten()
            {
                if !tree.people.iter().any(|person| person.id == parent) {
                    continue;
                }
                people.insert(parent);
                if best_depth
                    .get(parent)
                    .is_none_or(|known| *known > depth + 1)
                {
                    queue.push_back((parent, depth + 1));
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
        let mut people = vec![
            person("root"),
            person("father"),
            person("mother"),
            person("gf"),
            person("gm"),
            person("sibling"),
            person("child"),
            person("spouse"),
            person("other"),
        ];
        people.extend([
            person("ggf"),
            person("ggm"),
            person("gggf"),
            person("gggm"),
            person("ggggf"),
            person("ggggm"),
        ]);
        FamilyTree {
            people,
            families: vec![
                family("f1", Some("father"), Some("mother")),
                family("f2", Some("gf"), Some("gm")),
                family("f3", Some("root"), Some("spouse")),
                family("f4", Some("ggf"), Some("ggm")),
                family("f5", Some("gggf"), Some("gggm")),
                family("f6", Some("ggggf"), Some("ggggm")),
            ],
            child_links: vec![
                ("f1".to_string(), "root".to_string()),
                ("f1".to_string(), "sibling".to_string()),
                ("f2".to_string(), "father".to_string()),
                ("f3".to_string(), "child".to_string()),
                ("f4".to_string(), "gf".to_string()),
                ("f5".to_string(), "ggf".to_string()),
                ("f6".to_string(), "gggf".to_string()),
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
    fn five_generations_include_four_ancestor_levels() {
        let result = pedigree_tree(&tree(), "root", 5);
        let ids = ids(&result);
        for expected in ["root", "father", "mother", "gf", "gm", "ggf", "ggm"] {
            assert!(ids.contains(expected), "missing {expected}");
        }
        assert!(ids.contains("gggf"), "fourth ancestor level included");
        assert!(!ids.contains("ggggf"), "fifth ancestor level excluded");
        assert!(!ids.contains("sibling"));
        assert!(!ids.contains("child"));
        assert!(!ids.contains("spouse"));
        assert!(!ids.contains("other"));
        assert!(
            !result
                .child_links
                .iter()
                .any(|(_, child)| child == "sibling")
        );
    }

    #[test]
    fn six_generations_add_one_more_ancestor_level() {
        let result = pedigree_tree(&tree(), "root", 6);
        let ids = ids(&result);
        assert!(ids.contains("ggggf"));
        assert!(ids.contains("ggggm"));
    }

    #[test]
    fn missing_root_or_zero_generations_yield_empty_tree() {
        assert!(pedigree_tree(&tree(), "missing", 5).people.is_empty());
        assert!(pedigree_tree(&tree(), "root", 0).people.is_empty());
    }

    #[test]
    fn multiple_parent_families_are_all_kept() {
        let mut tree = tree();
        tree.families.push(family("f7", Some("other"), None));
        tree.child_links
            .push(("f7".to_string(), "root".to_string()));
        let result = pedigree_tree(&tree, "root", 5);
        let ids = ids(&result);
        assert!(ids.contains("other"), "second parent set is kept");
        assert!(
            result
                .child_links
                .iter()
                .any(|(family, child)| { family == "f7" && child == "root" })
        );
    }

    #[test]
    fn cycles_terminate_and_keep_original_order() {
        let mut tree = tree();
        tree.child_links.push(("f1".to_string(), "gf".to_string()));
        let result = pedigree_tree(&tree, "root", 6);
        let order: Vec<&str> = result.people.iter().map(|p| p.id.as_str()).collect();
        let expected: Vec<&str> = tree
            .people
            .iter()
            .map(|person| person.id.as_str())
            .filter(|id| order.contains(id))
            .collect();
        assert_eq!(order, expected, "original rowid order is preserved");
        assert_eq!(
            order.iter().filter(|id| **id == "gf").count(),
            1,
            "cycles do not duplicate people"
        );
    }
}
