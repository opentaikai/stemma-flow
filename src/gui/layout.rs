//! Deterministic layered layout: turns loaded family data into a
//! pre-computed canvas scene that the paint loop can traverse without
//! allocating.
//!
//! People joined by a marriage form one compound and share a single rank,
//! remarriages extend the compound into a spouse chain with one union gap
//! per family, and each family's children row is packed under its own
//! union point. Everything is derived from rowid-ordered input, so
//! repeated builds are byte-identical.

use std::collections::{HashMap, HashSet, VecDeque};

use egui::{Pos2, Rect, Vec2, vec2};

use crate::db::{Family, FamilyTree, Person};

/// Node width in canvas units.
pub const NODE_W: f32 = 140.0;
/// Node height in canvas units.
pub const NODE_H: f32 = 56.0;
/// Horizontal gap between adjacent nodes of the same rank.
const H_GAP: f32 = 40.0;
/// Vertical gap between two ranks (spec floor: 120).
const V_GAP: f32 = 120.0;
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
    let full = [
        person.given_name.as_str(),
        person.middle_name.as_str(),
        person.surname.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    truncate_name(full.trim(), NAME_MAX_CHARS)
}

/// Builds the canvas scene for `tree`.
pub fn build_scene(tree: &FamilyTree) -> TreeScene {
    let mut builder = LayoutBuilder::new(tree);
    builder.layout();
    builder.finish()
}

/// Width of `count` nodes laid out with [`H_GAP`] between them.
fn row_width(count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        count as f32 * NODE_W + (count - 1) as f32 * H_GAP
    }
}

/// Centre of the gap that follows chain slot `slot`.
fn gap_centre(slot: usize) -> f32 {
    slot as f32 * (NODE_W + H_GAP) + NODE_W + H_GAP * 0.5
}

/// Centre of the node sitting in chain slot `slot`.
fn slot_centre(slot: usize) -> f32 {
    slot as f32 * (NODE_W + H_GAP) + NODE_W * 0.5
}

/// One rank unit: people joined by marriages, drawn as a spouse chain.
#[derive(Debug)]
struct Compound<'a> {
    /// Every member, in person rowid order.
    members: Vec<&'a str>,
    /// Family indices of this compound, in rowid order.
    families: Vec<usize>,
    /// Layer index; both spouses always share it.
    rank: u32,
}

/// Horizontal packing of one compound, relative to its left edge.
#[derive(Debug, Clone)]
struct BlockLayout {
    /// Left edge of the chain within the block.
    chain_x: f32,
    /// `(family index, group left edge)`, ordered by union position.
    groups: Vec<(usize, f32)>,
    /// Total width of the packed block.
    width: f32,
}

struct LayoutBuilder<'a> {
    tree: &'a FamilyTree,
    node_ids: Vec<&'a str>,
    index: HashMap<&'a str, usize>,
    family_by_id: HashMap<&'a str, &'a Family>,
    children_by_family: HashMap<&'a str, Vec<&'a str>>,
    compounds: Vec<Compound<'a>>,
    compound_of: HashMap<&'a str, usize>,
    chains: Vec<Vec<&'a str>>,
    blocks: HashMap<usize, BlockLayout>,
    block_active: HashSet<usize>,
    place_active: HashSet<usize>,
    placed: HashSet<usize>,
    bounds_by_id: HashMap<&'a str, Rect>,
    /// Family id -> union x, recorded while its chain is placed.
    union_x: HashMap<&'a str, f32>,
    /// Compounds without parents, in compound order.
    roots: Vec<usize>,
}

impl<'a> LayoutBuilder<'a> {
    fn new(tree: &'a FamilyTree) -> Self {
        let node_ids: Vec<&str> = tree
            .people
            .iter()
            .map(|person| person.id.as_str())
            .collect();
        let mut index = HashMap::with_capacity(node_ids.len());
        for (position, id) in node_ids.iter().enumerate() {
            index.insert(*id, position);
        }
        let mut family_by_id = HashMap::with_capacity(tree.families.len());
        for family in &tree.families {
            family_by_id.insert(family.id.as_str(), family);
        }
        let mut children_by_family: HashMap<&str, Vec<&str>> = HashMap::new();
        for (family_id, child_id) in &tree.child_links {
            children_by_family
                .entry(family_id.as_str())
                .or_default()
                .push(child_id.as_str());
        }

        let (compounds, compound_of) = Self::compute_compounds(tree, &index);
        let chains = compounds
            .iter()
            .map(|compound| Self::build_chain(tree, &index, compound))
            .collect();
        let mut builder = Self {
            tree,
            node_ids,
            index,
            family_by_id,
            children_by_family,
            compounds,
            compound_of,
            chains,
            blocks: HashMap::new(),
            block_active: HashSet::new(),
            place_active: HashSet::new(),
            placed: HashSet::new(),
            bounds_by_id: HashMap::new(),
            union_x: HashMap::new(),
            roots: Vec::new(),
        };
        builder.compute_ranks();
        builder
    }

