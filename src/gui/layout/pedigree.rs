//! Family-aware layered layout for the ancestor-only pedigree view.

use std::collections::{HashMap, HashSet, VecDeque};

use egui::{Pos2, Rect, vec2};

use crate::db::{Family, FamilyTree};

use super::builder::assign_trunk_lanes;
use super::scene::{NodeGeom, TreeScene, UnionGeom, person_display_name};
use super::{H_SPACING, NODE_H, NODE_W, RANK_PITCH, SPOUSE_GAP, TRUNK_TOP_MARGIN};

/// Builds a deterministic ancestor chart with the root at the bottom.
pub fn build_pedigree_scene(
    tree: &FamilyTree,
    root_id: &str,
    generations: usize,
    surname_first: bool,
) -> TreeScene {
    if generations == 0 || !tree.people.iter().any(|person| person.id == root_id) {
        return TreeScene {
            nodes: Vec::new(),
            unions: Vec::new(),
            bounds: Rect::from_min_size(Pos2::ZERO, vec2(0.0, 0.0)),
        };
    }
    let max_depth = generations.saturating_sub(1);
    let family_by_id: HashMap<&str, &Family> = tree
        .families
        .iter()
        .map(|family| (family.id.as_str(), family))
        .collect();
    let mut parent_families: HashMap<&str, Vec<&Family>> = HashMap::new();
    for (family_id, child_id) in &tree.child_links {
        if let Some(family) = family_by_id.get(family_id.as_str()) {
            parent_families
                .entry(child_id.as_str())
                .or_default()
                .push(family);
        }
    }

    let mut used_links: HashSet<(&str, &str)> = HashSet::new();
    let mut depth: HashMap<&str, usize> = HashMap::from([(root_id, 0)]);
    let mut layers: Vec<Vec<&str>> = vec![Vec::new(); max_depth + 1];
    layers[0].push(root_id);
    let mut queue = VecDeque::from([(root_id, 0usize)]);
    while let Some((child, child_depth)) = queue.pop_front() {
        if child_depth >= max_depth {
            continue;
        }
        let Some(families) = parent_families.get(child) else {
            continue;
        };
        for family in families {
            used_links.insert((family.id.as_str(), child));
            for parent in [family.husband_id.as_deref(), family.wife_id.as_deref()]
                .into_iter()
                .flatten()
            {
                if !tree.people.iter().any(|person| person.id == parent) {
                    continue;
                }
                let next = child_depth + 1;
                match depth.get(parent) {
                    Some(existing) if *existing <= next => {}
                    _ => {
                        depth.insert(parent, next);
                        if layers
                            .get(next)
                            .is_some_and(|layer| !layer.contains(&parent))
                        {
                            layers[next].push(parent);
                        }
                        queue.push_back((parent, next));
                    }
                }
            }
        }
    }

    let mut x: HashMap<&str, f32> = HashMap::new();
    for layer in &layers {
        let mut cursor = 0.0;
        for person in layer {
            x.insert(person, cursor);
            cursor += NODE_W + H_SPACING;
        }
    }
    // Alternate downward and upward barycentric passes while preserving the
    // family-aware order established during the ancestor walk.
    for _ in 0..4 {
        for d in 1..=max_depth {
            let targets = layer_targets_from_children(tree, &layers, &x, d);
            resolve_layer(layers.get(d), &targets, &mut x);
        }
        for d in (0..max_depth).rev() {
            let targets = layer_targets_from_parents(tree, &layers, &x, d);
            resolve_layer(layers.get(d), &targets, &mut x);
        }
    }
    for layer in &layers {
        pack_layer(tree, layer, &mut x);
    }
    let root_x = x.get(root_id).copied().unwrap_or(0.0) + NODE_W * 0.5;
    for value in x.values_mut() {
        *value -= root_x;
    }

    let mut nodes = Vec::new();
    for (d, layer) in layers.iter().enumerate() {
        let y = (max_depth - d) as f32 * RANK_PITCH;
        for &person_id in layer {
            let Some(person) = tree.people.iter().find(|person| person.id == person_id) else {
                continue;
            };
            let left = x.get(person_id).copied().unwrap_or(0.0);
            nodes.push(NodeGeom {
                person_id: person_id.to_string(),
                display_name: person_display_name(person, surname_first),
                bounds: Rect::from_min_size(Pos2::new(left, y), vec2(NODE_W, NODE_H)),
            });
        }
    }

    let mut unions = build_unions(tree, &nodes, &used_links);
    assign_trunk_lanes(&mut unions);
    let bounds = nodes
        .iter()
        .map(|node| node.bounds)
        .reduce(Rect::union)
        .unwrap_or_else(|| Rect::from_min_size(Pos2::ZERO, vec2(0.0, 0.0)))
        .expand(16.0);
    TreeScene {
        nodes,
        unions,
        bounds,
    }
}

