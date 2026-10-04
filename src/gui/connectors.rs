//! Orthogonal family connector routing and painting.

use egui::{Color32, Painter, Pos2, Rect, Stroke};

use super::canvas::ViewportState;
use super::layout::{TreeScene, UnionGeom};

const LINE_COLOR: Color32 = Color32::from_gray(140);
const BASE_WIDTH: f32 = 1.5;
const MIN_WIDTH: f32 = 1.0;

/// One straight connector segment in canvas coordinates: `[start, end]`.
pub type Segment = [Pos2; 2];

/// Builds the orthogonal connector route for every union: the spouse bar,
/// a drop line to a branch row midway to the children, the branch itself
/// spanning all children, and one drop per child. Degenerate (zero-length)
/// segments are skipped. Pure geometry, unit-tested without a UI.
pub fn build_segments(unions: &[UnionGeom]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for union in unions {
        if let Some(bar) = union.spouse_line {
            let [bar_start, bar_end] = bar;
            push_segment(&mut segments, bar_start, bar_end);
        }
        if union.children.is_empty() {
            continue;
        }
        let children_y = union
            .children
            .iter()
            .map(|(_, anchor)| anchor.y)
            .fold(f32::INFINITY, f32::min);
        let branch_y = union.union.y + (children_y - union.union.y) * 0.5;

        push_segment(
            &mut segments,
            union.union,
            Pos2::new(union.union.x, branch_y),
        );

        let mut span_start = union.union.x;
        let mut span_end = union.union.x;
        for (_, anchor) in &union.children {
            span_start = span_start.min(anchor.x);
            span_end = span_end.max(anchor.x);
        }
        push_segment(
            &mut segments,
            Pos2::new(span_start, branch_y),
            Pos2::new(span_end, branch_y),
        );
        for (_, anchor) in &union.children {
            push_segment(&mut segments, Pos2::new(anchor.x, branch_y), *anchor);
        }
    }
    segments
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
    for segment in build_segments(&scene.unions) {
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

    fn union(spouse_line: Option<[Pos2; 2]>, union: Pos2, children: Vec<Pos2>) -> UnionGeom {
        UnionGeom {
            family_id: "f1".to_string(),
            spouse_line,
            union,
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
        let segments = build_segments(&[union(Some(bar), Pos2::new(170.0, 100.0), Vec::new())]);
        assert_eq!(segments, vec![bar], "childless couple keeps only the bar");
    }

    #[test]
    fn couple_routes_drop_branch_and_child_lines() {
        let bar = [Pos2::new(140.0, 100.0), Pos2::new(200.0, 100.0)];
        let segments = build_segments(&[union(
            Some(bar),
            Pos2::new(170.0, 100.0),
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
            "branch row sits halfway to the children"
        );
    }

    #[test]
    fn single_parent_without_children_stops_at_the_drop_origin() {
        let segments = build_segments(&[union(None, Pos2::new(70.0, 56.0), Vec::new())]);
        assert!(segments.is_empty(), "nothing to connect");
    }

    #[test]
    fn aligned_child_skips_the_degenerate_branch() {
        let segments = build_segments(&[union(
            None,
            Pos2::new(70.0, 56.0),
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
