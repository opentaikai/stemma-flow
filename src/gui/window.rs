//! The main egui window: toolbar, status bar and the interactive canvas.

use eframe::{App, Frame};
use egui::{Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Ui, Vec2, vec2};

use super::canvas::ViewportState;
use super::connectors::paint_connectors;
use super::layout::{self, TreeScene};
use super::nodes::{hit_test, paint_nodes};
use crate::app::{StemmaApp, UiStatus};
use crate::db;

const HINT_TEXT: &str = "Import a GEDCOM file to begin";
const HINT_COLOR: Color32 = Color32::from_gray(150);

/// Root widget owning the canvas view state and the scene rebuilt from the
/// SQLite database whenever an import completes.
pub struct TreeWindow {
    app: StemmaApp,
    viewport: ViewportState,
    scene: TreeScene,
    seen_generation: u64,
    pending_framing: bool,
    drag_started_on_node: bool,
}

impl TreeWindow {
    pub fn new(app: StemmaApp) -> Self {
        let mut window = Self {
            app,
            viewport: ViewportState::default(),
            scene: empty_scene(),
            seen_generation: 0,
            pending_framing: false,
            drag_started_on_node: false,
        };
        window.reload_scene();
        window
    }

    /// Rebuilds the scene when a background import bumped the generation.
    fn sync_scene(&mut self) {
        if self.app.import_generation() != self.seen_generation {
            self.reload_scene();
        }
    }

    /// Loads people, families and children from the database and queues an
    /// initial re-centre of the viewport.
    fn reload_scene(&mut self) {
        self.scene = db::open_connection(self.app.db_path())
            .ok()
            .and_then(|conn| db::load_family_tree(&conn).ok())
            .map(|tree| layout::build_scene(&tree))
            .unwrap_or_else(empty_scene);
        self.seen_generation = self.app.import_generation();
        self.pending_framing = true;
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let busy = self.app.is_busy();
            if ui
                .add_enabled(!busy, egui::Button::new("Import GEDCOM\u{2026}"))
                .clicked()
            {
                self.app.begin_import();
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Export GEDCOM\u{2026}"))
                .clicked()
            {
                self.app.begin_export();
            }
            ui.separator();
            match self.app.selected_person_id() {
                Some(person_id) => {
                    let name = self
                        .scene
                        .nodes
                        .iter()
                        .find(|node| node.person_id == person_id)
                        .map(|node| node.display_name.as_str())
                        .unwrap_or(person_id);
                    ui.label(format!("Selected: {name}"));
                }
                None => {
                    ui.label("No selection");
                }
            }
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| match self.app.status() {
            UiStatus::Idle => {
                ui.label("Ready");
            }
            UiStatus::Running(text) => {
                ui.spinner();
                ui.label(*text);
            }
            UiStatus::Success(message) => {
                ui.colored_label(Color32::from_rgb(34, 197, 94), message);
            }
            UiStatus::Cancelled => {
                ui.colored_label(Color32::from_gray(150), "Cancelled");
            }
            UiStatus::Error(message) => {
                ui.colored_label(Color32::from_rgb(239, 68, 68), message);
            }
        });
    }

