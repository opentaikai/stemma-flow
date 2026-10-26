//! The main egui window: menu bar, status bar, welcome dashboard and the
//! interactive canvas.

mod actions;
mod lifecycle;
mod loading;
mod panels;

use loading::{empty_scene, loadable};

use super::canvas::ViewportState;
use super::citations;
use super::import_report;
use super::inspector::InspectorForm;
use super::inspector::RelationForm;
use super::layout::TreeScene;
use super::repositories;
use super::settings;
use super::sources;
use super::tabs;
use super::watchlist;
use super::watchlist::PersonRow;
use crate::app::StemmaApp;
use crate::app::UiStatus;
use crate::config::AppConfig;
use crate::db::FamilyTree;
use crate::db::MarriageInfo;
use crate::db::people::VitalDates;
use crate::gedcom::ImportReport;
use eframe::{App, Frame};
use egui::Color32;
use egui::Ui;
use std::collections::HashMap;
use std::path::PathBuf;

const HINT_TEXT: &str = "Import a GEDCOM file to begin";
const HINT_COLOR: Color32 = Color32::from_gray(150);
const RECENT_TREES_KEY: &str = "recent_trees";

/// Root widget owning the canvas view state, the recent-trees list and the
/// scene rebuilt from SQLite whenever the active tree changes.
pub struct TreeWindow {
    pub(super) app: StemmaApp,
    pub(super) viewport: ViewportState,
    pub(super) scene: TreeScene,
    pub(super) seen_db: Option<PathBuf>,
    pub(super) seen_generation: u64,
    pub(super) recent: Vec<PathBuf>,
    pub(super) pending_framing: bool,
    pub(super) drag_started_on_node: bool,
    pub(super) tab: tabs::AppTab,
    pub(super) watch: watchlist::WatchState,
    pub(super) sources: sources::SourcesState,
    pub(super) citations: citations::CitationsState,
    pub(super) repositories: repositories::RepositoriesState,
    pub(super) tree: FamilyTree,
    pub(super) dates: HashMap<String, VitalDates>,
    pub(super) marriages: HashMap<String, MarriageInfo>,
    pub(super) rows: Vec<PersonRow>,
    pub(super) inspector: InspectorForm,
    pub(super) inspected: Option<String>,
    pub(super) pending_relation: Option<RelationForm>,
    pub(super) config: AppConfig,
    pub(super) show_settings: bool,
    pub(super) pending_apply: bool,
    pub(super) import_report: Option<ImportReport>,
    pub(super) show_import_report: bool,
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
            sources: sources::SourcesState::default(),
            citations: citations::CitationsState::default(),
            repositories: repositories::RepositoriesState::default(),
            tree: FamilyTree::default(),
            dates: HashMap::new(),
            marriages: HashMap::new(),
            rows: Vec::new(),
            inspector: InspectorForm::default(),
            inspected: None,
            pending_relation: None,
            config: AppConfig::load(),
            show_settings: false,
            pending_apply: true,
            import_report: None,
            show_import_report: false,
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
}

impl App for TreeWindow {
    fn ui(&mut self, ui: &mut Ui, frame: &mut Frame) {
        self.apply_pending_config(ui.ctx());
        self.app.poll();
        self.sync_scene();
        if let Some(report) = self.app.take_new_import_report()
            && !report.warnings.is_empty()
        {
            self.import_report = Some(report);
            self.show_import_report = true;
        }
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
                .min_size(260.0)
                .default_size(300.0)
                .show(ui, |ui| self.person_inspector(ui));
        }
        if self.app.active_db().is_some() {
            egui::CentralPanel::default().show(ui, |ui| match self.tab {
                tabs::AppTab::Graph => self.canvas(ui),
                tabs::AppTab::WatchList => self.watch_list(ui),
                tabs::AppTab::Sources => self.sources_tab(ui),
                tabs::AppTab::Repositories => self.repositories_tab(ui),
            });
        } else {
            self.welcome(ui, frame);
        }
        if self.app.active_db().is_some() {
            self.show_relation_window(ui.ctx());
        }
        if self.show_settings
            && let Some(error) = settings::show(ui.ctx(), &mut self.config, &mut self.show_settings)
        {
            self.app.set_status(UiStatus::Error(error));
        }
        if self.show_import_report
            && let Some(report) = self.import_report.as_ref()
        {
            import_report::show(ui.ctx(), report, &mut self.show_import_report);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, RECENT_TREES_KEY, &self.recent);
    }
}

