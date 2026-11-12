//! Person node painting and pointer hit testing.

use std::collections::HashMap;

use egui::{Color32, CornerRadius, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, vec2};

use crate::model::person_badge::PersonBadge;

use super::canvas::ViewportState;
use super::layout::{NodeGeom, TreeScene};

const NODE_FILL: Color32 = Color32::from_gray(250);
const NODE_BORDER: Color32 = Color32::from_gray(200);
const NODE_TEXT: Color32 = Color32::from_gray(30);
const SELECTED_BORDER: Color32 = Color32::from_rgb(59, 130, 246);
const CORNER: CornerRadius = CornerRadius::same(6);
const SELECTION_WIDTH: f32 = 2.0;
const BORDER_WIDTH: f32 = 1.0;
const NAME_FONT_BASE: f32 = 13.0;
const NAME_FONT_MIN: f32 = 7.0;
const NAME_FONT_MAX: f32 = 24.0;
const TEXT_PADDING: f32 = 8.0;

/// Finds the topmost node containing `canvas_pos` (the node painted last
/// wins) for selection clicks.
pub fn hit_test(scene: &TreeScene, canvas_pos: Pos2) -> Option<&NodeGeom> {
    scene
        .nodes
        .iter()
        .rev()
        .find(|node| node.bounds.contains(canvas_pos))
}

