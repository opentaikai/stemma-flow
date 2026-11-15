//! Numbered descendant-list traversal for printable reports.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::db::FamilyTree;

/// Descendant numbering used by list reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DescendantNumbering {
    /// Children are `1.1`, `1.2`, grandchildren `1.1.1`, ...
    #[default]
    DAboville,
    /// Children are `11`, `12`, grandchildren `111`, `112`, ...
    Henry,
}

impl DescendantNumbering {
    pub fn label(self) -> &'static str {
        match self {
            Self::DAboville => "d'Aboville",
            Self::Henry => "Henry",
        }
    }

    /// Appends the 1-based child position to the parent's number.
    pub fn child_number(self, parent: &str, index: usize) -> String {
        match self {
            Self::DAboville => format!("{parent}.{index}"),
            Self::Henry => format!("{parent}{}", henry_token(index)),
        }
    }
}

/// One numbered descendant in generation-first order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescendantNumber {
    pub person_id: String,
    pub number: String,
    pub depth: usize,
}

/// Walks the descendants of `root_id`, assigning `numbering` positions.
///
/// People are visited breadth first, children keep the order of
/// `child_links`, and repeated people or cycles appear only once.
pub fn descendant_entries(
    tree: &FamilyTree,
    root_id: &str,
    generations: usize,
    numbering: DescendantNumbering,
) -> Vec<DescendantNumber> {
    if generations == 0 || !tree.people.iter().any(|person| person.id == root_id) {
        return Vec::new();
    }
    let max_depth = generations.saturating_sub(1);
    let mut visited = HashSet::from([root_id.to_string()]);
    let mut entries = Vec::new();
    let mut queue = VecDeque::from([(root_id.to_string(), "1".to_string(), 0usize)]);
    while let Some((person_id, number, depth)) = queue.pop_front() {
        entries.push(DescendantNumber {
            person_id: person_id.clone(),
            number: number.clone(),
            depth,
        });
        if depth >= max_depth {
            continue;
        }
        let mut index = 0;
        for child_id in children_of(tree, &person_id) {
            index += 1;
            if !visited.insert(child_id.clone()) {
                continue;
            }
            queue.push_back((child_id, numbering.child_number(&number, index), depth + 1));
        }
    }
    entries
}

/// Returns the blood-descendant path from `root_id` to `target_id`, if any.
///
/// Numbers use the real child positions so the path keeps its d'Aboville or
/// Henry identity.
pub fn direct_line(
    tree: &FamilyTree,
    root_id: &str,
    target_id: &str,
    numbering: DescendantNumbering,
) -> Option<Vec<DescendantNumber>> {
    if !tree.people.iter().any(|person| person.id == root_id)
        || !tree.people.iter().any(|person| person.id == target_id)
    {
        return None;
    }
    if root_id == target_id {
        return Some(vec![DescendantNumber {
            person_id: root_id.to_string(),
            number: "1".to_string(),
            depth: 0,
        }]);
    }
    let mut seen = HashSet::from([root_id.to_string()]);
    let mut nodes: HashMap<String, (String, usize)> =
        HashMap::from([(root_id.to_string(), ("1".to_string(), 0usize))]);
    let mut predecessor: HashMap<String, String> = HashMap::new();
    let mut queue = VecDeque::from([root_id.to_string()]);
    while let Some(person_id) = queue.pop_front() {
        let (number, depth) = nodes
            .get(&person_id)
            .cloned()
            .unwrap_or_else(|| ("1".to_string(), 0));
        let mut index = 0;
        for child_id in children_of(tree, &person_id) {
            index += 1;
            if !seen.insert(child_id.clone()) {
                continue;
            }
            let child_number = numbering.child_number(&number, index);
            nodes.insert(child_id.clone(), (child_number, depth + 1));
            predecessor.insert(child_id.clone(), person_id.clone());
            queue.push_back(child_id.clone());
        }
    }
    if !nodes.contains_key(target_id) {
        return None;
    }
    let mut chain = Vec::new();
    let mut cursor = target_id.to_string();
    loop {
        let (number, depth) = nodes.get(&cursor)?.clone();
        chain.push(DescendantNumber {
            person_id: cursor.clone(),
            number,
            depth,
        });
        match predecessor.get(&cursor) {
            Some(parent) => cursor = parent.clone(),
            None => break,
        }
    }
    chain.reverse();
    Some(chain)
}

/// Every descendant of `root_id` in breadth-first order, excluding the root.
pub fn descendant_targets(tree: &FamilyTree, root_id: &str) -> Vec<String> {
    let mut targets = Vec::new();
    if !tree.people.iter().any(|person| person.id == root_id) {
        return targets;
    }
    let mut seen = HashSet::from([root_id.to_string()]);
    let mut queue = VecDeque::from([root_id.to_string()]);
    while let Some(person_id) = queue.pop_front() {
        for child_id in children_of(tree, &person_id) {
            if seen.insert(child_id.clone()) {
                targets.push(child_id.clone());
                queue.push_back(child_id);
            }
        }
    }
    targets
}

/// Children of one person across all spouse families, in source order.
fn children_of(tree: &FamilyTree, person_id: &str) -> Vec<String> {
    let families = tree.families.iter().filter(|family| {
        family.husband_id.as_deref() == Some(person_id)
            || family.wife_id.as_deref() == Some(person_id)
    });
    let mut children = Vec::new();
    for family in families {
        for (family_id, child_id) in &tree.child_links {
            if family_id != &family.id
                || children.iter().any(|known| known == child_id)
                || !tree.people.iter().any(|person| &person.id == child_id)
            {
                continue;
            }
            children.push(child_id.clone());
        }
    }
    children
}

