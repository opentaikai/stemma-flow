//! The `File > Settings...` modal: theme and font size with live apply.

use egui::{Context, Window};

use crate::config::{AppConfig, ThemeMode};
use crate::gui::theme;

/// Renders the settings window while `open` is true.
///
/// Theme changes apply to the context immediately; the font-size slider
/// stays live in the dialog but only rescales the app (and saves) once the
/// mouse button is released, so the interface does not jump mid-drag.
/// A returned message is a save failure for the status bar (the app keeps
/// running on the in-memory config either way).
pub fn show(ctx: &Context, config: &mut AppConfig, open: &mut bool) -> Option<String> {
    let mut theme_changed = false;
    let mut font_apply = false;
    Window::new("Settings")
        .id(egui::Id::new("settings_window"))
        .collapsible(false)
        .resizable(false)
        .open(open)
        .show(ctx, |ui| {
            ui.heading("Appearance");
            ui.separator();

            ui.label("Theme:");
            ui.horizontal(|ui| {
                for (mode, label) in [
                    (ThemeMode::Light, "Light"),
                    (ThemeMode::Dark, "Dark"),
                    (ThemeMode::System, "System"),
                ] {
                    if ui
                        .selectable_value(&mut config.theme, mode, label)
                        .changed()
                    {
                        theme_changed = true;
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Font size:");
                let response =
                    ui.add(egui::Slider::new(&mut config.font_size, 10.0..=24.0).suffix("pt"));
                if font_apply_pending(
                    response.drag_stopped(),
                    response.changed(),
                    response.dragged(),
                ) {
                    font_apply = true;
                }
            });
        });

    if !(theme_changed || font_apply) {
        return None;
    }
    theme::apply_config_to_ctx(ctx, config);
    match config.save() {
        Ok(()) => None,
        Err(error) => Some(format!("Settings could not be saved: {error}")),
    }
}

/// Applies the font size on slider release and on non-drag edits such as
/// keyboard arrows; mid-drag frames defer so the zoom stays stable.
fn font_apply_pending(drag_stopped: bool, changed: bool, dragged: bool) -> bool {
    drag_stopped || (changed && !dragged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::RawInput;

    #[test]
    fn window_renders_headless_without_changing_anything() {
        let ctx = Context::default();
        let config = AppConfig::default();
        let mut live = config.clone();
        let mut open = true;

        let mut outcome = Some("pending".to_string());
        let mut output = ctx.run_ui(RawInput::default(), |_ui| {
            outcome = show(&ctx, &mut live, &mut open);
        });
        output.textures_delta.clear();

        assert!(!output.shapes.is_empty(), "the dialog paints");
        assert_eq!(outcome, None, "no interaction changes nothing");
        assert!(open, "the window stays open");
        assert_eq!(live, config, "config untouched");
    }

    #[test]
    fn a_closed_window_paints_nothing() {
        let ctx = Context::default();
        let mut config = AppConfig::default();
        let mut open = false;

        let mut outcome = Some("pending".to_string());
        let mut output = ctx.run_ui(RawInput::default(), |_ui| {
            outcome = show(&ctx, &mut config, &mut open);
        });
        output.textures_delta.clear();

        assert!(output.shapes.is_empty(), "nothing to draw");
        assert_eq!(outcome, None);
        assert!(!open, "stays closed");
    }

    #[test]
    fn font_size_applies_on_release_and_on_keyboard_changes_only() {
        assert!(
            font_apply_pending(true, false, false),
            "mouse release applies even when the last drag frame already set the value"
        );
        assert!(
            font_apply_pending(true, true, false),
            "release with a final value change applies"
        );
        assert!(
            !font_apply_pending(false, true, true),
            "mid-drag frames defer the zoom"
        );
        assert!(
            font_apply_pending(false, true, false),
            "keyboard/track clicks without dragging apply immediately"
        );
        assert!(
            !font_apply_pending(false, false, false),
            "an idle slider applies nothing"
        );
    }
}
