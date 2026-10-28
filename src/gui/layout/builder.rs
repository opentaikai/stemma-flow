//! The layout builder: ranks, chains, packing and scene assembly.

use std::collections::{HashMap, HashSet};

use egui::{Pos2, Rect, Vec2};

use crate::db::{Family, FamilyTree};

use super::compounds::Compound;
use super::graph::compute_person_components;
use super::placement::BlockLayout;
use super::scene::{NodeGeom, TreeScene, UnionGeom, person_display_name};
use super::{SPOUSE_GAP, V_GAP};

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
    pub(super) blocks: HashMap<usize, BlockLayout>,
    pub(super) block_active: HashSet<usize>,
    pub(super) place_active: HashSet<usize>,
    pub(super) placed: HashSet<usize>,
    pub(super) bounds_by_id: HashMap<&'a str, Rect>,
    pub(super) surname_first: bool,
    /// Family id -> union x, recorded while its chain is placed.
    pub(super) union_x: HashMap<&'a str, f32>,
    /// Compounds without parents, in compound order.
    pub(super) roots: Vec<usize>,
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
            blocks: HashMap::new(),
            block_active: HashSet::new(),
            place_active: HashSet::new(),
            placed: HashSet::new(),
            bounds_by_id: HashMap::new(),
            union_x: HashMap::new(),
            roots: Vec::new(),
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
            unions.push(UnionGeom {
                family_id: family.id.clone(),
                spouse_line,
                union,
                trunk_y: self.trunk_y(family),
                children,
            });
        }
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
