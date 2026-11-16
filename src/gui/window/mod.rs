//! The main egui window: menu bar, status bar, welcome dashboard and the
//! interactive canvas.

mod actions;
mod lifecycle;
mod loading;
mod panels;

use loading::{empty_scene, loadable};

use super::advanced_search;
use super::ahnentafel_list;
use super::associations as associations_dialog;
use super::associations_view;
use super::calendar_report;
use super::canvas::ViewportState;
use super::citations;
use super::connectors::ConnectorCache;
use super::couples;
use super::database_tools;
use super::dataclean as dataclean_ui;
use super::date_calculator;
use super::descendant_list;
use super::event_editor;
use super::fact_types;
use super::family_sheet;
use super::find_everywhere;
use super::gazetteer;
use super::geocode;
use super::import_report;
use super::individual_list;
use super::individual_summary;
use super::inspector::InspectorForm;
use super::inspector::ParticipantForm;
use super::inspector::RelationForm;
use super::kinship_report;
use super::layout::TreeScene;
use super::map;
use super::media;
use super::media_repair;
use super::merge as merge_dialog;
use super::names;
use super::notes as notes_dialog;
use super::pedigree;
use super::pedigree_chart;
use super::person_editor;
use super::photo_tree;
use super::place_clean;
use super::places;
use super::relationship_calculator;
use super::repositories;
use super::research;
use super::scrapbook;
use super::search_replace;
use super::settings;
use super::soundex;
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
use crate::db::bookmarks::BookmarkPerson;
use crate::db::facts::PersonFact;
use crate::db::people::VitalDates;
use crate::db::privacy::PersonPrivacy;
use crate::db::settings::TreeSettings;
use crate::gedcom::ImportReport;
use crate::i18n;
use crate::model::navigation::NavigationHistory;
use crate::model::person_badge::PersonBadge;
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
    pub(super) connectors: ConnectorCache,
    pub(super) seen_db: Option<PathBuf>,
    pub(super) seen_generation: u64,
    pub(super) recent: Vec<PathBuf>,
    pub(super) pending_framing: bool,
    pub(super) drag_started_on_node: bool,
    pub(super) tab: tabs::AppTab,
    pub(super) watch: watchlist::WatchState,
    pub(super) advanced_search: advanced_search::AdvancedSearchState,
    pub(super) find: find_everywhere::FindState,
    pub(super) search_replace: search_replace::SearchReplaceState,
    pub(super) couples: couples::CouplesState,
    pub(super) couple_rows: Vec<crate::model::couples::Couple>,
    pub(super) association_view: associations_view::AssociationViewState,
    pub(super) relationship_calculator: relationship_calculator::RelationshipCalculatorState,
    pub(super) merge: merge_dialog::MergeState,
    pub(super) db_tools: database_tools::DatabaseToolsState,
    pub(super) data_clean: dataclean_ui::DataCleanState,
    pub(super) place_clean: place_clean::PlaceCleanState,
    pub(super) geocode: geocode::GeocodeState,
    pub(super) gazetteer: gazetteer::GazetteerState,
    pub(super) date_calculator: date_calculator::DateCalculatorState,
    pub(super) association_rows: Vec<crate::db::associations::AssociationViewRow>,
    pub(super) places: places::PlacesState,
    pub(super) map: map::MapState,
    pub(super) media: media::MediaState,
    pub(super) media_repair: media_repair::MediaRepairState,
    pub(super) media_attach: Option<media::MediaAttachState>,
    pub(super) research: research::ResearchState,
    pub(super) task_attach: Option<tasks::TaskAttachState>,
    pub(super) names: names::NamesState,
    pub(super) notes: notes_dialog::NotesState,
    pub(super) person_editor: Option<person_editor::PersonEditor>,
    pub(super) photo_tree: photo_tree::PhotoTreeState,
    pub(super) scrapbook: scrapbook::ScrapbookState,
    pub(super) pedigree_chart: pedigree_chart::PedigreeChartState,
    pub(super) family_sheet: family_sheet::FamilySheetState,
    pub(super) individual_summary: individual_summary::IndividualSummaryState,
    pub(super) individual_list: individual_list::IndividualListState,
    pub(super) ahnentafel_list: ahnentafel_list::AhnentafelListState,
    pub(super) descendant_list: descendant_list::DescendantListState,
    pub(super) kinship_report: kinship_report::KinshipReportState,
    pub(super) calendar_report: calendar_report::CalendarReportState,
    pub(super) event_editor: Option<event_editor::EventEditor>,
    pub(super) pedigree: pedigree::PedigreeState,
    pub(super) fact_types: fact_types::FactTypesState,
    pub(super) sources: sources::SourcesState,
    pub(super) soundex: soundex::SoundexState,
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
    pub(super) person_badges: HashMap<String, Vec<PersonBadge>>,
    pub(super) bookmarks: Vec<BookmarkPerson>,
    pub(super) history: NavigationHistory,
    pub(super) pending_history_nav: Option<String>,
    pub(super) inspector: InspectorForm,
    pub(super) inspected: Option<String>,
    pub(super) pending_relation: Option<RelationForm>,
    pub(super) pending_new_person: Option<super::inspector::NewPersonState>,
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
            connectors: ConnectorCache::default(),
            seen_db: None,
            seen_generation: 0,
            recent,
            pending_framing: false,
            drag_started_on_node: false,
            tab: tabs::AppTab::default(),
            watch: watchlist::WatchState::default(),
            advanced_search: advanced_search::AdvancedSearchState::default(),
            find: find_everywhere::FindState::default(),
            search_replace: search_replace::SearchReplaceState::default(),
            couples: couples::CouplesState::default(),
            couple_rows: Vec::new(),
            association_view: associations_view::AssociationViewState::default(),
            relationship_calculator: relationship_calculator::RelationshipCalculatorState::default(
            ),
            merge: merge_dialog::MergeState::default(),
            db_tools: database_tools::DatabaseToolsState::default(),
            data_clean: dataclean_ui::DataCleanState::default(),
            place_clean: place_clean::PlaceCleanState::default(),
            geocode: geocode::GeocodeState::default(),
            gazetteer: gazetteer::GazetteerState::default(),
            date_calculator: date_calculator::DateCalculatorState::default(),
            association_rows: Vec::new(),
            places: places::PlacesState::default(),
            map: map::MapState::default(),
            media: media::MediaState::default(),
            media_repair: media_repair::MediaRepairState::default(),
            media_attach: None,
            research: research::ResearchState::default(),
            task_attach: None,
            names: names::NamesState::default(),
            notes: notes_dialog::NotesState::default(),
            person_editor: None,
            photo_tree: photo_tree::PhotoTreeState::default(),
            scrapbook: scrapbook::ScrapbookState::default(),
            pedigree_chart: pedigree_chart::PedigreeChartState::default(),
            family_sheet: family_sheet::FamilySheetState::default(),
            individual_summary: individual_summary::IndividualSummaryState::default(),
            individual_list: individual_list::IndividualListState::default(),
            ahnentafel_list: ahnentafel_list::AhnentafelListState::default(),
            descendant_list: descendant_list::DescendantListState::default(),
            kinship_report: kinship_report::KinshipReportState::default(),
            calendar_report: calendar_report::CalendarReportState::default(),
            event_editor: None,
            pedigree: pedigree::PedigreeState::default(),
            fact_types: fact_types::FactTypesState::default(),
            sources: sources::SourcesState::default(),
            soundex: soundex::SoundexState::default(),
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
            person_badges: HashMap::new(),
            bookmarks: Vec::new(),
            history: NavigationHistory::default(),
            pending_history_nav: None,
            inspector: InspectorForm::default(),
            inspected: None,
            pending_relation: None,
            pending_new_person: None,
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
        if self.app.is_maintenance() {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.4);
                    ui.spinner();
                    ui.label(
                        egui::RichText::new(crate::i18n::tr("db_tools.running"))
                            .strong()
                            .size(16.0),
                    );
                });
            });
            ui.ctx().request_repaint();
            return;
        }
        if self.app.active_db().is_some()
            && !self.app.is_busy()
            && ui.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::F,
                )
            })
        {
            self.find.open = true;
        }
        self.sync_scene();
        if self.app.active_db().is_some() && !self.app.is_busy() {
            if ui.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft)) {
                self.navigate_back();
            }
            if ui.input_mut(|input| input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight))
            {
                self.navigate_forward();
            }
            if ui.input_mut(|input| {
                input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::D,
                )
            }) {
                self.toggle_selected_bookmark();
            }
        }
        if let Some(path) = self.app.take_restored_path() {
            self.note_recent(path, frame);
        }
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
                tabs::AppTab::Couples => self.couples_tab(ui),
                tabs::AppTab::Associations => self.associations_tab(ui),
                tabs::AppTab::Places => self.places_tab(ui),
                tabs::AppTab::Map => self.map_tab(ui),
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
            self.show_new_person_window(ui.ctx());
            self.show_participant_window(ui.ctx());
            self.show_association_window(ui.ctx());
            self.show_media_attach(ui.ctx());
            self.show_media_repair_window(ui.ctx());
            self.show_person_editor(ui.ctx());
            self.show_photo_tree_window(ui.ctx());
            self.show_scrapbook_window(ui.ctx());
            self.show_pedigree_chart_window(ui.ctx());
            self.show_family_sheet_window(ui.ctx());
            self.show_individual_summary_window(ui.ctx());
            self.show_individual_list_window(ui.ctx());
            self.show_ahnentafel_list_window(ui.ctx());
            self.show_descendant_list_window(ui.ctx());
            self.show_kinship_report_window(ui.ctx());
            self.show_calendar_report_window(ui.ctx());
            self.show_event_editor(ui.ctx());
            self.show_relationship_calculator(ui.ctx());
            self.show_merge_window(ui.ctx());
            self.show_database_tools(ui.ctx());
            self.show_data_clean_window(ui.ctx());
            self.show_place_clean_window(ui.ctx());
            self.show_geocode_window(ui.ctx());
            self.show_gazetteer_window(ui.ctx());
            self.show_advanced_search_window(ui.ctx());
            self.show_find_window(ui.ctx());
            self.show_search_replace_window(ui.ctx());
            self.show_task_attach(ui.ctx());
            self.show_notes(ui.ctx());
            self.show_export_dialog(ui.ctx());
        }
        self.show_date_calculator(ui.ctx());
        self.show_soundex(ui.ctx());
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
        self.track_history();
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
    use crate::db::groups::GroupDraft;
    use crate::db::merge::MergeDraft;
    use crate::db::people::{ParentRole, PersonDetails};
    use crate::db::places::PlaceDraft;
    use crate::db::sources::SourceDraft;
    use crate::geocode::GazetteerHit;
    use crate::gui::event_editor::EventEditorOutcome;
    use crate::gui::fact_types::FactTypesEvent;
    use crate::gui::inspector;
    use crate::gui::inspector::InspectorAction;
    use crate::gui::inspector::RelationKind;
    use crate::gui::media::MediaEvent;
    use crate::gui::places::PlacesEvent;
    use crate::gui::repositories::RepositoriesEvent;
    use crate::gui::sources::SourcesEvent;
    use crate::gui::welcome;
    use crate::model::geocode::{GeocodeOutcome, GeocodeReport, GeocodeResult};
    use crate::model::media_repair::MediaRepairMatch;
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
    fn merge_dialog_prefills_the_kept_person() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 @I2@ INDI\n1 NAME Augusta /King/\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let first = window.rows[0].id.clone();
        window.open_merge_dialog(Some(first.clone()));
        assert!(window.merge.open);
        assert_eq!(window.merge.keep.as_deref(), Some(first.as_str()));
        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn applying_a_merge_selects_the_kept_person_and_deletes_the_duplicate() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 @I2@ INDI\n1 NAME Augusta /King/\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let keep_id = window.rows[0].id.clone();
        let merge_id = window.rows[1].id.clone();
        window.app.set_selected_person(Some(merge_id.clone()));
        let draft = MergeDraft {
            keep_id: keep_id.clone(),
            merge_id: merge_id.clone(),
            details: PersonDetails::new("Ada", "", "King", "", "U", None, None),
            private: false,
            living_override: None,
            preserve_primary_names: true,
        };

        window.apply_merge(&draft);

        assert_eq!(
            window.app.selected_person_id().map(str::to_string),
            Some(keep_id.clone())
        );
        assert!(window.tree.people.iter().any(|person| person.id == keep_id));
        assert!(
            window
                .tree
                .people
                .iter()
                .all(|person| person.id != merge_id),
            "the duplicate is gone"
        );
        assert!(matches!(window.app.status(), UiStatus::Success(_)));
        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn a_dirty_editor_blocks_merging() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 @I2@ INDI\n1 NAME Augusta /King/\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let keep_id = window.rows[0].id.clone();
        let merge_id = window.rows[1].id.clone();
        window.open_person_editor(&keep_id);
        window
            .person_editor
            .as_mut()
            .expect("editor")
            .form
            .given_name = "Changed".to_string();
        window.merge.open_with(Some(keep_id.clone()));
        let draft = MergeDraft {
            keep_id: keep_id.clone(),
            merge_id: merge_id.clone(),
            details: PersonDetails::new("Ada", "", "King", "", "U", None, None),
            private: false,
            living_override: None,
            preserve_primary_names: true,
        };

        window.apply_merge(&draft);

        assert_eq!(
            window.merge.error.as_deref(),
            Some(crate::i18n::tr("merge.editor_dirty").as_str())
        );
        assert!(
            window
                .tree
                .people
                .iter()
                .any(|person| person.id == merge_id),
            "nothing was merged"
        );
        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn problem_alerts_load_and_persist_dismissals() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        window.reload_problems();
        let key = window
            .research
            .problems
            .alerts
            .iter()
            .find(|alert| alert.kind == crate::model::problems::ProblemKind::MissingBirthDate)
            .expect("missing birth alert")
            .key
            .clone();
        assert!(!window.research.problems.dismissed.contains(&key));

        window.app.dismiss_problem(&key).expect("dismiss");
        window.reload_problems();
        assert!(window.research.problems.dismissed.contains(&key));

        window.app.clear_problem_dismissals().expect("reset");
        window.reload_problems();
        assert!(window.research.problems.dismissed.is_empty());
        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn adding_a_person_opens_the_dialog_then_inserts_on_submit() {
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

        assert!(window.pending_new_person.is_some(), "the dialog is open");
        assert_eq!(window.rows.len(), before, "nothing is inserted yet");

        let details = PersonDetails::new("New", "", "Person", "", "U", None, None);
        window.pending_new_person = None;
        window.submit_new_person(&details);

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
        window.create_blank_person();
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
    fn descendant_mode_follows_generations_and_keeps_the_root_pinned() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n\
             0 @I1@ INDI\n1 NAME Root /Person/\n\
             0 @I2@ INDI\n1 NAME Spouse /Person/\n\
             0 @I3@ INDI\n1 NAME Child /Person/\n\
             0 @I4@ INDI\n1 NAME ChildSpouse /Person/\n\
             0 @I5@ INDI\n1 NAME Grandchild /Person/\n\
             0 @I6@ INDI\n1 NAME Father /Person/\n\
             0 @I7@ INDI\n1 NAME Sibling /Person/\n\
             0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n\
             0 @F2@ FAM\n1 HUSB @I3@\n1 WIFE @I4@\n1 CHIL @I5@\n\
             0 @F3@ FAM\n1 HUSB @I6@\n1 CHIL @I1@\n1 CHIL @I7@\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let id_for = |window: &TreeWindow, given: &str| {
            window
                .rows
                .iter()
                .find(|row| row.given_name == given)
                .expect("person")
                .id
                .clone()
        };
        let root = id_for(&window, "Root");
        let spouse = id_for(&window, "Spouse");
        let child = id_for(&window, "Child");
        let child_spouse = id_for(&window, "ChildSpouse");
        let grandchild = id_for(&window, "Grandchild");
        let father = id_for(&window, "Father");
        let sibling = id_for(&window, "Sibling");

        window.pedigree.mode = crate::gui::pedigree::GraphMode::Descendants;
        window.pedigree.descendant_root_id = Some(root.clone());
        window.pedigree.descendant_generations = crate::gui::pedigree::DescendantGenerations::Two;
        window.rebuild_scene_layout();
        let ids: Vec<&str> = window
            .scene
            .nodes
            .iter()
            .map(|node| node.person_id.as_str())
            .collect();
        for expected in [&root, &spouse, &child, &child_spouse] {
            assert!(ids.contains(&expected.as_str()), "missing {expected}");
        }
        assert!(!ids.contains(&grandchild.as_str()));
        assert!(!ids.contains(&father.as_str()));
        assert!(!ids.contains(&sibling.as_str()));

        window.pedigree.descendant_generations = crate::gui::pedigree::DescendantGenerations::Three;
        window.rebuild_scene_layout();
        assert!(
            window
                .scene
                .nodes
                .iter()
                .any(|node| node.person_id == grandchild)
        );

        window.app.set_selected_person(Some(grandchild.clone()));
        window.rebuild_scene_layout();
        assert_eq!(
            window.pedigree.descendant_root_id.as_deref(),
            Some(root.as_str())
        );

        window.pedigree.mode = crate::gui::pedigree::GraphMode::All;
        window.rebuild_scene_layout();
        assert_eq!(window.scene.nodes.len(), 7);

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn family_mode_shows_the_pinned_couple_and_direct_children() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n\
             0 @I1@ INDI\n1 NAME Root /Person/\n\
             0 @I2@ INDI\n1 NAME Spouse /Person/\n\
             0 @I3@ INDI\n1 NAME Child /Person/\n\
             0 @I4@ INDI\n1 NAME Grandchild /Person/\n\
             0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n\
             0 @F2@ FAM\n1 HUSB @I3@\n1 CHIL @I4@\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let id_for = |window: &TreeWindow, given: &str| {
            window
                .rows
                .iter()
                .find(|row| row.given_name == given)
                .expect("person")
                .id
                .clone()
        };
        let root = id_for(&window, "Root");
        let spouse = id_for(&window, "Spouse");
        let child = id_for(&window, "Child");
        let grandchild = id_for(&window, "Grandchild");

        window.pedigree.mode = crate::gui::pedigree::GraphMode::Family;
        window.pedigree.family_root_id = Some(root.clone());
        window.rebuild_scene_layout();
        let ids: Vec<&str> = window
            .scene
            .nodes
            .iter()
            .map(|node| node.person_id.as_str())
            .collect();
        assert!(ids.contains(&root.as_str()));
        assert!(ids.contains(&spouse.as_str()));
        assert!(ids.contains(&child.as_str()));
        assert!(!ids.contains(&grandchild.as_str()));
        assert!(
            window.pedigree.family_id.is_some(),
            "default family selected"
        );

        window.app.set_selected_person(Some(grandchild.clone()));
        window.rebuild_scene_layout();
        assert_eq!(
            window.pedigree.family_root_id.as_deref(),
            Some(root.as_str()),
            "selection does not re-root the family view"
        );

        window.pedigree.mode = crate::gui::pedigree::GraphMode::All;
        window.rebuild_scene_layout();
        assert_eq!(window.scene.nodes.len(), 4);

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn pedigree_mode_filters_to_the_pinned_ancestor_tree() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n\
             0 @I1@ INDI\n1 NAME Root /Person/\n\
             0 @I2@ INDI\n1 NAME Father /Person/\n\
             0 @I3@ INDI\n1 NAME Mother /Person/\n\
             0 @I4@ INDI\n1 NAME Sibling /Person/\n\
             0 @I5@ INDI\n1 NAME Child /Person/\n\
             0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I3@\n1 CHIL @I1@\n1 CHIL @I4@\n\
             0 @F2@ FAM\n1 HUSB @I1@\n1 CHIL @I5@\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let id_for = |window: &TreeWindow, given: &str| {
            window
                .rows
                .iter()
                .find(|row| row.given_name == given)
                .expect("person")
                .id
                .clone()
        };
        let root = id_for(&window, "Root");
        let father = id_for(&window, "Father");
        let mother = id_for(&window, "Mother");
        let child = id_for(&window, "Child");
        let sibling = id_for(&window, "Sibling");

        window.pedigree.mode = crate::gui::pedigree::GraphMode::Pedigree;
        window.pedigree.root_id = Some(root.clone());
        window.rebuild_scene_layout();
        let ids: Vec<&str> = window
            .scene
            .nodes
            .iter()
            .map(|node| node.person_id.as_str())
            .collect();
        assert!(ids.contains(&root.as_str()));
        assert!(ids.contains(&father.as_str()));
        assert!(ids.contains(&mother.as_str()));
        assert!(!ids.contains(&child.as_str()), "descendants are excluded");
        assert!(!ids.contains(&sibling.as_str()), "siblings are excluded");

        window.pedigree.generations = crate::gui::pedigree::PedigreeGenerations::Six;
        window.rebuild_scene_layout();
        assert_eq!(window.scene.nodes.len(), 3, "missing ancestors stay absent");

        window.pedigree.mode = crate::gui::pedigree::GraphMode::All;
        window.rebuild_scene_layout();
        assert_eq!(window.scene.nodes.len(), 5, "All restores the full tree");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn relationship_calculator_prefills_root_and_selection() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n\
             0 @I2@ INDI\n1 NAME Charles /Babbage/\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let ids: Vec<String> = window.rows.iter().map(|row| row.id.clone()).collect();
        window.tree_settings.root_person_id = Some(ids[0].clone());
        window.app.set_selected_person(Some(ids[1].clone()));

        window.open_relationship_calculator();

        assert!(window.relationship_calculator.open);
        assert_eq!(
            window.relationship_calculator.from.as_deref(),
            Some(ids[0].as_str())
        );
        assert_eq!(
            window.relationship_calculator.to.as_deref(),
            Some(ids[1].as_str())
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn root_person_updates_people_relationships() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n\
             0 @I1@ INDI\n1 NAME Ada /King/\n\
             0 @I2@ INDI\n1 NAME Father /King/\n1 SEX M\n\
             0 @I3@ INDI\n1 NAME Mother /King/\n1 SEX F\n\
             0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I3@\n1 CHIL @I1@\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let id_for = |window: &TreeWindow, given: &str| {
            window
                .rows
                .iter()
                .find(|row| row.given_name == given)
                .expect("person")
                .id
                .clone()
        };
        let ada = id_for(&window, "Ada");
        let father = id_for(&window, "Father");
        let mother = id_for(&window, "Mother");

        window.set_root_person(Some(&ada));
        assert_eq!(
            window.tree_settings.root_person_id.as_deref(),
            Some(ada.as_str())
        );
        let relation = |window: &TreeWindow, id: &str| {
            window
                .rows
                .iter()
                .find(|row| row.id == id)
                .expect("row")
                .relationship_to_root
                .clone()
        };
        assert_eq!(relation(&window, &ada), "Root");
        assert_eq!(relation(&window, &father), "Father");
        assert_eq!(relation(&window, &mother), "Mother");

        window.set_root_person(None);
        assert!(window.tree_settings.root_person_id.is_none());
        assert_eq!(relation(&window, &ada), "");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn couple_rows_open_the_exact_family_in_family_graph_mode() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n\
             0 @I1@ INDI\n1 NAME Ada /King/\n\
             0 @I2@ INDI\n1 NAME Charles /Babbage/\n\
             0 @I3@ INDI\n1 NAME Child /King/\n\
             0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I1@\n1 CHIL @I3@\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        assert_eq!(window.couple_rows.len(), 1);
        let family_id = window.couple_rows[0].family_id.clone();
        let root = window.couple_rows[0].partner1_id.clone().expect("husband");

        window.open_couple_family(&family_id);

        assert_eq!(window.tab, crate::gui::tabs::AppTab::Graph);
        assert_eq!(
            window.pedigree.mode,
            crate::gui::pedigree::GraphMode::Family
        );
        assert_eq!(
            window.pedigree.family_id.as_deref(),
            Some(family_id.as_str())
        );
        assert_eq!(
            window.pedigree.family_root_id.as_deref(),
            Some(root.as_str())
        );
        assert_eq!(window.scene.nodes.len(), 3);

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn watch_list_edit_opens_and_selects_the_person() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();

        window.apply_watch_event(crate::gui::watchlist::WatchEvent::Edit(person_id.clone()));

        assert_eq!(window.app.selected_person_id(), Some(person_id.as_str()));
        assert_eq!(
            window.person_editor.as_ref().expect("editor").person_id,
            person_id
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn parent_slot_actions_open_the_picker_with_the_selected_role() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();

        window.apply_inspector_action(
            crate::gui::inspector::InspectorAction::AddParentAs(
                crate::db::people::ParentRole::Mother,
            ),
            &person_id,
        );
        let form = window.pending_relation.as_ref().expect("relation form");
        assert_eq!(form.kind, crate::gui::inspector::RelationKind::Parent);
        assert_eq!(form.role, crate::db::people::ParentRole::Mother);
        assert_eq!(form.details.gender, "F");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn modeless_editor_stays_pinned_while_browsing_and_saves_its_person() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n\
             0 @I2@ INDI\n1 NAME Charles /Babbage/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let ada = window
            .rows
            .iter()
            .find(|row| row.surname == "King")
            .expect("Ada")
            .id
            .clone();
        let charles = window
            .rows
            .iter()
            .find(|row| row.surname == "Babbage")
            .expect("Charles")
            .id
            .clone();

        window.app.set_selected_person(Some(ada.clone()));
        window.sync_inspector_form();
        window.open_person_editor(&ada);
        {
            let editor = window.person_editor.as_mut().expect("editor");
            editor.form.given_name = "Augusta".to_string();
            assert!(editor.is_dirty());
        }

        window.app.set_selected_person(Some(charles.clone()));
        window.sync_inspector_form();
        assert_eq!(
            window.person_editor.as_ref().expect("editor").person_id,
            ada,
            "browsing does not retarget the editor"
        );
        assert_eq!(
            window
                .person_editor
                .as_ref()
                .expect("editor")
                .form
                .given_name,
            "Augusta",
            "the draft survives selection changes"
        );

        let mut editor = window.person_editor.take().expect("editor");
        let details = editor.form.to_details();
        window.apply_editor_action(
            &mut editor,
            crate::gui::inspector::InspectorAction::Save(Box::new(details)),
        );
        window.person_editor = Some(editor);
        let ada_row = window
            .rows
            .iter()
            .find(|row| row.id == ada)
            .expect("Ada row");
        assert_eq!(ada_row.given_name, "Augusta");
        let charles_row = window
            .rows
            .iter()
            .find(|row| row.id == charles)
            .expect("Charles row");
        assert_eq!(charles_row.given_name, "Charles");
        assert!(
            !window.person_editor.as_ref().expect("editor").is_dirty(),
            "save clears dirty state"
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn editor_family_switch_follows_selection_and_respects_dirty_drafts() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n\
             0 @I2@ INDI\n1 NAME Charles /Babbage/\n0 TRLR\n",
        );
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let ada = window
            .rows
            .iter()
            .find(|row| row.surname == "King")
            .expect("Ada")
            .id
            .clone();
        let charles = window
            .rows
            .iter()
            .find(|row| row.surname == "Babbage")
            .expect("Charles")
            .id
            .clone();

        window.app.set_selected_person(Some(ada.clone()));
        window.open_person_editor(&ada);
        let mut editor = window.person_editor.take().expect("editor");
        window.apply_editor_action(
            &mut editor,
            crate::gui::inspector::InspectorAction::SwitchEditor(charles.clone()),
        );
        window.person_editor = Some(editor);
        assert_eq!(
            window.person_editor.as_ref().expect("editor").person_id,
            charles
        );
        assert_eq!(window.app.selected_person_id(), Some(charles.as_str()));

        let mut editor = window.person_editor.take().expect("editor");
        editor.form.given_name = "Changed".to_string();
        window.apply_editor_action(
            &mut editor,
            crate::gui::inspector::InspectorAction::SwitchEditor(ada.clone()),
        );
        assert_eq!(
            editor.person_id, charles,
            "dirty editor does not switch yet"
        );
        assert_eq!(
            editor.prompt,
            Some(crate::gui::person_editor::EditorPrompt::Switch(ada.clone()))
        );
        assert_eq!(
            window.app.selected_person_id(),
            Some(charles.as_str()),
            "selection follows only committed switches"
        );
        window.person_editor = Some(editor);

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn dirty_editor_prompts_before_switching_people() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n\
             0 @I2@ INDI\n1 NAME Charles /Babbage/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let ids: Vec<String> = window.rows.iter().map(|row| row.id.clone()).collect();
        window.open_person_editor(&ids[0]);
        window
            .person_editor
            .as_mut()
            .expect("editor")
            .form
            .given_name = "Changed".to_string();
        window.open_person_editor(&ids[1]);

        let editor = window.person_editor.as_ref().expect("editor");
        assert_eq!(editor.person_id, ids[0], "switch waits for confirmation");
        assert_eq!(
            editor.prompt,
            Some(crate::gui::person_editor::EditorPrompt::Switch(
                ids[1].clone()
            ))
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
    fn history_and_bookmarks_follow_person_selection() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 @I2@ INDI\n1 NAME Charles /Babbage/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let first = window.rows[0].id.clone();
        let second = window.rows[1].id.clone();

        window.app.set_selected_person(Some(first.clone()));
        window.track_history();
        window.app.set_selected_person(Some(second.clone()));
        window.track_history();
        assert_eq!(window.history.entries(), [first.as_str(), second.as_str()]);

        window.navigate_back();
        assert_eq!(window.app.selected_person_id(), Some(first.as_str()));
        window.track_history();
        assert_eq!(window.history.current(), Some(first.as_str()));
        window.navigate_forward();
        assert_eq!(window.app.selected_person_id(), Some(second.as_str()));

        window.toggle_selected_bookmark();
        assert_eq!(window.bookmarks.len(), 1);
        assert_eq!(window.bookmarks[0].person_id, second);
        window.remove_bookmark(&second);
        assert!(window.bookmarks.is_empty());

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn coloured_groups_and_labels_build_person_badges() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();

        let group_id = window
            .app
            .save_group(&crate::db::groups::GroupDraft {
                name: "Maternal".to_string(),
                color: Some(0xff3366),
                ..crate::db::groups::GroupDraft::default()
            })
            .expect("group");
        window
            .app
            .add_group_member(&group_id, &person_id)
            .expect("group member");
        let label_id = window
            .app
            .save_label(&crate::db::labels::LabelDraft {
                name: "Brick wall".to_string(),
                color: 0x33aa66,
                ..crate::db::labels::LabelDraft::default()
            })
            .expect("label");
        window
            .app
            .add_label_member(&label_id, &person_id)
            .expect("label member");
        window.refresh_person_badges();

        let badges = window.person_badges.get(&person_id).expect("person badges");
        assert_eq!(badges.len(), 2);
        assert_eq!(badges[0].name, "Maternal");
        assert_eq!(badges[0].color, Some(0xff3366));
        assert_eq!(badges[1].name, "Brick wall");
        assert_eq!(badges[1].color, Some(0x33aa66));
        assert_eq!(
            window
                .rows
                .first()
                .expect("row")
                .badges
                .iter()
                .map(|badge| badge.name.as_str())
                .collect::<Vec<_>>(),
            ["Maternal", "Brick wall"]
        );

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
    fn media_repair_opens_and_applies_folder_matches() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let media_id = window
            .app
            .save_media(&crate::db::media::MediaDraft {
                file_path: "missing/photo.jpg".to_string(),
                caption: "Photo".to_string(),
                ..crate::db::media::MediaDraft::default()
            })
            .expect("media saves");
        window.media.loaded = false;
        window.reload_media();
        window.open_media_repair();
        assert!(window.media_repair.open);
        assert_eq!(window.media_repair.missing.len(), 1);

        let media_dir = db_file.with_file_name(format!("repair-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&media_dir).expect("media dir");
        let file = media_dir.join("photo.jpg");
        std::fs::write(&file, b"photo").expect("media file");
        window.apply_media_repair_matches(&[MediaRepairMatch {
            media_id: media_id.clone(),
            old_path: "missing/photo.jpg".to_string(),
            new_path: file.to_string_lossy().to_string(),
        }]);
        assert!(window.media_repair.missing.is_empty());
        let record = window
            .app
            .load_media(&media_id)
            .expect("load")
            .expect("media");
        assert_eq!(record.caption.as_deref(), Some("Photo"));
        assert!(
            window
                .app
                .list_media()
                .expect("media list")
                .iter()
                .all(|media| !media.missing)
        );

        let _ = std::fs::remove_dir_all(media_dir);
        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_photo_tree_and_scrapbook_reports() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("person").id.clone();

        let photo_tree = window
            .build_photo_tree_report(&person_id, 3)
            .expect("photo tree report");
        assert_eq!(photo_tree.root_name, "Ada King");
        assert_eq!(photo_tree.people.len(), 1);
        assert_eq!(photo_tree.scene.nodes.len(), 1);

        let media_id = window
            .app
            .save_media(&crate::db::media::MediaDraft {
                file_path: "/tmp/photo.jpg".to_string(),
                caption: "Photo".to_string(),
                ..crate::db::media::MediaDraft::default()
            })
            .expect("media");
        let album_id = window
            .app
            .save_album(&crate::db::albums::AlbumDraft {
                name: "Trip".to_string(),
                ..crate::db::albums::AlbumDraft::default()
            })
            .expect("album");
        window
            .app
            .add_media_to_album(&album_id, &media_id)
            .expect("album media");
        let scrapbook = window
            .build_scrapbook_report(&[album_id])
            .expect("scrapbook report");
        assert_eq!(scrapbook.albums.len(), 1);
        assert_eq!(scrapbook.albums[0].name, "Trip");
        assert_eq!(scrapbook.albums[0].photos.len(), 1);
        assert_eq!(
            scrapbook.albums[0].photos[0].caption.as_deref(),
            Some("Photo")
        );

        let _ = std::fs::remove_file(&db_file);
    }

    fn import_family(db_file: &Path) {
        import_people(
            db_file,
            "0 HEAD\n1 CHAR UTF-8\n\
             0 @I1@ INDI\n1 NAME John /Stone/\n1 SEX M\n1 NOTE Parish register\n1 BIRT\n2 DATE 1810\n2 PLAC London\n1 DEAT\n2 DATE 1880\n2 PLAC Bath\n1 OCCU Teacher\n2 DATE 1840\n2 PLAC London\n\
             0 @I2@ INDI\n1 NAME Ada /King/\n1 SEX F\n1 BIRT\n2 DATE 1815\n2 PLAC London\n\
             0 @I3@ INDI\n1 NAME Bea /Stone/\n1 SEX F\n1 BIRT\n2 DATE 1840\n2 PLAC London\n\
             0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n1 MARR\n2 DATE 1838\n2 PLAC Bath\n\
             0 TRLR\n",
        );
    }

    fn import_descendants(db_file: &Path) {
        import_people(
            db_file,
            "0 HEAD\n1 CHAR UTF-8\n\
             0 @I1@ INDI\n1 NAME John /Stone/\n1 SEX M\n1 BIRT\n2 DATE 1810\n2 PLAC London\n1 DEAT\n2 DATE 1880\n2 PLAC Bath\n\
             0 @I2@ INDI\n1 NAME Ada /King/\n1 SEX F\n1 BIRT\n2 DATE 1815\n2 PLAC London\n\
             0 @I3@ INDI\n1 NAME Bea /Stone/\n1 SEX F\n1 BIRT\n2 DATE 1840\n2 PLAC London\n\
             0 @I4@ INDI\n1 NAME Carl /Stone/\n1 SEX M\n1 BIRT\n2 DATE 1842\n2 PLAC London\n\
             0 @I5@ INDI\n1 NAME Dan /Ward/\n1 SEX M\n1 BIRT\n2 DATE 1838\n2 PLAC Bath\n\
             0 @I6@ INDI\n1 NAME Eve /Ward/\n1 SEX F\n1 BIRT\n2 DATE 1865\n2 PLAC Bath\n\
             0 @I7@ INDI\n1 NAME Fay /Ward/\n1 SEX F\n1 BIRT\n2 DATE 1868\n2 PLAC Bath\n\
             0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n1 CHIL @I4@\n1 MARR\n2 DATE 1838\n2 PLAC Bath\n\
             0 @F2@ FAM\n1 HUSB @I5@\n1 WIFE @I3@\n1 CHIL @I6@\n1 CHIL @I7@\n1 MARR\n2 DATE 1860\n2 PLAC Bath\n\
             0 TRLR\n",
        );
    }

    fn person_id_by_given(window: &TreeWindow, given: &str) -> String {
        window
            .tree
            .people
            .iter()
            .find(|person| person.given_name == given)
            .expect("person")
            .id
            .clone()
    }

    #[test]
    fn builds_ahnentafel_list_report() {
        let db_file = temp_path("db");
        import_descendants(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let eve = person_id_by_given(&window, "Eve");

        let report = window
            .build_ahnentafel_list_report(&eve, 3)
            .expect("ahnentafel report");
        assert_eq!(report.root_name, "Eve Ward");
        assert_eq!(report.generations, 3);
        let rows: Vec<(u32, usize, &str)> = report
            .entries
            .iter()
            .map(|entry| (entry.number, entry.generation, entry.name.as_str()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (1, 1, "Eve Ward"),
                (2, 2, "Dan Ward"),
                (3, 2, "Bea Stone"),
                (6, 3, "John Stone"),
                (7, 3, "Ada King"),
            ]
        );
        let dan = report
            .entries
            .iter()
            .find(|entry| entry.number == 2)
            .expect("father");
        assert_eq!(dan.marriages[0].spouse, "Bea Stone");
        assert_eq!(dan.marriages[0].date, "1860");
        assert_eq!(dan.marriages[0].place, "Bath");

        let limited = window
            .build_ahnentafel_list_report(&eve, 2)
            .expect("ahnentafel report");
        assert_eq!(limited.entries.len(), 3, "generation three is excluded");

        assert!(window.build_ahnentafel_list_report("missing", 3).is_err());

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_descendant_list_reports() {
        use crate::model::descendant_list::DescendantNumbering;
        use crate::report::descendant_list::DescendantListMode;

        let db_file = temp_path("db");
        import_descendants(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let john = person_id_by_given(&window, "John");
        let eve = person_id_by_given(&window, "Eve");
        let bea = person_id_by_given(&window, "Bea");

        let indented = window
            .build_descendant_list_report(
                &john,
                4,
                DescendantListMode::Indented,
                DescendantNumbering::DAboville,
                Some(&eve),
            )
            .expect("descendant report");
        assert!(
            indented.target_name.is_none(),
            "indented lists ignore a stale direct target"
        );
        let rows: Vec<(&str, &str, usize)> = indented
            .entries
            .iter()
            .map(|entry| (entry.number.as_str(), entry.name.as_str(), entry.depth))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("1", "John Stone", 0),
                ("1.1", "Bea Stone", 1),
                ("1.2", "Carl Stone", 1),
                ("1.1.1", "Eve Ward", 2),
                ("1.1.2", "Fay Ward", 2),
            ]
        );
        assert_eq!(indented.entries[0].marriages[0].spouse, "Ada King");
        assert_eq!(indented.entries[1].marriages[0].spouse, "Dan Ward");
        assert_eq!(indented.entries[4].birth_place, "Bath");

        let henry = window
            .build_descendant_list_report(
                &john,
                4,
                DescendantListMode::Indented,
                DescendantNumbering::Henry,
                None,
            )
            .expect("descendant report");
        let numbers: Vec<&str> = henry
            .entries
            .iter()
            .map(|entry| entry.number.as_str())
            .collect();
        assert_eq!(numbers, ["1", "11", "12", "111", "112"]);

        let direct = window
            .build_descendant_list_report(
                &john,
                4,
                DescendantListMode::Direct,
                DescendantNumbering::DAboville,
                Some(&eve),
            )
            .expect("direct report");
        assert_eq!(direct.target_name.as_deref(), Some("Eve Ward"));
        let numbers: Vec<&str> = direct
            .entries
            .iter()
            .map(|entry| entry.number.as_str())
            .collect();
        assert_eq!(numbers, ["1", "1.1", "1.1.1"]);

        assert!(
            window
                .build_descendant_list_report(
                    &john,
                    4,
                    DescendantListMode::Direct,
                    DescendantNumbering::DAboville,
                    None,
                )
                .is_err()
        );
        assert!(
            window
                .build_descendant_list_report(
                    &bea,
                    4,
                    DescendantListMode::Direct,
                    DescendantNumbering::DAboville,
                    Some(&bea),
                )
                .is_ok(),
            "a person may be the direct-line target of themselves"
        );
        assert!(
            window
                .build_descendant_list_report(
                    &bea,
                    4,
                    DescendantListMode::Direct,
                    DescendantNumbering::DAboville,
                    Some(&john),
                )
                .is_err(),
            "ancestors are not descendants"
        );

        let _ = std::fs::remove_file(&db_file);
    }

    fn import_kinship(db_file: &Path) {
        import_people(
            db_file,
            "0 HEAD\n1 CHAR UTF-8\n\
             0 @I1@ INDI\n1 NAME John /Stone/\n1 SEX M\n1 BIRT\n2 DATE 1810\n2 PLAC London\n1 DEAT\n2 DATE 1880\n2 PLAC Bath\n\
             0 @I2@ INDI\n1 NAME Ada /King/\n1 SEX F\n1 BIRT\n2 DATE 1815\n2 PLAC London\n\
             0 @I3@ INDI\n1 NAME Bea /Stone/\n1 SEX F\n1 BIRT\n2 DATE 1840\n2 PLAC London\n\
             0 @I4@ INDI\n1 NAME Dan /Ward/\n1 SEX M\n\
             0 @I5@ INDI\n1 NAME Eve /Ward/\n1 SEX F\n1 BIRT\n2 DATE 1865\n2 PLAC Bath\n\
             0 @I6@ INDI\n1 NAME Lone /Person/\n1 SEX U\n\
             0 @I7@ INDI\n1 NAME George /Stone/\n1 SEX M\n\
             0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n1 MARR\n2 DATE 1838\n2 PLAC Bath\n\
             0 @F2@ FAM\n1 HUSB @I4@\n1 WIFE @I3@\n1 CHIL @I5@\n1 MARR\n2 DATE 1860\n2 PLAC Bath\n\
             0 @F3@ FAM\n1 HUSB @I7@\n1 CHIL @I1@\n\
             0 TRLR\n",
        );
    }

    #[test]
    fn builds_kinship_reports_for_every_person() {
        let db_file = temp_path("db");
        import_kinship(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let john = person_id_by_given(&window, "John");

        let all = window
            .build_kinship_report(&john, false)
            .expect("kinship report");
        assert_eq!(all.subject_name, "John Stone");
        assert_eq!(all.rows.len(), 6);
        let relationships: Vec<&str> = all
            .rows
            .iter()
            .map(|row| row.relationship.as_str())
            .collect();
        assert!(relationships.contains(&"Wife"));
        assert!(relationships.contains(&"Daughter"));
        assert!(relationships.contains(&"Father"));
        assert!(relationships.contains(&"Granddaughter"));
        assert!(relationships.contains(&"Related by marriage"));
        assert_eq!(
            all.rows.last().map(|row| row.relationship.as_str()),
            Some("Not directly related"),
            "unrelated people sort last"
        );
        let ada = all
            .rows
            .iter()
            .find(|row| row.name == "Ada King")
            .expect("wife row");
        assert_eq!(ada.birth, "1815");
        assert_eq!(ada.birth_place, "London");

        let relatives = window
            .build_kinship_report(&john, true)
            .expect("kinship report");
        assert!(relatives.only_relatives);
        assert_eq!(relatives.rows.len(), 5);
        assert!(
            relatives
                .rows
                .iter()
                .all(|row| row.relationship != "Not directly related")
        );

        assert!(window.build_kinship_report("missing", false).is_err());

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_relationship_report() {
        let db_file = temp_path("db");
        import_kinship(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let john = person_id_by_given(&window, "John");
        let eve = person_id_by_given(&window, "Eve");

        let report = window
            .build_relationship_report(&john, &eve)
            .expect("relationship report");
        assert_eq!(report.from_name, "John Stone");
        assert_eq!(report.to_name, "Eve Ward");
        assert_eq!(report.kinship, "Granddaughter");
        assert_eq!(report.common_ancestor.as_deref(), Some("George Stone"));
        let hops: Vec<(&str, &str)> = report
            .hops
            .iter()
            .map(|hop| (hop.relation.as_str(), hop.person.as_str()))
            .collect();
        assert_eq!(
            hops,
            vec![("child of", "Bea Stone"), ("child of", "Eve Ward")]
        );

        assert!(window.build_relationship_report(&john, "missing").is_err());
        assert!(window.build_relationship_report("missing", &eve).is_err());

        let _ = std::fs::remove_file(&db_file);
    }

    fn import_calendar(db_file: &Path) {
        import_people(
            db_file,
            "0 HEAD\n1 CHAR UTF-8\n\
             0 @I1@ INDI\n1 NAME John /Stone/\n1 SEX M\n1 BIRT\n2 DATE 12 JAN 1810\n2 PLAC London\n1 DEAT\n2 DATE 3 MAR 1880\n2 PLAC Bath\n\
             0 @I2@ INDI\n1 NAME Ada /King/\n1 SEX F\n1 BIRT\n2 DATE 20 DEC 1815\n2 PLAC London\n\
             0 @I3@ INDI\n1 NAME Bea /Stone/\n1 SEX F\n1 BIRT\n2 DATE 15 MAR 1840\n2 PLAC London\n\
             0 @I4@ INDI\n1 NAME Carl /Stone/\n1 SEX M\n1 BIRT\n2 DATE 29 FEB 1844\n2 PLAC London\n\
             0 @I5@ INDI\n1 NAME Year /Only/\n1 SEX U\n1 BIRT\n2 DATE 1850\n\
             0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n1 CHIL @I4@\n1 MARR\n2 DATE 5 JUN 1838\n2 PLAC Bath\n\
             0 TRLR\n",
        );
    }

    #[test]
    fn builds_upcoming_birthday_report() {
        use crate::report::calendar::CalendarMode;
        let db_file = temp_path("db");
        import_calendar(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("date");

        let report = window
            .build_calendar_report_at(CalendarMode::Birthdays, 2026, today)
            .expect("birthday report");
        assert_eq!(report.mode, CalendarMode::Birthdays);
        assert!(report.year.is_none());
        assert!(
            report.range_label.contains("1 JAN 2026"),
            "{}",
            report.range_label
        );
        let rows: Vec<(&str, &str, Option<u32>)> = report
            .entries
            .iter()
            .map(|entry| (entry.date.as_str(), entry.names.as_str(), entry.years))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("12 JAN 2026", "John Stone", Some(216)),
                ("28 FEB 2026", "Carl Stone", Some(182)),
                ("15 MAR 2026", "Bea Stone", Some(186)),
                ("20 DEC 2026", "Ada King", Some(211)),
            ]
        );
        assert_eq!(report.entries[0].place, "London");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_upcoming_anniversary_report() {
        use crate::report::calendar::CalendarMode;
        let db_file = temp_path("db");
        import_calendar(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("date");

        let report = window
            .build_calendar_report_at(CalendarMode::Anniversaries, 2026, today)
            .expect("anniversary report");
        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].date, "5 JUN 2026");
        assert_eq!(report.entries[0].names, "John Stone & Ada King");
        assert_eq!(report.entries[0].years, Some(188));
        assert_eq!(report.entries[0].place, "Bath");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_year_calendar_and_skips_undated_events() {
        use crate::model::calendar::CalendarEventKind;
        use crate::report::calendar::CalendarMode;
        let db_file = temp_path("db");
        import_calendar(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("date");

        let report = window
            .build_calendar_report_at(CalendarMode::Year, 2026, today)
            .expect("calendar report");
        assert_eq!(report.year, Some(2026));
        assert_eq!(
            report.events.len(),
            6,
            "four births, one death, one marriage"
        );
        assert!(
            report
                .events
                .iter()
                .all(|event| event.people[0] != "Year Only"),
            "year-only dates are skipped"
        );
        assert!(
            report
                .events
                .iter()
                .any(|event| event.kind == CalendarEventKind::Marriage
                    && event.month == 6
                    && event.day == 5)
        );

        let past = window
            .build_calendar_report_at(CalendarMode::Year, 1800, today)
            .expect("calendar report");
        assert!(past.events.is_empty(), "future events are omitted");

        let mid = window
            .build_calendar_report_at(CalendarMode::Year, 1850, today)
            .expect("calendar report");
        assert!(
            mid.events
                .iter()
                .all(|event| event.kind != CalendarEventKind::Death),
            "deaths after the selected year are omitted"
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_pedigree_report() {
        let db_file = temp_path("db");
        import_family(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);

        let husband_id = window
            .tree
            .people
            .iter()
            .find(|person| person.given_name == "John")
            .expect("husband")
            .id
            .clone();

        let pedigree = window
            .build_pedigree_report(&husband_id, 4, false)
            .expect("pedigree report");
        assert_eq!(pedigree.title, "Pedigree Chart");
        assert!(pedigree.show_places);
        assert!(!pedigree.include_photos);
        assert_eq!(pedigree.people.len(), 1);
        assert_eq!(pedigree.people[0].number, Some(1));
        assert_eq!(pedigree.people[0].birth_place, "London");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_family_sheet_report() {
        let db_file = temp_path("db");
        import_family(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let family_id = window.tree.families.first().expect("family").id.clone();

        let choices = window.family_choices();
        assert_eq!(choices.len(), 1);
        assert!(choices[0].label.contains("John"));
        assert_eq!(choices[0].detail, "1 child");

        let sheet = window
            .build_family_sheet_report(&family_id)
            .expect("family sheet report");
        assert_eq!(sheet.children.len(), 1);
        assert_eq!(sheet.marriage_date, "1838");
        assert_eq!(sheet.marriage_place, "Bath");
        let husband = sheet.husband.as_ref().expect("husband");
        assert_eq!(husband.role, "Husband");
        assert_eq!(husband.birth_place, "London");
        assert!(
            husband
                .facts
                .iter()
                .any(|fact| fact.label == "Occupation" && fact.value == "Teacher"),
            "non-vital facts are included: {:?}",
            husband.facts
        );
        assert_eq!(sheet.children[0].name, "Bea Stone");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_individual_summary_report() {
        let db_file = temp_path("db");
        import_family(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let window = TreeWindow::new(app, None);
        let husband = window
            .tree
            .people
            .iter()
            .find(|person| person.given_name == "John")
            .expect("husband");
        let husband_id = husband.id.clone();

        let summary = window
            .build_individual_summary_report(&husband_id, true)
            .expect("summary report");
        assert_eq!(summary.name, "John Stone");
        assert_eq!(summary.birth_place, "London");
        assert_eq!(summary.spouses.len(), 1);
        assert_eq!(summary.spouses[0].marriage_date, "1838");
        assert_eq!(summary.spouses[0].marriage_place, "Bath");
        assert_eq!(summary.children.len(), 1);
        assert_eq!(summary.children[0].name, "Bea Stone");
        assert!(
            summary
                .facts
                .iter()
                .any(|fact| fact.label == "Occupation" && fact.value == "Teacher")
        );
        assert!(
            summary
                .notes
                .as_deref()
                .is_some_and(|notes| notes.contains("Parish register")),
            "notes are included: {:?}",
            summary.notes
        );

        let without_notes = window
            .build_individual_summary_report(&husband_id, false)
            .expect("summary report");
        assert!(without_notes.notes.is_none());

        assert!(
            window
                .build_individual_summary_report("missing", true)
                .is_err()
        );

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn builds_individual_list_report_from_the_visible_rows() {
        let db_file = temp_path("db");
        import_family(&db_file);
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);

        window.watch.filter = "john".to_string();
        window.watch.mark_dirty();
        let list = window
            .build_individual_list_report()
            .expect("individual list report");
        assert_eq!(list.rows.len(), 1);
        assert!(
            list.columns
                .iter()
                .any(|column| column.eq_ignore_ascii_case("given name")),
            "{:?}",
            list.columns
        );
        assert_eq!(list.rows[0][0], "John");
        assert!(list.filter.iter().any(|line| line.contains("john")));

        window.watch.filter = "nobody".to_string();
        window.watch.mark_dirty();
        let list = window
            .build_individual_list_report()
            .expect("individual list report");
        assert!(list.rows.is_empty());
        assert!(window.individual_list_preview().count == 0);

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn media_albums_create_add_move_and_remove() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let first = window
            .app
            .save_media(&crate::db::media::MediaDraft {
                file_path: "/tmp/one.jpg".to_string(),
                ..crate::db::media::MediaDraft::default()
            })
            .expect("first media");
        let second = window
            .app
            .save_media(&crate::db::media::MediaDraft {
                file_path: "/tmp/two.jpg".to_string(),
                ..crate::db::media::MediaDraft::default()
            })
            .expect("second media");
        window.media.loaded = false;
        window.media.albums_loaded = false;
        window.reload_media();
        window.reload_albums();
        assert_eq!(window.media.media.len(), 2);

        window.apply_media_event(MediaEvent::NewAlbum);
        window.media.album_edit.name = "Trip".to_string();
        let draft = window.media.album_edit.to_draft();
        window.apply_media_event(MediaEvent::SaveAlbum(Box::new(draft)));
        let album_id = window.media.selected_album.clone().expect("album selected");
        assert_eq!(window.media.albums.len(), 1);

        window.apply_media_event(MediaEvent::AddToAlbum {
            album_id: album_id.clone(),
            media_id: first.clone(),
        });
        window.apply_media_event(MediaEvent::AddToAlbum {
            album_id: album_id.clone(),
            media_id: second.clone(),
        });
        assert_eq!(window.media.album_media.len(), 2);
        assert_eq!(window.media.album_media[1].id, second);

        window.apply_media_event(MediaEvent::MoveInAlbum {
            album_id: album_id.clone(),
            media_id: second.clone(),
            before_id: Some(first.clone()),
        });
        assert_eq!(window.media.album_media[0].id, second);

        window.load_media_draft(&first);
        assert_eq!(window.media.media_albums.len(), 1);
        window.apply_media_event(MediaEvent::RemoveFromAlbum {
            album_id: album_id.clone(),
            media_id: first.clone(),
        });
        assert_eq!(window.media.album_media.len(), 1);
        assert!(window.media.media_albums.is_empty());

        window.apply_media_event(MediaEvent::DeleteAlbum(album_id));
        assert!(window.media.albums.is_empty());
        assert_eq!(window.app.list_media().expect("media").len(), 2);

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
    fn place_events_can_be_edited_from_the_usage_row() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n1 BIRT\n2 DATE 12 JAN 1850\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let person_id = window.rows.first().expect("one person").id.clone();
        window.app.set_selected_person(Some(person_id.clone()));
        window.sync_inspector_form();
        window.inspector.birth_place = "Uppsala".to_string();
        let details = window.inspector.to_details();
        window.edit_person(&details);
        window.reload_places();

        let place_id = window.places.places[0].id.clone();
        window.apply_places_event(PlacesEvent::Select(place_id.clone()));
        let event_id = window.places.usage[0].event_id.clone();
        window.apply_places_event(PlacesEvent::EditEvent(event_id.clone()));

        let mut editor = window.event_editor.take().expect("event editor opens");
        assert_eq!(editor.draft.date, "12 JAN 1850");
        assert_eq!(editor.event_label, "Birth");
        assert_eq!(editor.place_name.as_deref(), Some("Uppsala"));
        assert_eq!(editor.person_id.as_deref(), Some(person_id.as_str()));
        assert!(!editor.is_dirty());

        editor.draft.date = "3 mar 1851".to_string();
        editor.draft.description = "Moved into the parish".to_string();
        editor.draft.private = true;
        let draft = editor.draft.clone();
        window.apply_event_editor_outcome(editor, EventEditorOutcome::Save(draft));

        assert!(window.event_editor.is_none());
        assert_eq!(window.places.selected.as_deref(), Some(place_id.as_str()));
        assert_eq!(window.places.usage[0].date.as_deref(), Some("3 MAR 1851"));
        let record = window
            .app
            .load_event(&event_id)
            .expect("event loads")
            .expect("event exists");
        assert_eq!(record.description.as_deref(), Some("Moved into the parish"));
        assert!(record.private);
        window.sync_inspector_form();
        assert_eq!(window.inspector.birth, "3 MAR 1851");

        window.apply_places_event(PlacesEvent::EditEvent(event_id));
        let editor = window.event_editor.take().expect("editor reopens");
        window.apply_event_editor_outcome(editor, EventEditorOutcome::Cancel);
        assert!(window.event_editor.is_none(), "cancel closes the editor");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn gazetteer_results_fill_the_place_draft_and_focus_the_map() {
        let db_file = temp_path("db");
        import_people(&db_file, "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 TRLR\n");
        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let place_id = window
            .app
            .save_place(&PlaceDraft {
                name: "Old".to_string(),
                ..PlaceDraft::default()
            })
            .expect("place saves");
        window.reload_places();
        window.apply_places_event(PlacesEvent::Select(place_id.clone()));

        let hit = GazetteerHit {
            display_name: "Uppsala, Sweden".to_string(),
            latitude: 59.8586,
            longitude: 17.6389,
            kind: Some("city".to_string()),
            class: Some("place".to_string()),
        };
        window.use_gazetteer_hit(hit.clone());
        assert_eq!(window.tab, tabs::AppTab::Places);
        assert_eq!(window.places.draft.id.as_deref(), Some(place_id.as_str()));
        assert_eq!(window.places.draft.name, "Uppsala, Sweden");
        assert_eq!(window.places.draft.latitude, "59.858600");
        assert_eq!(window.places.draft.longitude, "17.638900");
        assert!(window.places.editing);

        window.places.selected = None;
        window.places.editing = false;
        window.use_gazetteer_hit(hit.clone());
        assert_eq!(window.places.draft.id, None);
        assert!(window.places.usage.is_empty());
        assert!(window.places.editing);

        window.show_gazetteer_on_map(hit.clone());
        assert_eq!(window.tab, tabs::AppTab::Map);
        assert!(!window.map.needs_fit);
        assert_eq!(window.map.zoom, 12);
        assert!((window.map.center_x - crate::model::map::lon_to_x(17.6389)).abs() < 1e-9);
        assert!((window.map.center_y - crate::model::map::lat_to_y(59.8586)).abs() < 1e-9);

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn map_tab_filters_places_by_group_and_opens_the_place() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ada /King/\n0 @I2@ INDI\n1 NAME Bo /King/\n0 TRLR\n",
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
        for (person_id, place) in [(&ada, "Uppsala"), (&bo, "Lund")] {
            window.app.set_selected_person(Some(person_id.clone()));
            window.sync_inspector_form();
            window.inspector.birth_place = place.to_string();
            let details = window.inspector.to_details();
            window.edit_person(&details);
        }
        window.reload_places();
        assert_eq!(window.places.places.len(), 2);
        let uppsala = window
            .places
            .places
            .iter()
            .find(|place| place.name == "Uppsala")
            .expect("Uppsala")
            .id
            .clone();
        let lund = window
            .places
            .places
            .iter()
            .find(|place| place.name == "Lund")
            .expect("Lund")
            .id
            .clone();
        for (place_id, latitude, longitude) in [
            (&uppsala, "59.8586", "17.6389"),
            (&lund, "55.7047", "13.1910"),
        ] {
            window.apply_places_event(PlacesEvent::Select(place_id.clone()));
            window.places.draft.latitude = latitude.to_string();
            window.places.draft.longitude = longitude.to_string();
            let draft = window.places.draft.to_draft().expect("coordinate draft");
            window.apply_places_event(PlacesEvent::Save(Box::new(draft)));
        }

        let group_id = window
            .app
            .save_group(&GroupDraft {
                name: "One".to_string(),
                ..GroupDraft::default()
            })
            .expect("group saves");
        window
            .app
            .add_group_member(&group_id, &ada)
            .expect("member added");

        window.reload_map();
        assert_eq!(window.map.places.len(), 2);
        assert_eq!(window.map.groups.len(), 1);
        window.select_map_group(Some(group_id));
        let visible = window.map.visible_ids.as_ref().expect("group filter set");
        assert_eq!(visible.len(), 1);
        assert!(visible.contains(&uppsala));

        window.open_place_from_map(uppsala.clone());
        assert_eq!(window.tab, tabs::AppTab::Places);
        assert_eq!(window.places.selected.as_deref(), Some(uppsala.as_str()));
        assert_eq!(window.places.usage.len(), 1);

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn geocode_results_update_place_coordinates() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let id = window
            .app
            .save_place(&PlaceDraft {
                name: "Uppsala".to_string(),
                ..PlaceDraft::default()
            })
            .expect("place saves");
        window.reload_places();
        window.apply_places_event(PlacesEvent::Select(id.clone()));
        window.open_geocode();
        assert_eq!(window.geocode.places.len(), 1);
        assert_eq!(window.geocode.places[0].id, id);

        let result = GeocodeResult {
            place_id: id.clone(),
            place_name: "Uppsala".to_string(),
            query: "uppsala".to_string(),
            outcome: GeocodeOutcome::Found {
                display_name: "Uppsala, Sweden".to_string(),
                latitude: 59.8586,
                longitude: 17.6389,
            },
            cached: false,
        };
        window.geocode.finish(GeocodeReport {
            results: vec![result.clone()],
            cancelled: false,
            error: None,
        });
        window.apply_geocode(vec![result]);

        assert!(!window.geocode.open);
        assert_eq!(window.places.places.len(), 1);
        assert_eq!(window.places.places[0].latitude, Some(59.8586));
        assert_eq!(window.places.places[0].longitude, Some(17.6389));
        assert_eq!(window.places.draft.latitude, "59.8586");
        assert_eq!(window.places.draft.longitude, "17.6389");

        let _ = std::fs::remove_file(&db_file);
    }

    #[test]
    fn place_clean_renames_and_merges_from_the_places_tab() {
        let db_file = temp_path("db");
        import_people(
            &db_file,
            "0 HEAD\n0 @I1@ INDI\n1 NAME Johan /Ahlberg/\n0 TRLR\n",
        );

        let mut app = StemmaApp::new();
        app.activate_tree(&db_file);
        let mut window = TreeWindow::new(app, None);
        let save = |window: &mut TreeWindow, name: &str| {
            window
                .app
                .save_place(&PlaceDraft {
                    name: name.to_string(),
                    ..PlaceDraft::default()
                })
                .expect("place saves")
        };
        save(&mut window, "PARIS");
        let selected = save(&mut window, "paris");
        window.reload_places();
        window.apply_places_event(PlacesEvent::Select(selected.clone()));

        window.open_place_clean();
        let key = window
            .place_clean
            .components
            .iter()
            .find(|component| component.key == "paris")
            .expect("paris component")
            .key
            .clone();
        window.place_clean.selected.insert(key);
        window.place_clean.refresh_preview();
        assert_eq!(window.place_clean.preview.places.len(), 1);
        let preview = window.place_clean.preview.clone();
        window.apply_place_clean_plan(&preview);

        assert_eq!(window.places.places.len(), 1);
        assert_eq!(window.places.places[0].name, "Paris");
        let keeper = window.places.places[0].id.clone();
        assert_eq!(
            window.places.selected.as_deref(),
            Some(keeper.as_str()),
            "a merged place remaps the selection to the keeper"
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