/// Packs a generation into rigid family units so spouses stay adjacent.
fn pack_layer<'a>(tree: &'a FamilyTree, layer: &[&'a str], x: &mut HashMap<&'a str, f32>) {
    let present: HashSet<&str> = layer.iter().copied().collect();
    let mut used: HashSet<&str> = HashSet::new();
    let mut units: Vec<Vec<&str>> = Vec::new();
    for &person in layer {
        if used.contains(person) {
            continue;
        }
        let spouse = tree
            .families
            .iter()
            .find_map(|family| {
                if family.husband_id.as_deref() == Some(person) {
                    family.wife_id.as_deref().filter(|id| present.contains(id))
                } else if family.wife_id.as_deref() == Some(person) {
                    family
                        .husband_id
                        .as_deref()
                        .filter(|id| present.contains(id))
                } else {
                    None
                }
            })
            .filter(|spouse| !used.contains(*spouse));
        let unit = match spouse {
            Some(spouse) => {
                used.insert(spouse);
                vec![person, spouse]
            }
            None => vec![person],
        };
        used.insert(person);
        units.push(unit);
    }
    let desired = units
        .iter()
        .map(|unit| {
            unit.iter()
                .map(|id| x.get(id).copied().unwrap_or(0.0) + NODE_W * 0.5)
                .sum::<f32>()
                / unit.len() as f32
        })
        .collect::<Vec<_>>();
    if desired.is_empty() {
        return;
    }
    let total_width = units
        .iter()
        .map(|unit| unit.len() as f32 * NODE_W + unit.len().saturating_sub(1) as f32 * SPOUSE_GAP)
        .sum::<f32>()
        + units.len().saturating_sub(1) as f32 * H_SPACING;
    let target_center = desired.iter().sum::<f32>() / desired.len() as f32;
    let mut cursor = target_center - total_width * 0.5;
    for (unit, desired) in units.iter().zip(desired) {
        let width = unit.len() as f32 * NODE_W + unit.len().saturating_sub(1) as f32 * SPOUSE_GAP;
        let shift = desired - (cursor + width * 0.5);
        for (index, person) in unit.iter().enumerate() {
            let left = cursor + index as f32 * (NODE_W + SPOUSE_GAP) + shift;
            x.insert(person, left);
        }
        cursor += width + H_SPACING;
    }
}

fn resolve_layer<'a>(
    layer: Option<&Vec<&'a str>>,
    targets: &HashMap<&'a str, f32>,
    x: &mut HashMap<&'a str, f32>,
) {
    let Some(layer) = layer else {
        return;
    };
    let mut cursor = f32::NEG_INFINITY;
    for person in layer {
        let desired = targets
            .get(person)
            .copied()
            .unwrap_or_else(|| x.get(person).copied().unwrap_or(0.0));
        let value = if cursor.is_finite() {
            desired.max(cursor + NODE_W + H_SPACING)
        } else {
            desired
        };
        x.insert(person, value);
        cursor = value;
    }
}

