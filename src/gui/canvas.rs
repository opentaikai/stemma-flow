//! Canvas viewport state and world-to-screen coordinate transformations.
//!
//! World ("canvas") coordinates are logical tree positions; screen
//! coordinates are pixels inside the allocated `egui` viewport. The mapping
//! is `screen = viewport.center + pan + canvas * zoom`, so `(0, 0)` sits at
//! the viewport centre when `pan` is zero.

use egui::{Pos2, Rect, Ui, Vec2};

/// Lower bound for the zoom factor.
pub const ZOOM_MIN: f32 = 0.3;
/// Upper bound for the zoom factor.
pub const ZOOM_MAX: f32 = 3.0;

/// Zoom speed: one wheel notch (~50pt of scroll) changes the scale by ~10%.
const SCROLL_ZOOM_SPEED: f32 = 0.002;
/// Guards the per-frame zoom multiplier against extreme scroll deltas.
const SCROLL_FACTOR_MIN: f32 = 0.01;
const SCROLL_FACTOR_MAX: f32 = 100.0;

/// Panning offset (in screen points) and scale factor of the canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportState {
    pub pan: Vec2,
    pub zoom: f32,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl ViewportState {
    /// Maps a world position to a pixel position inside `viewport_rect`.
    pub fn canvas_to_screen(&self, canvas_pos: Pos2, viewport_rect: Rect) -> Pos2 {
        (viewport_rect.center().to_vec2() + self.pan + canvas_pos.to_vec2() * self.zoom).to_pos2()
    }

    /// Reverse of [`Self::canvas_to_screen`] for pointer hit testing.
    pub fn screen_to_canvas(&self, screen_pos: Pos2, viewport_rect: Rect) -> Pos2 {
        ((screen_pos.to_vec2() - viewport_rect.center().to_vec2() - self.pan) / self.zoom).to_pos2()
    }

    /// Screen-space rectangle covering canvas-space `bounds`.
    pub fn screen_rect(&self, bounds: Rect, viewport_rect: Rect) -> Rect {
        Rect::from_two_pos(
            self.canvas_to_screen(bounds.min, viewport_rect),
            self.canvas_to_screen(bounds.max, viewport_rect),
        )
    }

    /// Whether the transformed `bounds` intersect the viewport — used to
    /// cull off-screen elements before painting.
    pub fn is_visible(&self, bounds: Rect, viewport_rect: Rect) -> bool {
        self.screen_rect(bounds, viewport_rect)
            .intersects(viewport_rect)
    }

    /// Translates the view by a screen-space `delta`.
    pub fn pan_by(&mut self, delta: Vec2) {
        self.pan += delta;
    }

    /// Multiplies the zoom by `factor`, clamped to `[ZOOM_MIN, ZOOM_MAX]`,
    /// while keeping the canvas point under `cursor` fixed.
    pub fn zoom_at(&mut self, factor: f32, cursor: Pos2, viewport_rect: Rect) {
        let anchor = self.screen_to_canvas(cursor, viewport_rect);
        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        self.pan =
            cursor.to_vec2() - viewport_rect.center().to_vec2() - anchor.to_vec2() * self.zoom;
    }

    /// Applies vertical scroll (in points) as zoom around `cursor`.
    pub fn apply_scroll_y(&mut self, scroll_y: f32, cursor: Pos2, viewport_rect: Rect) {
        if scroll_y == 0.0 {
            return;
        }
        let factor =
            (1.0 + scroll_y * SCROLL_ZOOM_SPEED).clamp(SCROLL_FACTOR_MIN, SCROLL_FACTOR_MAX);
        self.zoom_at(factor, cursor, viewport_rect);
    }

    /// Applies drag panning and scroll-wheel zooming for one frame.
    ///
    /// `primary_drag_background` must only be true while the primary button
    /// drags across empty canvas (dragging a node must not pan the view);
    /// middle-button drags always pan.
    pub fn handle_navigation(
        &mut self,
        ui: &Ui,
        viewport_rect: Rect,
        primary_drag_background: bool,
        middle_drag: bool,
    ) {
        let (delta, scroll_y, cursor) = ui.input(|i| {
            (
                i.pointer.delta(),
                i.smooth_scroll_delta.y,
                i.pointer.hover_pos(),
            )
        });
        if let Some(cursor) = cursor
            && viewport_rect.contains(cursor)
        {
            self.apply_scroll_y(scroll_y, cursor, viewport_rect);
        }
        if middle_drag || primary_drag_background {
            self.pan_by(delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))
    }

    fn assert_approx(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn defaults_are_centered_at_scale_one() {
        let state = ViewportState::default();
        assert_eq!(state.zoom, 1.0);
        assert_eq!(state.pan, Vec2::ZERO);
        let rect = viewport();
        assert_eq!(state.canvas_to_screen(Pos2::ZERO, rect), rect.center());
        assert_eq!(state.screen_to_canvas(rect.center(), rect), Pos2::ZERO);
    }

    #[test]
    fn screen_and_canvas_round_trip() {
        let rect = viewport();
        let cases = [
            ViewportState::default(),
            ViewportState {
                pan: vec2(120.0, -45.0),
                zoom: 2.5,
            },
            ViewportState {
                pan: vec2(-900.0, 300.0),
                zoom: 0.3,
            },
        ];
        let points = [Pos2::ZERO, Pos2::new(140.0, 56.0), Pos2::new(-320.5, 75.25)];
        for state in cases {
            for point in points {
                let screen = state.canvas_to_screen(point, rect);
                let back = state.screen_to_canvas(screen, rect);
                assert_approx(back.x, point.x);
                assert_approx(back.y, point.y);
            }
        }
    }

    #[test]
    fn pan_accumulates_screen_deltas() {
        let mut state = ViewportState::default();
        state.pan_by(vec2(10.0, 20.0));
        state.pan_by(vec2(-4.0, 6.0));
        assert_eq!(state.pan, vec2(6.0, 26.0));
    }

    #[test]
    fn zoom_is_clamped_between_bounds() {
        let rect = viewport();
        let cursor = rect.center();

        let mut zoomed_in = ViewportState::default();
        zoomed_in.apply_scroll_y(1.0e6, cursor, rect);
        assert_eq!(zoomed_in.zoom, ZOOM_MAX);

        let mut zoomed_out = ViewportState::default();
        zoomed_out.apply_scroll_y(-1.0e6, cursor, rect);
        assert_eq!(zoomed_out.zoom, ZOOM_MIN);

        let mut beyond_max = ViewportState {
            zoom: ZOOM_MAX,
            ..Default::default()
        };
        beyond_max.apply_scroll_y(50.0, cursor, rect);
        assert_eq!(beyond_max.zoom, ZOOM_MAX);

        let mut beyond_min = ViewportState {
            zoom: ZOOM_MIN,
            ..Default::default()
        };
        beyond_min.apply_scroll_y(-50.0, cursor, rect);
        assert_eq!(beyond_min.zoom, ZOOM_MIN);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor_fixed() {
        let rect = viewport();
        let cursor = Pos2::new(620.0, 140.0);
        let mut state = ViewportState {
            pan: vec2(40.0, -20.0),
            zoom: 1.4,
        };
        let anchor_before = state.screen_to_canvas(cursor, rect);

        state.apply_scroll_y(50.0, cursor, rect);
        assert!(state.zoom > 1.4, "scroll up must zoom in");
        let anchor_after = state.screen_to_canvas(cursor, rect);
        assert_approx(anchor_after.x, anchor_before.x);
        assert_approx(anchor_after.y, anchor_before.y);
    }

    #[test]
    fn zero_scroll_leaves_zoom_untouched() {
        let rect = viewport();
        let mut state = ViewportState::default();
        state.apply_scroll_y(0.0, rect.center(), rect);
        assert_eq!(state.zoom, 1.0);
    }

    #[test]
    fn culling_detects_offscreen_bounds() {
        let rect = viewport();
        let mut state = ViewportState::default();
        let near = Rect::from_center_size(Pos2::ZERO, vec2(140.0, 56.0));
        assert!(state.is_visible(near, rect), "centred node is visible");

        state.pan = vec2(50_000.0, 0.0);
        assert!(!state.is_visible(near, rect), "far-away node is culled");
    }
}
