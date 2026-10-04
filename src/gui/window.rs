//! The main egui window: menu bar, status bar, welcome dashboard and the
//! interactive canvas.

use std::path::{Path, PathBuf};

use eframe::{App, Frame};
use egui::{Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Ui, Vec2, vec2};

use super::canvas::ViewportState;
use super::connectors::paint_connectors;
use super::dialogs;
use super::layout::{self, TreeScene};
use super::nodes::{hit_test, paint_nodes};
use super::welcome::{self, WelcomeAction};
use crate::app::{self, StemmaApp, UiStatus};
use crate::db;

const HINT_TEXT: &str = "Import a GEDCOM file to begin";
const HINT_COLOR: Color32 = Color32::from_gray(150);
const RECENT_TREES_KEY: &str = "recent_trees";

/// Root widget owning the canvas view state, the recent-trees list and the
/// scene rebuilt from SQLite whenever the active tree changes.
pub struct TreeWindow {
    app: StemmaApp,
    viewport: ViewportState,
    scene: TreeScene,
    seen_db: Option<PathBuf>,
    seen_generation: u64,
    recent: Vec<PathBuf>,
    pending_framing: bool,
    drag_started_on_node: bool,
}

impl TreeWindow {
    /// Restores the recent-trees list from `storage` (when the persistence
    /// feature is on) and loads the initial scene.
    pub fn new(app: StemmaApp, storage: Option<&dyn eframe::Storage>) -> Self {
        let recent = storage
            .and_then(|storage| eframe::get_value::<Vec<PathBuf>>(storage, RECENT_TREES_KEY))
            .unwrap_or_default();
        let mut window = Self {
            app,
            viewport: ViewportState::default(),
            scene: empty_scene(),
            seen_db: None,
            seen_generation: 0,
            recent,
            pending_framing: false,
            drag_started_on_node: false,
        };
        window.reload_scene();
        window
    }

    /// Rebuilds the scene when a background import bumped the generation
    /// or the active tree switched to another database.
    fn sync_scene(&mut self) {
        let active = self.app.active_db().map(Path::to_path_buf);
        if active != self.seen_db || self.app.import_generation() != self.seen_generation {
            self.reload_scene();
        }
    }

    /// Loads people, families and children from the active database and
    /// queues an initial re-centre of the viewport.
    fn reload_scene(&mut self) {
        self.scene = self
            .app
            .active_db()
            .and_then(|path| db::open_connection(path).ok())
            .and_then(|conn| db::load_family_tree(&conn).ok())
            .map(|tree| layout::build_scene(&tree))
            .unwrap_or_else(empty_scene);
        self.seen_db = self.app.active_db().map(Path::to_path_buf);
        self.seen_generation = self.app.import_generation();
        self.pending_framing = true;
    }

