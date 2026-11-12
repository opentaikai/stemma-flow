//! Font-safe interface symbols and painted swatches.

use egui::{Color32, Sense, Ui, vec2};

/// Ascending sort marker used in headers.
pub const SORT_ASC: &str = " ^";
/// Descending sort marker used in headers.
pub const SORT_DESC: &str = " v";
/// Compact ascending button label.
pub const SORT_UP: &str = "^";
/// Compact descending button label.
pub const SORT_DOWN: &str = "v";
/// Plain text replacement for an unavailable arrow glyph.
pub const ARROW_RIGHT: &str = " -> ";

/// Paints a small filled colour swatch.
pub fn color_swatch(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

/// Paints a small filled colour dot.
pub fn color_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn symbols_are_font_safe_ascii() {
        for value in [SORT_ASC, SORT_DESC, SORT_UP, SORT_DOWN, ARROW_RIGHT] {
            assert!(value.is_ascii(), "{value:?} must stay ASCII");
        }
    }

    #[test]
    fn swatches_paint_headless() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            color_swatch(ui, Color32::RED);
            color_dot(ui, Color32::BLUE);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
