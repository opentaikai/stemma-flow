//! Orthogonal family connector routing and painting.

use egui::{Color32, Painter, Pos2, Rect, Stroke};

use super::canvas::ViewportState;
use super::layout::{NodeGeom, TreeScene, UnionGeom};

const LINE_COLOR: Color32 = Color32::from_gray(140);
/// Small clearance used when a long edge has to detour around a node row.
const ROUTE_GAP: f32 = 8.0;
const BASE_WIDTH: f32 = 1.5;
const MIN_WIDTH: f32 = 1.0;

/// One straight connector segment in canvas coordinates: `[start, end]`.
pub type Segment = [Pos2; 2];

/// Builds the orthogonal connector route for every union: the spouse bar,
/// a drop line to the trunk row anchored half a rank gap below the
/// couple, the branch itself spanning all children, and one drop per
/// child. Multi-rank child edges detour around intermediate node rows.
/// Degenerate (zero-length) segments are skipped. Pure geometry,
/// unit-tested without a UI.
pub fn build_segments(scene: &TreeScene) -> Vec<Segment> {
    let mut segments = Vec::new();
    for union in &scene.unions {
        if let Some(bar) = union.spouse_line {
            let [bar_start, bar_end] = bar;
            push_segment(&mut segments, bar_start, bar_end);
        }
        if union.children.is_empty() {
            continue;
        }
        let branch_y = union.trunk_y;
        push_segment(
            &mut segments,
            union.union,
            Pos2::new(union.union.x, branch_y),
        );

        let mut routes: Vec<(Pos2, Option<f32>)> = Vec::with_capacity(union.children.len());
        for (child_id, anchor) in &union.children {
            routes.push((
                *anchor,
                detour_lane(&scene.nodes, child_id, *anchor, branch_y),
            ));
        }
        let mut span_start = union.union.x;
        let mut span_end = union.union.x;
        for (anchor, lane) in &routes {
            span_start = span_start.min(anchor.x).min(lane.unwrap_or(anchor.x));
            span_end = span_end.max(anchor.x).max(lane.unwrap_or(anchor.x));
        }
        push_segment(
            &mut segments,
            Pos2::new(span_start, branch_y),
            Pos2::new(span_end, branch_y),
        );
        for (anchor, lane) in routes {
            match lane {
                None => push_segment(&mut segments, Pos2::new(anchor.x, branch_y), anchor),
                Some(lane) if (lane - anchor.x).abs() < 0.01 => {
                    push_segment(&mut segments, Pos2::new(anchor.x, branch_y), anchor);
                }
                Some(lane) => {
                    let approach_y = anchor.y - ROUTE_GAP;
                    if lane < span_start || lane > span_end {
                        push_segment(
                            &mut segments,
                            Pos2::new(anchor.x, branch_y),
                            Pos2::new(lane, branch_y),
                        );
                    }
                    push_segment(
                        &mut segments,
                        Pos2::new(lane, branch_y),
                        Pos2::new(lane, approach_y),
                    );
                    push_segment(
                        &mut segments,
                        Pos2::new(lane, approach_y),
                        Pos2::new(anchor.x, approach_y),
                    );
                    push_segment(&mut segments, Pos2::new(anchor.x, approach_y), anchor);
                }
            }
        }
    }
    segments
}

/// Picks a vertical lane when the direct child drop would cross a node.
fn detour_lane(nodes: &[NodeGeom], child_id: &str, anchor: Pos2, branch_y: f32) -> Option<f32> {
    let approach_y = anchor.y - ROUTE_GAP;
    if !nodes.iter().any(|node| {
        node.person_id != child_id && vertical_hits(node, anchor.x, branch_y, approach_y)
    }) {
        return None;
    }
    let mut candidates = vec![anchor.x];
    for node in nodes.iter().filter(|node| node.person_id != child_id) {
        if node.bounds.max.y < branch_y || node.bounds.min.y > approach_y {
            continue;
        }
        candidates.push(node.bounds.min.x - ROUTE_GAP);
        candidates.push(node.bounds.max.x + ROUTE_GAP);
    }
    candidates.sort_by(f32::total_cmp);
    candidates.dedup_by(|left, right| (*left - *right).abs() < 0.01);
    candidates
        .into_iter()
        .filter(|lane| {
            !nodes.iter().any(|node| {
                node.person_id != child_id && vertical_hits(node, *lane, branch_y, approach_y)
            })
        })
        .min_by(|left, right| {
            (left - anchor.x)
                .abs()
                .total_cmp(&(right - anchor.x).abs())
                .then_with(|| left.total_cmp(right))
        })
        .or(Some(anchor.x))
}

fn vertical_hits(node: &NodeGeom, x: f32, top: f32, bottom: f32) -> bool {
    let inner = node.bounds.expand(-0.5);
    let (low, high) = if top <= bottom {
        (top, bottom)
    } else {
        (bottom, top)
    };
    x >= inner.min.x && x <= inner.max.x && high >= inner.min.y && low <= inner.max.y
}

fn push_segment(segments: &mut Vec<Segment>, start: Pos2, end: Pos2) {
    if start != end {
        segments.push([start, end]);
    }
}

