//! Deterministic layered layout: turns loaded family data into a
//! pre-computed canvas scene that the paint loop can traverse without
//! allocating.
//!
//! People are grouped into generations (parents above children), couples
//! sit side by side, and each family's children are centred under the
//! couple's union point. Everything is derived from rowid-ordered input, so
//! repeated builds are byte-identical.

use std::collections::{HashMap, HashSet, VecDeque};

use egui::{Pos2, Rect, Vec2, vec2};

use crate::db::{Family, FamilyTree, Person};

/// Node width in canvas units.
pub const NODE_W: f32 = 140.0;
/// Node height in canvas units.
pub const NODE_H: f32 = 56.0;
/// Horizontal gap between couple and sibling nodes.
const H_GAP: f32 = 40.0;
/// Vertical gap between generation rows.
const V_GAP: f32 = 90.0;
/// Character budget for node name labels.
const NAME_MAX_CHARS: usize = 16;

/// A person node positioned in canvas space.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeGeom {
    pub person_id: String,
    pub display_name: String,
    pub bounds: Rect,
}

/// One family's union point and the anchors leading to each child.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionGeom {
    pub family_id: String,
    /// Horizontal bar between the spouses, when both are placed.
    pub spouse_line: Option<[Pos2; 2]>,
    /// Where the drop line to the children starts.
    pub union: Pos2,
    /// `(child_id, top-centre anchor of the child node)`, rowid order.
    pub children: Vec<(String, Pos2)>,
}

/// Pre-computed scene consumed by the paint loop.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeScene {
    pub nodes: Vec<NodeGeom>,
    pub unions: Vec<UnionGeom>,
    /// Canvas-space bounding box of every node, for initial framing.
    pub bounds: Rect,
}

/// Shortens `name` to at most `max_chars` characters with an ellipsis.
pub fn truncate_name(name: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let prefix: String = name.chars().take(max_chars - 1).collect();
    let mut out = prefix.trim_end().to_string();
    out.push('…');
    out
}

fn person_display_name(person: &Person) -> String {
    let full = format!("{} {}", person.given_name, person.surname);
    truncate_name(full.trim(), NAME_MAX_CHARS)
}

/// Builds the canvas scene for `tree`.
pub fn build_scene(tree: &FamilyTree) -> TreeScene {
    let mut builder = LayoutBuilder::new(tree);
    builder.layout();
    builder.finish()
}

struct LayoutBuilder<'a> {
    tree: &'a FamilyTree,
    node_ids: Vec<&'a str>,
    index: HashMap<&'a str, usize>,
    family_by_id: HashMap<&'a str, &'a Family>,
    families_by_spouse: HashMap<&'a str, Vec<usize>>,
    children_by_family: HashMap<&'a str, Vec<&'a str>>,
    generations: Vec<u32>,
    bounds_by_id: HashMap<&'a str, Rect>,
    widths: HashMap<&'a str, f32>,
    width_active: HashSet<&'a str>,
    place_active: HashSet<&'a str>,
}

impl<'a> LayoutBuilder<'a> {
    fn new(tree: &'a FamilyTree) -> Self {
        let node_ids: Vec<&str> = tree
            .people
            .iter()
            .map(|person| person.id.as_str())
            .collect();
        let mut index = HashMap::new();
        let mut families_by_spouse: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut family_by_id = HashMap::new();
        let mut children_by_family: HashMap<&str, Vec<&str>> = HashMap::new();

        for (position, id) in node_ids.iter().enumerate() {
            index.insert(*id, position);
        }
        for (position, family) in tree.families.iter().enumerate() {
            family_by_id.insert(family.id.as_str(), family);
            if let Some(husband) = family.husband_id.as_deref() {
                families_by_spouse
                    .entry(husband)
                    .or_default()
                    .push(position);
            }
            if let Some(wife) = family.wife_id.as_deref() {
                families_by_spouse.entry(wife).or_default().push(position);
            }
        }
        for (family_id, child_id) in &tree.child_links {
            children_by_family
                .entry(family_id.as_str())
                .or_default()
                .push(child_id.as_str());
        }

        let builder = Self {
            tree,
            node_ids,
            index,
            family_by_id,
            families_by_spouse,
            children_by_family,
            generations: Vec::new(),
            bounds_by_id: HashMap::new(),
            widths: HashMap::new(),
            width_active: HashSet::new(),
            place_active: HashSet::new(),
        };
        let generations = builder.compute_generations();
        Self {
            generations,
            ..builder
        }
    }