/// Paints every on-screen node: fill, border (accent stroke when selected)
/// and the pre-truncated name. Off-screen nodes are culled first so the
/// loop never touches invisible elements.
pub fn paint_nodes(
    painter: &Painter,
    scene: &TreeScene,
    viewport: &ViewportState,
    viewport_rect: Rect,
    selected_person_id: Option<&str>,
    badges: &HashMap<String, Vec<PersonBadge>>,
) {
    let font_size = (NAME_FONT_BASE * viewport.zoom).clamp(NAME_FONT_MIN, NAME_FONT_MAX);
    let font_id = FontId::proportional(font_size);
    let chip_font = FontId::proportional((7.0 * viewport.zoom).clamp(6.0, 12.0));
    for node in &scene.nodes {
        if !viewport.is_visible(node.bounds, viewport_rect) {
            continue;
        }
        let screen_rect = viewport.screen_rect(node.bounds, viewport_rect);
        painter.rect_filled(screen_rect, CORNER, NODE_FILL);

        let colored: Vec<&PersonBadge> = badges
            .get(&node.person_id)
            .into_iter()
            .flatten()
            .filter(|badge| badge.color.is_some())
            .collect();
        let accent = colored.first().and_then(|badge| badge.color);
        if let Some(color) = accent {
            let stripe = Rect::from_min_max(
                screen_rect.min,
                Pos2::new(
                    screen_rect.min.x + (4.0 * viewport.zoom).max(2.0),
                    screen_rect.max.y,
                ),
            );
            painter.rect_filled(
                stripe,
                CornerRadius::ZERO,
                crate::gui::labels::color32(color),
            );
        }

        if selected_person_id == Some(node.person_id.as_str()) {
            painter.rect_stroke(
                screen_rect,
                CORNER,
                Stroke::new(SELECTION_WIDTH, SELECTED_BORDER),
                StrokeKind::Outside,
            );
        } else {
            painter.rect_stroke(
                screen_rect,
                CORNER,
                Stroke::new(BORDER_WIDTH, NODE_BORDER),
                StrokeKind::Outside,
            );
        }

        let name_y = if colored.is_empty() {
            screen_rect.center().y
        } else {
            screen_rect.center().y - (7.0 * viewport.zoom).clamp(4.0, 14.0)
        };
        painter.text(
            Pos2::new(screen_rect.min.x + TEXT_PADDING * viewport.zoom, name_y),
            egui::Align2::LEFT_CENTER,
            &node.display_name,
            font_id.clone(),
            NODE_TEXT,
        );

        if colored.is_empty() {
            continue;
        }
        let chip_height = (10.0 * viewport.zoom).clamp(8.0, 18.0);
        let chip_y = screen_rect.max.y - chip_height - (3.0 * viewport.zoom).max(1.0);
        let mut chip_x = screen_rect.min.x + (8.0 * viewport.zoom).max(4.0);
        let gap = (4.0 * viewport.zoom).max(2.0);
        let mut chips: Vec<(String, u32)> = colored
            .iter()
            .take(2)
            .map(|badge| {
                (
                    crate::gui::layout::truncate_name(&badge.name, 9),
                    badge.color.unwrap_or(0),
                )
            })
            .collect();
        if colored.len() > 2 {
            chips.pop();
            chips.push((format!("+{}", colored.len() - 1), 0x6b7280));
        }
        for (text, color) in chips {
            let color = crate::gui::labels::color32(color);
            let galley = painter.layout_no_wrap(text, chip_font.clone(), text_on(color));
            let width = galley.size().x + (6.0 * viewport.zoom).max(4.0);
            let rect = Rect::from_min_size(Pos2::new(chip_x, chip_y), vec2(width, chip_height));
            painter.rect_filled(rect, CornerRadius::same(3), color);
            painter.galley(
                Pos2::new(
                    rect.min.x + (3.0 * viewport.zoom).max(2.0),
                    rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                text_on(color),
            );
            chip_x += width + gap;
        }
    }
}

fn text_on(color: Color32) -> Color32 {
    let luminance = 0.299 * color.r() as f32 + 0.587 * color.g() as f32 + 0.114 * color.b() as f32;
    if luminance > 150.0 {
        Color32::from_gray(20)
    } else {
        Color32::from_gray(250)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::layout::TreeScene;
    use egui::{Context, vec2};

    fn node(id: &str, min: Pos2, size: egui::Vec2) -> NodeGeom {
        NodeGeom {
            person_id: id.to_string(),
            display_name: id.to_string(),
            bounds: Rect::from_min_size(min, size),
        }
    }

    fn scene(nodes: Vec<NodeGeom>) -> TreeScene {
        TreeScene {
            nodes,
            unions: Vec::new(),
            bounds: Rect::from_min_size(Pos2::ZERO, vec2(0.0, 0.0)),
        }
    }

    #[test]
    fn hit_test_matches_node_bounds() {
        let scene = scene(vec![node("a", Pos2::new(0.0, 0.0), vec2(140.0, 56.0))]);

        let inside = hit_test(&scene, Pos2::new(70.0, 28.0));
        assert_eq!(
            inside.map(|found| found.person_id.as_str()),
            Some("a"),
            "click inside the node selects it"
        );

        assert!(
            hit_test(&scene, Pos2::new(200.0, 200.0)).is_none(),
            "click outside the node selects nothing"
        );
        assert!(
            hit_test(&scene, Pos2::new(-1.0, 28.0)).is_none(),
            "the node edge is exclusive at the minimum corner"
        );
    }

    #[test]
    fn hit_test_prefers_the_topmost_node() {
        let scene = scene(vec![
            node("under", Pos2::new(0.0, 0.0), vec2(200.0, 56.0)),
            node("over", Pos2::new(100.0, 0.0), vec2(200.0, 56.0)),
        ]);
        let hit = hit_test(&scene, Pos2::new(150.0, 28.0))
            .unwrap_or_else(|| panic!("overlapping region must hit"));
        assert_eq!(hit.person_id, "over", "the later-painted node wins");

        let only_left = hit_test(&scene, Pos2::new(50.0, 28.0))
            .unwrap_or_else(|| panic!("left strip must hit"));
        assert_eq!(only_left.person_id, "under");
    }

    #[test]
    fn hit_test_on_empty_scene_selects_nothing() {
        assert!(hit_test(&scene(Vec::new()), Pos2::new(10.0, 10.0)).is_none());
    }

    #[test]
    fn paint_nodes_draws_coloured_badges() {
        use crate::model::person_badge::{BadgeKind, PersonBadge};

        let ctx = Context::default();
        let scene = scene(vec![node("a", Pos2::ZERO, vec2(140.0, 56.0))]);
        let viewport = ViewportState::default();
        let viewport_rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let mut badges: HashMap<String, Vec<PersonBadge>> = HashMap::new();
        badges.insert(
            "a".to_string(),
            vec![
                PersonBadge {
                    name: "Maternal".to_string(),
                    color: Some(0xff3366),
                    kind: BadgeKind::Group,
                },
                PersonBadge {
                    name: "Brick wall".to_string(),
                    color: Some(0x33aa66),
                    kind: BadgeKind::Label,
                },
            ],
        );
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_nodes(
                ui.painter(),
                &scene,
                &viewport,
                viewport_rect,
                None,
                &badges,
            );
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "badges are painted");
    }

    #[test]
    fn paint_nodes_renders_headless_without_panicking() {
        let ctx = Context::default();
        let scene = scene(vec![node("a", Pos2::ZERO, vec2(140.0, 56.0))]);
        let viewport = ViewportState::default();
        let viewport_rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));

        let mut on_screen = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_nodes(
                ui.painter(),
                &scene,
                &viewport,
                viewport_rect,
                Some("a"),
                &HashMap::new(),
            );
        });
        assert!(!on_screen.shapes.is_empty(), "visible node is painted");
        on_screen.textures_delta.clear();

        let mut panned = viewport;
        panned.pan = vec2(50_000.0, 0.0);
        let mut culled = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_nodes(
                ui.painter(),
                &scene,
                &panned,
                viewport_rect,
                None,
                &HashMap::new(),
            );
        });
        culled.textures_delta.clear();
        assert!(
            culled.shapes.is_empty(),
            "off-screen nodes must be culled before painting"
        );
    }
}
