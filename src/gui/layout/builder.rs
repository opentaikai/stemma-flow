//! The layout builder: ranks, chains, packing and scene assembly.

use std::collections::HashMap;

use egui::{Pos2, Rect, Vec2};

use crate::db::{Family, FamilyTree};

use super::compounds::Compound;
use super::graph::compute_person_components;
use super::scene::{NodeGeom, TreeScene, UnionGeom, person_display_name};
use super::{
    NODE_H, RANK_PITCH, SPOUSE_GAP, TRUNK_BOTTOM_MARGIN, TRUNK_LANE_SPACING, TRUNK_TOP_MARGIN,
    V_GAP,
};

pub(super) struct LayoutBuilder<'a> {
    pub(super) tree: &'a FamilyTree,
    pub(super) node_ids: Vec<&'a str>,
    pub(super) index: HashMap<&'a str, usize>,
    pub(super) family_by_id: HashMap<&'a str, &'a Family>,
    pub(super) children_by_family: HashMap<&'a str, Vec<&'a str>>,
    pub(super) compounds: Vec<Compound<'a>>,
    pub(super) compound_of: HashMap<&'a str, usize>,
    pub(super) chains: Vec<Vec<&'a str>>,
    /// Leading x of every chain slot, per compound.
    pub(super) chain_xs: Vec<Vec<f32>>,
    pub(super) bounds_by_id: HashMap<&'a str, Rect>,
    pub(super) surname_first: bool,
    /// Family id -> union x, recorded while its chain is placed.
    pub(super) union_x: HashMap<&'a str, f32>,
    /// Connected family group of every compound.
    pub(super) component_of: Vec<usize>,
    /// Canvas offset per component, set by [`Self::layout`].
    pub(super) component_offsets: HashMap<usize, (f32, f32)>,
}

impl<'a> LayoutBuilder<'a> {
    pub(super) fn new(tree: &'a FamilyTree, surname_first: bool) -> Self {
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
        let chains: Vec<Vec<&str>> = compounds
            .iter()
            .map(|compound| Self::build_chain(tree, &index, compound))
            .collect();
        let chain_xs = Self::compute_chain_xs(tree, &compounds, &chains);
        let person_components = compute_person_components(tree);
        let component_of = compounds
            .iter()
            .map(|compound| {
                compound
                    .members
                    .first()
                    .and_then(|&id| index.get(id))
                    .and_then(|&position| person_components.get(position).copied())
                    .unwrap_or(0)
            })
            .collect();
        let mut builder = Self {
            surname_first,
            tree,
            node_ids,
            index,
            family_by_id,
            children_by_family,
            compounds,
            compound_of,
            chains,
            chain_xs,
            bounds_by_id: HashMap::new(),
            union_x: HashMap::new(),
            component_of,
            component_offsets: HashMap::new(),
        };
        builder.compute_ranks();
        builder
    }

    /// Rank of a family's compound (either spouse's), and the trunk row
    /// half a rank gap below it.
    pub(super) fn trunk_y(&self, family: &Family) -> f32 {
        let compound = family
            .husband_id
            .as_deref()
            .and_then(|id| self.compound_of.get(id).copied())
            .or_else(|| {
                family
                    .wife_id
                    .as_deref()
                    .and_then(|id| self.compound_of.get(id).copied())
            });
        compound
            .map(|compound| self.rank_y(compound) + V_GAP * 0.5)
            .unwrap_or(V_GAP * 0.5)
    }

    pub(super) fn build_unions(&self) -> Vec<UnionGeom> {
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
                    let adjacent = (second.x - first.x - SPOUSE_GAP).abs() < 0.5;
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
            let parent_rank_y = family
                .husband_id
                .as_deref()
                .and_then(|id| self.compound_of.get(id).copied())
                .or_else(|| {
                    family
                        .wife_id
                        .as_deref()
                        .and_then(|id| self.compound_of.get(id).copied())
                })
                .map(|compound| self.rank_y(compound))
                .unwrap_or(0.0);
            unions.push(UnionGeom {
                family_id: family.id.clone(),
                spouse_line,
                union,
                trunk_y: self.trunk_y(family),
                trunk_lane: 0,
                parent_rank_y,
                children,
            });
        }
        assign_trunk_lanes(&mut unions);
        unions
    }

    pub(super) fn finish(self) -> TreeScene {
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
                display_name: person_display_name(person, self.surname_first),
                bounds: *bounds,
            });
        }
        let unions = self.build_unions();
        let mut bounds = nodes
            .iter()
            .map(|node| node.bounds)
            .reduce(Rect::union)
            .unwrap_or_else(|| Rect::from_min_size(Pos2::ZERO, Vec2::ZERO));
        if !nodes.is_empty() {
            bounds = bounds.expand(16.0);
        }
        TreeScene {
            nodes,
            unions,
            bounds,
        }
    }
}

/// Gives overlapping family trunks distinct lanes within their generation gap.
fn assign_trunk_lanes(unions: &mut [UnionGeom]) {
    use std::collections::HashMap;

    const SPAN_PADDING: f32 = 6.0;
    let mut groups: HashMap<i64, Vec<usize>> = HashMap::new();
    for (index, union) in unions.iter().enumerate() {
        if union.children.is_empty() {
            continue;
        }
        let key = (union.parent_rank_y * 100.0).round() as i64;
        groups.entry(key).or_default().push(index);
    }
    for mut indices in groups.into_values() {
        indices.sort_by(|&left, &right| {
            let a = &unions[left];
            let b = &unions[right];
            let a_span = span(a);
            let b_span = span(b);
            a_span
                .0
                .total_cmp(&b_span.0)
                .then_with(|| a_span.1.total_cmp(&b_span.1))
                .then_with(|| a.union.x.total_cmp(&b.union.x))
                .then_with(|| a.family_id.cmp(&b.family_id))
        });
        let mut active: Vec<(f32, u32)> = Vec::new();
        let mut max_lane = 0u32;
        for &index in &indices {
            let (start, end) = span(&unions[index]);
            active.retain(|(active_end, _)| *active_end >= start - SPAN_PADDING);
            let mut lane = 0u32;
            while active.iter().any(|(_, used)| *used == lane) {
                lane += 1;
            }
            active.push((end + SPAN_PADDING, lane));
            max_lane = max_lane.max(lane);
            if let Some(union) = unions.get_mut(index) {
                union.trunk_lane = lane;
            }
        }
        let Some(first) = indices.first().and_then(|&index| unions.get(index)) else {
            continue;
        };
        let top = first.parent_rank_y + NODE_H + TRUNK_TOP_MARGIN;
        let bottom = first.parent_rank_y + RANK_PITCH - TRUNK_BOTTOM_MARGIN;
        let available = (bottom - top).max(1.0);
        let step = (available / (max_lane + 1) as f32).clamp(2.0, TRUNK_LANE_SPACING);
        for index in indices {
            if let Some(union) = unions.get_mut(index) {
                union.trunk_y = top + (union.trunk_lane + 1) as f32 * step;
            }
        }
    }
}

fn span(union: &UnionGeom) -> (f32, f32) {
    let mut start = union.union.x;
    let mut end = union.union.x;
    for (_, anchor) in &union.children {
        start = start.min(anchor.x);
        end = end.max(anchor.x);
    }
    (start, end)
}
