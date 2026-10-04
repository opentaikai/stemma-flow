//! Applies [`AppConfig`] to an `egui::Context` without restarting.

use egui::{Context, ThemePreference};

use crate::config::AppConfig;

/// The font size that maps to zoom 1.0; matches `AppConfig::default()`.
pub const BASELINE_FONT_SIZE: f32 = 14.0;

/// UI zoom factor for a configured font size (14.0pt -> 1.0).
pub fn zoom_for(font_size: f32) -> f32 {
    font_size / BASELINE_FONT_SIZE
}

/// Pushes theme and zoom into the context; both take effect live
/// (zoom on the next pass), so tabs and windows restyle immediately.
pub fn apply_config_to_ctx(ctx: &Context, config: &AppConfig) {
    let preference = match config.theme {
        crate::config::ThemeMode::Light => ThemePreference::Light,
        crate::config::ThemeMode::Dark => ThemePreference::Dark,
        crate::config::ThemeMode::System => ThemePreference::System,
    };
    ctx.set_theme(preference);
    ctx.set_zoom_factor(zoom_for(config.font_size));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ThemeMode;
    use egui::{RawInput, Theme};

    #[test]
    fn zoom_scales_around_the_baseline() {
        assert_eq!(zoom_for(14.0), 1.0);
        assert_eq!(zoom_for(21.0), 1.5);
        assert!((zoom_for(10.0) - 10.0 / 14.0).abs() < f32::EPSILON);
    }

    #[test]
    fn applying_dark_and_light_switches_the_context_theme() {
        let ctx = Context::default();
        let dark = AppConfig {
            theme: ThemeMode::Dark,
            ..AppConfig::default()
        };
        let light = AppConfig {
            theme: ThemeMode::Light,
            ..AppConfig::default()
        };

        apply_config_to_ctx(&ctx, &dark);
        assert_eq!(ctx.theme(), Theme::Dark);

        apply_config_to_ctx(&ctx, &light);
        assert_eq!(ctx.theme(), Theme::Light);
    }

    #[test]
    fn zoom_becomes_visible_after_one_pass() {
        let ctx = Context::default();
        let config = AppConfig {
            theme: ThemeMode::Light,
            font_size: 21.0,
        };

        apply_config_to_ctx(&ctx, &config);
        let mut output = ctx.run_ui(RawInput::default(), |_ui| {});
        output.textures_delta.clear();
        assert_eq!(ctx.zoom_factor(), 1.5);
    }
}
