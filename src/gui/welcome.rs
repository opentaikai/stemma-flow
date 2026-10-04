//! First-launch welcome dashboard with create, import and recent actions.

use std::path::{Path, PathBuf};

use egui::{CentralPanel, Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, vec2};

/// How many recent tree paths the dashboard lists at most.
pub const RECENT_CAP: usize = 8;

const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
const CONNECTOR: Color32 = Color32::from_gray(140);
const CARD_WIDTH: f32 = 440.0;
const CARD_RADIUS: u8 = 16;
const CARD_MARGIN_X: i8 = 28;
const CARD_MARGIN_Y: i8 = 32;
const BUTTON_HEIGHT: f32 = 44.0;
const LOGO_SIZE: f32 = 72.0;
const BASE_CARD_HEIGHT: f32 = 340.0;
const RECENT_ROW_HEIGHT: f32 = 26.0;
const MIN_TOP_SPACE: f32 = 16.0;
const TAGLINE: &str = "Fast, open-source, event-centric genealogy software";

/// What the user asked the app to do from the dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WelcomeAction {
    CreateNew,
    ImportGedcom,
    OpenRecent(PathBuf),
}

/// Moves `path` to the front of the recent list: duplicates collapse to a
/// single entry at the front and the list never grows past `cap`.
pub fn push_recent(recent: &mut Vec<PathBuf>, path: impl Into<PathBuf>, cap: usize) {
    let path = path.into();
    recent.retain(|existing| existing != &path);
    recent.insert(0, path);
    recent.truncate(cap);
}

/// Renders the welcome dashboard inside a `CentralPanel` and returns the
/// action the user picked, if any.
pub fn show(ui: &mut Ui, recent: &[PathBuf]) -> Option<WelcomeAction> {
    let mut action = None;
    CentralPanel::default().show(ui, |ui| {
        let rows = recent.len().min(RECENT_CAP) as f32 + 1.0;
        let estimated = BASE_CARD_HEIGHT + rows * RECENT_ROW_HEIGHT;
        let top_space = ((ui.available_height() - estimated) * 0.5).max(MIN_TOP_SPACE);
        ui.vertical_centered(|ui| {
            ui.add_space(top_space);
            ui.allocate_ui(vec2(CARD_WIDTH, 0.0), |ui| {
                card(ui, recent, &mut action);
            });
        });
    });
    action
}

fn card(ui: &mut Ui, recent: &[PathBuf], action: &mut Option<WelcomeAction>) {
    Frame::default()
        .corner_radius(CornerRadius::same(CARD_RADIUS))
        .fill(ui.visuals().window_fill())
        .stroke(Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .inner_margin(Margin::symmetric(CARD_MARGIN_X, CARD_MARGIN_Y))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                logo(ui);
                ui.add_space(12.0);
                ui.heading("Stemma Flow");
                ui.add_space(4.0);
                ui.label(RichText::new(TAGLINE).weak().size(13.0));
            });
            ui.add_space(20.0);

            let width = ui.available_width();
            let create = egui::Button::new(
                RichText::new("+ Create New Family Tree")
                    .size(15.0)
                    .strong(),
            )
            .fill(ACCENT);
            if ui.add_sized([width, BUTTON_HEIGHT], create).clicked() {
                *action = Some(WelcomeAction::CreateNew);
            }
            ui.add_space(8.0);
            let import = egui::Button::new(RichText::new("Import GEDCOM File").size(15.0).strong())
                .fill(ACCENT);
            if ui.add_sized([width, BUTTON_HEIGHT], import).clicked() {
                *action = Some(WelcomeAction::ImportGedcom);
            }

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(RichText::new("Recent trees").weak().size(12.0));
            ui.add_space(4.0);
            if recent.is_empty() {
                ui.label(RichText::new("No recent trees yet").weak().size(12.0));
            }
            for path in recent.iter().take(RECENT_CAP) {
                let name = display_name(path);
                let row = ui
                    .add(egui::Button::new(RichText::new(name).size(13.0)).frame(false))
                    .on_hover_text(path.display().to_string());
                if row.clicked() {
                    *action = Some(WelcomeAction::OpenRecent(path.clone()));
                }
            }
        });
}

/// Draws the logo: a spouse bar, a drop line and three nodes, echoing the
/// canvas motif.
fn logo(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(LOGO_SIZE, LOGO_SIZE), Sense::hover());
    let painter = ui.painter_at(rect);
    let radius = 8.0;
    let origin = rect.left_top();
    let left = origin + vec2(0.30 * rect.width(), 0.32 * rect.height());
    let right = origin + vec2(0.70 * rect.width(), 0.32 * rect.height());
    let child = origin + vec2(0.50 * rect.width(), 0.76 * rect.height());
    let bar_mid = origin + vec2(0.50 * rect.width(), 0.32 * rect.height());
    let stroke = Stroke::new(2.0, CONNECTOR);

    painter.line_segment(
        [left + vec2(radius, 0.0), right - vec2(radius, 0.0)],
        stroke,
    );
    painter.line_segment([bar_mid, child - vec2(0.0, radius)], stroke);
    painter.circle_filled(left, radius, ACCENT);
    painter.circle_filled(right, radius, ACCENT);
    painter.circle_filled(child, radius, ACCENT);
}

/// The file name shown for a tree path in lists and menus.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("/tmp/{name}.db"))
    }

    #[test]
    fn push_recent_deduplicates_front_and_caps() {
        let mut recent = Vec::new();
        push_recent(&mut recent, path("a"), 3);
        push_recent(&mut recent, path("b"), 3);
        push_recent(&mut recent, path("a"), 3);
        assert_eq!(
            recent,
            vec![path("a"), path("b")],
            "a duplicate moves to the front"
        );

        push_recent(&mut recent, path("c"), 3);
        push_recent(&mut recent, path("d"), 3);
        assert_eq!(
            recent,
            vec![path("d"), path("c"), path("a")],
            "the list is capped and the oldest entry drops off"
        );
    }

    #[test]
    fn push_recent_with_zero_cap_stays_empty() {
        let mut recent = vec![path("a")];
        push_recent(&mut recent, path("b"), 0);
        assert!(recent.is_empty());
    }

    #[test]
    fn display_name_prefers_the_file_name() {
        assert_eq!(display_name(Path::new("/tmp/family.db")), "family.db");
        assert_eq!(display_name(Path::new("/")), "/");
    }

    #[test]
    fn dashboard_renders_headless_without_taking_actions() {
        let ctx = Context::default();
        let recent = vec![path("a"), path("b")];
        let mut captured = None;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            captured = show(ui, &recent);
        });
        output.textures_delta.clear();
        assert_eq!(captured, None, "no pointer interaction picks nothing");
        assert!(!output.shapes.is_empty(), "the dashboard paints");
    }

    #[test]
    fn dashboard_renders_with_no_recent_trees() {
        let ctx = Context::default();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let _ = show(ui, &[]);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the empty state paints");
    }
}
