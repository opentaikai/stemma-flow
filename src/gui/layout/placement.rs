//! Rank-based coordinate assignment for family compounds.

use std::collections::HashMap;

use egui::{Pos2, Rect, vec2};

use super::builder::LayoutBuilder;
use super::packing::{gap_centre, pack_components, slot_centre};
use super::{H_SPACING, NODE_H, NODE_W, RANK_PITCH};

/// How many downward/upward ordering sweeps to run per component.
const ORDERING_SWEEPS: usize = 4;

impl<'a> LayoutBuilder<'a> {
    pub(super) fn children_of_family(&self, family: usize) -> Vec<&'a str> {
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
    pub(super) fn union_offset(&self, compound: usize, family: usize) -> f32 {
        let Some(chain) = self.chains.get(compound) else {
            return 0.0;
        };
        let Some(xs) = self.chain_xs.get(compound) else {
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
                let Some(first_gap) = gap_centre(xs, low) else {
                    return 0.0;
                };
                if high - low == 1 {
                    return first_gap;
                }
                let midpoint = match (slot_centre(xs, husband), slot_centre(xs, wife)) {
                    (Some(husband), Some(wife)) => (husband + wife) * 0.5,
                    _ => return 0.0,
                };
                let mut best = first_gap;
                let mut best_distance = (best - midpoint).abs();
                for gap in low..high {
                    let Some(centre) = gap_centre(xs, gap) else {
                        break;
                    };
                    let distance = (centre - midpoint).abs();
                    if distance <= best_distance {
                        best = centre;
                        best_distance = distance;
                    }
                }
                best
            }
            (Some(position), None) | (None, Some(position)) => {
                slot_centre(xs, position).unwrap_or(0.0)
            }
            (None, None) => 0.0,
        }
    }

    pub(super) fn insert_bounds(&mut self, id: &'a str, rect: Rect) {
        if !self.bounds_by_id.contains_key(id) {
            self.bounds_by_id.insert(id, rect);
        }
    }

    /// Assigns x coordinates per rank, orders compounds using parent/child
    /// barycentres, and then shelf-packs the disconnected components.
    pub(super) fn layout(&mut self) {
        let component_count = self
            .component_of
            .iter()
            .copied()
            .max()
            .map(|highest| highest + 1)
            .unwrap_or(0);
        let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); component_count];
        for compound in 0..self.compounds.len() {
            let Some(&component) = self.component_of.get(compound) else {
                continue;
            };
            if let Some(bucket) = buckets.get_mut(component) {
                bucket.push(compound);
            }
        }

        let mut layouts = Vec::with_capacity(buckets.len());
        let mut sizes = Vec::with_capacity(buckets.len());
        for bucket in &buckets {
            let (positions, size) = self.layout_component(bucket);
            layouts.push(positions);
            sizes.push(size);
        }
        for (component, offset) in pack_components(&sizes).into_iter().enumerate() {
            self.component_offsets.insert(component, offset);
        }

        self.bounds_by_id.clear();
        self.union_x.clear();
        for (component, bucket) in buckets.iter().enumerate() {
            let positions = &layouts[component];
            let dx = self
                .component_offsets
                .get(&component)
                .map(|(x, _)| *x)
                .unwrap_or(0.0);
            for &compound in bucket {
                let Some(&local_x) = positions.get(&compound) else {
                    continue;
                };
                let x = local_x + dx;
                let y = self.rank_y(compound);
                let chain = self.chains.get(compound).cloned().unwrap_or_default();
                let chain_xs = self.chain_xs.get(compound).cloned().unwrap_or_default();
                for (slot, id) in chain.iter().enumerate() {
                    let left = x + chain_xs.get(slot).copied().unwrap_or(0.0);
                    self.insert_bounds(
                        id,
                        Rect::from_min_size(Pos2::new(left, y), vec2(NODE_W, NODE_H)),
                    );
                }
                let families = self
                    .compounds
                    .get(compound)
                    .map(|compound| compound.families.clone())
                    .unwrap_or_default();
                for family in families {
                    let union = x + self.union_offset(compound, family);
                    if let Some(family) = self.tree.families.get(family) {
                        self.union_x.insert(family.id.as_str(), union);
                    }
                }
            }
        }
    }

    /// Lays out one connected component in component-local coordinates.
    fn layout_component(&self, bucket: &[usize]) -> (HashMap<usize, f32>, (f32, f32)) {
        if bucket.is_empty() {
            return (HashMap::new(), (0.0, 0.0));
        }
        let max_rank = bucket
            .iter()
            .filter_map(|compound| self.compounds.get(*compound))
            .map(|compound| compound.rank as usize)
            .max()
            .unwrap_or(0);
        let mut by_rank: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
        for &compound in bucket {
            let rank = self
                .compounds
                .get(compound)
                .map(|compound| compound.rank as usize)
                .unwrap_or(0);
            if let Some(list) = by_rank.get_mut(rank) {
                list.push(compound);
            }
        }
        for list in &mut by_rank {
            list.sort_unstable();
        }

        let component = self.component_of.get(bucket[0]).copied().unwrap_or(0);
        let (parent_families_of, children_of) = self.compound_adjacency(bucket, component);
        let widths: HashMap<usize, f32> = bucket
            .iter()
            .map(|&compound| (compound, self.chain_width(compound)))
            .collect();
        let mut positions: HashMap<usize, f32> = HashMap::with_capacity(bucket.len());
        for list in &by_rank {
            let mut cursor = 0.0;
            for &compound in list {
                positions.insert(compound, cursor);
                cursor += widths.get(&compound).copied().unwrap_or(0.0) + H_SPACING;
            }
        }

        for _ in 0..ORDERING_SWEEPS {
            for rank in 1..=max_rank {
                self.sweep_down(
                    rank,
                    &mut by_rank,
                    &mut positions,
                    &widths,
                    &parent_families_of,
                );
            }
            for rank in (0..max_rank).rev() {
                self.sweep_up(rank, &mut by_rank, &mut positions, &widths, &children_of);
            }
        }
        // Finish with family-grouped child rows; roots align to their children.
        for rank in 1..=max_rank {
            self.sweep_down(
                rank,
                &mut by_rank,
                &mut positions,
                &widths,
                &parent_families_of,
            );
        }
        self.sweep_up(0, &mut by_rank, &mut positions, &widths, &children_of);

        let min_x = positions.values().copied().fold(f32::INFINITY, f32::min);
        for x in positions.values_mut() {
            *x -= min_x;
        }
        let max_x = positions
            .iter()
            .map(|(compound, x)| x + widths.get(compound).copied().unwrap_or(0.0))
            .fold(f32::NEG_INFINITY, f32::max);
        let width = max_x.max(0.0);
        let height = max_rank as f32 * (RANK_PITCH) + NODE_H;
        (positions, (width, height))
    }

    fn sweep_down(
        &self,
        rank: usize,
        by_rank: &mut [Vec<usize>],
        positions: &mut HashMap<usize, f32>,
        widths: &HashMap<usize, f32>,
        parent_families_of: &HashMap<usize, Vec<usize>>,
    ) {
        let order = by_rank[rank].clone();
        let current: Vec<(usize, f32, f32)> = order
            .iter()
            .map(|&compound| {
                let center = positions.get(&compound).copied().unwrap_or(0.0)
                    + widths.get(&compound).copied().unwrap_or(0.0) * 0.5;
                let key = parent_families_of
                    .get(&compound)
                    .filter(|families| !families.is_empty())
                    .map(|families| {
                        families
                            .iter()
                            .filter_map(|family| self.family_union_center(*family, positions))
                            .fold((0.0, 0usize), |(sum, count), value| {
                                (sum + value, count + 1)
                            })
                    })
                    .filter(|(_, count)| *count > 0)
                    .map(|(sum, count)| sum / count as f32)
                    .unwrap_or(center);
                (compound, key, center)
            })
            .collect();
        let mut sorted = current.clone();
        sorted.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.2.total_cmp(&right.2))
                .then_with(|| left.0.cmp(&right.0))
        });
        let order: Vec<usize> = sorted.iter().map(|(compound, _, _)| *compound).collect();
        let desired: HashMap<usize, f32> = sorted
            .iter()
            .map(|(compound, key, center)| {
                (*compound, if key.is_finite() { *key } else { *center })
            })
            .collect();
        self.assign_rank(&order, &desired, widths, positions);
        by_rank[rank] = order;
    }

    fn sweep_up(
        &self,
        rank: usize,
        by_rank: &mut [Vec<usize>],
        positions: &mut HashMap<usize, f32>,
        widths: &HashMap<usize, f32>,
        children_of: &HashMap<usize, Vec<usize>>,
    ) {
        let order = by_rank[rank].clone();
        let current: Vec<(usize, f32, f32)> = order
            .iter()
            .map(|&compound| {
                let center = positions.get(&compound).copied().unwrap_or(0.0)
                    + widths.get(&compound).copied().unwrap_or(0.0) * 0.5;
                let child_center = children_of
                    .get(&compound)
                    .filter(|children| !children.is_empty())
                    .map(|children| {
                        children
                            .iter()
                            .map(|child| {
                                positions.get(child).copied().unwrap_or(0.0)
                                    + widths.get(child).copied().unwrap_or(0.0) * 0.5
                            })
                            .sum::<f32>()
                            / children.len() as f32
                    })
                    .unwrap_or(center);
                (compound, child_center, center)
            })
            .collect();
        let mut sorted = current.clone();
        sorted.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.2.total_cmp(&right.2))
                .then_with(|| left.0.cmp(&right.0))
        });
        let order: Vec<usize> = sorted.iter().map(|(compound, _, _)| *compound).collect();
        let desired: HashMap<usize, f32> = sorted
            .iter()
            .map(|(compound, center, _)| (*compound, *center))
            .collect();
        self.assign_rank(&order, &desired, widths, positions);
        by_rank[rank] = order;
    }

    fn compound_adjacency(
        &self,
        bucket: &[usize],
        component: usize,
    ) -> (HashMap<usize, Vec<usize>>, HashMap<usize, Vec<usize>>) {
        let members: std::collections::HashSet<usize> = bucket.iter().copied().collect();
        let mut parent_families_of: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut children_of: HashMap<usize, Vec<usize>> = HashMap::new();
        for (family_index, family) in self.tree.families.iter().enumerate() {
            let parent_compounds: Vec<usize> =
                [family.husband_id.as_deref(), family.wife_id.as_deref()]
                    .into_iter()
                    .flatten()
                    .filter_map(|id| self.compound_of.get(id).copied())
                    .collect();
            for child_id in self.children_of_family(family_index) {
                let Some(&child) = self.compound_of.get(child_id) else {
                    continue;
                };
                if !members.contains(&child)
                    || self.component_of.get(child).copied() != Some(component)
                {
                    continue;
                }
                parent_families_of
                    .entry(child)
                    .or_default()
                    .push(family_index);
                for &parent in &parent_compounds {
                    if parent == child {
                        continue;
                    }
                    children_of.entry(parent).or_default().push(child);
                }
            }
        }
        for values in parent_families_of.values_mut() {
            values.sort_unstable();
            values.dedup();
        }
        for values in children_of.values_mut() {
            values.sort_unstable();
            values.dedup();
        }
        (parent_families_of, children_of)
    }

    fn family_union_center(
        &self,
        family_index: usize,
        positions: &HashMap<usize, f32>,
    ) -> Option<f32> {
        let family = self.tree.families.get(family_index)?;
        let compound = family
            .husband_id
            .as_deref()
            .and_then(|id| self.compound_of.get(id).copied())
            .or_else(|| {
                family
                    .wife_id
                    .as_deref()
                    .and_then(|id| self.compound_of.get(id).copied())
            })?;
        let x = positions.get(&compound).copied()?;
        Some(
            x + self.union_offset(
                compound,
                self.tree
                    .families
                    .iter()
                    .position(|item| item.id == family.id)?,
            ),
        )
    }

    fn assign_rank(
        &self,
        order: &[usize],
        desired: &HashMap<usize, f32>,
        widths: &HashMap<usize, f32>,
        positions: &mut HashMap<usize, f32>,
    ) {
        let mut cursor = f32::NEG_INFINITY;
        for &compound in order {
            let width = widths.get(&compound).copied().unwrap_or(0.0);
            let mut x = desired.get(&compound).copied().unwrap_or(0.0) - width * 0.5;
            if cursor.is_finite() {
                x = x.max(cursor + H_SPACING);
            }
            positions.insert(compound, x);
            cursor = x + width;
        }
    }
}
