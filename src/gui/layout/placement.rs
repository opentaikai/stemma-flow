//! Block packing and placement of compounds onto the canvas.

use egui::{Pos2, Rect, vec2};

use super::builder::LayoutBuilder;
use super::packing::{Contour, compact_subtrees, gap_centre, pack_components, slot_centre};
use super::{H_SPACING, NODE_H, NODE_W};

/// One family's children row, origins as packed by [`compact_subtrees`].
#[derive(Debug, Clone)]
pub(super) struct GroupLayout {
    pub(super) family: usize,
    /// `(child compound, x of the child's block within this block)`.
    pub(super) children: Vec<(usize, f32)>,
}

/// Horizontal packing of one compound, relative to its left edge.
#[derive(Debug, Clone)]
pub(super) struct BlockLayout {
    /// Left edge of the chain within the block.
    pub(super) chain_x: f32,
    /// Children rows, ordered by union position.
    pub(super) groups: Vec<GroupLayout>,
    /// Total width of the packed block.
    pub(super) width: f32,
    /// Depth contours: depth 0 is the chain, depth d + 1 the row of
    /// blocks one generation further down.
    pub(super) contour: Contour,
}

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

    /// Packed horizontal layout of one compound: the chain plus every
    /// family's children row, ordered by union position. Rows are
    /// compacted with [`compact_subtrees`] and pushed right only when
    /// their contours would clash with the previous group.
    pub(super) fn compute_block(&mut self, compound: usize) -> BlockLayout {
        let chain_width = self.chain_width(compound);
        let families = self
            .compounds
            .get(compound)
            .map(|compound| compound.families.clone())
            .unwrap_or_default();

        let mut measured = Vec::with_capacity(families.len());
        for family in families {
            let mut row = Vec::new();
            for child in self.children_of_family(family) {
                let Some(&child_compound) = self.compound_of.get(child) else {
                    continue;
                };
                let contour = self.block_of(child_compound).contour;
                row.push((child_compound, contour));
            }
            let union = self.union_offset(compound, family);
            measured.push((family, union, row));
        }
        measured.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.0.cmp(&right.0))
        });

        let mut packed = Vec::with_capacity(measured.len());
        let mut previous: Option<Contour> = None;
        let mut low: f32 = 0.0;
        let mut high = chain_width;
        for (family, union, row) in measured {
            let (mut origins, mut extent) = compact_subtrees(&row, union);
            if let Some(previous) = &previous {
                let mut shift: f32 = 0.0;
                for (depth, (min, _)) in extent.iter().enumerate() {
                    if let Some((_, previous_max)) = previous.get(depth) {
                        shift = shift.max(previous_max + H_SPACING - min);
                    }
                }
                if shift > 0.0 {
                    for origin in &mut origins {
                        *origin += shift;
                    }
                    for span in &mut extent {
                        span.0 += shift;
                        span.1 += shift;
                    }
                }
            }
            for (min, max) in &extent {
                low = low.min(*min);
                high = high.max(*max);
            }
            previous = Some(extent.clone());
            packed.push((family, origins, row, extent));
        }

        let chain_x = -low;
        let mut contour: Contour = vec![(chain_x, chain_x + chain_width)];
        let mut groups = Vec::with_capacity(packed.len());
        for (family, origins, row, extent) in packed {
            let children = origins
                .into_iter()
                .zip(row.into_iter().map(|(child_compound, _)| child_compound))
                .map(|(origin, child_compound)| (child_compound, origin + chain_x))
                .collect();
            groups.push(GroupLayout { family, children });
            for (depth, (min, max)) in extent.iter().enumerate() {
                let shifted = (min + chain_x, max + chain_x);
                if let Some(slot) = contour.get_mut(depth + 1) {
                    slot.0 = slot.0.min(shifted.0);
                    slot.1 = slot.1.max(shifted.1);
                } else {
                    contour.push(shifted);
                }
            }
        }
        BlockLayout {
            chain_x,
            groups,
            width: high - low,
            contour,
        }
    }

    /// Memoised [`Self::compute_block`]; compounds already being measured
    /// (a cycle) fall back to their bare chain so recursion terminates.
    pub(super) fn block_of(&mut self, compound: usize) -> BlockLayout {
        if let Some(block) = self.blocks.get(&compound) {
            return block.clone();
        }
        if !self.block_active.insert(compound) {
            let width = self.chain_width(compound);
            return BlockLayout {
                chain_x: 0.0,
                groups: Vec::new(),
                width,
                contour: vec![(0.0, width)],
            };
        }
        let block = self.compute_block(compound);
        self.block_active.remove(&compound);
        self.blocks.insert(compound, block.clone());
        block
    }

    pub(super) fn insert_bounds(&mut self, id: &'a str, rect: Rect) {
        if !self.bounds_by_id.contains_key(id) {
            self.bounds_by_id.insert(id, rect);
        }
    }

    /// Places a compound's chain at `left` and packs each family's
    /// children row under its union. Placed and in-progress compounds are
    /// skipped so shared children and cycles terminate.
    pub(super) fn place_compound(&mut self, compound: usize, left: f32) {
        if self.placed.contains(&compound) || !self.place_active.insert(compound) {
            return;
        }
        let block = self.block_of(compound);
        let chain = self.chains.get(compound).cloned().unwrap_or_default();
        let chain_xs = self.chain_xs.get(compound).cloned().unwrap_or_default();
        let y = self.rank_y(compound);

        for (slot, id) in chain.iter().enumerate() {
            let x = left + block.chain_x + chain_xs.get(slot).copied().unwrap_or(0.0);
            self.insert_bounds(
                id,
                Rect::from_min_size(Pos2::new(x, y), vec2(NODE_W, NODE_H)),
            );
        }
        for group in &block.groups {
            let union = left + block.chain_x + self.union_offset(compound, group.family);
            if let Some(id) = self
                .tree
                .families
                .get(group.family)
                .map(|family| family.id.as_str())
            {
                self.union_x.insert(id, union);
            }
            for &(child_compound, child_x) in &group.children {
                self.place_compound(child_compound, left + child_x);
            }
        }

        self.place_active.remove(&compound);
        self.placed.insert(compound);
    }

    /// Clears every placed node and union so a component can be measured
    /// or placed from scratch.
    pub(super) fn reset_placement(&mut self) {
        self.placed.clear();
        self.place_active.clear();
        self.bounds_by_id.clear();
        self.union_x.clear();
    }

    /// Bounding box a component's compounds occupy when laid out from the
    /// origin. Measuring the real extent beats summing block widths, which
    /// counts every descendant twice and inflates the box handed to
    /// [`pack_components`] (mis-sizing the shelf rows).
    pub(super) fn measure_bucket(&mut self, bucket: &[usize]) -> (f32, f32) {
        self.reset_placement();
        let mut cursor: f32 = 0.0;
        for &compound in bucket {
            if self.placed.contains(&compound) {
                continue;
            }
            let width = self.block_of(compound).width;
            self.place_compound(compound, cursor);
            cursor += width + H_SPACING;
        }
        let mut min = Pos2::new(f32::INFINITY, f32::INFINITY);
        let mut max = Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for rect in self.bounds_by_id.values() {
            min.x = min.x.min(rect.min.x);
            min.y = min.y.min(rect.min.y);
            max.x = max.x.max(rect.max.x);
            max.y = max.y.max(rect.max.y);
        }
        if min.x > max.x || min.y > max.y {
            return (0.0, 0.0);
        }
        (max.x - min.x, max.y - min.y)
    }

    /// Buckets every compound into its connected family group (roots
    /// first, then the cycle leftovers), shelf-packs the group boxes
    /// onto the grid, and places each group from its grid offset.
    pub(super) fn layout(&mut self) {
        let roots = self.roots.clone();
        let components = self
            .component_of
            .iter()
            .copied()
            .max()
            .map(|highest| highest + 1)
            .unwrap_or(0);
        let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); components];
        for compound in &roots {
            let Some(&component) = self.component_of.get(*compound) else {
                continue;
            };
            if let Some(bucket) = buckets.get_mut(component) {
                bucket.push(*compound);
            }
        }
        for compound in 0..self.compounds.len() {
            if roots.contains(&compound) {
                continue;
            }
            let Some(&component) = self.component_of.get(compound) else {
                continue;
            };
            if let Some(bucket) = buckets.get_mut(component) {
                bucket.push(compound);
            }
        }

        let mut sizes = Vec::with_capacity(buckets.len());
        for bucket in &buckets {
            sizes.push(self.measure_bucket(bucket));
        }
        for (component, offset) in pack_components(&sizes).into_iter().enumerate() {
            self.component_offsets.insert(component, offset);
        }
        self.reset_placement();
        for (component, bucket) in buckets.into_iter().enumerate() {
            let mut cursor = self
                .component_offsets
                .get(&component)
                .map(|(x, _)| *x)
                .unwrap_or(0.0);
            for compound in bucket {
                if self.placed.contains(&compound) {
                    continue;
                }
                let width = self.block_of(compound).width;
                self.place_compound(compound, cursor);
                cursor += width + H_SPACING;
            }
        }
    }
}