/// Paints every on-screen connector with a zoom-scaled stroke; segments
/// leaving the viewport are culled before hitting the painter.
pub fn paint_connectors(
    painter: &Painter,
    scene: &TreeScene,
    viewport: &ViewportState,
    viewport_rect: Rect,
) {
    let stroke = Stroke::new((BASE_WIDTH * viewport.zoom).max(MIN_WIDTH), LINE_COLOR);
    for segment in build_segments(scene) {
        let [start, end] = segment;
        let canvas_bounds = Rect::from_two_pos(start, end).expand(BASE_WIDTH);
        if !viewport.is_visible(canvas_bounds, viewport_rect) {
            continue;
        }
        painter.line_segment(
            [
                viewport.canvas_to_screen(start, viewport_rect),
                viewport.canvas_to_screen(end, viewport_rect),
            ],
            stroke,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::layout::{TreeScene, UnionGeom};
    use egui::{Context, RawInput, vec2};

    fn union(
        spouse_line: Option<[Pos2; 2]>,
        union: Pos2,
        trunk_y: f32,
        children: Vec<Pos2>,
    ) -> UnionGeom {
        UnionGeom {
            family_id: "f1".to_string(),
            spouse_line,
            union,
            trunk_y,
            children: children
                .into_iter()
                .enumerate()
                .map(|(index, anchor)| (format!("c{index}"), anchor))
                .collect(),
        }
    }

    fn is_axis_aligned([start, end]: &Segment) -> bool {
        start.x == end.x || start.y == end.y
    }

    #[test]
    fn spouse_bar_is_drawn_between_the_spouses() {
        let bar = [Pos2::new(140.0, 100.0), Pos2::new(200.0, 100.0)];
        let segments =
            build_segments(&[union(Some(bar), Pos2::new(170.0, 100.0), 60.0, Vec::new())]);
        assert_eq!(segments, vec![bar], "childless couple keeps only the bar");
    }

    #[test]
    fn couple_routes_drop_branch_and_child_lines() {
        let bar = [Pos2::new(140.0, 100.0), Pos2::new(200.0, 100.0)];
        let segments = build_segments(&[union(
            Some(bar),
            Pos2::new(170.0, 100.0),
            200.0,
            vec![Pos2::new(50.0, 300.0), Pos2::new(250.0, 300.0)],
        )]);

        assert!(
            segments.iter().all(is_axis_aligned),
            "every connector segment must be orthogonal"
        );
        assert_eq!(
            segments.len(),
            5,
            "bar, union drop, branch span and two child drops"
        );
        let expected = vec![
            bar,
            [Pos2::new(170.0, 100.0), Pos2::new(170.0, 200.0)],
            [Pos2::new(50.0, 200.0), Pos2::new(250.0, 200.0)],
            [Pos2::new(50.0, 200.0), Pos2::new(50.0, 300.0)],
            [Pos2::new(250.0, 200.0), Pos2::new(250.0, 300.0)],
        ];
        assert_eq!(
            segments, expected,
            "the drop stops at the trunk row the layout anchored"
        );
    }

    #[test]
    fn trunk_follows_the_layout_anchor() {
        let bar = [Pos2::new(140.0, 28.0), Pos2::new(180.0, 28.0)];
        let segments = build_segments(&[union(
            Some(bar),
            Pos2::new(160.0, 28.0),
            60.0,
            vec![Pos2::new(50.0, 176.0)],
        )]);
        assert!(
            segments.contains(&[Pos2::new(160.0, 28.0), Pos2::new(160.0, 60.0)]),
            "the drop stops at the layout's trunk row, not a midpoint"
        );
        assert!(
            segments.contains(&[Pos2::new(50.0, 60.0), Pos2::new(160.0, 60.0)]),
            "the branch runs along the trunk row"
        );
    }

    #[test]
    fn single_parent_without_children_stops_at_the_drop_origin() {
        let segments = build_segments(&[union(None, Pos2::new(70.0, 56.0), 60.0, Vec::new())]);
        assert!(segments.is_empty(), "nothing to connect");
    }

    #[test]
    fn aligned_child_skips_the_degenerate_branch() {
        let segments = build_segments(&[union(
            None,
            Pos2::new(70.0, 56.0),
            128.0,
            vec![Pos2::new(70.0, 200.0)],
        )]);
        assert_eq!(
            segments,
            vec![
                [Pos2::new(70.0, 56.0), Pos2::new(70.0, 128.0)],
                [Pos2::new(70.0, 128.0), Pos2::new(70.0, 200.0)],
            ],
            "a straight-through line collapses into the drop plus child drop"
        );
    }

    #[test]
    fn painted_connectors_survive_a_headless_frame() {
        let ctx = Context::default();
        let scene = TreeScene {
            nodes: Vec::new(),
            unions: vec![union(
                Some([Pos2::new(0.0, 0.0), Pos2::new(60.0, 0.0)]),
                Pos2::new(30.0, 0.0),
                100.0,
                vec![Pos2::new(30.0, 200.0)],
            )],
            bounds: Rect::from_min_size(Pos2::ZERO, vec2(100.0, 260.0)),
        };
        let viewport = ViewportState::default();
        let viewport_rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));

        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            paint_connectors(ui.painter(), &scene, &viewport, viewport_rect);
        });
        assert!(!output.shapes.is_empty(), "connectors are painted");
        output.textures_delta.clear();

        let mut panned = viewport;
        panned.pan = vec2(50_000.0, 0.0);
        let mut culled = ctx.run_ui(RawInput::default(), |ui| {
            paint_connectors(ui.painter(), &scene, &panned, viewport_rect);
        });
        culled.textures_delta.clear();
        assert!(
            culled.shapes.is_empty(),
            "segments off-screen must be culled"
        );
    }
}
