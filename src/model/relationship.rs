//! Relationship labels relative to a tree's root person.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::db::FamilyTree;
use crate::i18n::{tr, tr_args};

/// Gender used to pick relationship words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sex {
    Male,
    Female,
    Unknown,
}

impl Sex {
    fn from_code(code: &str) -> Self {
        match code {
            "M" => Self::Male,
            "F" => Self::Female,
            _ => Self::Unknown,
        }
    }
}

/// Structured relationship to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kinship {
    Root,
    Spouse(Sex),
    Ancestor { sex: Sex, generations: usize },
    Descendant { sex: Sex, generations: usize },
    Sibling(Sex),
    AuntUncle(Sex),
    NieceNephew(Sex),
    Cousin { degree: usize, removed: usize },
    RelatedByMarriage,
    Unrelated,
}

impl Kinship {
    pub fn label(&self) -> String {
        match self {
            Self::Root => tr("relationship.root"),
            Self::Spouse(sex) => tr(match sex {
                Sex::Male => "relationship.husband",
                Sex::Female => "relationship.wife",
                Sex::Unknown => "relationship.spouse",
            }),
            Self::Ancestor { sex, generations } => match generations {
                1 => tr(match sex {
                    Sex::Male => "relationship.father",
                    Sex::Female => "relationship.mother",
                    Sex::Unknown => "relationship.parent",
                }),
                2 => tr(match sex {
                    Sex::Male => "relationship.grandfather",
                    Sex::Female => "relationship.grandmother",
                    Sex::Unknown => "relationship.grandparent",
                }),
                3 => tr(match sex {
                    Sex::Male => "relationship.great_grandfather",
                    Sex::Female => "relationship.great_grandmother",
                    Sex::Unknown => "relationship.great_grandparent",
                }),
                _ => tr_args(
                    "relationship.ancestor",
                    &[("count", generations.to_string())],
                ),
            },
            Self::Descendant { sex, generations } => match generations {
                1 => tr(match sex {
                    Sex::Male => "relationship.son",
                    Sex::Female => "relationship.daughter",
                    Sex::Unknown => "relationship.child",
                }),
                2 => tr(match sex {
                    Sex::Male => "relationship.grandson",
                    Sex::Female => "relationship.granddaughter",
                    Sex::Unknown => "relationship.grandchild",
                }),
                3 => tr(match sex {
                    Sex::Male => "relationship.great_grandson",
                    Sex::Female => "relationship.great_granddaughter",
                    Sex::Unknown => "relationship.great_grandchild",
                }),
                _ => tr_args(
                    "relationship.descendant",
                    &[("count", generations.to_string())],
                ),
            },
            Self::Sibling(sex) => tr(match sex {
                Sex::Male => "relationship.brother",
                Sex::Female => "relationship.sister",
                Sex::Unknown => "relationship.sibling",
            }),
            Self::AuntUncle(sex) => tr(match sex {
                Sex::Male => "relationship.uncle",
                Sex::Female => "relationship.aunt",
                Sex::Unknown => "relationship.aunt_uncle",
            }),
            Self::NieceNephew(sex) => tr(match sex {
                Sex::Male => "relationship.nephew",
                Sex::Female => "relationship.niece",
                Sex::Unknown => "relationship.niece_nephew",
            }),
            Self::Cousin { degree, removed } => {
                if *removed == 0 {
                    tr_args("relationship.cousin", &[("degree", degree.to_string())])
                } else {
                    tr_args(
                        "relationship.cousin_removed",
                        &[
                            ("degree", degree.to_string()),
                            ("removed", removed.to_string()),
                        ],
                    )
                }
            }
            Self::RelatedByMarriage => tr("relationship.by_marriage"),
            Self::Unrelated => tr("relationship.unrelated"),
        }
    }
}