#[cfg(test)]
mod tests {
    use super::loading::{load_error, loadable, pan_centering};
    use super::*;
    use crate::app::JobEvent;
    use crate::db;
    use crate::db::people::ParentRole;
    use crate::db::sources::SourceDraft;
    use crate::gui::inspector;
    use crate::gui::inspector::InspectorAction;
    use crate::gui::inspector::RelationKind;
    use crate::gui::repositories::RepositoriesEvent;
    use crate::gui::sources::SourcesEvent;
    use crate::gui::welcome;
    use crate::model::quality::SourceQuality;
    use egui::{Pos2, Rect, vec2};
    use std::collections::HashMap;
    use std::path::Path;

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
    fn first_frame_fits_the_whole_scene_into_the_viewport() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 @I2@ INDI\n1 NAME Maria /Ekdahl/\n\
             0 @I3@ INDI\n1 NAME Elsa /Ahlberg/\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        assert!(window.pending_framing, "a fresh scene wants framing");
        assert_eq!(window.scene.nodes.len(), 3);

        let viewport_rect = Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0));
        window.fit_scene(viewport_rect);

        for node in &window.scene.nodes {
            let screen = window.viewport.screen_rect(node.bounds, viewport_rect);
            assert!(
                screen.min.x >= viewport_rect.min.x - 0.5
                    && screen.max.x <= viewport_rect.max.x + 0.5
                    && screen.min.y >= viewport_rect.min.y - 0.5
                    && screen.max.y <= viewport_rect.max.y + 0.5,
                "node {} must be visible after framing",
                node.person_id
            );
        }
        let _ = std::fs::remove_file(&db_file);
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

        let foreign = temp_path("db");
        {
            let conn = db::open_connection(&foreign).expect("foreign file must open");
            conn.execute_batch("CREATE TABLE other(x TEXT)")
                .expect("unrelated schema must be writable");
        }
        let reason = load_error(&foreign).expect("an unrelated sqlite file is not a tree");
        assert!(
            reason.contains("people"),
            "the rejection names the missing marker: {reason}"
        );
        {
            let conn = db::open_connection(&foreign).expect("foreign file must reopen");
            let planted = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'people'",
                    [],
                    |_| Ok(()),
                )
                .is_ok();
            assert!(!planted, "a foreign database must not gain our schema");
        }

        let good = temp_path("db");
        import_people(
            &good,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );
        assert!(loadable(&good), "an imported database opens");

        let _ = std::fs::remove_file(&junk);
        let _ = std::fs::remove_file(&foreign);
        let _ = std::fs::remove_file(&good);
    }

    #[test]
    fn loadable_upgrades_a_legacy_database() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );
        {
            let conn = db::open_connection(&db_file).expect("connection opens");
            conn.execute_batch("ALTER TABLE people DROP COLUMN notes")
                .expect("simulated legacy schema must be writable");
            assert!(
                db::load_family_tree(&conn).is_err(),
                "a pre-notes database fails to load before migration"
            );
        }

        assert!(loadable(&db_file), "opening migrates the schema in place");

        let conn = db::open_connection(&db_file).expect("connection reopens");
        let has_notes = conn
            .query_row(
                "SELECT 1 FROM pragma_table_info('people') WHERE name = 'notes'",
                [],
                |_| Ok(()),
            )
            .is_ok();
        assert!(has_notes, "the notes column comes back on open");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM people", [], |row| row.get(0))
            .expect("rows must be countable");
        assert_eq!(count, 1, "rows survive the migration");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn opens_the_local_family_tree_db_when_present() {
        // Private database in gitignored `test_data/`; skipped in CI.
        let Ok(entries) = std::fs::read_dir("test_data") else {
            return;
        };
        let db_file = entries
            .flatten()
            .find(|entry| entry.path().extension().is_some_and(|ext| ext == "db"));
        let Some(db_file) = db_file else {
            return;
        };
        assert!(
            loadable(&db_file.path()),
            "test_data/*.db must open: {:?}",
            load_error(&db_file.path())
        );
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
    fn startup_applies_the_loaded_config_once() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.config.theme = crate::config::ThemeMode::Dark;
        window.pending_apply = true;

        let ctx = egui::Context::default();
        window.apply_pending_config(&ctx);

        assert_eq!(ctx.theme(), egui::Theme::Dark, "settings hit the context");
        assert!(!window.pending_apply, "applied exactly once");
        window.apply_pending_config(&ctx);
        assert!(!window.pending_apply);

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
    fn a_blank_draft_cannot_be_saved_until_named() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.add_person();
        let person_id = window
            .app
            .selected_person_id()
            .expect("the draft is selected")
            .to_string();
        window.sync_inspector_form();
        assert!(
            !window.inspector.to_details().has_name(),
            "the draft starts unnamed"
        );

        let blank = window.inspector.to_details();
        window.edit_person(&blank);
        assert!(
            matches!(window.app.status(), UiStatus::Error(message) if message.contains("first or last name")),
            "an unnamed draft is rejected"
        );

        window.inspector.given_name = "New".to_string();
        let named = window.inspector.to_details();
        window.edit_person(&named);
        assert!(matches!(window.app.status(), UiStatus::Success(_)));
        let person = window
            .tree
            .people
            .iter()
            .find(|person| person.id == person_id)
            .expect("the draft is still there");
        assert_eq!(person.given_name, "New");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn saving_a_source_refreshes_the_sources_list() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.reload_sources();
        assert!(window.sources.loaded);
        assert_eq!(window.sources.templates.len(), 5);
        assert!(window.sources.list.is_empty());

        let mut draft = SourceDraft {
            template_key: Some("book".to_string()),
            ..SourceDraft::default()
        };
        draft
            .fields
            .insert("title".to_string(), "Family History".to_string());
        let id = window.app.save_source(&draft).expect("source saves");
        window.sources.selected = Some(id);
        window.reload_sources();

        assert_eq!(window.sources.list.len(), 1);
        assert_eq!(window.sources.list[0].title, "Family History");
        assert_eq!(
            window.sources.draft.fields.get("title").map(String::as_str),
            Some("Family History")
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn citations_load_and_save_for_the_selected_person() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();
        window.app.set_selected_person(Some(person_id.clone()));

        let source_id = window
            .app
            .save_source(&SourceDraft {
                title: "Parish Book".to_string(),
                ..SourceDraft::default()
            })
            .expect("source saves");
        window
            .app
            .add_citation(&source_id, &person_id, "BIRTH", Some("12"))
            .expect("citation adds");

        window.sync_citations();
        assert_eq!(window.citations.citations.len(), 1);
        assert_eq!(window.citations.citations[0].source_title, "Parish Book");
        assert_eq!(window.citations.citations[0].page, "12");
        assert_eq!(window.citations.citations[0].usage, 1);
        assert_eq!(window.citations.sources.len(), 1);
        assert_eq!(window.citations.targets.len(), 2);
        assert_eq!(window.citations.targets[0].key, "BIRTH");

        window.citations.add_source = Some(source_id.clone());
        window.citations.loaded = false;
        window.sync_citations();
        assert_eq!(window.citations.choices.len(), 1);
        assert_eq!(
            window.citations.choices_source.as_deref(),
            Some(source_id.as_str())
        );

        let citation_id = window.citations.citations[0].id.clone();
        window.apply_inspector_action(
            InspectorAction::AttachCitation {
                citation_id: citation_id.clone(),
                target: "DEATH".to_string(),
            },
            &person_id,
        );
        window.citations.loaded = false;
        window.sync_citations();
        assert_eq!(
            window.citations.citations.len(),
            1,
            "the shared citation is grouped into one draft"
        );
        assert_eq!(window.citations.citations[0].uses.len(), 2);
        assert_eq!(window.citations.citations[0].usage, 2);
        assert_eq!(window.citations.choices[0].usage, 2);

        let mut edit = window.citations.citations[0].to_edit();
        edit.quality_source = Some(SourceQuality::Original);
        window.apply_inspector_action(
            InspectorAction::SaveCitations(Box::new(vec![edit])),
            &person_id,
        );
        window.citations.loaded = false;
        window.sync_citations();
        assert_eq!(
            window.citations.citations[0].quality_source,
            Some(SourceQuality::Original)
        );

        let death_event = window.citations.citations[0]
            .uses
            .iter()
            .find(|use_| use_.event_type == "DEATH")
            .expect("death use")
            .event_id
            .clone();
        window.apply_inspector_action(
            InspectorAction::DetachCitation {
                citation_id,
                event_id: death_event,
            },
            &person_id,
        );
        window.citations.loaded = false;
        window.sync_citations();
        assert_eq!(window.citations.citations[0].uses.len(), 1);
        assert_eq!(window.citations.citations[0].usage, 1);

        window.apply_inspector_action(
            InspectorAction::LoadCitationChoices(source_id.clone()),
            &person_id,
        );
        assert_eq!(
            window.citations.choices_source.as_deref(),
            Some(source_id.as_str())
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn repository_events_refresh_the_grid_and_editor() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.reload_repositories();
        assert!(window.repositories.loaded);
        let source_id = window
            .app
            .save_source(&SourceDraft {
                title: "Parish Book".to_string(),
                ..SourceDraft::default()
            })
            .expect("source saves");

        window.apply_repositories_event(RepositoriesEvent::New);
        window.repositories.draft.name = "Uppsala Archive".to_string();
        window.repositories.draft.address = "Main Street".to_string();
        window.repositories.draft.links = vec![crate::db::repositories::RepositoryLinkDraft {
            source_id: source_id.clone(),
            call_number: "12/34".to_string(),
            media_type: "film".to_string(),
        }];
        let draft = window.repositories.draft.clone();
        window.apply_repositories_event(RepositoriesEvent::Save(Box::new(draft)));

        assert_eq!(window.repositories.repositories.len(), 1);
        assert_eq!(window.repositories.repositories[0].source_count, 1);
        let id = window
            .repositories
            .selected
            .clone()
            .expect("the saved repository is selected");
        window.repositories.draft.name = "Renamed Archive".to_string();
        let draft = window.repositories.draft.clone();
        window.apply_repositories_event(RepositoriesEvent::Save(Box::new(draft)));
        assert_eq!(window.repositories.repositories[0].source_count, 1);
        let record = window
            .app
            .load_repository(&id)
            .expect("load")
            .expect("repository exists");
        assert_eq!(record.name, "Renamed Archive");
        assert_eq!(record.links.len(), 1, "metadata saves keep links");

        window.apply_repositories_event(RepositoriesEvent::Delete(id));
        assert!(window.repositories.repositories.is_empty());
        assert!(!window.repositories.editing);
        assert!(matches!(window.app.status(), UiStatus::Success(_)));

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn merging_sources_moves_citations_and_refreshes_the_list() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.reload_sources();
        let target_id = window
            .app
            .save_source(&SourceDraft {
                title: "Target".to_string(),
                ..SourceDraft::default()
            })
            .expect("target saves");
        let source_id = window
            .app
            .save_source(&SourceDraft {
                title: "Source".to_string(),
                ..SourceDraft::default()
            })
            .expect("source saves");
        let person_id = window.rows.first().expect("one person").id.clone();
        window
            .app
            .add_citation(&source_id, &person_id, "BIRTH", Some("12"))
            .expect("citation adds");
        window.sources.selected = Some(source_id.clone());
        window.reload_sources();
        window.sources.merge_open = true;

        window.apply_sources_event(SourcesEvent::Merge {
            source_id,
            target_id: target_id.clone(),
        });

        assert_eq!(window.sources.list.len(), 1);
        assert_eq!(window.sources.selected.as_deref(), Some(target_id.as_str()));
        assert_eq!(window.sources.draft.title, "Target");
        assert!(!window.sources.merge_open);
        assert!(!window.citations.loaded, "the inspector reloads citations");
        assert!(matches!(window.app.status(), UiStatus::Success(_)));

        window.app.set_selected_person(Some(person_id.clone()));
        window.sync_citations();
        assert_eq!(window.citations.citations.len(), 1);
        assert_eq!(window.citations.citations[0].source_title, "Target");
        assert_eq!(window.citations.citations[0].page, "12");

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
    fn closing_the_inspector_clears_the_selection() {
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
        assert!(window.app.selected_person_id().is_some());

        window.apply_inspector_action(InspectorAction::Close, &person_id);

        assert_eq!(
            window.app.selected_person_id(),
            None,
            "closing the panel clears the selection so it hides"
        );

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
        window.app.set_selected_person(Some(person_id.clone()));
        window.sync_inspector_form();
        assert_eq!(window.inspector.given_name, "Johan", "buffer synced");

        window.inspector.given_name = "Hans".to_string();
        window.inspector.birth = "1870".to_string();
        window.inspector.birth_place = "Cuckfield".to_string();
        window.inspector.notes = "Emigrated in 1870.".to_string();
        let details = window.inspector.to_details();
        window.edit_person(&details);

        let row = window.rows.first().expect("the row survived");
        assert_eq!(row.given_name, "Hans");
        assert_eq!(row.birth.as_deref(), Some("1870"), "vitals reloaded");
        let vitals = window.dates.get(&person_id).expect("vitals reloaded");
        assert_eq!(
            vitals.birth_place.as_deref(),
            Some("Cuckfield"),
            "places reload with the dates"
        );
        let person = window
            .tree
            .people
            .iter()
            .find(|person| person.id == person_id)
            .expect("person reloaded");
        assert_eq!(person.notes, "Emigrated in 1870.", "notes persist");
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
            inspector::connections(&window.tree, &person_id, &window.marriages)
                .parents
                .len(),
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

    #[test]
    fn linking_an_existing_person_as_a_spouse_updates_the_tree() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n\
             0 @I2@ INDI\n1 NAME Maria /Ekdahl/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let first = window.rows.first().expect("first person").id.clone();
        let second = window.rows.get(1).expect("second person").id.clone();

        window.link_existing_relation(&first, RelationKind::Spouse, &second, ParentRole::Father);

        let linked = window.tree.families.iter().any(|family| {
            let couple = [family.husband_id.as_deref(), family.wife_id.as_deref()];
            couple.contains(&Some(first.as_str())) && couple.contains(&Some(second.as_str()))
        });
        assert!(linked, "the two people share a family after linking");
        assert!(matches!(window.app.status(), UiStatus::Success(_)));

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn focusing_a_person_selects_it_and_recenters_the_view() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.tab = tabs::AppTab::WatchList;
        let person_id = window.rows.first().expect("one person").id.clone();

        window.focus_person(&person_id);

        assert_eq!(
            window.app.selected_person_id(),
            Some(person_id.as_str()),
            "the relative becomes the selection"
        );
        assert_eq!(window.tab, tabs::AppTab::Graph, "the graph opens");
        let node = window
            .scene
            .nodes
            .iter()
            .find(|node| node.person_id == person_id)
            .expect("the node is laid out");
        assert_eq!(
            window.viewport.pan,
            pan_centering(node.bounds.center(), window.viewport.zoom),
            "the viewport centres on the node"
        );

        let _ = std::fs::remove_file(&db_file);
    }
}