fn henry_token(index: usize) -> String {
    if index < 10 {
        return index.to_string();
    }
    char::from(b'A' + ((index - 10) % 26) as u8).to_string()
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
                "child1",
                "child2",
                "child1_spouse",
                "gc1a",
                "gc1b",
                "other_child",
                "unrelated",
            ]
            .into_iter()
            .map(person)
            .collect(),
            families: vec![
                family("f1", Some("root"), Some("spouse")),
                family("f2", Some("child1"), Some("child1_spouse")),
                family("f3", Some("root"), Some("child1_spouse")),
            ],
            child_links: vec![
                ("f1".to_string(), "child1".to_string()),
                ("f1".to_string(), "child2".to_string()),
                ("f2".to_string(), "gc1a".to_string()),
                ("f2".to_string(), "gc1b".to_string()),
                ("f3".to_string(), "other_child".to_string()),
            ],
        }
    }

    fn numbers(entries: &[DescendantNumber]) -> Vec<(String, String, usize)> {
        entries
            .iter()
            .map(|entry| (entry.person_id.clone(), entry.number.clone(), entry.depth))
            .collect()
    }

    #[test]
    fn d_aboville_numbers_children_across_families() {
        let entries = descendant_entries(&tree(), "root", 4, DescendantNumbering::DAboville);
        assert_eq!(
            numbers(&entries),
            vec![
                ("root".to_string(), "1".to_string(), 0),
                ("child1".to_string(), "1.1".to_string(), 1),
                ("child2".to_string(), "1.2".to_string(), 1),
                ("other_child".to_string(), "1.3".to_string(), 1),
                ("gc1a".to_string(), "1.1.1".to_string(), 2),
                ("gc1b".to_string(), "1.1.2".to_string(), 2),
            ]
        );
    }

    #[test]
    fn henry_numbers_concatenate_child_positions() {
        let entries = descendant_entries(&tree(), "root", 4, DescendantNumbering::Henry);
        let numbers: Vec<&str> = entries.iter().map(|entry| entry.number.as_str()).collect();
        assert_eq!(numbers, ["1", "11", "12", "13", "111", "112"]);
    }

    #[test]
    fn henry_uses_letters_after_nine_children() {
        let mut tree = FamilyTree {
            people: vec![person("root")],
            families: vec![family("f1", Some("root"), None)],
            child_links: Vec::new(),
        };
        for index in 1..=11 {
            let id = format!("c{index}");
            tree.people.push(person(&id));
            tree.child_links.push(("f1".to_string(), id));
        }
        let entries = descendant_entries(&tree, "root", 2, DescendantNumbering::Henry);
        let numbers: Vec<&str> = entries.iter().map(|entry| entry.number.as_str()).collect();
        assert_eq!(numbers[1], "11");
        assert_eq!(numbers[10], "1A");
        assert_eq!(numbers[11], "1B");
    }

    #[test]
    fn generations_limit_the_walk() {
        let entries = descendant_entries(&tree(), "root", 2, DescendantNumbering::DAboville);
        assert!(entries.iter().all(|entry| entry.depth <= 1));
        assert!(!entries.iter().any(|entry| entry.person_id == "gc1a"));
        assert!(descendant_entries(&tree(), "root", 0, DescendantNumbering::DAboville).is_empty());
        assert!(
            descendant_entries(&tree(), "missing", 4, DescendantNumbering::DAboville).is_empty()
        );
    }

    #[test]
    fn repeated_people_and_cycles_appear_once() {
        let mut tree = tree();
        tree.child_links
            .push(("f2".to_string(), "root".to_string()));
        let entries = descendant_entries(&tree, "root", 8, DescendantNumbering::DAboville);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.person_id == "root")
                .count(),
            1
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.person_id == "child1")
                .count(),
            1
        );
    }

    #[test]
    fn direct_line_keeps_the_real_path_numbers() {
        let line =
            direct_line(&tree(), "root", "gc1b", DescendantNumbering::DAboville).expect("path");
        let numbers: Vec<&str> = line.iter().map(|entry| entry.number.as_str()).collect();
        assert_eq!(numbers, ["1", "1.1", "1.1.2"]);
        assert_eq!(line.last().map(|entry| entry.depth), Some(2));

        let line = direct_line(&tree(), "root", "gc1b", DescendantNumbering::Henry).expect("path");
        let numbers: Vec<&str> = line.iter().map(|entry| entry.number.as_str()).collect();
        assert_eq!(numbers, ["1", "11", "112"]);
    }

    #[test]
    fn direct_line_handles_root_missing_and_unrelated_targets() {
        let line = direct_line(&tree(), "root", "root", DescendantNumbering::DAboville)
            .expect("root path");
        assert_eq!(line.len(), 1);
        assert!(direct_line(&tree(), "root", "missing", DescendantNumbering::DAboville).is_none());
        assert!(
            direct_line(&tree(), "root", "unrelated", DescendantNumbering::DAboville).is_none()
        );
    }

    #[test]
    fn targets_exclude_the_root_and_keep_order() {
        let targets = descendant_targets(&tree(), "root");
        assert_eq!(targets, ["child1", "child2", "other_child", "gc1a", "gc1b"]);
        assert!(descendant_targets(&tree(), "missing").is_empty());
    }

    #[test]
    fn numbering_labels_are_stable() {
        assert_eq!(DescendantNumbering::DAboville.label(), "d'Aboville");
        assert_eq!(DescendantNumbering::Henry.label(), "Henry");
        assert_eq!(
            DescendantNumbering::DAboville.child_number("1.2", 3),
            "1.2.3"
        );
        assert_eq!(DescendantNumbering::Henry.child_number("1", 12), "1C");
    }
}