/// Computes a person's relationship to `root_id`.
pub fn relationship_to_root(tree: &FamilyTree, root_id: &str, person_id: &str) -> Option<Kinship> {
    tree.people.iter().find(|person| person.id == root_id)?;
    let person = tree.people.iter().find(|person| person.id == person_id)?;
    if root_id == person_id {
        return Some(Kinship::Root);
    }
    let sex = Sex::from_code(&person.gender);
    let parents = parent_map(tree);
    let spouses = spouse_map(tree);
    if spouses
        .get(root_id)
        .is_some_and(|set| set.contains(person_id))
    {
        return Some(Kinship::Spouse(sex));
    }
    let root_ancestors = ancestor_depths(&parents, root_id);
    let person_ancestors = ancestor_depths(&parents, person_id);
    // This person is a descendant of root, or an ancestor of root.
    if let Some(depth) = person_ancestors.get(root_id) {
        return Some(Kinship::Descendant {
            sex,
            generations: *depth,
        });
    }
    if let Some(depth) = root_ancestors.get(person_id) {
        return Some(Kinship::Ancestor {
            sex,
            generations: *depth,
        });
    }
    if shares_parent(&parents, root_id, person_id) {
        return Some(Kinship::Sibling(sex));
    }
    if parents.get(root_id).is_some_and(|root_parents| {
        root_parents
            .iter()
            .any(|parent| shares_parent(&parents, parent, person_id))
    }) {
        return Some(Kinship::AuntUncle(sex));
    }
    if parents.get(person_id).is_some_and(|person_parents| {
        person_parents
            .iter()
            .any(|parent| shares_parent(&parents, root_id, parent))
    }) {
        return Some(Kinship::NieceNephew(sex));
    }
    if let Some((degree, removed)) = cousin(root_ancestors, person_ancestors) {
        return Some(Kinship::Cousin { degree, removed });
    }
    if spouses.get(person_id).is_some_and(|people| {
        people
            .iter()
            .any(|other| relationship_to_blood(tree, &parents, root_id, other).is_some())
    }) {
        return Some(Kinship::RelatedByMarriage);
    }
    Some(Kinship::Unrelated)
}

fn relationship_to_blood(
    tree: &FamilyTree,
    parents: &HashMap<&str, HashSet<&str>>,
    root_id: &str,
    person_id: &str,
) -> Option<()> {
    if root_id == person_id || shares_parent(parents, root_id, person_id) {
        return Some(());
    }
    let root_ancestors = ancestor_depths(parents, root_id);
    let person_ancestors = ancestor_depths(parents, person_id);
    if person_ancestors.contains_key(root_id)
        || root_ancestors.contains_key(person_id)
        || cousin(root_ancestors, person_ancestors).is_some()
    {
        return Some(());
    }
    let _ = tree;
    None
}

fn parent_map(tree: &FamilyTree) -> HashMap<&str, HashSet<&str>> {
    let mut map: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (family_id, child_id) in &tree.child_links {
        let Some(family) = tree.families.iter().find(|family| &family.id == family_id) else {
            continue;
        };
        let entry = map.entry(child_id.as_str()).or_default();
        for parent in [family.husband_id.as_deref(), family.wife_id.as_deref()]
            .into_iter()
            .flatten()
        {
            entry.insert(parent);
        }
    }
    map
}

fn spouse_map(tree: &FamilyTree) -> HashMap<&str, HashSet<&str>> {
    let mut map: HashMap<&str, HashSet<&str>> = HashMap::new();
    for family in &tree.families {
        if let (Some(husband), Some(wife)) = (&family.husband_id, &family.wife_id) {
            map.entry(husband.as_str()).or_default().insert(wife);
            map.entry(wife.as_str()).or_default().insert(husband);
        }
    }
    map
}

fn shares_parent(parents: &HashMap<&str, HashSet<&str>>, left: &str, right: &str) -> bool {
    parents
        .get(left)
        .zip(parents.get(right))
        .is_some_and(|(left, right)| !left.is_disjoint(right))
}