    fn canvas(&mut self, ui: &mut Ui) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), Sense::click_and_drag());
        let viewport_rect = response.rect;
        if viewport_rect.width() <= 0.0 || viewport_rect.height() <= 0.0 {
            return;
        }

        if self.pending_framing {
            self.viewport.pan = pan_centering(self.scene.bounds.center(), self.viewport.zoom);
            self.pending_framing = false;
        }

        let (scroll_y, pointer_pos) =
            ui.input(|i| (i.smooth_scroll_delta.y, i.pointer.hover_pos()));
        if response.hovered()
            && let Some(cursor) = pointer_pos
        {
            self.viewport
                .apply_scroll_y(scroll_y, cursor, viewport_rect);
        }

        if response.drag_started_by(PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let canvas_pos = self.viewport.screen_to_canvas(pos, viewport_rect);
            self.drag_started_on_node = hit_test(&self.scene, canvas_pos).is_some();
        }

        let pointer_delta = ui.input(|i| i.pointer.delta());
        let pans = (response.dragged_by(PointerButton::Primary) && !self.drag_started_on_node)
            || response.dragged_by(PointerButton::Middle);
        if pans && pointer_delta != Vec2::ZERO {
            self.viewport.pan_by(pointer_delta);
        }

        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let canvas_pos = self.viewport.screen_to_canvas(pos, viewport_rect);
            let selected = hit_test(&self.scene, canvas_pos).map(|node| node.person_id.clone());
            self.app.set_selected_person(selected);
        }

        paint_connectors(&painter, &self.scene, &self.viewport, viewport_rect);
        paint_nodes(
            &painter,
            &self.scene,
            &self.viewport,
            viewport_rect,
            self.app.selected_person_id(),
        );

        if self.scene.nodes.is_empty() {
            painter.text(
                viewport_rect.center(),
                Align2::CENTER_CENTER,
                HINT_TEXT,
                FontId::proportional(16.0),
                HINT_COLOR,
            );
        }
    }
}

impl App for TreeWindow {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        self.app.poll();
        self.sync_scene();
        if self.app.is_busy() {
            ui.ctx().request_repaint();
        }

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| self.canvas(ui));
    }
}

fn empty_scene() -> TreeScene {
    TreeScene {
        nodes: Vec::new(),
        unions: Vec::new(),
        bounds: Rect::from_min_size(Pos2::ZERO, vec2(0.0, 0.0)),
    }
}

/// Pan that places `target` at the centre of the viewport (inverse of
/// [`ViewportState::canvas_to_screen`]).
fn pan_centering(target: Pos2, zoom: f32) -> Vec2 {
    Vec2::new(-target.x * zoom, -target.y * zoom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::JobEvent;

    fn temp_path(extension: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "stemma-flow-window-{}.{}",
            uuid::Uuid::new_v4(),
            extension
        ))
    }

    #[test]
    fn pan_centering_moves_the_target_to_the_viewport_centre() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), vec2(800.0, 600.0));
        let target = Pos2::new(120.0, 340.0);
        for zoom in [0.3, 1.0, 2.5] {
            let viewport = ViewportState {
                pan: pan_centering(target, zoom),
                zoom,
            };
            let screen = viewport.canvas_to_screen(target, rect);
            assert!(
                (screen - rect.center()).length() < 0.01,
                "zoom {zoom} must centre the target"
            );
        }
    }

    #[test]
    fn window_starts_with_an_empty_scene_when_the_database_is_new() {
        let path = temp_path("db");
        let window = TreeWindow::new(StemmaApp::new(&path));
        assert!(window.scene.nodes.is_empty());
        assert!(window.pending_framing, "first frame must centre the view");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sync_scene_reloads_after_an_import_event() {
        let db_file = temp_path("db");
        let ged_file = temp_path("ged");
        std::fs::write(
            &ged_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        )
        .expect("temp gedcom must be writable");
        let report = crate::app::perform_import(&db_file, &ged_file).expect("import must succeed");

        let mut window = TreeWindow::new(StemmaApp::new(&db_file));
        assert_eq!(
            window.scene.nodes.len(),
            1,
            "the imported person must be laid out"
        );

        window.app.apply_event(JobEvent::Imported(report));
        assert_eq!(
            window.seen_generation, 0,
            "the generation changed but the scene has not synced yet"
        );
        window.sync_scene();
        assert_eq!(window.seen_generation, window.app.import_generation());
        assert_eq!(window.scene.nodes.len(), 1, "scene stays consistent");

        let _ = std::fs::remove_file(&db_file);
        let _ = std::fs::remove_file(&ged_file);
    }
}