    /// Kahn pass over parent -> child edges; cycle leftovers stay at
    /// generation 0 instead of hanging the layout.
    fn compute_generations(&self) -> Vec<u32> {
        let count = self.node_ids.len();
        let mut generations = vec![0u32; count];
        let mut pending = vec![0u32; count];
        let mut children_of: HashMap<usize, Vec<usize>> = HashMap::new();

        for (family_id, child_id) in &self.tree.child_links {
            let Some(&child_pos) = self.index.get(child_id.as_str()) else {
                continue;
            };
            let Some(family) = self.family_by_id.get(family_id.as_str()) else {
                continue;
            };
            for parent in [family.husband_id.as_deref(), family.wife_id.as_deref()]
                .into_iter()
                .flatten()
            {
                let Some(&parent_pos) = self.index.get(parent) else {
                    continue;
                };
                pending[child_pos] += 1;
                children_of.entry(parent_pos).or_default().push(child_pos);
            }
        }

        let mut queue: VecDeque<usize> = (0..count)
            .filter(|position| pending[*position] == 0)
            .collect();
        while let Some(parent_pos) = queue.pop_front() {
            let parent_generation = generations[parent_pos];
            for &child_pos in children_of.get(&parent_pos).into_iter().flatten() {
                generations[child_pos] = generations[child_pos].max(parent_generation + 1);
                pending[child_pos] -= 1;
                if pending[child_pos] == 0 {
                    queue.push_back(child_pos);
                }
            }
        }
        generations
    }

    fn y_for(&self, id: &str) -> f32 {
        self.index
            .get(id)
            .and_then(|&position| self.generations.get(position))
            .copied()
            .unwrap_or(0) as f32
            * (NODE_H + V_GAP)
    }

