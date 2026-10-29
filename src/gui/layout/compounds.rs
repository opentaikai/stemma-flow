//! Marriage compounds: spouse chains, ranks and their geometry.

use std::collections::{HashMap, VecDeque};

use crate::db::{Family, FamilyTree};

use super::builder::LayoutBuilder;
use super::{H_SPACING, NODE_W, RANK_PITCH, SPOUSE_GAP};

/// One rank unit: people joined by marriages, drawn as a spouse chain.
#[derive(Debug)]
pub(super) struct Compound<'a> {
    /// Every member, in person rowid order.
    pub(super) members: Vec<&'a str>,
    /// Family indices of this compound, in rowid order.
    pub(super) families: Vec<usize>,
    /// Layer index; both spouses always share it.
    pub(super) rank: u32,
}

impl<'a> LayoutBuilder<'a> {
    /// Groups people into marriage compounds by flood-filling spouse
    /// links, so both spouses of every family share one rank. Components
    /// surface in first-member rowid order and members stay rowid-ordered.
    pub(super) fn compute_compounds(
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
    pub(super) fn chainable(
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
    pub(super) fn insert_partner(chain: &mut Vec<&'a str>, anchor: usize, partner: &'a str) {
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
    pub(super) fn build_chain(
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

    /// Leading x of every chain slot: spouses of the same family sit
    /// [`SPOUSE_GAP`] apart, any other chain neighbour keeps the node
    /// separation [`H_SPACING`]. Slots start at 0; an empty chain has
    /// no slots.
    pub(super) fn compute_chain_xs(
        tree: &'a FamilyTree,
        compounds: &[Compound<'a>],
        chains: &[Vec<&'a str>],
    ) -> Vec<Vec<f32>> {
        chains
            .iter()
            .enumerate()
            .map(|(compound, chain)| {
                let mut xs = Vec::with_capacity(chain.len());
                let mut x = 0.0;
                for pair in chain.windows(2) {
                    xs.push(x);
                    let gap = if Self::spouse_pair(tree, compounds.get(compound), pair) {
                        SPOUSE_GAP
                    } else {
                        H_SPACING
                    };
                    x += NODE_W + gap;
                }
                if !chain.is_empty() {
                    xs.push(x);
                }
                xs
            })
            .collect()
    }

    /// Whether `pair` are the two spouses of one of the compound's
    /// families, in either order.
    pub(super) fn spouse_pair(
        tree: &'a FamilyTree,
        compound: Option<&Compound<'a>>,
        pair: &[&'a str],
    ) -> bool {
        let [first, second] = pair else {
            return false;
        };
        let Some(compound) = compound else {
            return false;
        };
        compound.families.iter().any(|&position| {
            let Some(family) = tree.families.get(position) else {
                return false;
            };
            let (Some(husband), Some(wife)) =
                (family.husband_id.as_deref(), family.wife_id.as_deref())
            else {
                return false;
            };
            (husband == *first && wife == *second) || (husband == *second && wife == *first)
        })
    }

    /// Full width of the compound's chain, empty chains are zero wide.
    pub(super) fn chain_width(&self, compound: usize) -> f32 {
        self.chain_xs
            .get(compound)
            .and_then(|xs| xs.last())
            .map(|last| last + NODE_W)
            .unwrap_or(0.0)
    }

    /// Kahn pass over compound -> compound edges; cycle leftovers keep
    /// rank 0 instead of hanging the layout. Compounds without parents
    /// become the placement roots.
    pub(super) fn compute_ranks(&mut self) {
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

        let mut queue: VecDeque<usize> = VecDeque::new();
        for compound in 0..self.compounds.len() {
            if pending.get(compound).copied().unwrap_or(0) == 0 {
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
    }

    pub(super) fn rank_y(&self, compound: usize) -> f32 {
        let rank = self
            .compounds
            .get(compound)
            .map(|compound| compound.rank)
            .unwrap_or(0);
        let offset = self
            .component_of
            .get(compound)
            .and_then(|&component| self.component_offsets.get(&component))
            .map(|(_, dy)| *dy)
            .unwrap_or(0.0);
        rank as f32 * (RANK_PITCH) + offset
    }
}