fn layer_targets_from_children<'a>(
    tree: &'a FamilyTree,
    layers: &[Vec<&'a str>],
    x: &HashMap<&'a str, f32>,
    depth: usize,
) -> HashMap<&'a str, f32> {
    let mut targets: HashMap<&str, (f32, usize)> = HashMap::new();
    let Some(children) = layers.get(depth - 1) else {
        return HashMap::new();
    };
    for (family_id, child_id) in &tree.child_links {
        if !children.contains(&child_id.as_str()) {
            continue;
        }
        let Some(family) = tree.families.iter().find(|family| &family.id == family_id) else {
            continue;
        };
        let child_x = x.get(child_id.as_str()).copied().unwrap_or(0.0) + NODE_W * 0.5;
        let parents: Vec<&str> = [family.husband_id.as_deref(), family.wife_id.as_deref()]
            .into_iter()
            .flatten()
            .filter(|id| x.contains_key(id))
            .collect();
        let count = parents.len() as f32;
        for (index, parent) in parents.iter().enumerate() {
            let offset = (index as f32 - (count - 1.0) * 0.5) * (NODE_W + SPOUSE_GAP);
            let desired = child_x + offset - NODE_W * 0.5;
            let entry = targets.entry(parent).or_insert((0.0, 0));
            entry.0 += desired;
            entry.1 += 1;
        }
    }
    targets
        .into_iter()
        .map(|(person, (sum, count))| (person, sum / count as f32))
        .collect()
}

fn layer_targets_from_parents<'a>(
    tree: &'a FamilyTree,
    layers: &[Vec<&'a str>],
    x: &HashMap<&'a str, f32>,
    depth: usize,
) -> HashMap<&'a str, f32> {
    let mut targets: HashMap<&str, (f32, usize)> = HashMap::new();
    let Some(children) = layers.get(depth) else {
        return HashMap::new();
    };
    for (family_id, child_id) in &tree.child_links {
        if !children.contains(&child_id.as_str()) {
            continue;
        }
        let Some(family) = tree.families.iter().find(|family| &family.id == family_id) else {
            continue;
        };
        let parents: Vec<f32> = [family.husband_id.as_deref(), family.wife_id.as_deref()]
            .into_iter()
            .flatten()
            .filter_map(|id| x.get(id).copied().map(|value| value + NODE_W * 0.5))
            .collect();
        if parents.is_empty() {
            continue;
        }
        let centre = parents.iter().sum::<f32>() / parents.len() as f32;
        let entry = targets.entry(child_id.as_str()).or_insert((0.0, 0));
        entry.0 += centre - NODE_W * 0.5;
        entry.1 += 1;
    }
    targets
        .into_iter()
        .map(|(person, (sum, count))| (person, sum / count as f32))
        .collect()
}

