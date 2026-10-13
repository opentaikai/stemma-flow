//! Depth-contour packing of sibling subtrees and component boxes.

use super::{H_SPACING, NODE_W, V_GAP};

/// Centre of the gap that follows chain slot `slot` in `xs`.
pub(super) fn gap_centre(xs: &[f32], slot: usize) -> Option<f32> {
    let left = *xs.get(slot)?;
    let right = *xs.get(slot + 1)?;
    Some((left + right + NODE_W) * 0.5)
}

/// Centre of the node sitting in chain slot `slot` in `xs`.
pub(super) fn slot_centre(xs: &[f32], slot: usize) -> Option<f32> {
    xs.get(slot).map(|left| left + NODE_W * 0.5)
}

/// `(min_x, max_x)` per depth, depth 0 first.
pub(super) type Contour = Vec<(f32, f32)>;

/// Packs sibling subtrees so their depth contours clear each other by
/// [`H_SPACING`], then shifts the row so its bounding box centres on
/// `centre`. Returns each subtree's origin and the merged contour, both
/// relative to the same frame. Subtrees shorter than the row impose no
/// constraint on deeper levels, so a childless sibling tucks beside the
/// deepest row of its neighbour.
pub(super) fn compact_subtrees<T>(rows: &[(T, Contour)], centre: f32) -> (Vec<f32>, Contour) {
    let mut origins = Vec::with_capacity(rows.len());
    let mut merged: Contour = Vec::new();
    for (_, row) in rows {
        let mut origin: f32 = 0.0;
        for (depth, (min, _)) in row.iter().enumerate() {
            if let Some((_, previous_max)) = merged.get(depth) {
                origin = origin.max(previous_max + H_SPACING - min);
            }
        }
        for (depth, (min, max)) in row.iter().enumerate() {
            let shifted = (min + origin, max + origin);
            if let Some(slot) = merged.get_mut(depth) {
                slot.0 = slot.0.min(shifted.0);
                slot.1 = slot.1.max(shifted.1);
            } else {
                merged.push(shifted);
            }
        }
        origins.push(origin);
    }
    if let (Some(low), Some(high)) = (
        merged.iter().map(|(min, _)| *min).reduce(f32::min),
        merged.iter().map(|(_, max)| *max).reduce(f32::max),
    ) {
        let shift = centre - (low + high) * 0.5;
        for origin in &mut origins {
            *origin += shift;
        }
        for extent in &mut merged {
            extent.0 += shift;
            extent.1 += shift;
        }
    }
    (origins, merged)
}

/// Shelf-packs component boxes into whole rows, choosing the row count
/// whose bounding box scores closest to a 16:9 target. Returns one
/// offset per component; rows sit `H_SPACING` apart horizontally and
/// [`V_GAP`] apart vertically.
pub(super) fn pack_components(sizes: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if sizes.is_empty() {
        return Vec::new();
    }
    let area: f32 = sizes.iter().map(|(width, height)| width * height).sum();
    let target_w = (area * 16.0 / 9.0).sqrt();
    let target_h = (area * 9.0 / 16.0).sqrt();
    let mut best: Option<(f32, Vec<(f32, f32)>)> = None;
    for rows in 1..=sizes.len() {
        let base = sizes.len() / rows;
        let extra = sizes.len() % rows;
        let mut offsets = Vec::with_capacity(sizes.len());
        let mut canvas_w: f32 = 0.0;
        let mut y: f32 = 0.0;
        let mut taken = 0usize;
        for row in 0..rows {
            let length = base + usize::from(row < extra);
            let mut x: f32 = 0.0;
            let mut row_height: f32 = 0.0;
            for _ in 0..length {
                let (width, height) = sizes.get(taken).copied().unwrap_or((0.0, 0.0));
                offsets.push((x, y));
                taken += 1;
                x += width + H_SPACING;
                row_height = row_height.max(height);
            }
            canvas_w = canvas_w.max(x - H_SPACING);
            y += row_height + V_GAP;
        }
        let canvas_h = y - V_GAP;
        let score = (canvas_w - target_w).powi(2) + (canvas_h - target_h).powi(2);
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score < *best_score)
        {
            best = Some((score, offsets));
        }
    }
    best.map(|(_, offsets)| offsets).unwrap_or_default()
}