    /// Groups people into marriage compounds by flood-filling spouse
    /// links, so both spouses of every family share one rank. Components
    /// surface in first-member rowid order and members stay rowid-ordered.
    fn compute_compounds(
        tree: &'a FamilyTree,
        index: &HashMap<&'a str, usize>,
    ) -> (Vec<Compound<'a>>, HashMap<&'a str, usize>) {
        let count = tree.people.len();
        let mut spouse_links: HashMap<usize, Vec<usize>> = HashMap::new();
        for family in &tree.families {
            let husband = family
                .husband_id
                .as_deref()
                .filter(|id| index.contains_key(*id))
                .and_then(|id| index.get(id))
                .copied();
            let wife = family
                .wife_id
                .as_deref()
                .filter(|id| index.contains_key(*id))
                .and_then(|id| index.get(id))
                .copied();
            if let (Some(husband), Some(wife)) = (husband, wife) {
                spouse_links.entry(husband).or_default().push(wife);
                spouse_links.entry(wife).or_default().push(husband);
            }
        }

        let mut component_of: Vec<Option<usize>> = vec![None; count];
        let mut component_count = 0usize;
        for person in 0..count {
            if component_of.get(person).copied().flatten().is_some() {
                continue;
            }
            let mut stack = vec![person];
            if let Some(slot) = component_of.get_mut(person) {
                *slot = Some(component_count);
            }
            component_count += 1;
            while let Some(person) = stack.pop() {
                for &partner in spouse_links.get(&person).into_iter().flatten() {
                    if component_of.get(partner).copied().flatten().is_none() {
                        if let Some(slot) = component_of.get_mut(partner) {
                            *slot = Some(component_count - 1);
                        }
                        stack.push(partner);
                    }
                }
            }
        }

        let mut compounds: Vec<Compound<'a>> = (0..component_count)
            .map(|_| Compound {
                members: Vec::new(),
                families: Vec::new(),
                rank: 0,
            })
            .collect();
        let mut compound_of_ids = HashMap::with_capacity(count);
        for (position, component) in component_of.iter().enumerate() {
            let Some(component) = component else {
                continue;
            };
            let Some(person) = tree.people.get(position) else {
                continue;
            };
            if let Some(compound) = compounds.get_mut(*component) {
                compound.members.push(person.id.as_str());
            }
            compound_of_ids.insert(person.id.as_str(), *component);
        }

        let component_of_id = |id: Option<&str>| -> Option<usize> {
            id.and_then(|id| index.get(id))
                .and_then(|&position| component_of.get(position))
                .copied()
                .flatten()
        };
        for (position, family) in tree.families.iter().enumerate() {
            let component = component_of_id(family.husband_id.as_deref())
                .or_else(|| component_of_id(family.wife_id.as_deref()));
            let Some(component) = component else {
                continue;
            };
            if let Some(compound) = compounds.get_mut(component) {
                compound.families.push(position);
            }
        }
        (compounds, compound_of_ids)
    }

    /// Both spouses when each is present in the tree.
    fn chainable(
        family: &'a Family,
        index: &HashMap<&'a str, usize>,
    ) -> Option<(&'a str, &'a str)> {
        let husband = family
            .husband_id
            .as_deref()
            .filter(|id| index.contains_key(*id))?;
        let wife = family
            .wife_id
            .as_deref()
            .filter(|id| index.contains_key(*id))?;
        Some((husband, wife))
    }