fn build_unions(
    tree: &FamilyTree,
    nodes: &[NodeGeom],
    used_links: &HashSet<(&str, &str)>,
) -> Vec<UnionGeom> {
    let positions: HashMap<&str, Rect> = nodes
        .iter()
        .map(|node| (node.person_id.as_str(), node.bounds))
        .collect();
    let mut unions = Vec::new();
    for family in &tree.families {
        let husband = family
            .husband_id
            .as_deref()
            .and_then(|id| positions.get(id));
        let wife = family.wife_id.as_deref().and_then(|id| positions.get(id));
        let (spouse_line, union) = match (husband, wife) {
            (Some(husband), Some(wife)) => {
                let (left, right) = if husband.center().x <= wife.center().x {
                    (husband.right_center(), wife.left_center())
                } else {
                    (wife.right_center(), husband.left_center())
                };
                let adjacent = (right.x - left.x - SPOUSE_GAP).abs() < 0.5;
                let bar = adjacent.then_some([left, right]);
                (bar, left + (right - left) * 0.5)
            }
            (Some(parent), None) | (None, Some(parent)) => (None, parent.center_bottom()),
            (None, None) => continue,
        };
        let children: Vec<(String, Pos2)> = tree
            .child_links
            .iter()
            .filter(|(family_id, child_id)| {
                family_id == &family.id
                    && used_links.contains(&(family_id.as_str(), child_id.as_str()))
                    && positions.contains_key(child_id.as_str())
            })
            .filter_map(|(_, child_id)| {
                let bounds = positions.get(child_id.as_str())?;
                Some((child_id.clone(), Pos2::new(bounds.center().x, bounds.min.y)))
            })
            .collect();
        if children.is_empty() {
            continue;
        }
        let parent_rank_y = [husband, wife]
            .into_iter()
            .flatten()
            .map(|bounds| bounds.min.y)
            .fold(f32::INFINITY, f32::min);
        if !parent_rank_y.is_finite() {
            continue;
        }
        unions.push(UnionGeom {
            family_id: family.id.clone(),
            spouse_line,
            union,
            trunk_y: parent_rank_y + NODE_H + TRUNK_TOP_MARGIN,
            trunk_lane: 0,
            parent_rank_y,
            children,
        });
    }
    unions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Family, Person};
    use std::collections::HashSet;

    fn person(id: &str) -> Person {
        let mut person = Person::new(id, "Person", "U");
        person.id = id.to_string();
        person
    }

    fn family(id: &str, husband: &str, wife: &str) -> Family {
        Family {
            id: id.to_string(),
            husband_id: Some(husband.to_string()),
            wife_id: Some(wife.to_string()),
            created_at: String::new(),
        }
    }

    fn tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("root"),
                person("father"),
                person("mother"),
                person("gf"),
                person("gm"),
                person("older"),
            ],
            families: vec![family("f1", "father", "mother"), family("f2", "gf", "gm")],
            child_links: vec![
                ("f1".to_string(), "root".to_string()),
                ("f2".to_string(), "father".to_string()),
            ],
        }
    }

    #[test]
    fn root_is_bottom_centred_and_parents_are_above() {
        let scene = build_pedigree_scene(&tree(), "root", 4, false);
        let root = scene
            .nodes
            .iter()
            .find(|node| node.person_id == "root")
            .expect("root");
        assert!(
            scene
                .nodes
                .iter()
                .all(|node| node.bounds.min.y <= root.bounds.min.y)
        );
        assert!((root.bounds.center().x).abs() < 0.01, "root is centred");
        let father = scene
            .nodes
            .iter()
            .find(|node| node.person_id == "father")
            .expect("father");
        let mother = scene
            .nodes
            .iter()
            .find(|node| node.person_id == "mother")
            .expect("mother");
        println!(
            "F {} M {} gap {}",
            father.bounds.min.x,
            mother.bounds.min.x,
            mother.bounds.min.x - father.bounds.max.x
        );
        assert!(father.bounds.min.y < root.bounds.min.y);
        assert!(mother.bounds.min.y < root.bounds.min.y);
        assert!(
            (mother.bounds.min.x - father.bounds.min.x - NODE_W - SPOUSE_GAP).abs() < 0.01,
            "parents stay adjacent as a couple"
        );
        assert!(scene.unions.iter().any(|union| union.spouse_line.is_some()));
        assert!(
            !scene.nodes.iter().any(|node| node.person_id == "older"),
            "generation limit applies"
        );
    }

    #[test]
    fn only_ancestry_family_units_produce_connectors() {
        let mut tree = tree();
        let mut unrelated = family("other", "father", "mother");
        unrelated.id = "other".to_string();
        tree.families.push(unrelated);
        let scene = build_pedigree_scene(&tree, "root", 4, false);
        assert!(scene.unions.iter().all(|union| !union.children.is_empty()));
        assert_eq!(scene.unions.len(), 2, "only the two ancestry families draw");
    }

    #[test]
    fn pedigree_layout_is_deterministic_and_duplicate_free() {
        let first = build_pedigree_scene(&tree(), "root", 4, false);
        let second = build_pedigree_scene(&tree(), "root", 4, false);
        assert_eq!(first, second);
        let ids: HashSet<&str> = first
            .nodes
            .iter()
            .map(|node| node.person_id.as_str())
            .collect();
        assert_eq!(ids.len(), first.nodes.len(), "no duplicate person nodes");
    }
}
