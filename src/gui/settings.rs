//! The `File > Settings...` modal: theme and font size with live apply.

use egui::{Context, Window};

use crate::config::{AppConfig, ThemeMode};
use crate::gui::theme;
use crate::i18n::{self, tr, tr_args};

/// Renders the settings window while `open` is true.
///
/// Theme changes apply to the context immediately. The font-size slider
/// rescales the app (and saves) once the mouse button is released, and its
/// number input commits only on Enter or focus loss — so neither dragging
/// nor typing makes the interface jump. A returned message is a save
/// failure for the status bar (the app keeps running on the in-memory
/// config either way).
pub fn show(ctx: &Context, config: &mut AppConfig, open: &mut bool) -> Option<String> {
    let mut theme_changed = false;
    let mut font_apply = false;
    let mut places_changed = false;
    let mut language_changed = false;
    Window::new(tr("app.settings"))
        .id(egui::Id::new("settings_window"))
        .collapsible(false)
        .resizable(false)
        .open(open)
        .show(ctx, |ui| {
            ui.heading(tr("app.appearance"));
            ui.separator();

            ui.label(tr("settings.theme"));
            ui.horizontal(|ui| {
                for (mode, label) in [
                    (ThemeMode::Light, tr("common.light")),
                    (ThemeMode::Dark, tr("common.dark")),
                    (ThemeMode::System, tr("common.system")),
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
                ui.label(tr("settings.font_size"));
                let response = ui.add(
                    egui::Slider::new(&mut config.font_size, 10.0..=24.0)
                        .suffix("pt")
                        .update_while_editing(false),
                );
                if font_apply_pending(
                    response.drag_stopped(),
                    response.changed(),
                    response.dragged(),
                ) {
                    font_apply = true;
                }
            });

            ui.separator();
            ui.label(tr("app.language"));
            let languages = i18n::available_languages();
            let current = languages
                .iter()
                .find(|language| language.code == config.language)
                .map(|language| language.name.clone())
                .unwrap_or_else(|| config.language.clone());
            egui::ComboBox::from_id_salt("settings_language")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for language in languages {
                        if ui
                            .selectable_value(
                                &mut config.language,
                                language.code.clone(),
                                &language.name,
                            )
                            .changed()
                        {
                            i18n::set_language(&language.code);
                            language_changed = true;
                        }
                    }
                });
            ui.separator();
            ui.heading(tr("settings.places"));
            if ui
                .checkbox(
                    &mut config.reverse_place_names,
                    tr("settings.reverse_places"),
                )
                .changed()
            {
                places_changed = true;
            }
            ui.label(
                egui::RichText::new(tr("settings.reverse_places_hint"))
                    .weak()
                    .small(),
            );
        });

    if !(theme_changed || font_apply || places_changed || language_changed) {
        return None;
    }
    theme::apply_config_to_ctx(ctx, config);
    if language_changed {
        ctx.request_repaint();
    }
    match config.save() {
        Ok(()) => None,
        Err(error) => Some(tr_args("settings.could_not_save", &[("error", error)])),
    }
}

/// Applies the font size on slider release and on non-drag edits such as
/// keyboard steps and text-input commits (Enter/focus loss — egui only
/// writes the value then, via `update_while_editing(false)`); mid-drag and
/// mid-typing frames defer so the zoom stays stable.
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