    fn top_panel(&mut self, ui: &mut Ui, frame: &mut Frame) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Family Tree\u{2026}").clicked() {
                    ui.close();
                    self.create_new_tree(frame);
                }
                if ui.button("Import GEDCOM\u{2026}").clicked() {
                    ui.close();
                    self.app.begin_import_new();
                }
                let can_export = self.app.active_db().is_some() && !self.app.is_busy();
                if ui
                    .add_enabled(can_export, egui::Button::new("Export GEDCOM\u{2026}"))
                    .clicked()
                {
                    ui.close();
                    self.app.begin_export();
                }
                if !self.recent.is_empty() {
                    self.recent_menu(ui, frame);
                }
                if self.app.active_db().is_some() {
                    ui.separator();
                    if ui.button("Close Tree").clicked() {
                        ui.close();
                        self.app.close_tree();
                    }
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.close();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            if self.app.active_db().is_some() {
                ui.separator();
                let busy = self.app.is_busy();
                if ui
                    .add_enabled(!busy, egui::Button::new("Import GEDCOM\u{2026}"))
                    .clicked()
                {
                    self.app.begin_import();
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
            }
        });
    }

    fn recent_menu(&mut self, ui: &mut Ui, frame: &mut Frame) {
        let mut chosen = None;
        ui.menu_button("Recent", |ui| {
            for path in self.recent.clone().iter().take(welcome::RECENT_CAP) {
                let name = welcome::display_name(path);
                if ui
                    .button(name)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    chosen = Some(path.clone());
                    ui.close();
                }
            }
        });
        if let Some(path) = chosen {
            self.open_recent(path, frame);
        }
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

    fn welcome(&mut self, ui: &mut Ui, frame: &mut Frame) {
        let Some(action) = welcome::show(ui, &self.recent) else {
            return;
        };
        match action {
            WelcomeAction::CreateNew => self.create_new_tree(frame),
            WelcomeAction::ImportGedcom => self.app.begin_import_new(),
            WelcomeAction::OpenRecent(path) => self.open_recent(path, frame),
        }
    }

    /// Inline create flow: pick a destination, initialise the schema and
    /// switch to the canvas within the same frame.
    fn create_new_tree(&mut self, frame: &mut Frame) {
        let Some(path) = dialogs::prompt_new_tree_path() else {
            self.app.set_status(UiStatus::Cancelled);
            return;
        };
        match app::create_tree(&path) {
            Ok(()) => {
                self.app.activate_tree(path.clone());
                self.note_recent(path, frame);
            }
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    /// Opens a recent tree, dropping entries whose file no longer loads.
    fn open_recent(&mut self, path: PathBuf, frame: &mut Frame) {
        let loadable = db::open_connection(&path)
            .map(|conn| db::load_family_tree(&conn).is_ok())
            .unwrap_or(false);
        if !loadable {
            self.app
                .set_status(UiStatus::Error(format!("Cannot open {}", path.display())));
            self.recent.retain(|entry| *entry != path);
            self.save_recent(frame);
            return;
        }
        self.app.activate_tree(path.clone());
        self.note_recent(path, frame);
    }

    fn note_recent(&mut self, path: PathBuf, frame: &mut Frame) {
        welcome::push_recent(&mut self.recent, path, welcome::RECENT_CAP);
        self.save_recent(frame);
    }

    fn save_recent(&mut self, frame: &mut Frame) {
        if let Some(storage) = frame.storage_mut() {
            eframe::set_value(storage, RECENT_TREES_KEY, &self.recent);
        }
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
    fn ui(&mut self, ui: &mut Ui, frame: &mut Frame) {
        self.app.poll();
        self.sync_scene();
        if self.app.is_busy() {
            ui.ctx().request_repaint();
        }

        egui::Panel::top("toolbar").show(ui, |ui| self.top_panel(ui, frame));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.app.active_db().is_some() {
            egui::CentralPanel::default().show(ui, |ui| self.canvas(ui));
        } else {
            self.welcome(ui, frame);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, RECENT_TREES_KEY, &self.recent);
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
    use std::collections::HashMap;

    fn temp_path(extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "stemma-flow-window-{}.{}",
            uuid::Uuid::new_v4(),
            extension
        ))
    }

    fn import_people(db_file: &Path, ged: &str) {
        let ged_file = temp_path("ged");
        std::fs::write(&ged_file, ged).expect("temp gedcom must be writable");
        crate::app::perform_import(db_file, &ged_file).expect("import must succeed");
        let _ = std::fs::remove_file(&ged_file);
    }

    #[derive(Default)]
    struct MapStorage(HashMap<String, String>);

    impl eframe::Storage for MapStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_string(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
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
    fn window_starts_with_an_empty_scene_when_no_tree_is_active() {
        let window = TreeWindow::new(StemmaApp::new(), None);
        assert!(window.scene.nodes.is_empty());
        assert!(window.pending_framing, "first frame must centre the view");
    }

    #[test]
    fn window_ignores_a_database_file_that_cannot_be_read() {
        let path = temp_path("db");
        let mut app = StemmaApp::new();
        app.activate_tree(&path);
        let window = TreeWindow::new(app, None);
        assert!(
            window.scene.nodes.is_empty(),
            "an unreadable tree degrades to an empty scene"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sync_scene_reloads_after_an_import_event() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        assert_eq!(
            window.scene.nodes.len(),
            1,
            "the imported person must be laid out"
        );

        window
            .app
            .apply_event(JobEvent::Imported(crate::gedcom::ImportReport {
                people: 1,
                families: 0,
                child_links: 0,
                events: 0,
                citations: 0,
                warnings: Vec::new(),
            }));
        assert_eq!(
            window.seen_generation, 0,
            "the generation changed but the scene has not synced yet"
        );
        window.sync_scene();
        assert_eq!(window.seen_generation, window.app.import_generation());
        assert_eq!(window.scene.nodes.len(), 1, "scene stays consistent");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn sync_scene_reloads_when_the_active_tree_switches() {
        let db_one = temp_path("db");
        let db_two = temp_path("db");
        import_people(
            &db_one,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );
        import_people(
            &db_two,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Anna /Berg/\n0 @I2@ INDI\n1 NAME Bo /Berg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_one);
        let mut window = TreeWindow::new(app, None);
        assert_eq!(window.scene.nodes.len(), 1);

        window.app.activate_tree(&db_two);
        window.sync_scene();
        assert_eq!(
            window.scene.nodes.len(),
            2,
            "switching databases reloads without a generation bump"
        );
        assert_eq!(window.seen_db, Some(db_two.clone()));

        let _ = std::fs::remove_file(&db_one);
        let _ = std::fs::remove_file(&db_two);
    }

    #[test]
    fn recent_trees_survive_a_storage_roundtrip() {
        let mut storage = MapStorage::default();
        let mut window = TreeWindow::new(StemmaApp::new(), None);
        welcome::push_recent(&mut window.recent, "/tmp/tree-a.db", welcome::RECENT_CAP);
        welcome::push_recent(&mut window.recent, "/tmp/tree-b.db", welcome::RECENT_CAP);
        App::save(&mut window, &mut storage);

        let restored = TreeWindow::new(StemmaApp::new(), Some(&storage));
        assert_eq!(
            restored.recent, window.recent,
            "the saved list comes back through eframe persistence"
        );
    }
}
