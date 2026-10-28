//! The main egui window: menu bar, status bar, welcome dashboard and the
//! interactive canvas.

mod actions;
mod lifecycle;
mod loading;
mod panels;

use loading::{empty_scene, loadable};

use super::associations as associations_dialog;
use super::canvas::ViewportState;
use super::citations;
use super::fact_types;
use super::import_report;
use super::inspector::InspectorForm;
use super::inspector::ParticipantForm;
use super::inspector::RelationForm;
use super::layout::TreeScene;
use super::media;
use super::names;
use super::notes as notes_dialog;
use super::places;
use super::repositories;
use super::research;
use super::settings;
use super::sources;
use super::tabs;
use super::tasks;
use super::watchlist;
use super::watchlist::PersonRow;
use crate::app::StemmaApp;
use crate::app::UiStatus;
use crate::config::AppConfig;
use crate::db::FamilyTree;
use crate::db::MarriageInfo;
use crate::db::associations::AssociationRecord;
use crate::db::facts::PersonFact;
use crate::db::people::VitalDates;
use crate::db::privacy::PersonPrivacy;
use crate::db::settings::TreeSettings;
use crate::gedcom::ImportReport;
use crate::i18n;
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
    pub(super) places: places::PlacesState,
    pub(super) media: media::MediaState,
    pub(super) media_attach: Option<media::MediaAttachState>,
    pub(super) research: research::ResearchState,
    pub(super) task_attach: Option<tasks::TaskAttachState>,
    pub(super) names: names::NamesState,
    pub(super) notes: notes_dialog::NotesState,
    pub(super) fact_types: fact_types::FactTypesState,
    pub(super) sources: sources::SourcesState,
    pub(super) citations: citations::CitationsState,
    pub(super) repositories: repositories::RepositoriesState,
    pub(super) tree: FamilyTree,
    pub(super) dates: HashMap<String, VitalDates>,
    pub(super) marriages: HashMap<String, MarriageInfo>,
    pub(super) person_facts: HashMap<String, Vec<PersonFact>>,
    pub(super) associations: Vec<AssociationRecord>,
    pub(super) associations_owner: Option<String>,
    pub(super) pending_association: Option<associations_dialog::AssociationForm>,
    pub(super) tree_settings: TreeSettings,
    pub(super) person_privacy: Option<PersonPrivacy>,
    pub(super) rows: Vec<PersonRow>,
    pub(super) inspector: InspectorForm,
    pub(super) inspected: Option<String>,
    pub(super) pending_relation: Option<RelationForm>,
    pub(super) pending_participants: Option<ParticipantForm>,
    pub(super) config: AppConfig,
    pub(super) show_settings: bool,
    pub(super) show_export: bool,
    pub(super) export_options: crate::gedcom::ExportOptions,
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
            places: places::PlacesState::default(),
            media: media::MediaState::default(),
            media_attach: None,
            research: research::ResearchState::default(),
            task_attach: None,
            names: names::NamesState::default(),
            notes: notes_dialog::NotesState::default(),
            fact_types: fact_types::FactTypesState::default(),
            sources: sources::SourcesState::default(),
            citations: citations::CitationsState::default(),
            repositories: repositories::RepositoriesState::default(),
            tree: FamilyTree::default(),
            dates: HashMap::new(),
            marriages: HashMap::new(),
            person_facts: HashMap::new(),
            associations: Vec::new(),
            associations_owner: None,
            pending_association: None,
            tree_settings: TreeSettings::default(),
            person_privacy: None,
            rows: Vec::new(),
            inspector: InspectorForm::default(),
            inspected: None,
            pending_relation: None,
            pending_participants: None,
            config: AppConfig::load(),
            show_settings: false,
            show_export: false,
            export_options: crate::gedcom::ExportOptions::default(),
            pending_apply: true,
            import_report: None,
            show_import_report: false,
        };
        i18n::set_language(&window.config.language);
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
                tabs::AppTab::Places => self.places_tab(ui),
                tabs::AppTab::Media => self.media_tab(ui),
                tabs::AppTab::Research => self.research_tab(ui),
                tabs::AppTab::FactTypes => self.fact_types_tab(ui),
                tabs::AppTab::Sources => self.sources_tab(ui),
                tabs::AppTab::Repositories => self.repositories_tab(ui),
            });
        } else {
            self.welcome(ui, frame);
        }
        if self.app.active_db().is_some() {
            self.show_relation_window(ui.ctx());
            self.show_participant_window(ui.ctx());
            self.show_association_window(ui.ctx());
            self.show_media_attach(ui.ctx());
            self.show_task_attach(ui.ctx());
            self.show_notes(ui.ctx());
            self.show_export_dialog(ui.ctx());
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
    use crate::gui::fact_types::FactTypesEvent;
    use crate::gui::inspector;
    use crate::gui::inspector::InspectorAction;
    use crate::gui::inspector::RelationKind;
    use crate::gui::places::PlacesEvent;
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
                participants: 0,
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
            conn.execute_batch(
                "ALTER TABLE people ADD COLUMN notes TEXT NOT NULL DEFAULT '';
                 UPDATE people SET notes = 'Legacy note';
                 PRAGMA user_version = 12;",
            )
            .expect("simulated legacy schema must be writable");
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
        assert!(!has_notes, "the legacy notes column is dropped");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM people", [], |row| row.get(0))
            .expect("rows must be countable");
        assert_eq!(count, 1, "rows survive the migration");
        let note: String = conn
            .query_row("SELECT body FROM notes", [], |row| row.get(0))
            .expect("legacy note migrates");
        assert_eq!(note, "Legacy note");

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
    fn fact_type_events_save_custom_types_and_roles() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.reload_fact_types();
        assert_eq!(window.fact_types.types.len(), 61);

        window.apply_fact_types_event(FactTypesEvent::New);
        window.fact_types.draft.label = "Heirloom".to_string();
        window.fact_types.draft.category = "Other".to_string();
        window.fact_types.draft.gedcom_tag = "_HEIR".to_string();
        window.fact_types.draft.scope = crate::model::fact::FactScope::Person;
        window.fact_types.draft.kind = crate::model::fact::FactKind::Attribute;
        window.fact_types.draft.roles = vec![crate::db::facts::FactRoleDraft {
            key: None,
            label: "Owner".to_string(),
        }];
        window.places.loaded = true;
        let draft = window.fact_types.draft.clone();
        window.apply_fact_types_event(FactTypesEvent::Save(Box::new(draft)));

        assert_eq!(window.fact_types.types.len(), 62);
        assert_eq!(window.fact_types.selected.as_deref(), Some("HEIRLOOM"));
        assert!(!window.places.loaded, "place labels refresh");
        let custom = window
            .app
            .load_fact_type("HEIRLOOM")
            .expect("load")
            .expect("custom fact exists");
        assert!(!custom.is_builtin);
        assert_eq!(custom.roles[0].role_key, "OWNER");

        window.apply_fact_types_event(FactTypesEvent::Select("BIRTH".to_string()));
        window.fact_types.draft.roles = vec![crate::db::facts::FactRoleDraft {
            key: None,
            label: "Witness".to_string(),
        }];
        let draft = window.fact_types.draft.clone();
        window.apply_fact_types_event(FactTypesEvent::Save(Box::new(draft)));
        let birth = window
            .app
            .load_fact_type("BIRTH")
            .expect("load")
            .expect("birth exists");
        assert!(birth.is_builtin);
        assert_eq!(birth.roles[0].role_key, "WITNESS");

        window.apply_fact_types_event(FactTypesEvent::Delete("HEIRLOOM".to_string()));
        assert_eq!(window.fact_types.types.len(), 61);
        assert!(window.fact_types.selected.is_none());
        assert!(matches!(window.app.status(), UiStatus::Success(_)));

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn alternate_name_events_save_and_delete() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();
        window.app.set_selected_person(Some(person_id.clone()));
        window.sync_inspector_form();
        window.sync_alternate_names();
        assert!(window.names.loaded);
        assert!(window.names.names.is_empty());

        window.apply_inspector_action(
            InspectorAction::SaveAlternateName(Box::new(crate::db::names::AlternateNameDraft {
                name_type: "married".to_string(),
                given_name: "Ada".to_string(),
                surname: "Lovelace".to_string(),
                ..crate::db::names::AlternateNameDraft::default()
            })),
            &person_id,
        );
        window.sync_alternate_names();
        assert_eq!(window.names.names.len(), 1);
        assert_eq!(window.names.names[0].name_type, "married");
        assert_eq!(window.names.names[0].display_name(false), "Ada Lovelace");

        let id = window.names.names[0].id.clone();
        window.apply_inspector_action(InspectorAction::DeleteAlternateName(id), &person_id);
        window.sync_alternate_names();
        assert!(window.names.names.is_empty());

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn surname_first_toggle_persists_and_relabels_views() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada Lynn /King/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        assert_eq!(
            window.rows.first().expect("one person").display_name(),
            "Ada Lynn King"
        );

        window.set_surname_first(true);

        assert!(
            window
                .app
                .tree_settings()
                .expect("settings load")
                .surname_first
        );
        assert_eq!(
            window.rows.first().expect("one person").display_name(),
            "King, Ada Lynn"
        );
        assert_eq!(
            window.scene.nodes.first().expect("one node").display_name,
            "King, Ada Lynn"
        );

        window.set_surname_first(false);
        assert_eq!(
            window.rows.first().expect("one person").display_name(),
            "Ada Lynn King"
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn research_tasks_and_groups_load_and_link() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();

        let task_id = window
            .app
            .save_task(&crate::db::tasks::TaskDraft {
                title: "Find the birth record".to_string(),
                folder: "Parish".to_string(),
                priority: Some(3),
                ..crate::db::tasks::TaskDraft::default()
            })
            .expect("task saves");
        window.research.tasks.loaded = false;
        window.reload_tasks();
        assert_eq!(window.research.tasks.tasks.len(), 1);
        assert_eq!(window.research.tasks.folders, ["Parish".to_string()]);

        let owner = crate::db::tasks::TaskOwner::Person(person_id.clone());
        window.open_tasks(owner.clone(), "Tasks — Ada King".to_string());
        assert_eq!(
            window.task_attach.as_ref().expect("dialog").choices.len(),
            1
        );
        window.app.attach_task(&task_id, &owner).expect("attach");
        window.open_tasks(owner, "Tasks — Ada King".to_string());
        assert_eq!(
            window.task_attach.as_ref().expect("dialog").attached.len(),
            1
        );

        let group_id = window
            .app
            .save_group(&crate::db::groups::GroupDraft {
                name: "Friends".to_string(),
                ..crate::db::groups::GroupDraft::default()
            })
            .expect("group saves");
        window
            .app
            .add_group_member(&group_id, &person_id)
            .expect("member");
        window.research.groups.loaded = false;
        window.reload_groups();
        window.research.groups.selected = Some(group_id.clone());
        window.load_group_draft(&group_id);
        assert_eq!(window.research.groups.members.len(), 1);
        assert_eq!(window.research.groups.members[0].name, "Ada King");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn media_tab_loads_and_attachment_dialog_tracks_links() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();
        let media_id = window
            .app
            .save_media(&crate::db::media::MediaDraft {
                file_path: "/tmp/photo.jpg".to_string(),
                caption: "Photo".to_string(),
                ..crate::db::media::MediaDraft::default()
            })
            .expect("media saves");
        window.media.loaded = false;
        window.reload_media();
        assert_eq!(window.media.media.len(), 1);
        assert_eq!(window.media.media[0].link_count, 0);

        let owner = crate::db::media::MediaOwner::Person(person_id);
        window.open_media(owner.clone(), "Media — Ada King".to_string());
        assert_eq!(
            window.media_attach.as_ref().expect("dialog").choices.len(),
            1
        );
        assert!(
            window
                .media_attach
                .as_ref()
                .expect("dialog")
                .attached
                .is_empty()
        );

        window.app.attach_media(&media_id, &owner).expect("attach");
        window.open_media(owner, "Media — Ada King".to_string());
        assert_eq!(
            window.media_attach.as_ref().expect("dialog").attached.len(),
            1
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn associations_load_and_refresh_for_the_selected_person() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n\
             0 @I2@ INDI\n1 NAME Charles /Babbage/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window
            .rows
            .iter()
            .find(|row| row.surname == "King")
            .expect("Ada imported")
            .id
            .clone();
        let associate_id = window
            .rows
            .iter()
            .find(|row| row.surname == "Babbage")
            .expect("Charles imported")
            .id
            .clone();
        window.app.set_selected_person(Some(person_id.clone()));
        window.sync_associations();
        assert!(window.associations.is_empty());

        let mut draft = crate::db::associations::AssociationDraft::new(person_id.clone());
        draft.associate_id = associate_id;
        draft.relationship = "Friend".to_string();
        window
            .app
            .save_association(&draft)
            .expect("association saves");
        window.associations_owner = None;
        window.sync_associations();

        assert_eq!(window.associations.len(), 1);
        assert_eq!(window.associations[0].relationship, "Friend");
        assert_eq!(window.associations[0].associate_name, "Charles Babbage");

        let id = window.associations[0].id.clone();
        window
            .app
            .delete_association(&id)
            .expect("association deletes");
        window.associations_owner = None;
        window.sync_associations();
        assert!(window.associations.is_empty());

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn notes_open_for_people_and_reload_after_changes() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();
        let owner = crate::db::notes::NoteOwner::Person(person_id.clone());

        window.open_notes(owner.clone(), "Notes — Ada King".to_string());
        assert!(window.notes.notes.is_empty());
        assert_eq!(window.notes.target.as_ref(), Some(&owner));

        window
            .app
            .add_note(&owner, "First line\nSecond line")
            .expect("note saves");
        window.open_notes(owner.clone(), "Notes — Ada King".to_string());
        assert_eq!(window.notes.notes.len(), 1);
        assert_eq!(window.notes.notes[0].body, "First line\nSecond line");

        let note_id = window.notes.notes[0].id.clone();
        window.app.delete_note(&note_id).expect("note deletes");
        assert!(window.app.list_notes(&owner).expect("list").is_empty());

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn privacy_toggles_update_the_person_and_facts() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n1 BIRT\n2 DATE 2000\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();
        window.app.set_selected_person(Some(person_id.clone()));
        window.sync_inspector_form();
        let privacy = window.person_privacy.as_ref().expect("privacy loads");
        assert!(privacy.auto_living);
        assert!(!privacy.private);
        let event_id = privacy.events[0].event_id.clone();

        window.apply_inspector_action(InspectorAction::SetPersonPrivate(true), &person_id);
        assert!(window.person_privacy.as_ref().expect("privacy").private);
        window.apply_inspector_action(InspectorAction::SetLivingOverride(Some(false)), &person_id);
        let privacy = window.person_privacy.as_ref().expect("privacy");
        assert!(privacy.auto_living);
        assert!(!privacy.effective_living);
        window.apply_inspector_action(
            InspectorAction::SetEventPrivate {
                event_id: event_id.clone(),
                private: true,
            },
            &person_id,
        );
        assert!(
            window
                .person_privacy
                .as_ref()
                .expect("privacy")
                .events
                .iter()
                .find(|event| event.event_id == event_id)
                .expect("birth event")
                .private
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn witness_facts_and_the_participant_editor_load() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n1 BIRT\n2 DATE 1850\n2 _WITN @I2@\n3 _ROLE Godparent\n0 @I2@ INDI\n1 NAME Bo /King/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let ada = window
            .rows
            .iter()
            .find(|row| row.given_name == "Ada")
            .expect("Ada")
            .id
            .clone();
        let bo = window
            .rows
            .iter()
            .find(|row| row.given_name == "Bo")
            .expect("Bo")
            .id
            .clone();
        let ada_facts = window.person_facts.get(&ada).expect("Ada facts");
        let birth = ada_facts
            .iter()
            .find(|fact| fact.event_type == "BIRTH")
            .expect("birth with witness");
        assert_eq!(birth.participants.len(), 1);
        assert_eq!(birth.participants[0].role_label, "Godparent");
        let bo_facts = window.person_facts.get(&bo).expect("Bo facts");
        assert_eq!(bo_facts.len(), 1);
        assert_eq!(bo_facts[0].role.as_deref(), Some("Godparent"));
        assert_eq!(bo_facts[0].principal.as_deref(), Some("Ada King"));

        let event_id = birth.event_id.clone();
        window.app.set_selected_person(Some(ada.clone()));
        window.apply_inspector_action(InspectorAction::EditParticipants(event_id.clone()), &ada);
        let form = window
            .pending_participants
            .as_ref()
            .expect("participant editor opens");
        assert_eq!(form.event_id, event_id);
        assert_eq!(form.roles[0].label, "Godparent");
        assert_eq!(form.participants.len(), 1);

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn imported_facts_load_for_the_inspector() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n1 OCCU Teacher\n2 DATE 1870\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();
        let facts = window
            .person_facts
            .get(&person_id)
            .expect("facts loaded for the person");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].event_type, "OCCUPATION");
        assert_eq!(facts[0].label, "Occupation");
        assert_eq!(facts[0].date.as_deref(), Some("1870"));
        assert_eq!(facts[0].value.as_deref(), Some("Teacher"));

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn place_events_refresh_the_grid_and_inspector() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.reload_places();
        assert!(window.places.loaded);

        let person_id = window.rows.first().expect("one person").id.clone();
        window.app.set_selected_person(Some(person_id.clone()));
        window.sync_inspector_form();
        window.inspector.birth_place = "Uppsala".to_string();
        let details = window.inspector.to_details();
        window.edit_person(&details);
        window.reload_places();
        assert_eq!(window.places.places.len(), 1);
        assert_eq!(window.places.places[0].event_count, 1);
        let id = window.places.places[0].id.clone();

        window.apply_places_event(PlacesEvent::Select(id.clone()));
        assert_eq!(window.places.usage.len(), 1);
        assert_eq!(window.places.usage[0].event_type, "BIRTH");
        assert_eq!(
            window.places.usage[0].person_name.as_deref(),
            Some("Johan Ahlberg")
        );
        window.app.set_selected_person(None);
        window.tab = crate::gui::tabs::AppTab::Places;
        window.apply_places_event(PlacesEvent::SelectPerson(person_id.clone()));
        assert_eq!(window.app.selected_person_id(), Some(person_id.as_str()));
        assert_eq!(window.tab, crate::gui::tabs::AppTab::Places);
        window.sync_inspector_form();
        assert_eq!(window.inspector.given_name, "Johan");

        window.places.draft.name = "Uppsala, Sweden".to_string();
        let draft = window.places.draft.to_draft().expect("draft parses");
        window.apply_places_event(PlacesEvent::Save(Box::new(draft)));
        assert_eq!(window.places.places.len(), 1);
        assert_eq!(window.places.places[0].name, "Uppsala, Sweden");
        window.sync_inspector_form();
        assert_eq!(
            window.inspector.birth_place, "Uppsala, Sweden",
            "the inspector reloads the renamed place"
        );

        window.apply_places_event(PlacesEvent::Delete(id));
        assert!(window.places.places.is_empty());
        assert!(window.places.usage.is_empty());
        assert!(!window.places.editing);
        window.sync_inspector_form();
        assert!(
            window.inspector.birth_place.is_empty(),
            "deleting the place clears the event place"
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
        assert_eq!(window.repositories.sources.len(), 1);
        assert_eq!(window.repositories.sources[0].title, "Parish Book");
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
            inspector::connections(&window.tree, &person_id, &window.marriages, false)
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