fn ancestor_depths(parents: &HashMap<&str, HashSet<&str>>, start: &str) -> HashMap<String, usize> {
    let mut depths = HashMap::new();
    let mut queue = VecDeque::from([(start.to_string(), 0usize)]);
    while let Some((person, depth)) = queue.pop_front() {
        let next = depth + 1;
        let Some(parents) = parents.get(person.as_str()) else {
            continue;
        };
        for parent in parents {
            if depths.get(*parent).is_none_or(|known| *known > next) {
                depths.insert((*parent).to_string(), next);
                queue.push_back(((*parent).to_string(), next));
            }
        }
    }
    depths
}

fn cousin(
    root_ancestors: HashMap<String, usize>,
    person_ancestors: HashMap<String, usize>,
) -> Option<(usize, usize)> {
    root_ancestors
        .iter()
        .filter_map(|(ancestor, root_depth)| {
            let person_depth = person_ancestors.get(ancestor.as_str())?;
            Some((root_depth + person_depth, *root_depth, *person_depth))
        })
        .min()
        .map(|(_, root_depth, person_depth)| {
            let degree = root_depth.min(person_depth).saturating_sub(1).max(1);
            (degree, root_depth.abs_diff(person_depth))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Family, Person};

    fn person(id: &str, gender: &str) -> Person {
        let mut person = Person::new(id, "Person", gender);
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
                "grandfather",
                "grandmother",
                "sibling",
                "child",
                "grandchild",
                "aunt",
                "cousin",
                "unrelated",
            ]
            .into_iter()
            .map(|id| person(id, "U"))
            .collect(),
            families: vec![
                family("f1", Some("father"), Some("mother")),
                family("f2", Some("grandfather"), Some("grandmother")),
                family("f3", Some("root"), None),
                family("f4", Some("child"), None),
                family("f5", Some("aunt"), None),
            ],
            child_links: vec![
                ("f1".to_string(), "root".to_string()),
                ("f1".to_string(), "sibling".to_string()),
                ("f2".to_string(), "father".to_string()),
                ("f3".to_string(), "child".to_string()),
                ("f4".to_string(), "grandchild".to_string()),
                ("f5".to_string(), "aunt".to_string()),
                ("f2".to_string(), "aunt".to_string()),
                ("f5".to_string(), "cousin".to_string()),
            ],
        }
    }

    #[test]
    fn close_family_labels_are_derived() {
        let tree = tree();
        assert_eq!(
            relationship_to_root(&tree, "root", "root"),
            Some(Kinship::Root)
        );
        assert_eq!(
            relationship_to_root(&tree, "root", "father"),
            Some(Kinship::Ancestor {
                sex: Sex::Unknown,
                generations: 1
            })
        );
        assert_eq!(
            relationship_to_root(&tree, "root", "grandfather"),
            Some(Kinship::Ancestor {
                sex: Sex::Unknown,
                generations: 2
            })
        );
        assert_eq!(
            relationship_to_root(&tree, "root", "sibling"),
            Some(Kinship::Sibling(Sex::Unknown))
        );
        assert_eq!(
            relationship_to_root(&tree, "root", "child"),
            Some(Kinship::Descendant {
                sex: Sex::Unknown,
                generations: 1
            })
        );
        assert_eq!(
            relationship_to_root(&tree, "root", "aunt"),
            Some(Kinship::AuntUncle(Sex::Unknown))
        );
        assert_eq!(
            relationship_to_root(&tree, "root", "cousin"),
            Some(Kinship::Cousin {
                degree: 1,
                removed: 0
            })
        );
        assert_eq!(
            relationship_to_root(&tree, "root", "unrelated"),
            Some(Kinship::Unrelated)
        );
    }

    #[test]
    fn relationship_labels_are_localized() {
        assert_eq!(Kinship::Root.label(), "Root");
        assert_eq!(Kinship::Spouse(Sex::Female).label(), "Wife");
        assert_eq!(
            Kinship::Cousin {
                degree: 2,
                removed: 1
            }
            .label(),
            "Cousin (degree 2, removed 1)"
        );
    }

    #[test]
    fn missing_root_or_person_has_no_label() {
        assert!(relationship_to_root(&tree(), "missing", "root").is_none());
        assert!(relationship_to_root(&tree(), "root", "missing").is_none());
    }
}