    fn first_family(&self, id: &'a str) -> Option<&'a Family> {
        self.families_by_spouse
            .get(id)
            .and_then(|positions| positions.first())
            .and_then(|&position| self.tree.families.get(position))
    }

    fn partner_of(family: &'a Family, id: &str) -> Option<&'a str> {
        if family.husband_id.as_deref() == Some(id) {
            family.wife_id.as_deref()
        } else if family.wife_id.as_deref() == Some(id) {
            family.husband_id.as_deref()
        } else {
            None
        }
    }

    fn children_of_first_family(&self, id: &'a str) -> Vec<&'a str> {
        self.first_family(id)
            .and_then(|family| self.children_by_family.get(family.id.as_str()))
            .cloned()
            .unwrap_or_default()
    }

    /// Subtree width: the couple block or the children row, whichever is
    /// wider. Cycle back-edges return a plain node width without memoising.
    fn width_of(&mut self, id: &'a str) -> f32 {
        if let Some(&width) = self.widths.get(id) {
            return width;
        }
        if !self.width_active.insert(id) {
            return NODE_W;
        }
        let couple_w = match self.first_family(id) {
            Some(family) if Self::partner_of(family, id).is_some() => NODE_W * 2.0 + H_GAP,
            _ => NODE_W,
        };
        let children = self.children_of_first_family(id);
        let mut children_w = 0.0;
        for (position, child) in children.iter().enumerate() {
            if position > 0 {
                children_w += H_GAP;
            }
            children_w += self.width_of(child);
        }
        self.width_active.remove(id);
        let total = couple_w.max(children_w);
        self.widths.insert(id, total);
        total
    }

    fn insert_bounds(&mut self, id: &'a str, rect: Rect) {
        if !self.bounds_by_id.contains_key(id) {
            self.bounds_by_id.insert(id, rect);
        }
    }

    /// Places `id` (and their partner) at `x_left`, then centres the
    /// children row under the couple. Returns the occupied width.
    fn place(&mut self, id: &'a str, x_left: f32) -> f32 {
        if self.bounds_by_id.contains_key(id) {
            return self.widths.get(id).copied().unwrap_or(NODE_W);
        }
        if !self.place_active.insert(id) {
            return self.widths.get(id).copied().unwrap_or(NODE_W);
        }
        let total = self.width_of(id);
        let y = self.y_for(id);
        let first_family = self.first_family(id);
        let partner = first_family.and_then(|family| Self::partner_of(family, id));

        if let Some(partner) = partner {
            self.insert_bounds(
                id,
                Rect::from_min_size(Pos2::new(x_left, y), vec2(NODE_W, NODE_H)),
            );
            let partner_x = x_left + NODE_W + H_GAP;
            self.insert_bounds(
                partner,
                Rect::from_min_size(
                    Pos2::new(partner_x, self.y_for(partner)),
                    vec2(NODE_W, NODE_H),
                ),
            );
            self.widths.insert(partner, NODE_W * 2.0 + H_GAP);
        } else {
            self.insert_bounds(
                id,
                Rect::from_min_size(Pos2::new(x_left, y), vec2(NODE_W, NODE_H)),
            );
        }

        let children = self.children_of_first_family(id);
        let mut children_w = 0.0;
        for (position, child) in children.iter().enumerate() {
            if position > 0 {
                children_w += H_GAP;
            }
            children_w += self.widths.get(child).copied().unwrap_or(NODE_W);
        }
        let mut child_x = x_left + (total - children_w) / 2.0;
        for child in children {
            let width = self.place(child, child_x);
            child_x += width + H_GAP;
        }

        self.place_active.remove(id);
        total
    }

    fn layout(&mut self) {
        let mut cursor = 0.0;
        for id in self.node_ids.clone() {
            let position = self.index.get(id).copied().unwrap_or(0);
            let generation = self.generations.get(position).copied().unwrap_or(0);
            if generation != 0 || self.bounds_by_id.contains_key(id) {
                continue;
            }
            cursor += self.place(id, cursor) + H_GAP;
        }
        // Cycles and unreachable people fall back to the end of the row.
        for id in self.node_ids.clone() {
            if self.bounds_by_id.contains_key(id) {
                continue;
            }
            cursor += self.place(id, cursor) + H_GAP;
        }
    }

    fn build_unions(&self) -> Vec<UnionGeom> {
        let mut unions = Vec::new();
        for family in &self.tree.families {
            let husband = family
                .husband_id
                .as_deref()
                .and_then(|id| self.bounds_by_id.get(id));
            let wife = family
                .wife_id
                .as_deref()
                .and_then(|id| self.bounds_by_id.get(id));
            let (spouse_line, union) = match (husband, wife) {
                (Some(husband), Some(wife)) => {
                    let (first, second) = if husband.center().x <= wife.center().x {
                        (husband.right_center(), wife.left_center())
                    } else {
                        (wife.right_center(), husband.left_center())
                    };
                    let union = first + (second - first) * 0.5;
                    (Some([first, second]), union)
                }
                (Some(solo), None) => (None, solo.center_bottom()),
                (None, Some(solo)) => (None, solo.center_bottom()),
                (None, None) => continue,
            };
            let children = self
                .children_by_family
                .get(family.id.as_str())
                .into_iter()
                .flatten()
                .filter_map(|child_id| {
                    let bounds = self.bounds_by_id.get(child_id)?;
                    let anchor = Pos2::new(bounds.center().x, bounds.min.y);
                    Some((child_id.to_string(), anchor))
                })
                .collect();
            unions.push(UnionGeom {
                family_id: family.id.clone(),
                spouse_line,
                union,
                children,
            });
        }
        unions
    }

    fn finish(self) -> TreeScene {
        let mut nodes = Vec::with_capacity(self.node_ids.len());
        for id in &self.node_ids {
            let Some(bounds) = self.bounds_by_id.get(id) else {
                continue;
            };
            let Some(person) = self
                .index
                .get(id)
                .and_then(|&position| self.tree.people.get(position))
            else {
                continue;
            };
            nodes.push(NodeGeom {
                person_id: person.id.clone(),
                display_name: person_display_name(person),
                bounds: *bounds,
            });
        }
        let unions = self.build_unions();
        let bounds = nodes
            .iter()
            .fold(Rect::from_min_size(Pos2::ZERO, Vec2::ZERO), |acc, node| {
                acc.union(node.bounds)
            });
        TreeScene {
            nodes,
            unions,
            bounds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: &str, given: &str, surname: &str) -> Person {
        Person {
            id: id.to_string(),
            given_name: given.to_string(),
            surname: surname.to_string(),
            gender: "U".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn family(id: &str, husband: Option<&str>, wife: Option<&str>) -> Family {
        Family {
            id: id.to_string(),
            husband_id: husband.map(str::to_string),
            wife_id: wife.map(str::to_string),
            created_at: String::new(),
        }
    }

    /// Johan + Maria with two children, plus an unrelated single person.
    fn fixture() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Elsa", "Ahlberg"),
                person("i4", "Nils", "Ahlberg"),
                person("i5", "Oskar", "Friis"),
            ],
            families: vec![family("f1", Some("i1"), Some("i2"))],
            child_links: vec![
                ("f1".to_string(), "i3".to_string()),
                ("f1".to_string(), "i4".to_string()),
            ],
        }
    }

    fn node<'a>(scene: &'a TreeScene, id: &str) -> &'a NodeGeom {
        scene
            .nodes
            .iter()
            .find(|node| node.person_id == id)
            .unwrap_or_else(|| panic!("missing node {id}"))
    }

    #[test]
    fn children_sit_below_their_parents() {
        let scene = build_scene(&fixture());
        let father = node(&scene, "i1");
        let mother = node(&scene, "i2");
        for child in ["i3", "i4"] {
            let child = node(&scene, child);
            assert!(
                child.bounds.min.y > father.bounds.max.y,
                "child must be below the father"
            );
            assert!(
                child.bounds.min.y > mother.bounds.max.y,
                "child must be below the mother"
            );
        }
    }

    #[test]
    fn spouses_are_adjacent_and_children_centred() {
        let scene = build_scene(&fixture());
        let father = node(&scene, "i1");
        let mother = node(&scene, "i2");
        assert!(
            (mother.bounds.min.x - father.bounds.max.x - H_GAP).abs() < 1e-3,
            "spouses are separated by exactly H_GAP"
        );

        let left = node(&scene, "i3").bounds.min.x;
        let right = node(&scene, "i4").bounds.max.x;
        let children_centre = (left + right) / 2.0;
        let couple_centre = (father.bounds.min.x + mother.bounds.max.x) / 2.0;
        assert!(
            (children_centre - couple_centre).abs() < 1e-3,
            "children row must be centred under the couple"
        );
    }

    #[test]
    fn layout_is_deterministic() {
        let tree = fixture();
        let first = build_scene(&tree);
        let second = build_scene(&tree);
        assert_eq!(first, second, "repeated builds must be identical");
        assert_eq!(first.nodes.len(), 5);
        assert_eq!(first.unions.len(), 1);
    }

    #[test]
    fn unions_carry_spouse_bar_and_child_anchors() {
        let scene = build_scene(&fixture());
        let union = scene
            .unions
            .first()
            .unwrap_or_else(|| panic!("union must exist"));
        let line = union
            .spouse_line
            .unwrap_or_else(|| panic!("spouse bar must exist"));
        let [bar_start, bar_end] = line;
        let midpoint = bar_start + (bar_end - bar_start) * 0.5;
        assert_eq!(
            union.union, midpoint,
            "union point is the spouse-bar midpoint"
        );
        assert_eq!(union.children.len(), 2);
        for (child_id, anchor) in &union.children {
            let bounds = node(&scene, child_id).bounds;
            assert_eq!(*anchor, Pos2::new(bounds.center().x, bounds.min.y));
        }
    }

    #[test]
    fn single_parent_unions_drop_from_the_node_bottom() {
        let mut tree = fixture();
        tree.families = vec![family("f1", Some("i1"), None)];
        let scene = build_scene(&tree);
        let union = scene
            .unions
            .first()
            .unwrap_or_else(|| panic!("union must exist"));
        assert!(union.spouse_line.is_none());
        let parent = node(&scene, "i1").bounds;
        assert_eq!(union.union, parent.center_bottom());
    }

    #[test]
    fn empty_tree_yields_empty_scene() {
        let scene = build_scene(&FamilyTree::default());
        assert!(scene.nodes.is_empty());
        assert!(scene.unions.is_empty());
    }

    #[test]
    fn layout_survives_cycles() {
        let tree = FamilyTree {
            people: vec![person("i1", "Self", "Loop")],
            families: vec![family("f1", Some("i1"), None)],
            child_links: vec![("f1".to_string(), "i1".to_string())],
        };
        let scene = build_scene(&tree);
        assert_eq!(scene.nodes.len(), 1, "cyclic data must not hang");
        assert_eq!(scene.unions.len(), 1);
    }

    #[test]
    fn names_are_truncated_with_ellipsis() {
        assert_eq!(truncate_name("Elsa", 16), "Elsa");
        assert_eq!(truncate_name("", 16), "");
        assert_eq!(
            truncate_name("Anna Maria von Rosenfelt", 16),
            "Anna Maria von…"
        );
        assert!(
            truncate_name("Anna Maria von Rosenfelt", 16)
                .chars()
                .count()
                <= 16,
            "truncated names stay within the budget"
        );
        assert_eq!(truncate_name("anything", 0), "");
        assert_eq!(
            person_display_name(&person("i1", "Anna Maria", "von Rosenfelt")),
            "Anna Maria von…"
        );
    }
}