    /// Seats an unknown partner next to `anchor`: right when the chain
    /// ends there, left when it starts there, otherwise pushed to the far
    /// end so existing adjacencies survive polygamy.
    fn insert_partner(chain: &mut Vec<&'a str>, anchor: usize, partner: &'a str) {
        if anchor + 1 == chain.len() {
            chain.insert(anchor + 1, partner);
        } else if anchor == 0 {
            chain.insert(0, partner);
        } else {
            chain.push(partner);
        }
    }

    /// Lays the compound's members out as one chain: the first family
    /// seeds the pair, later families insert their partner next to the
    /// spouse already seated. Passes repeat until nobody moves, then any
    /// remaining member is appended defensively.
    fn build_chain(
        tree: &'a FamilyTree,
        index: &HashMap<&'a str, usize>,
        compound: &Compound<'a>,
    ) -> Vec<&'a str> {
        let mut chain: Vec<&'a str> = Vec::with_capacity(compound.members.len());
        for &position in &compound.families {
            let Some(family) = tree.families.get(position) else {
                continue;
            };
            let Some((husband, wife)) = Self::chainable(family, index) else {
                continue;
            };
            chain.push(husband);
            chain.push(wife);
            break;
        }

        let mut changed = true;
        while changed {
            changed = false;
            for &position in &compound.families {
                let Some(family) = tree.families.get(position) else {
                    continue;
                };
                let Some((husband, wife)) = Self::chainable(family, index) else {
                    continue;
                };
                let husband_at = chain.iter().position(|member| *member == husband);
                let wife_at = chain.iter().position(|member| *member == wife);
                match (husband_at, wife_at) {
                    (Some(_), Some(_)) => {}
                    (Some(at), None) => {
                        Self::insert_partner(&mut chain, at, wife);
                        changed = true;
                    }
                    (None, Some(at)) => {
                        Self::insert_partner(&mut chain, at, husband);
                        changed = true;
                    }
                    (None, None) => {
                        chain.push(husband);
                        chain.push(wife);
                        changed = true;
                    }
                }
            }
        }
        for &member in &compound.members {
            if !chain.contains(&member) {
                chain.push(member);
            }
        }
        chain
    }

    /// Kahn pass over compound -> compound edges; cycle leftovers keep
    /// rank 0 instead of hanging the layout. Compounds without parents
    /// become the placement roots.
    fn compute_ranks(&mut self) {
        let tree = self.tree;
        let mut pending = vec![0u32; self.compounds.len()];
        let mut children_of: HashMap<usize, Vec<usize>> = HashMap::new();
        for (family_id, child_id) in &tree.child_links {
            let Some(child) = self.compound_of.get(child_id.as_str()).copied() else {
                continue;
            };
            let Some(family) = self.family_by_id.get(family_id.as_str()) else {
                continue;
            };
            for parent in [family.husband_id.as_deref(), family.wife_id.as_deref()]
                .into_iter()
                .flatten()
            {
                let Some(parent_compound) = self.compound_of.get(parent).copied() else {
                    continue;
                };
                if let Some(slot) = pending.get_mut(child) {
                    *slot += 1;
                }
                children_of.entry(parent_compound).or_default().push(child);
            }
        }

        let mut roots = Vec::new();
        let mut queue: VecDeque<usize> = VecDeque::new();
        for compound in 0..self.compounds.len() {
            if pending.get(compound).copied().unwrap_or(0) == 0 {
                roots.push(compound);
                queue.push_back(compound);
            }
        }
        while let Some(parent) = queue.pop_front() {
            let parent_rank = self
                .compounds
                .get(parent)
                .map(|compound| compound.rank)
                .unwrap_or(0);
            for child in children_of.get(&parent).into_iter().flatten().copied() {
                if let Some(compound) = self.compounds.get_mut(child) {
                    compound.rank = compound.rank.max(parent_rank + 1);
                }
                if let Some(slot) = pending.get_mut(child) {
                    *slot = slot.saturating_sub(1);
                    if *slot == 0 {
                        queue.push_back(child);
                    }
                }
            }
        }
        self.roots = roots;
    }

    fn rank_y(&self, compound: usize) -> f32 {
        self.compounds
            .get(compound)
            .map(|compound| compound.rank)
            .unwrap_or(0) as f32
            * (NODE_H + V_GAP)
    }

    fn children_of_family(&self, family: usize) -> Vec<&'a str> {
        self.tree
            .families
            .get(family)
            .and_then(|family| self.children_by_family.get(family.id.as_str()))
            .cloned()
            .unwrap_or_default()
    }

    /// Chain-relative x of a family's union: the gap centre between the
    /// spouses (nearest to their midpoint when remarriage separates them,
    /// ties resolved to the later gap), or the member centre for a
    /// single-parent family.
    fn union_offset(&self, compound: usize, family: usize) -> f32 {
        let Some(chain) = self.chains.get(compound) else {
            return 0.0;
        };
        let Some(family) = self.tree.families.get(family) else {
            return 0.0;
        };
        let husband = family
            .husband_id
            .as_deref()
            .and_then(|id| chain.iter().position(|member| *member == id));
        let wife = family
            .wife_id
            .as_deref()
            .and_then(|id| chain.iter().position(|member| *member == id));
        match (husband, wife) {
            (Some(husband), Some(wife)) => {
                let (low, high) = if husband <= wife {
                    (husband, wife)
                } else {
                    (wife, husband)
                };
                if high - low == 1 {
                    return gap_centre(low);
                }
                let midpoint = (slot_centre(husband) + slot_centre(wife)) * 0.5;
                let mut best = gap_centre(low);
                let mut best_distance = (best - midpoint).abs();
                for gap in low..high {
                    let centre = gap_centre(gap);
                    let distance = (centre - midpoint).abs();
                    if distance <= best_distance {
                        best = centre;
                        best_distance = distance;
                    }
                }
                best
            }
            (Some(position), None) | (None, Some(position)) => slot_centre(position),
            (None, None) => 0.0,
        }
    }

    /// Packed horizontal layout of one compound: the chain plus every
    /// family's children row, ordered by union position and pushed right
    /// when ideal centring would overlap the previous group.
    fn compute_block(&mut self, compound: usize) -> BlockLayout {
        let chain_width = self
            .chains
            .get(compound)
            .map(|chain| row_width(chain.len()))
            .unwrap_or(0.0);
        let families = self
            .compounds
            .get(compound)
            .map(|compound| compound.families.clone())
            .unwrap_or_default();

        let mut groups = Vec::with_capacity(families.len());
        for family in families {
            let mut children_width = 0.0;
            for child in self.children_of_family(family) {
                let Some(&child_compound) = self.compound_of.get(child) else {
                    continue;
                };
                let width = self.block_of(child_compound).width;
                if children_width > 0.0 {
                    children_width += H_GAP;
                }
                children_width += width;
            }
            let union = self.union_offset(compound, family);
            groups.push((family, union, children_width));
        }
        groups.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });

        let mut packed = Vec::with_capacity(groups.len());
        let mut previous_end = f32::NEG_INFINITY;
        let mut low: f32 = 0.0;
        let mut high = chain_width;
        for (family, union, children_width) in groups {
            let ideal = union - children_width * 0.5;
            let start = ideal.max(previous_end + H_GAP);
            previous_end = start + children_width;
            low = low.min(start);
            high = high.max(previous_end);
            packed.push((family, start));
        }
        let chain_x = -low;
        BlockLayout {
            chain_x,
            groups: packed
                .into_iter()
                .map(|(family, start)| (family, start + chain_x))
                .collect(),
            width: high - low,
        }
    }

    /// Memoised [`Self::compute_block`]; compounds already being measured
    /// (a cycle) fall back to their bare chain so recursion terminates.
    fn block_of(&mut self, compound: usize) -> BlockLayout {
        if let Some(block) = self.blocks.get(&compound) {
            return block.clone();
        }
        if !self.block_active.insert(compound) {
            let width = self
                .chains
                .get(compound)
                .map(|chain| row_width(chain.len()))
                .unwrap_or(0.0);
            return BlockLayout {
                chain_x: 0.0,
                groups: Vec::new(),
                width,
            };
        }
        let block = self.compute_block(compound);
        self.block_active.remove(&compound);
        self.blocks.insert(compound, block.clone());
        block
    }

    fn insert_bounds(&mut self, id: &'a str, rect: Rect) {
        if !self.bounds_by_id.contains_key(id) {
            self.bounds_by_id.insert(id, rect);
        }
    }

    /// Places a compound's chain at `left` and packs each family's
    /// children row under its union. Placed and in-progress compounds are
    /// skipped so shared children and cycles terminate.
    fn place_compound(&mut self, compound: usize, left: f32) {
        if self.placed.contains(&compound) || !self.place_active.insert(compound) {
            return;
        }
        let block = self.block_of(compound);
        let chain = self.chains.get(compound).cloned().unwrap_or_default();
        let families = self
            .compounds
            .get(compound)
            .map(|compound| compound.families.clone())
            .unwrap_or_default();
        let y = self.rank_y(compound);

        for (slot, id) in chain.iter().enumerate() {
            let x = left + block.chain_x + slot as f32 * (NODE_W + H_GAP);
            self.insert_bounds(
                id,
                Rect::from_min_size(Pos2::new(x, y), vec2(NODE_W, NODE_H)),
            );
        }
        for &family in &families {
            let union = left + block.chain_x + self.union_offset(compound, family);
            if let Some(id) = self
                .tree
                .families
                .get(family)
                .map(|family| family.id.as_str())
            {
                self.union_x.insert(id, union);
            }
        }
        for &(family, group_x) in &block.groups {
            let mut child_x = left + group_x;
            for child in self.children_of_family(family) {
                let Some(&child_compound) = self.compound_of.get(child) else {
                    continue;
                };
                let width = self.block_of(child_compound).width;
                self.place_compound(child_compound, child_x);
                child_x += width + H_GAP;
            }
        }

        self.place_active.remove(&compound);
        self.placed.insert(compound);
    }

    fn layout(&mut self) {
        let roots = self.roots.clone();
        let mut cursor = 0.0;
        for compound in roots {
            let width = self.block_of(compound).width;
            self.place_compound(compound, cursor);
            cursor += width + H_GAP;
        }
        // Cycles and compounds behind them fall back to the end of the row.
        for compound in 0..self.compounds.len() {
            if self.placed.contains(&compound) {
                continue;
            }
            let width = self.block_of(compound).width;
            self.place_compound(compound, cursor);
            cursor += width + H_GAP;
        }
    }

    fn build_unions(&self) -> Vec<UnionGeom> {
        let mut unions = Vec::with_capacity(self.tree.families.len());
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
                    // Spouses seated in different chain gaps (remarriage)
                    // get no bar: it would cut across the nodes between.
                    let adjacent = (second.x - first.x - H_GAP).abs() < 0.5;
                    let spouse_line = adjacent.then_some([first, second]);
                    let union = if adjacent {
                        first + (second - first) * 0.5
                    } else {
                        let x = self
                            .union_x
                            .get(family.id.as_str())
                            .copied()
                            .unwrap_or((first.x + second.x) * 0.5);
                        Pos2::new(x, husband.center().y)
                    };
                    (spouse_line, union)
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
            .map(|node| node.bounds)
            .reduce(Rect::union)
            .unwrap_or_else(|| Rect::from_min_size(Pos2::ZERO, Vec2::ZERO));
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
            middle_name: String::new(),
            surname: surname.to_string(),
            nickname: String::new(),
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

    fn union<'a>(scene: &'a TreeScene, family_id: &str) -> &'a UnionGeom {
        scene
            .unions
            .iter()
            .find(|union| union.family_id == family_id)
            .unwrap_or_else(|| panic!("missing union {family_id}"))
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
        let mut with_middle = person("i2", "Ana", "Lee");
        with_middle.middle_name = "Mar".to_string();
        assert_eq!(
            person_display_name(&with_middle),
            "Ana Mar Lee",
            "canvas labels include the middle name"
        );
    }

    #[test]
    fn cross_generation_couple_shares_one_rank() {
        // i1 + i2 -> i3; i3 marries i4 -> i5. The married-in spouse must
        // not stay on the root rank next to the grandparents.
        let tree = FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Elsa", "Ahlberg"),
                person("i4", "Nils", "Friis"),
                person("i5", "Otto", "Friis"),
            ],
            families: vec![
                family("f1", Some("i1"), Some("i2")),
                family("f2", Some("i3"), Some("i4")),
            ],
            child_links: vec![
                ("f1".to_string(), "i3".to_string()),
                ("f2".to_string(), "i5".to_string()),
            ],
        };
        let scene = build_scene(&tree);
        let child = node(&scene, "i3");
        let spouse = node(&scene, "i4");
        assert_eq!(
            child.bounds.min.y, spouse.bounds.min.y,
            "both spouses sit on the same rank"
        );
        let grandparent = node(&scene, "i1");
        assert_eq!(
            child.bounds.min.y - grandparent.bounds.min.y,
            NODE_H + V_GAP,
            "the couple sits exactly one rank below the grandparents"
        );
        assert_eq!(
            node(&scene, "i5").bounds.min.y - child.bounds.min.y,
            NODE_H + V_GAP,
            "the next generation follows one rank below"
        );
    }

    #[test]
    fn second_family_children_stay_in_the_block() {
        // i1 remarries: f1(i1, i2) -> i4, f2(i3, i1) -> i5. Both children
        // belong under their own family's union, inside the chain block.
        let tree = FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Karin", "Vik"),
                person("i4", "Elsa", "Ahlberg"),
                person("i5", "Nils", "Ahlberg"),
            ],
            families: vec![
                family("f1", Some("i1"), Some("i2")),
                family("f2", Some("i3"), Some("i1")),
            ],
            child_links: vec![
                ("f1".to_string(), "i4".to_string()),
                ("f2".to_string(), "i5".to_string()),
            ],
        };
        let scene = build_scene(&tree);
        let first = node(&scene, "i1").bounds;
        let second = node(&scene, "i2").bounds;
        let third = node(&scene, "i3").bounds;
        assert!(
            third.center().x < first.center().x && first.center().x < second.center().x,
            "chain order is i3, i1, i2"
        );

        let first_child = node(&scene, "i4").bounds;
        let second_child = node(&scene, "i5").bounds;
        assert_eq!(
            first_child.min.y, second_child.min.y,
            "both children rows share the child rank"
        );
        assert!(
            (first_child.center().x - union(&scene, "f1").union.x).abs() < 1e-3,
            "f1 children centred under the f1 union"
        );
        assert!(
            (second_child.center().x - union(&scene, "f2").union.x).abs() < 1e-3,
            "f2 children centred under the f2 union"
        );
        assert!(
            first_child.min.x >= second_child.max.x + H_GAP - 1e-3,
            "the two children rows keep at least H_GAP apart"
        );
    }

    #[test]
    fn spacing_constants_meet_spec_minimums() {
        let scene = build_scene(&fixture());
        let parent = node(&scene, "i1").bounds;
        let mother = node(&scene, "i2").bounds;
        let child = node(&scene, "i3").bounds;
        assert_eq!(
            child.min.y - parent.min.y,
            NODE_H + V_GAP,
            "row pitch is NODE_H + V_GAP"
        );
        assert!(
            child.min.y - parent.max.y >= 120.0 - 1e-6,
            "generations are at least 120 apart"
        );
        assert!(
            mother.min.x - parent.max.x >= 40.0 - 1e-6,
            "spouses are at least 40 apart"
        );
    }

    #[test]
    fn polygamy_skips_the_crossing_spouse_bar() {
        // i1 has three spouses; the third marriage is not adjacent on the
        // chain, so its bar would cut across the nodes in between.
        let tree = FamilyTree {
            people: vec![
                person("i1", "Johan", "Ahlberg"),
                person("i2", "Maria", "Ekdahl"),
                person("i3", "Karin", "Vik"),
                person("i4", "Ingrid", "Holm"),
            ],
            families: vec![
                family("f1", Some("i1"), Some("i2")),
                family("f2", Some("i1"), Some("i3")),
                family("f3", Some("i1"), Some("i4")),
            ],
            child_links: Vec::new(),
        };
        let scene = build_scene(&tree);
        assert!(
            union(&scene, "f1").spouse_line.is_some(),
            "the first marriage keeps its bar"
        );
        assert!(
            union(&scene, "f2").spouse_line.is_some(),
            "the second marriage sits adjacent and keeps its bar"
        );
        assert!(
            union(&scene, "f3").spouse_line.is_none(),
            "the far-apart marriage skips the crossing bar"
        );
        let third_union = union(&scene, "f3").union.x;
        for id in ["i1", "i2", "i3", "i4"] {
            let bounds = node(&scene, id).bounds;
            assert!(
                third_union <= bounds.min.x || third_union >= bounds.max.x,
                "the union sits in a gap, not inside node {id}"
            );
        }
    }
}
