//! The main egui window: menu bar, status bar, welcome dashboard and the
//! interactive canvas.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::{App, Frame};
use egui::{Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Ui, Vec2, vec2};

use super::canvas::ViewportState;
use super::connectors::paint_connectors;
use super::dialogs;
use super::inspector::{
    self, InspectorAction, InspectorForm, RelationForm, RelationKind, RelationOutcome,
};
use super::layout::{self, TreeScene};
use super::nodes::{hit_test, paint_nodes};
use super::tabs;
use super::watchlist::{self, PersonRow, WatchEvent};
use super::welcome::{self, WelcomeAction};
use crate::app::{self, StemmaApp, UiStatus};
use crate::db::people::{PersonDetails, VitalDates};
use crate::db::{self, FamilyTree};

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
    tab: tabs::AppTab,
    watch: watchlist::WatchState,
    tree: FamilyTree,
    dates: HashMap<String, VitalDates>,
    rows: Vec<PersonRow>,
    inspector: InspectorForm,
    inspected: Option<String>,
    pending_relation: Option<RelationForm>,
}

impl TreeWindow {
    /// Restores the recent-trees list from `storage` (when the persistence
    /// feature is on), reopens the most recent entry when its database
    /// still loads, and loads the initial scene.
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
            tab: tabs::AppTab::default(),
            watch: watchlist::WatchState::default(),
            tree: FamilyTree::default(),
            dates: HashMap::new(),
            rows: Vec::new(),
            inspector: InspectorForm::default(),
            inspected: None,
            pending_relation: None,
        };
        if let Some(last) = window.recent.first().cloned() {
            if loadable(&last) {
                window.app.activate_tree(last);
            } else {
                window.recent.remove(0);
            }
        }
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

    /// Reloads the scene without re-centring the viewport, so edits keep
    /// the user's current pan and zoom.
    fn reload_scene_quiet(&mut self) {
        self.reload_data();
    }

    /// Loads people, families, vitals and dates from the active database,
    /// refreshes the watch-list rows and queues a re-centre of the viewport.
    fn reload_scene(&mut self) {
        self.reload_data();
        self.pending_framing = true;
    }

    fn reload_data(&mut self) {
        let loaded = self
            .app
            .active_db()
            .and_then(|path| db::open_connection(path).ok())
            .and_then(|conn| {
                let tree = db::load_family_tree(&conn).ok()?;
                let dates = db::people::load_vital_dates(&conn).ok()?;
                Some((tree, dates))
            });
        let (tree, dates) = loaded.unwrap_or_else(|| (FamilyTree::default(), HashMap::new()));
        self.scene = layout::build_scene(&tree);
        self.rows = watchlist::rows(&tree, &dates);
        self.tree = tree;
        self.dates = dates;
        self.watch.mark_dirty();
        self.seen_db = self.app.active_db().map(Path::to_path_buf);
        self.seen_generation = self.app.import_generation();
    }

    fn top_panel(&mut self, ui: &mut Ui, frame: &mut Frame) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Family Tree\u{2026}").clicked() {
                    ui.close();
                    self.create_new_tree(frame);
                }
                if ui.button("Open Tree\u{2026}").clicked() {
                    ui.close();
                    self.open_tree(frame);
                }
                if ui.button("Import GEDCOM\u{2026}").clicked() {
                    ui.close();
                    if self.app.active_db().is_some() {
                        self.app.begin_import();
                    } else {
                        self.app.begin_import_new();
                    }
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
            if let Some(path) = self.app.active_db() {
                let label = format!("Active Tree: {}", welcome::display_name(path));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(label).weak());
                });
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
            self.open_tree_at(path, frame);
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
            WelcomeAction::OpenTree => self.open_tree(frame),
            WelcomeAction::ImportGedcom => self.app.begin_import_new(),
            WelcomeAction::OpenRecent(path) => self.open_tree_at(path, frame),
        }
    }

    /// Inline open flow: pick an existing database file and switch to it.
    fn open_tree(&mut self, frame: &mut Frame) {
        let Some(path) = dialogs::pick_tree_open_path() else {
            self.app.set_status(UiStatus::Cancelled);
            return;
        };
        self.open_tree_at(path, frame);
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

    /// Opens the tree at `path`, dropping recent entries whose file no
    /// longer loads.
    fn open_tree_at(&mut self, path: PathBuf, frame: &mut Frame) {
        if !loadable(&path) {
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

    /// Renders the Watch List tab and applies the grid's events.
    fn watch_list(&mut self, ui: &mut Ui) {
        let events = watchlist::show(
            ui,
            &mut self.watch,
            &self.rows,
            self.app.selected_person_id(),
        );
        for event in events {
            match event {
                WatchEvent::Select(person_id) => self.app.set_selected_person(Some(person_id)),
                WatchEvent::AddPerson => self.add_person(),
                WatchEvent::Delete(person_id) => self.delete_person(&person_id),
            }
        }
    }

    /// Instant add: inserts an unnamed person and focuses it.
    fn add_person(&mut self) {
        match self.app.add_person(&PersonDetails::unnamed()) {
            Ok(person_id) => {
                self.app.set_selected_person(Some(person_id));
                self.reload_scene();
            }
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    fn delete_person(&mut self, person_id: &str) {
        match self.app.delete_person(person_id) {
            Ok(()) => self.reload_scene(),
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    /// Renders the inspector side panel for the selected person.
    fn person_inspector(&mut self, ui: &mut Ui) {
        self.sync_inspector_form();
        let Some(person_id) = self.app.selected_person_id().map(str::to_string) else {
            return;
        };
        let person = self
            .tree
            .people
            .iter()
            .find(|person| person.id == person_id);
        let connections = inspector::connections(&self.tree, &person_id);
        if let Some(action) = inspector::show(ui, person, &mut self.inspector, connections) {
            match action {
                InspectorAction::Save(details) => self.edit_person(&details),
                InspectorAction::AddParent => {
                    self.open_relation(&person_id, RelationKind::Parent);
                }
                InspectorAction::AddSpouse => {
                    self.open_relation(&person_id, RelationKind::Spouse);
                }
                InspectorAction::AddChild => {
                    self.open_relation(&person_id, RelationKind::Child);
                }
            }
        }
    }

    /// Refills the buffer whenever the selection changes.
    fn sync_inspector_form(&mut self) {
        let selected = self.app.selected_person_id().map(str::to_string);
        if selected == self.inspected {
            return;
        }
        self.inspected = selected;
        let person = self.inspected.as_ref().and_then(|person_id| {
            self.tree
                .people
                .iter()
                .find(|person| person.id == *person_id)
        });
        self.inspector = person
            .map(|person| InspectorForm::from_person(person, &self.dates))
            .unwrap_or_default();
    }

    /// Persists the buffered profile through the app layer.
    fn edit_person(&mut self, details: &PersonDetails) {
        let Some(person_id) = self.app.selected_person_id().map(str::to_string) else {
            return;
        };
        match self.app.edit_person(&person_id, details) {
            Ok(()) => self.reload_scene_quiet(),
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    fn open_relation(&mut self, target: &str, kind: RelationKind) {
        let target_label = self
            .tree
            .people
            .iter()
            .find(|person| person.id == target)
            .map(inspector::person_label)
            .unwrap_or_else(|| "(unnamed)".to_string());
        self.pending_relation = Some(RelationForm::new(target, kind, &target_label));
    }

    fn show_relation_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.pending_relation.take() else {
            return;
        };
        match inspector::relation_window(ctx, &mut form) {
            None => self.pending_relation = Some(form),
            Some(RelationOutcome::Cancel) => {}
            Some(RelationOutcome::Submit) => self.submit_relation(form),
        }
    }

    fn submit_relation(&mut self, form: RelationForm) {
        let details = form.details.to_details();
        let result = match form.kind {
            RelationKind::Parent => self.app.add_parent(&form.target, form.role, &details),
            RelationKind::Spouse => self.app.add_spouse(&form.target, &details),
            RelationKind::Child => self.app.add_child(&form.target, &details),
        };
        match result {
            Ok(_) => self.reload_scene(),
            Err(message) => self.app.set_status(UiStatus::Error(message)),
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

        egui::Panel::top("menu_bar").show(ui, |ui| self.top_panel(ui, frame));
        if self.app.active_db().is_some() {
            egui::Panel::top("tab_bar").show(ui, |ui| tabs::show(ui, &mut self.tab));
        }
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.app.active_db().is_some() && self.app.selected_person_id().is_some() {
            egui::Panel::right("person_inspector")
                .min_size(240.0)
                .default_size(260.0)
                .show(ui, |ui| self.person_inspector(ui));
        }
        if self.app.active_db().is_some() {
            egui::CentralPanel::default().show(ui, |ui| match self.tab {
                tabs::AppTab::Graph => self.canvas(ui),
                tabs::AppTab::WatchList => self.watch_list(ui),
            });
        } else {
            self.welcome(ui, frame);
        }
        if self.app.active_db().is_some() {
            self.show_relation_window(ui.ctx());
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

/// True when `path` is a readable database carrying the stemma-flow schema.
fn loadable(path: &Path) -> bool {
    db::open_connection(path)
        .map(|conn| db::load_family_tree(&conn).is_ok())
        .unwrap_or(false)
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
    fn loadable_rejects_files_that_are_not_databases() {
        let junk = temp_path("db");
        std::fs::write(&junk, b"this is not sqlite").expect("junk file must be writable");
        assert!(!loadable(&junk), "garbage bytes are not a tree");

        let good = temp_path("db");
        import_people(
            &good,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );
        assert!(loadable(&good), "an imported database opens");

        let _ = std::fs::remove_file(&junk);
        let _ = std::fs::remove_file(&good);
    }

    #[test]
    fn launch_reopens_the_last_tree_when_it_still_loads() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut storage = MapStorage::default();
        eframe::set_value(&mut storage, RECENT_TREES_KEY, &vec![db_file.clone()]);

        let window = TreeWindow::new(StemmaApp::new(), Some(&storage));
        assert_eq!(window.app.active_db(), Some(db_file.as_path()));
        assert_eq!(
            window.scene.nodes.len(),
            1,
            "the scene loads during construction"
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn launch_falls_back_to_the_dashboard_when_the_last_tree_is_gone() {
        let missing = temp_path("db");

        let mut storage = MapStorage::default();
        eframe::set_value(&mut storage, RECENT_TREES_KEY, &vec![missing.clone()]);

        let window = TreeWindow::new(StemmaApp::new(), Some(&storage));
        assert_eq!(window.app.active_db(), None, "the dashboard shows instead");
        assert!(window.recent.is_empty(), "the dead entry is dropped");
    }

    #[test]
    fn recent_trees_survive_a_storage_roundtrip() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut storage = MapStorage::default();
        let mut window = TreeWindow::new(StemmaApp::new(), None);
        welcome::push_recent(&mut window.recent, "/tmp/tree-a.db", welcome::RECENT_CAP);
        welcome::push_recent(&mut window.recent, db_file.clone(), welcome::RECENT_CAP);
        App::save(&mut window, &mut storage);

        let restored = TreeWindow::new(StemmaApp::new(), Some(&storage));
        assert_eq!(
            restored.recent, window.recent,
            "the saved list comes back through eframe persistence"
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn adding_a_person_selects_it_and_refreshes_the_watch_rows() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let before = window.rows.len();
        let generation = window.app.import_generation();

        window.add_person();

        assert_eq!(window.rows.len(), before + 1, "the grid sees the new row");
        let selected = window
            .app
            .selected_person_id()
            .expect("the new person is selected");
        assert!(
            window.rows.iter().any(|row| row.id == selected),
            "the selection points at a row"
        );
        assert!(
            window.app.import_generation() > generation,
            "the scene generation bumped"
        );
        assert!(
            matches!(window.app.status(), UiStatus::Success(_)),
            "the status reports success"
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn deleting_the_selected_person_clears_the_selection() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one imported person").id.clone();
        window.app.set_selected_person(Some(person_id.clone()));

        window.delete_person(&person_id);

        assert!(window.rows.is_empty(), "the row is gone");
        assert_eq!(
            window.app.selected_person_id(),
            None,
            "selection cleared with the person"
        );
        assert!(matches!(window.app.status(), UiStatus::Success(_)));

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn saving_inspector_edits_updates_the_row_and_vitals() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one imported person").id.clone();
        window.app.set_selected_person(Some(person_id));
        window.sync_inspector_form();
        assert_eq!(window.inspector.given_name, "Johan", "buffer synced");

        window.inspector.given_name = "Hans".to_string();
        window.inspector.birth = "1870".to_string();
        let details = window.inspector.to_details();
        window.edit_person(&details);

        let row = window.rows.first().expect("the row survived");
        assert_eq!(row.given_name, "Hans");
        assert_eq!(row.birth.as_deref(), Some("1870"), "vitals reloaded");
        assert!(matches!(window.app.status(), UiStatus::Success(_)));

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn submitting_a_parent_relation_links_both_people() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one imported person").id.clone();
        window.app.set_selected_person(Some(person_id.clone()));

        let mut form = RelationForm::new(&person_id, RelationKind::Parent, "Johan Senior");
        form.details.given_name = "Johan Senior".to_string();
        form.details.surname = "Ahlberg".to_string();
        window.submit_relation(form);

        assert_eq!(window.tree.people.len(), 2, "the parent joined the tree");
        assert_eq!(
            inspector::connections(&window.tree, &person_id).parents,
            1,
            "the child gained one parent"
        );
        assert!(matches!(window.app.status(), UiStatus::Success(_)));
        assert_eq!(
            window.app.selected_person_id(),
            Some(person_id.as_str()),
            "focus stays on the child"
        );

        let _ = std::fs::remove_file(&db_file);
    }
}
