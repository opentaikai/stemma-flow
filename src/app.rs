//! Application state and background job orchestration for import/export.
//!
//! Native file dialogs and file I/O run on a spawned worker thread so the
//! egui render loop stays responsive; results travel back through an
//! `mpsc` channel that [`StemmaApp::poll`] drains every frame.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use crate::backup::{self, BackupReport, RestoreReport};
use crate::db;
use crate::db::albums::{self as album_store, AlbumDraft, AlbumRecord, AlbumSummary};
use crate::db::associations::{self, AssociationDraft, AssociationRecord, AssociationViewRow};
use crate::db::bookmarks::{self as bookmark_store, BookmarkPerson};
use crate::db::citations::{
    self as citation_store, CitationEdit, CitationRecord, CitationTarget, SourceCitation,
};
use crate::db::dataclean::{self as dataclean_store, DataCleanReport, DataCleanSelection};
use crate::db::events::{self as event_store, PlaceEventDraft, PlaceEventRecord};
use crate::db::facts::{
    self as fact_store, FactRoleDraft, FactTypeDraft, FactTypeRecord, FactTypeSummary,
};
use crate::db::find::{self as find_store, FindResults};
use crate::db::groups::{self as group_store, GroupDraft, GroupMember, GroupRecord, GroupSummary};
use crate::db::labels::{
    self as label_store, LabelDraft, LabelMember, LabelRecord, LabelSummary, PersonLabel,
};
use crate::db::maintenance::{self as maintenance_store, MaintenanceKind, MaintenanceReport};
use crate::db::media::{
    self as media_store, MediaDraft, MediaLinkSummary, MediaOwner, MediaRecord, MediaSummary,
};
use crate::db::merge::{self as merge_store, MergeDraft, MergePreview, MergeReport};
use crate::db::names::{self as name_store, AlternateName, AlternateNameDraft};
use crate::db::notes::{self as note_store, Note, NoteOwner};
use crate::db::participants::{self as participant_store, EventContext, ParticipantDraft};
use crate::db::people::{self, ParentRole, PersonDetails};
use crate::db::places::{self as place_store, PlaceDraft, PlaceRecord, PlaceSummary, PlaceUsage};
use crate::db::privacy::{self as privacy_store, DEFAULT_LIVING_AGE, PersonPrivacy};
use crate::db::problems as problem_store;
use crate::db::repositories::{
    self as repository_store, RepositoryDraft, RepositoryRecord, RepositorySummary,
};
use crate::db::search_replace::{self as replace_store, ReplaceReport, ReplaceSelection};
use crate::db::settings::{self as settings_store, TreeSettings};
use crate::db::sources::{
    self as source_store, SourceDraft, SourceMergeReport, SourceRecord, SourceSummary,
};
use crate::db::tasks::{
    self as task_store, TaskDraft, TaskLinkSummary, TaskOwner, TaskRecord, TaskSummary,
};
use crate::gedcom::{ExportOptions, ImportReport, export_to_gedcom_with_bases};
use crate::geocode;
use crate::gui::dialogs;
use crate::model::dataclean::{DataCleanPlan, build_plan};
use crate::model::geocode::{GeocodeReport, GeocodeRequest, GeocodeResult};
use crate::model::search_replace::{ReplacePlan, ReplaceRequest};
use crate::model::source_template::SourceTemplate;
use crate::report::group_roster::{self, GroupRosterReport};
use crate::report::research_log::{self, ResearchLogReport};

/// Outcome of a background import/export job.
#[derive(Debug)]
pub enum JobEvent {
    Imported(ImportReport),
    /// A freshly created tree database finished importing; the UI should
    /// make it the active tree.
    TreeReady {
        report: ImportReport,
        path: PathBuf,
    },
    Exported {
        bytes: usize,
        path: PathBuf,
    },
    BackedUp {
        path: PathBuf,
        report: BackupReport,
    },
    Restored {
        path: PathBuf,
        report: RestoreReport,
    },
    Cancelled,
    Maintenance {
        report: MaintenanceReport,
        backup_path: Option<PathBuf>,
    },
    ResearchLogExported {
        path: PathBuf,
        bytes: usize,
        tasks: usize,
    },
    GroupRosterExported {
        path: PathBuf,
        bytes: usize,
        members: usize,
    },
    GeocodeProgress {
        done: usize,
        total: usize,
    },
    Geocoded(Box<GeocodeReport>),
    Failed(String),
}

/// Completed maintenance plus an optional backup created first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceResult {
    pub report: MaintenanceReport,
    pub backup_path: Option<PathBuf>,
}

/// Presentation state derived from the most recent job event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiStatus {
    Idle,
    Running(&'static str),
    Success(String),
    Cancelled,
    Error(String),
}

enum Job {
    /// Stream a GEDCOM file into the already active tree.
    Import,
    /// Pick a source file and a fresh target database, then import into it.
    ImportNew,
    Export(ExportOptions),
    Backup,
    Restore,
    Maintenance {
        kind: MaintenanceKind,
        backup_first: bool,
    },
    ResearchLog(Box<ResearchLogReport>),
    GroupRoster(Box<GroupRosterReport>),
    Geocode {
        requests: Vec<GeocodeRequest>,
        cancel: Arc<AtomicBool>,
    },
}

/// Application state shared between the UI shell and job workers.
pub struct StemmaApp {
    active_db: Option<PathBuf>,
    busy: bool,
    status: UiStatus,
    rx: Option<Receiver<JobEvent>>,
    selected_person_id: Option<String>,
    import_generation: u64,
    /// Last import's report waiting for the UI layer to consume it once.
    new_import_report: Option<ImportReport>,
    /// Restored tree waiting for the UI to add it to the recent list.
    restored_path: Option<PathBuf>,
    maintenance: bool,
    maintenance_result: Option<MaintenanceResult>,
    maintenance_error: Option<String>,
    geocode_progress: Option<(usize, usize)>,
    geocode_cancel: Option<Arc<AtomicBool>>,
    geocode_report: Option<GeocodeReport>,
}

impl StemmaApp {
    /// Starts without an active tree; the welcome dashboard shows until
    /// [`Self::activate_tree`] or a background import opens one.
    pub fn new() -> Self {
        Self {
            active_db: None,
            busy: false,
            status: UiStatus::Idle,
            rx: None,
            selected_person_id: None,
            import_generation: 0,
            new_import_report: None,
            restored_path: None,
            maintenance: false,
            maintenance_result: None,
            maintenance_error: None,
            geocode_progress: None,
            geocode_cancel: None,
            geocode_report: None,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn status(&self) -> &UiStatus {
        &self.status
    }

    /// Overwrites the presentation status, e.g. when an inline action is
    /// cancelled before any background job starts.
    pub fn set_status(&mut self, status: UiStatus) {
        self.status = status;
    }

    /// The tree database currently open, if any.
    pub fn active_db(&self) -> Option<&Path> {
        self.active_db.as_deref()
    }

    /// Searches every top-level record type in the active tree.
    pub fn find_everywhere(&self, query: &str, surname_first: bool) -> Result<FindResults, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        find_store::find_everywhere(&conn, query, surname_first, find_store::HITS_PER_KIND)
    }

    /// Makes `path` the active tree and clears any stale selection.
    pub fn activate_tree(&mut self, path: impl Into<PathBuf>) {
        self.active_db = Some(path.into());
        self.selected_person_id = None;
    }

    /// Unloads the active tree so the welcome dashboard takes over.
    pub fn close_tree(&mut self) {
        self.active_db = None;
        self.selected_person_id = None;
    }

    /// Person under the most recent canvas click, if any.
    pub fn selected_person_id(&self) -> Option<&str> {
        self.selected_person_id.as_deref()
    }

    /// Records the canvas selection; `None` clears it.
    pub fn set_selected_person(&mut self, person_id: Option<String>) {
        self.selected_person_id = person_id;
    }

    /// Incremented on every successful import or person mutation so the
    /// canvas knows when to rebuild its scene.
    pub fn import_generation(&self) -> u64 {
        self.import_generation
    }

    /// Streams the chosen file into the active tree on a worker thread.
    pub fn begin_import(&mut self) {
        if self.active_db.is_none() {
            return;
        }
        self.begin(Job::Import, self.active_db.clone());
    }

    /// Picks a source GEDCOM plus a brand-new target database and imports
    /// into it on a worker thread; the resulting tree becomes active.
    pub fn begin_import_new(&mut self) {
        self.begin(Job::ImportNew, None);
    }

    /// Runs a database maintenance operation on a worker thread.
    pub fn begin_maintenance(&mut self, kind: MaintenanceKind, backup_first: bool) {
        if self.active_db.is_none() || self.busy {
            return;
        }
        self.maintenance = true;
        self.begin(
            Job::Maintenance { kind, backup_first },
            self.active_db.clone(),
        );
    }

    /// Whether a maintenance operation is blocking normal editing.
    pub fn is_maintenance(&self) -> bool {
        self.maintenance
    }

    /// Returns the last maintenance result exactly once.
    pub fn take_maintenance_result(&mut self) -> Option<MaintenanceResult> {
        self.maintenance_result.take()
    }

    /// Returns a maintenance failure exactly once for the tools dialog.
    pub fn take_maintenance_error(&mut self) -> Option<String> {
        self.maintenance_error.take()
    }

    /// Writes a print-ready HTML research log on a worker thread.
    pub fn begin_research_log_export(&mut self, report: ResearchLogReport) {
        self.begin(Job::ResearchLog(Box::new(report)), self.active_db.clone());
    }

    /// Writes a print-ready HTML group roster on a worker thread.
    pub fn begin_group_roster_export(&mut self, report: GroupRosterReport) {
        self.begin(Job::GroupRoster(Box::new(report)), self.active_db.clone());
    }

    /// Starts a cache-first place geocoding batch.
    pub fn begin_geocode(&mut self, requests: Vec<GeocodeRequest>) -> Result<(), String> {
        if self.busy {
            return Err("Another operation is running".to_string());
        }
        if self.active_db.is_none() {
            return Err("No tree is open".to_string());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.geocode_cancel = Some(cancel.clone());
        self.geocode_progress = Some((0, requests.len()));
        self.geocode_report = None;
        self.begin(Job::Geocode { requests, cancel }, self.active_db.clone());
        Ok(())
    }

    /// Requests cancellation of the running geocoding batch.
    pub fn cancel_geocode(&mut self) {
        if let Some(cancel) = &self.geocode_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Reports the latest geocoding progress as `(done, total)`.
    pub fn geocode_progress(&self) -> Option<(usize, usize)> {
        self.geocode_progress
    }

    /// Returns a completed geocoding report exactly once.
    pub fn take_geocode_report(&mut self) -> Option<GeocodeReport> {
        self.geocode_report.take()
    }

    /// Removes every cached geocoding result from the active tree.
    pub fn clear_geocode_cache(&mut self) -> Result<usize, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let removed = db::geocode_cache::clear(&conn)?;
        self.set_status(UiStatus::Success(format!(
            "Cleared {removed} cached place lookups"
        )));
        Ok(removed)
    }

    /// Writes coordinates from approved geocoding results.
    pub fn apply_geocode_results(&mut self, results: &[GeocodeResult]) -> Result<usize, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        let mut changed = 0;
        for result in results {
            if let Some((latitude, longitude)) = result.coordinates() {
                db::places::set_coordinates(&tx, &result.place_id, latitude, longitude)?;
                changed += 1;
            }
        }
        tx.commit().map_err(|error| error.to_string())?;
        self.set_status(UiStatus::Success(format!(
            "Updated coordinates for {changed} places"
        )));
        Ok(changed)
    }

    /// Writes a timestamped backup archive of the active tree and its media.
    pub fn begin_backup(&mut self) {
        if self.active_db.is_none() {
            return;
        }
        self.begin(Job::Backup, self.active_db.clone());
    }

    /// Opens a backup archive in a new tree on a worker thread.
    pub fn begin_restore(&mut self) {
        self.begin(Job::Restore, None);
    }

    /// Writes the active database as GEDCOM with default privacy options.
    pub fn begin_export(&mut self) {
        self.begin_export_with_options(ExportOptions::default());
    }

    /// Writes the active database as GEDCOM with explicit privacy options.
    pub fn begin_export_with_options(&mut self, options: ExportOptions) {
        if self.active_db.is_none() {
            return;
        }
        self.begin(Job::Export(options), self.active_db.clone());
    }

    /// Inserts an unlinked person into the active tree.
    pub fn add_person(&mut self, details: &PersonDetails) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let person_id = people::insert_person(&mut conn, details)?;
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "Added {}",
            details.display_name()
        )));
        Ok(person_id)
    }

    /// Loads both sides of a potential duplicate merge.
    pub fn preview_people_merge(
        &self,
        keep_id: &str,
        merge_id: &str,
    ) -> Result<MergePreview, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        merge_store::preview_merge(&conn, keep_id, merge_id)
    }

    /// Merges one person into another and deletes the merged record.
    pub fn merge_people(&mut self, draft: &MergeDraft) -> Result<MergeReport, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let report = merge_store::merge_people(&mut conn, draft)?;
        if self.selected_person_id.as_deref() == Some(draft.merge_id.as_str()) {
            self.selected_person_id = Some(draft.keep_id.clone());
        }
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "Merged duplicate into {}",
            draft.details.display_name()
        )));
        Ok(report)
    }

    /// Removes a person; FK cascades clean up events, citations and links.
    pub fn delete_person(&mut self, person_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        people::delete_person(&mut conn, person_id)?;
        if self.selected_person_id.as_deref() == Some(person_id) {
            self.selected_person_id = None;
        }
        self.import_generation += 1;
        self.set_status(UiStatus::Success("Person deleted".to_string()));
        Ok(())
    }

    fn require_active_db(&self) -> Result<PathBuf, String> {
        self.active_db
            .clone()
            .ok_or_else(|| "No tree is open".to_string())
    }

    /// Writes edited profile fields back over `person_id`.
    pub fn edit_person(&mut self, person_id: &str, details: &PersonDetails) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        people::update_person(&mut conn, person_id, details)?;
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "Saved {}",
            details.display_name()
        )));
        Ok(())
    }

    /// Creates a new parent of `child_id` under `role` in one step.
    pub fn add_parent(
        &mut self,
        child_id: &str,
        role: ParentRole,
        details: &PersonDetails,
    ) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let person_id = people::link_parent(&mut conn, child_id, role, details)?;
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "Added {}",
            details.display_name()
        )));
        Ok(person_id)
    }

    /// Creates a partner for `person_id` in a new family record.
    pub fn add_spouse(
        &mut self,
        person_id: &str,
        details: &PersonDetails,
    ) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let spouse_id = people::link_spouse(&mut conn, person_id, details)?;
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "Added {}",
            details.display_name()
        )));
        Ok(spouse_id)
    }

    /// Adds a child to `person_id`'s primary family, creating it if needed.
    pub fn add_child(
        &mut self,
        person_id: &str,
        details: &PersonDetails,
    ) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let child_id = people::link_child(&mut conn, person_id, details)?;
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "Added {}",
            details.display_name()
        )));
        Ok(child_id)
    }

    /// Links an existing person as `child_id`'s parent.
    pub fn link_existing_parent(
        &mut self,
        child_id: &str,
        parent_id: &str,
        role: ParentRole,
    ) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        people::link_existing_parent(&mut conn, child_id, parent_id, role)?;
        self.finish_link()
    }

    /// Links an existing person as `person_id`'s spouse.
    pub fn link_existing_spouse(&mut self, person_id: &str, spouse_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        people::link_existing_spouse(&mut conn, person_id, spouse_id)?;
        self.finish_link()
    }

    /// Links an existing person as `parent_id`'s child.
    pub fn link_existing_child(&mut self, parent_id: &str, child_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        people::link_existing_child(&mut conn, parent_id, child_id)?;
        self.finish_link()
    }

    fn finish_link(&mut self) -> Result<(), String> {
        self.import_generation += 1;
        self.set_status(UiStatus::Success("Linked existing person".to_string()));
        Ok(())
    }

    /// Lists the citations attached to a person's events.
    pub fn list_person_citations(&self, person_id: &str) -> Result<Vec<CitationRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        citation_store::list_person_citations(&conn, person_id)
    }

    /// Updates a citation's page and quality values.
    pub fn save_citation(&mut self, edit: &CitationEdit) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        citation_store::save_citation(&conn, edit)?;
        self.set_status(UiStatus::Success("Citation saved".to_string()));
        Ok(())
    }

    /// Lists the events the person inspector can attach citations to.
    pub fn citation_targets(&self, person_id: &str) -> Result<Vec<CitationTarget>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        citation_store::list_citation_targets(&conn, person_id)
    }

    /// Lists a source's citations for reuse, with usage counts.
    pub fn source_citations(&self, source_id: &str) -> Result<Vec<SourceCitation>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        citation_store::list_source_citations(&conn, source_id)
    }

    /// Attaches a source to a target event, creating the event if needed.
    pub fn add_citation(
        &mut self,
        source_id: &str,
        person_id: &str,
        target: &str,
        page: Option<&str>,
    ) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = citation_store::add_citation(&mut conn, source_id, person_id, target, page)?;
        self.set_status(UiStatus::Success("Citation added".to_string()));
        Ok(id)
    }

    /// Attaches an existing citation to another event.
    pub fn attach_citation(
        &mut self,
        citation_id: &str,
        person_id: &str,
        target: &str,
    ) -> Result<bool, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let added = citation_store::attach_citation(&mut conn, citation_id, person_id, target)?;
        self.set_status(UiStatus::Success(if added {
            "Citation attached".to_string()
        } else {
            "Citation already attached".to_string()
        }));
        Ok(added)
    }

    /// Removes one citation-to-event link.
    pub fn detach_citation(&mut self, citation_id: &str, event_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        citation_store::detach_citation(&conn, citation_id, event_id)?;
        self.set_status(UiStatus::Success("Citation detached".to_string()));
        Ok(())
    }

    /// Deletes a citation without touching its source or event.
    pub fn delete_citation(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        citation_store::delete_citation(&conn, id)?;
        self.set_status(UiStatus::Success("Citation deleted".to_string()));
        Ok(())
    }

    /// Loads the source template catalogue.
    pub fn source_templates(&self) -> Result<Vec<SourceTemplate>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        source_store::load_templates(&conn)
    }

    /// Lists sources with their citation counts.
    pub fn list_sources(&self) -> Result<Vec<SourceSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        source_store::list_sources(&conn)
    }

    /// Loads one source for editing.
    pub fn load_source(&self, id: &str) -> Result<Option<SourceRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        source_store::load_source(&conn, id)
    }

    /// Inserts or updates a source.
    pub fn save_source(&mut self, draft: &SourceDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = source_store::save_source(&mut conn, draft)?;
        self.set_status(UiStatus::Success("Source saved".to_string()));
        Ok(id)
    }

    /// Merges one source into another, moving citations and links.
    pub fn merge_sources(
        &mut self,
        source_id: &str,
        target_id: &str,
    ) -> Result<SourceMergeReport, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let report = source_store::merge_sources(&mut conn, source_id, target_id)?;
        let noun = if report.citations == 1 {
            "citation"
        } else {
            "citations"
        };
        self.set_status(UiStatus::Success(format!(
            "Merged {} {noun}",
            report.citations
        )));
        Ok(report)
    }

    /// Deletes a source and its citations.
    pub fn delete_source(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        source_store::delete_source(&mut conn, id)?;
        self.set_status(UiStatus::Success("Source deleted".to_string()));
        Ok(())
    }

    /// Lists every static group with member counts.
    pub fn list_groups(&self) -> Result<Vec<GroupSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        group_store::list_groups(&conn)
    }

    /// Lists groups containing one person.
    pub fn list_groups_for_person(&self, person_id: &str) -> Result<Vec<GroupSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        group_store::list_groups_for_person(&conn, person_id)
    }

    /// Loads one group for editing.
    pub fn load_group(&self, id: &str) -> Result<Option<GroupRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        group_store::load_group(&conn, id)
    }

    /// Inserts or updates a group.
    pub fn save_group(&mut self, draft: &GroupDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = group_store::save_group(&conn, draft)?;
        self.set_status(UiStatus::Success("Group saved".to_string()));
        Ok(id)
    }

    /// Deletes one group and its memberships.
    pub fn delete_group(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        group_store::delete_group(&conn, id)?;
        self.set_status(UiStatus::Success("Group deleted".to_string()));
        Ok(())
    }

    /// Lists bookmarked people in insertion order.
    pub fn list_bookmarks(&self, surname_first: bool) -> Result<Vec<BookmarkPerson>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        bookmark_store::list_bookmarks(&conn, surname_first)
    }

    /// Adds or removes a person bookmark.
    pub fn toggle_bookmark(&mut self, person_id: &str) -> Result<bool, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        bookmark_store::toggle_bookmark(&conn, person_id)
    }

    /// Removes a person bookmark when present.
    pub fn remove_bookmark(&mut self, person_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        bookmark_store::remove_bookmark(&conn, person_id)
    }

    /// Lists every coloured person label with member counts.
    pub fn list_labels(&self) -> Result<Vec<LabelSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        label_store::list_labels(&conn)
    }

    /// Loads one label for editing.
    pub fn load_label(&self, id: &str) -> Result<Option<LabelRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        label_store::load_label(&conn, id)
    }

    /// Inserts or updates a label.
    pub fn save_label(&mut self, draft: &LabelDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = label_store::save_label(&conn, draft)?;
        self.set_status(UiStatus::Success("Label saved".to_string()));
        Ok(id)
    }

    /// Deletes one label and its assignments.
    pub fn delete_label(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        label_store::delete_label(&conn, id)?;
        self.set_status(UiStatus::Success("Label deleted".to_string()));
        Ok(())
    }

    /// Lists a label's members.
    pub fn list_label_members(
        &self,
        label_id: &str,
        surname_first: bool,
    ) -> Result<Vec<LabelMember>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        label_store::list_members(&conn, label_id, surname_first)
    }

    /// Lists labels carried by one person.
    pub fn list_labels_for_person(&self, person_id: &str) -> Result<Vec<LabelSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        label_store::list_labels_for_person(&conn, person_id)
    }

    /// Loads every person-label assignment for badge building.
    pub fn list_all_person_labels(&self) -> Result<Vec<PersonLabel>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        label_store::list_all_person_labels(&conn)
    }

    /// Adds a person to a label.
    pub fn add_label_member(&mut self, label_id: &str, person_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        label_store::add_member(&conn, label_id, person_id)?;
        Ok(())
    }

    /// Removes a person from a label.
    pub fn remove_label_member(&mut self, label_id: &str, person_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        label_store::remove_member(&conn, label_id, person_id)?;
        Ok(())
    }

    /// Lists a group's members.
    pub fn list_group_members(
        &self,
        group_id: &str,
        surname_first: bool,
    ) -> Result<Vec<GroupMember>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        group_store::list_members(&conn, group_id, surname_first)
    }

    /// Adds a person to a group.
    pub fn add_group_member(&mut self, group_id: &str, person_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        group_store::add_member(&conn, group_id, person_id)?;
        self.set_status(UiStatus::Success("Group member added".to_string()));
        Ok(())
    }

    /// Removes a person from a group.
    pub fn remove_group_member(&mut self, group_id: &str, person_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        group_store::remove_member(&conn, group_id, person_id)?;
        self.set_status(UiStatus::Success("Group member removed".to_string()));
        Ok(())
    }

    /// Lists every task with its link count.
    pub fn list_tasks(&self) -> Result<Vec<TaskSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        task_store::list_tasks(&conn)
    }

    /// Loads one task for editing.
    pub fn load_task(&self, id: &str) -> Result<Option<TaskRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        task_store::load_task(&conn, id)
    }

    /// Inserts or updates a task.
    pub fn save_task(&mut self, draft: &TaskDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = task_store::save_task(&conn, draft)?;
        self.set_status(UiStatus::Success("Task saved".to_string()));
        Ok(id)
    }

    /// Deletes one task and its links.
    pub fn delete_task(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        task_store::delete_task(&conn, id)?;
        self.set_status(UiStatus::Success("Task deleted".to_string()));
        Ok(())
    }

    /// Builds a DataClean preview from the active tree.
    pub fn data_clean_plan(&self) -> Result<DataCleanPlan, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let tree = db::load_family_tree(&conn).map_err(|error| error.to_string())?;
        let alternate_names = name_store::list_all_alternate_names(&conn)?;
        let places = place_store::list_places(&conn)?;
        Ok(build_plan(&tree.people, &alternate_names, &places))
    }

    /// Applies the selected DataClean rows and bumps the scene generation.
    pub fn apply_data_clean(
        &mut self,
        plan: &DataCleanPlan,
        selection: &DataCleanSelection,
    ) -> Result<DataCleanReport, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let report = dataclean_store::apply_plan(&mut conn, plan, selection)?;
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "DataClean applied: {} names, {} alternate names, {} places",
            report.names_changed,
            report.alternate_names_changed,
            report.places_renamed + report.places_merged
        )));
        Ok(report)
    }

    /// Applies a PlaceClean plan and bumps the scene generation.
    pub fn apply_place_clean(
        &mut self,
        plan: &DataCleanPlan,
        selection: &DataCleanSelection,
    ) -> Result<DataCleanReport, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let report = dataclean_store::apply_plan(&mut conn, plan, selection)?;
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "PlaceClean applied: {} places, {} merges",
            report.places_renamed, report.places_merged
        )));
        Ok(report)
    }

    /// Builds a literal search-and-replace preview from the active tree.
    pub fn preview_search_replace(
        &self,
        request: &ReplaceRequest,
        surname_first: bool,
    ) -> Result<ReplacePlan, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        replace_store::preview(&conn, request, surname_first)
    }

    /// Applies selected replacement rows and bumps the scene generation.
    pub fn apply_search_replace(
        &mut self,
        plan: &ReplacePlan,
        selection: &ReplaceSelection,
    ) -> Result<ReplaceReport, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let report = replace_store::apply_plan(&mut conn, plan, selection)?;
        self.import_generation += 1;
        self.set_status(UiStatus::Success(format!(
            "Replaced {} fields",
            report.total()
        )));
        Ok(report)
    }

    /// Loads the keys of problem alerts marked as not a problem.
    pub fn list_problem_dismissals(&self) -> Result<std::collections::HashSet<String>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        problem_store::list_dismissed(&conn)
    }

    /// Marks one problem alert as not a problem.
    pub fn dismiss_problem(&mut self, alert_key: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        problem_store::dismiss(&conn, alert_key)
    }

    /// Restores one dismissed problem alert.
    pub fn restore_problem(&mut self, alert_key: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        problem_store::restore(&conn, alert_key)
    }

    /// Restores every dismissed problem alert.
    pub fn clear_problem_dismissals(&mut self) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        problem_store::clear(&conn)
    }

    /// Lists the distinct task folders.
    pub fn list_task_folders(&self) -> Result<Vec<String>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        task_store::list_folders(&conn)
    }

    /// Lists tasks linked to one record.
    pub fn list_owner_tasks(&self, owner: &TaskOwner) -> Result<Vec<TaskSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        task_store::list_owner_tasks(&conn, owner)
    }

    /// Attaches a task to one record.
    pub fn attach_task(&mut self, task_id: &str, owner: &TaskOwner) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        task_store::attach_task(&conn, task_id, owner)?;
        self.set_status(UiStatus::Success("Task attached".to_string()));
        Ok(())
    }

    /// Detaches a task from one record.
    pub fn detach_task(&mut self, task_id: &str, owner: &TaskOwner) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        task_store::detach_task(&conn, task_id, owner)?;
        self.set_status(UiStatus::Success("Task detached".to_string()));
        Ok(())
    }

    /// Lists the records linked to one task.
    pub fn list_task_links(
        &self,
        task_id: &str,
        surname_first: bool,
    ) -> Result<Vec<TaskLinkSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        task_store::list_task_links(&conn, task_id, surname_first)
    }

    /// Lists every media record.
    pub fn list_media(&self) -> Result<Vec<MediaSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        media_store::list_media_with_base(&conn, path.parent())
    }

    /// Loads one media record for editing.
    pub fn load_media(&self, id: &str) -> Result<Option<MediaRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        media_store::load_media(&conn, id)
    }

    /// Inserts or updates a media record.
    pub fn save_media(&mut self, draft: &MediaDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        let id = media_store::save_media_with_base(&conn, draft, path.parent())?;
        self.set_status(UiStatus::Success("Media saved".to_string()));
        Ok(id)
    }

    /// Points an existing media record at a new file while preserving its links.
    pub fn relink_media(&mut self, media_id: &str, file_path: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        let record = media_store::load_media(&conn, media_id)?
            .ok_or_else(|| format!("no media with id {media_id}"))?;
        let format = media_store::inferred_format(file_path)
            .or(record.format)
            .unwrap_or_default();
        let draft = MediaDraft {
            id: Some(record.id),
            file_path: file_path.to_string(),
            format,
            caption: record.caption.unwrap_or_default(),
            notes: record.notes.unwrap_or_default(),
        };
        media_store::save_media_with_base(&conn, &draft, path.parent())?;
        self.set_status(UiStatus::Success("Media relinked".to_string()));
        Ok(())
    }

    /// Deletes one media record and its links.
    pub fn delete_media(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        media_store::delete_media(&conn, id)?;
        self.set_status(UiStatus::Success("Media deleted".to_string()));
        Ok(())
    }

    /// Lists general and per-person media albums.
    pub fn list_albums(&self, surname_first: bool) -> Result<Vec<AlbumSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        album_store::list_albums(&conn, surname_first)
    }

    /// Loads one album for editing.
    pub fn load_album(&self, id: &str) -> Result<Option<AlbumRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        album_store::load_album(&conn, id)
    }

    /// Inserts or updates an album.
    pub fn save_album(&mut self, draft: &AlbumDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = album_store::save_album(&conn, draft)?;
        self.set_status(UiStatus::Success("Album saved".to_string()));
        Ok(id)
    }

    /// Deletes an album; its media records are kept.
    pub fn delete_album(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        album_store::delete_album(&conn, id)?;
        self.set_status(UiStatus::Success("Album deleted".to_string()));
        Ok(())
    }

    /// Lists the ordered media in one album.
    pub fn list_album_media(&self, album_id: &str) -> Result<Vec<MediaSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        album_store::list_album_media_with_base(&conn, album_id, path.parent())
    }

    /// Adds a media record to an album.
    pub fn add_media_to_album(&mut self, album_id: &str, media_id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        album_store::add_to_album(&conn, album_id, media_id)?;
        self.set_status(UiStatus::Success("Added to album".to_string()));
        Ok(())
    }

    /// Removes a media record from an album.
    pub fn remove_media_from_album(
        &mut self,
        album_id: &str,
        media_id: &str,
    ) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        album_store::remove_from_album(&conn, album_id, media_id)?;
        self.set_status(UiStatus::Success("Removed from album".to_string()));
        Ok(())
    }

    /// Moves a media record before another entry, or to the end.
    pub fn move_media_in_album(
        &mut self,
        album_id: &str,
        media_id: &str,
        before_id: Option<&str>,
    ) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        album_store::move_album_media(&conn, album_id, media_id, before_id)?;
        self.set_status(UiStatus::Success("Album order updated".to_string()));
        Ok(())
    }

    /// Lists the albums containing one media record.
    pub fn albums_for_media(
        &self,
        media_id: &str,
        surname_first: bool,
    ) -> Result<Vec<AlbumSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        album_store::albums_for_media(&conn, media_id, surname_first)
    }

    /// Lists media attached to one record.
    pub fn list_owner_media(&self, owner: &MediaOwner) -> Result<Vec<MediaSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        media_store::list_owner_media_with_base(&conn, owner, path.parent())
    }

    /// Attaches media to one record.
    pub fn attach_media(&mut self, media_id: &str, owner: &MediaOwner) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        media_store::attach_media(&conn, media_id, owner)?;
        self.set_status(UiStatus::Success("Media attached".to_string()));
        Ok(())
    }

    /// Detaches media from one record.
    pub fn detach_media(&mut self, media_id: &str, owner: &MediaOwner) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        media_store::detach_media(&conn, media_id, owner)?;
        self.set_status(UiStatus::Success("Media detached".to_string()));
        Ok(())
    }

    /// Lists the records using one media item.
    pub fn list_media_links(
        &self,
        media_id: &str,
        surname_first: bool,
    ) -> Result<Vec<MediaLinkSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        media_store::list_media_links(&conn, media_id, surname_first)
    }

    /// Lists every directed association.
    pub fn list_all_associations(
        &self,
        surname_first: bool,
    ) -> Result<Vec<AssociationViewRow>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        associations::list_all_associations(&conn, surname_first)
    }

    /// Lists the outgoing associations of a person.
    pub fn list_associations(
        &self,
        person_id: &str,
        surname_first: bool,
    ) -> Result<Vec<AssociationRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        associations::list_associations(&conn, person_id, surname_first)
    }

    /// Adds or updates a person association.
    pub fn save_association(&mut self, draft: &AssociationDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = associations::save_association(&mut conn, draft)?;
        self.set_status(UiStatus::Success("Association saved".to_string()));
        Ok(id)
    }

    /// Deletes one person association.
    pub fn delete_association(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        associations::delete_association(&conn, id)?;
        self.set_status(UiStatus::Success("Association deleted".to_string()));
        Ok(())
    }

    /// Lists the notes attached to one record.
    pub fn list_notes(&self, owner: &NoteOwner) -> Result<Vec<Note>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        note_store::list_notes(&conn, owner)
    }

    /// Adds a note to one record.
    pub fn add_note(&mut self, owner: &NoteOwner, body: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        note_store::add_note(&conn, owner, body)?;
        self.set_status(UiStatus::Success("Note added".to_string()));
        Ok(())
    }

    /// Updates an existing note.
    pub fn update_note(&mut self, id: &str, body: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        note_store::update_note(&conn, id, body)?;
        self.set_status(UiStatus::Success("Note saved".to_string()));
        Ok(())
    }

    /// Deletes one note.
    pub fn delete_note(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        note_store::delete_note(&conn, id)?;
        self.set_status(UiStatus::Success("Note deleted".to_string()));
        Ok(())
    }

    /// Loads the active tree's display settings.
    pub fn tree_settings(&self) -> Result<TreeSettings, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        settings_store::load_tree_settings(&conn)
    }

    /// Updates the active tree's surname-first preference.
    pub fn set_surname_first(&mut self, surname_first: bool) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        settings_store::set_surname_first(&conn, surname_first)?;
        self.set_status(UiStatus::Success("Display preference updated".to_string()));
        Ok(())
    }

    /// Stores or clears the active tree's root person.
    pub fn set_root_person(&mut self, person_id: Option<&str>) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        settings_store::set_root_person(&conn, person_id)
    }

    /// Lists a person's alternate names.
    pub fn alternate_names(&self, person_id: &str) -> Result<Vec<AlternateName>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        name_store::list_alternate_names(&conn, person_id)
    }

    /// Inserts or updates an alternate name.
    pub fn save_alternate_name(
        &mut self,
        person_id: &str,
        draft: &AlternateNameDraft,
    ) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = name_store::save_alternate_name(&mut conn, person_id, draft)?;
        self.set_status(UiStatus::Success("Alternate name saved".to_string()));
        Ok(id)
    }

    /// Deletes an alternate name.
    pub fn delete_alternate_name(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        name_store::delete_alternate_name(&conn, id)?;
        self.set_status(UiStatus::Success("Alternate name deleted".to_string()));
        Ok(())
    }

    /// Loads the selected person's privacy flags and event list.
    pub fn person_privacy(&self, person_id: &str) -> Result<Option<PersonPrivacy>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let year = privacy_store::current_year(&conn)?;
        privacy_store::load_person_privacy(&conn, person_id, year, DEFAULT_LIVING_AGE)
    }

    /// Updates a person's private flag.
    pub fn set_person_private(&mut self, person_id: &str, private: bool) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        privacy_store::set_person_private(&conn, person_id, private)?;
        self.set_status(UiStatus::Success("Privacy updated".to_string()));
        Ok(())
    }

    /// Updates the Auto/Living/Deceased override.
    pub fn set_living_override(
        &mut self,
        person_id: &str,
        override_value: Option<bool>,
    ) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        privacy_store::set_living_override(&conn, person_id, override_value)?;
        self.set_status(UiStatus::Success("Living status updated".to_string()));
        Ok(())
    }

    /// Updates an event's private flag.
    pub fn set_event_private(&mut self, event_id: &str, private: bool) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        privacy_store::set_event_private(&conn, event_id, private)?;
        self.set_status(UiStatus::Success("Fact privacy updated".to_string()));
        Ok(())
    }

    /// Loads an event with its roles and participants.
    pub fn load_event_context(
        &self,
        event_id: &str,
        surname_first: bool,
    ) -> Result<Option<EventContext>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        participant_store::load_event_context(&conn, event_id, surname_first)
    }

    /// Replaces an event's participants.
    pub fn save_event_participants(
        &mut self,
        event_id: &str,
        participants: &[ParticipantDraft],
    ) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        participant_store::save_event_participants(&mut conn, event_id, participants)?;
        self.set_status(UiStatus::Success("Witnesses saved".to_string()));
        Ok(())
    }

    /// Lists fact types with role/event counts.
    pub fn list_fact_types(&self) -> Result<Vec<FactTypeSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        fact_store::list_fact_type_summaries(&conn)
    }

    /// Loads one fact type and its roles.
    pub fn load_fact_type(&self, key: &str) -> Result<Option<FactTypeRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        fact_store::load_fact_type(&conn, key)
    }

    /// Inserts or updates a custom fact type.
    pub fn save_fact_type(&mut self, draft: &FactTypeDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let key = fact_store::save_fact_type(&mut conn, draft)?;
        self.set_status(UiStatus::Success("Fact type saved".to_string()));
        Ok(key)
    }

    /// Replaces the roles of a fact type, including built-ins.
    pub fn save_fact_roles(&mut self, key: &str, roles: &[FactRoleDraft]) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        fact_store::save_fact_roles(&mut conn, key, roles)?;
        self.set_status(UiStatus::Success("Fact roles saved".to_string()));
        Ok(())
    }

    /// Deletes an unused custom fact type.
    pub fn delete_fact_type(&mut self, key: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        fact_store::delete_fact_type(&mut conn, key)?;
        self.set_status(UiStatus::Success("Fact type deleted".to_string()));
        Ok(())
    }

    /// Lists places with their event counts.
    pub fn list_places(&self) -> Result<Vec<PlaceSummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        place_store::list_places(&conn)
    }

    /// Loads one place for editing.
    pub fn load_place(&self, id: &str) -> Result<Option<PlaceRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        place_store::load_place(&conn, id)
    }

    /// Lists the events that use a place.
    pub fn place_usage(&self, id: &str, surname_first: bool) -> Result<Vec<PlaceUsage>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        place_store::list_place_usage(&conn, id, surname_first)
    }

    /// Returns the places linked to any of `person_ids`.
    pub fn place_ids_for_people(
        &self,
        person_ids: &[String],
    ) -> Result<std::collections::HashSet<String>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        place_store::place_ids_for_people(&conn, person_ids)
    }

    /// Loads one event for editing from the place list.
    pub fn load_event(&self, id: &str) -> Result<Option<PlaceEventRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        event_store::load_event(&conn, id)
    }

    /// Updates an event's date, description and privacy.
    pub fn save_event(&mut self, draft: &PlaceEventDraft) -> Result<(), String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        event_store::save_event(&conn, draft)?;
        self.set_status(UiStatus::Success("Event saved".to_string()));
        Ok(())
    }

    /// Inserts or updates a place.
    pub fn save_place(&mut self, draft: &PlaceDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = place_store::save_place(&mut conn, draft)?;
        self.set_status(UiStatus::Success("Place saved".to_string()));
        Ok(id)
    }

    /// Deletes a place; events lose their place reference.
    pub fn delete_place(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        place_store::delete_place(&mut conn, id)?;
        self.set_status(UiStatus::Success("Place deleted".to_string()));
        Ok(())
    }

    /// Lists repositories with their linked-source counts.
    pub fn list_repositories(&self) -> Result<Vec<RepositorySummary>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        repository_store::list_repositories(&conn)
    }

    /// Loads one repository for editing.
    pub fn load_repository(&self, id: &str) -> Result<Option<RepositoryRecord>, String> {
        let path = self.require_active_db()?;
        let conn = db::open_connection(path).map_err(|error| error.to_string())?;
        repository_store::load_repository(&conn, id)
    }

    /// Inserts or updates a repository and its source links.
    pub fn save_repository(&mut self, draft: &RepositoryDraft) -> Result<String, String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        let id = repository_store::save_repository(&mut conn, draft)?;
        self.set_status(UiStatus::Success("Repository saved".to_string()));
        Ok(id)
    }

    /// Deletes a repository and its source links.
    pub fn delete_repository(&mut self, id: &str) -> Result<(), String> {
        let path = self.require_active_db()?;
        let mut conn = db::open_connection(path).map_err(|error| error.to_string())?;
        repository_store::delete_repository(&mut conn, id)?;
        self.set_status(UiStatus::Success("Repository deleted".to_string()));
        Ok(())
    }

    fn begin(&mut self, job: Job, db_path: Option<PathBuf>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = UiStatus::Running(match job {
            Job::Import | Job::ImportNew => "Choosing file to import\u{2026}",
            Job::Export(_) => "Choosing export destination\u{2026}",
            Job::Backup => "Choosing backup destination\u{2026}",
            Job::Restore => "Choosing backup to restore\u{2026}",
            Job::Maintenance { .. } => "Running database maintenance\u{2026}",
            Job::ResearchLog(_) => "Choosing research log destination\u{2026}",
            Job::GroupRoster(_) => "Choosing group report destination\u{2026}",
            Job::Geocode { .. } => "Looking up place coordinates\u{2026}",
        });
        let (tx, rx) = channel();
        self.rx = Some(rx);
        thread::spawn(move || job.run(db_path, &tx));
    }

    /// Applies every job event that arrived since the last frame.
    ///
    /// Returns `true` when at least one event was applied.
    pub fn poll(&mut self) -> bool {
        let events: Vec<JobEvent> = match &self.rx {
            Some(rx) => rx.try_iter().collect(),
            None => return false,
        };
        if events.is_empty() {
            return false;
        }
        for event in events {
            self.apply_event(event);
        }
        true
    }

    /// Applies a single job event to the presentation state.
    pub fn apply_event(&mut self, event: JobEvent) {
        if let JobEvent::GeocodeProgress { done, total } = event {
            self.geocode_progress = Some((done, total));
            return;
        }
        let was_maintenance = self.maintenance;
        let was_geocoding = self.geocode_cancel.is_some();
        self.busy = false;
        self.maintenance = false;
        self.status = match event {
            JobEvent::Imported(report) => {
                self.import_generation += 1;
                self.new_import_report = Some(report.clone());
                UiStatus::Success(import_summary(&report))
            }
            JobEvent::TreeReady { report, path } => {
                self.activate_tree(path);
                self.import_generation += 1;
                self.new_import_report = Some(report.clone());
                UiStatus::Success(import_summary(&report))
            }
            JobEvent::Exported { bytes, path } => {
                UiStatus::Success(format!("Exported {bytes} bytes to {}", path.display()))
            }
            JobEvent::BackedUp { path, report } => {
                let missing = report.media_missing.len();
                let suffix = match missing {
                    0 => String::new(),
                    1 => ", 1 media file missing".to_string(),
                    count => format!(", {count} media files missing"),
                };
                UiStatus::Success(format!(
                    "Backed up {} media files to {}{suffix}",
                    report.media_included,
                    path.display()
                ))
            }
            JobEvent::Restored { path, report } => {
                self.activate_tree(path.clone());
                self.import_generation += 1;
                self.restored_path = Some(path.clone());
                let missing = report.media_missing;
                let suffix = match missing {
                    0 => String::new(),
                    1 => ", 1 media file missing".to_string(),
                    count => format!(", {count} media files missing"),
                };
                UiStatus::Success(format!("Restored tree to {}{suffix}", path.display()))
            }
            JobEvent::Maintenance {
                report,
                backup_path,
            } => {
                let status = if report.kind == MaintenanceKind::Integrity && !report.healthy {
                    UiStatus::Error(format!(
                        "Integrity check found {} issues and {} foreign key violations",
                        report.issues.len(),
                        report.foreign_key_violations
                    ))
                } else {
                    match report.kind {
                        MaintenanceKind::Integrity => {
                            UiStatus::Success("Database integrity check passed".to_string())
                        }
                        MaintenanceKind::RebuildIndexes => UiStatus::Success(format!(
                            "Rebuilt {} database indexes",
                            report.indexes_rebuilt
                        )),
                        MaintenanceKind::Compress => UiStatus::Success(format!(
                            "Compressed database from {} to {} bytes",
                            report.size_before, report.size_after
                        )),
                    }
                };
                self.maintenance_result = Some(MaintenanceResult {
                    report,
                    backup_path,
                });
                self.maintenance_error = None;
                status
            }
            JobEvent::ResearchLogExported { path, bytes, tasks } => UiStatus::Success(format!(
                "Exported {tasks} tasks ({bytes} bytes) to {}",
                path.display()
            )),
            JobEvent::GroupRosterExported {
                path,
                bytes,
                members,
            } => UiStatus::Success(format!(
                "Exported {members} group members ({bytes} bytes) to {}",
                path.display()
            )),
            JobEvent::Geocoded(report) => {
                let found = report.found_count();
                let error = report.error.clone();
                let cancelled = report.cancelled;
                self.geocode_progress = None;
                self.geocode_cancel = None;
                self.geocode_report = Some(*report);
                if let Some(error) = error {
                    UiStatus::Error(error)
                } else if cancelled {
                    UiStatus::Cancelled
                } else {
                    UiStatus::Success(format!("Geocoded {found} places"))
                }
            }
            JobEvent::GeocodeProgress { .. } => {
                unreachable!("geocode progress is handled before the status update")
            }
            JobEvent::Cancelled => {
                if was_geocoding {
                    self.geocode_progress = None;
                    self.geocode_cancel = None;
                    self.geocode_report = Some(GeocodeReport {
                        results: Vec::new(),
                        cancelled: true,
                        error: None,
                    });
                }
                UiStatus::Cancelled
            }
            JobEvent::Failed(message) => {
                if was_maintenance {
                    self.maintenance_error = Some(message.clone());
                }
                if was_geocoding {
                    self.geocode_progress = None;
                    self.geocode_cancel = None;
                    self.geocode_report = Some(GeocodeReport {
                        results: Vec::new(),
                        cancelled: false,
                        error: Some(message.clone()),
                    });
                }
                UiStatus::Error(message)
            }
        };
    }

    /// Returns the path of a tree restored since the last UI pass.
    pub fn take_restored_path(&mut self) -> Option<PathBuf> {
        self.restored_path.take()
    }

    /// Returns the latest import's report exactly once so the UI can open
    /// the report window when it carries warnings.
    pub fn take_new_import_report(&mut self) -> Option<ImportReport> {
        self.new_import_report.take()
    }
}

/// Status-bar line for a finished import, warning count included so the
/// user always sees whether anything was skipped.
fn import_summary(report: &ImportReport) -> String {
    format!(
        "Imported {} people, {} families, {} events, {} citations, {} witnesses, {} warnings",
        report.people,
        report.families,
        report.events,
        report.citations,
        report.participants,
        report.warnings.len()
    )
}

impl Default for StemmaApp {
    fn default() -> Self {
        Self::new()
    }
}

impl Job {
    fn run(self, db_path: Option<PathBuf>, tx: &Sender<JobEvent>) {
        let event = self.run_with(db_path, tx);
        let _ = tx.send(event);
    }

    fn run_with(self, db_path: Option<PathBuf>, tx: &Sender<JobEvent>) -> JobEvent {
        match self {
            Job::Import => {
                let Some(db_path) = db_path else {
                    return JobEvent::Failed("No tree is open".to_string());
                };
                match dialogs::pick_gedcom_import_path() {
                    None => JobEvent::Cancelled,
                    Some(source) => match perform_import(&db_path, &source) {
                        Ok(report) => JobEvent::Imported(report),
                        Err(message) => JobEvent::Failed(message),
                    },
                }
            }
            Job::ImportNew => match dialogs::pick_gedcom_import_path() {
                None => JobEvent::Cancelled,
                Some(source) => match dialogs::prompt_new_tree_path() {
                    None => JobEvent::Cancelled,
                    Some(target) => match perform_import(&target, &source) {
                        Ok(report) => JobEvent::TreeReady {
                            report,
                            path: target,
                        },
                        Err(message) => JobEvent::Failed(message),
                    },
                },
            },
            Job::Export(options) => {
                let Some(db_path) = db_path else {
                    return JobEvent::Failed("No tree is open".to_string());
                };
                match dialogs::prompt_gedcom_export_path() {
                    None => JobEvent::Cancelled,
                    Some(dest) => match perform_export(&db_path, &dest, &options) {
                        Ok(bytes) => JobEvent::Exported { bytes, path: dest },
                        Err(message) => JobEvent::Failed(message),
                    },
                }
            }
            Job::Backup => {
                let Some(db_path) = db_path else {
                    return JobEvent::Failed("No tree is open".to_string());
                };
                let timestamp = match backup::timestamp_suffix() {
                    Ok(timestamp) => timestamp,
                    Err(message) => return JobEvent::Failed(message),
                };
                let default_name = backup::backup_file_name(&db_path, &timestamp);
                let directory = db_path.parent();
                match dialogs::prompt_backup_path(&default_name, directory) {
                    None => JobEvent::Cancelled,
                    Some(dest) => match backup::backup_tree(&db_path, &dest) {
                        Ok(report) => JobEvent::BackedUp { path: dest, report },
                        Err(message) => JobEvent::Failed(message),
                    },
                }
            }
            Job::Restore => {
                let Some(archive) = dialogs::pick_backup_restore_path() else {
                    return JobEvent::Cancelled;
                };
                let timestamp = match backup::timestamp_suffix() {
                    Ok(timestamp) => timestamp,
                    Err(message) => return JobEvent::Failed(message),
                };
                let default_name = backup::restore_file_name(&archive, &timestamp);
                let directory = archive.parent();
                match dialogs::prompt_restore_target(&default_name, directory) {
                    None => JobEvent::Cancelled,
                    Some(dest) => match backup::restore_backup(&archive, &dest) {
                        Ok(report) => JobEvent::Restored { path: dest, report },
                        Err(message) => JobEvent::Failed(message),
                    },
                }
            }
            Job::ResearchLog(report) => {
                let timestamp = match backup::timestamp_suffix() {
                    Ok(timestamp) => timestamp,
                    Err(message) => return JobEvent::Failed(message),
                };
                let default_name = research_log::file_name(&report.tree_name, &timestamp);
                let directory = db_path.as_deref().and_then(Path::parent);
                match dialogs::prompt_report_path(&default_name, directory) {
                    None => JobEvent::Cancelled,
                    Some(dest) => match research_log::write_html(&dest, &report) {
                        Ok(bytes) => {
                            let _ = open::that(&dest);
                            JobEvent::ResearchLogExported {
                                path: dest,
                                bytes,
                                tasks: report.tasks.len(),
                            }
                        }
                        Err(message) => JobEvent::Failed(message),
                    },
                }
            }
            Job::GroupRoster(report) => {
                let timestamp = match backup::timestamp_suffix() {
                    Ok(timestamp) => timestamp,
                    Err(message) => return JobEvent::Failed(message),
                };
                let default_name = group_roster::file_name(&report.group_name, &timestamp);
                let directory = db_path.as_deref().and_then(Path::parent);
                match dialogs::prompt_report_path(&default_name, directory) {
                    None => JobEvent::Cancelled,
                    Some(dest) => match group_roster::write_html(&dest, &report) {
                        Ok(bytes) => {
                            let _ = open::that(&dest);
                            JobEvent::GroupRosterExported {
                                path: dest,
                                bytes,
                                members: report.members.len(),
                            }
                        }
                        Err(message) => JobEvent::Failed(message),
                    },
                }
            }
            Job::Geocode { requests, cancel } => {
                let Some(db_path) = db_path else {
                    return JobEvent::Failed("No tree is open".to_string());
                };
                let conn = match db::open_connection(&db_path) {
                    Ok(conn) => conn,
                    Err(error) => return JobEvent::Failed(error.to_string()),
                };
                let agent = geocode::agent();
                let mut last_request: Option<Instant> = None;
                let report = geocode::run_batch(
                    &conn,
                    &requests,
                    &cancel,
                    |query| {
                        if let Some(last) = last_request {
                            let elapsed = last.elapsed();
                            if elapsed < Duration::from_secs(1) {
                                thread::sleep(Duration::from_secs(1) - elapsed);
                            }
                        }
                        let result = geocode::lookup(&agent, query);
                        last_request = Some(Instant::now());
                        result
                    },
                    |done, total| {
                        let _ = tx.send(JobEvent::GeocodeProgress { done, total });
                    },
                );
                JobEvent::Geocoded(Box::new(report))
            }
            Job::Maintenance { kind, backup_first } => {
                let Some(db_path) = db_path else {
                    return JobEvent::Failed("No tree is open".to_string());
                };
                let mut backup_path = None;
                if backup_first {
                    let timestamp = match backup::timestamp_suffix() {
                        Ok(timestamp) => timestamp,
                        Err(message) => return JobEvent::Failed(message),
                    };
                    let default_name = backup::backup_file_name(&db_path, &timestamp);
                    let Some(dest) = dialogs::prompt_backup_path(&default_name, db_path.parent())
                    else {
                        return JobEvent::Cancelled;
                    };
                    if let Err(message) = backup::backup_tree(&db_path, &dest) {
                        return JobEvent::Failed(message);
                    }
                    backup_path = Some(dest);
                }
                match maintenance_store::run(&db_path, kind) {
                    Ok(report) => JobEvent::Maintenance {
                        report,
                        backup_path,
                    },
                    Err(message) => JobEvent::Failed(message),
                }
            }
        }
    }
}

/// Creates a fresh database file at `path` with the schema initialised.
///
/// This is the dialog-free half of the "Create New Family Tree" action.
pub fn create_tree(path: &Path) -> Result<(), String> {
    let conn = db::open_connection(path).map_err(|error| error.to_string())?;
    db::init_db(&conn).map_err(|error| error.to_string())
}

/// Opens `source` and streams it into the database at `db_path`.
pub fn perform_import(db_path: &Path, source: &Path) -> Result<ImportReport, String> {
    let mut conn = db::open_connection(db_path).map_err(|error| error.to_string())?;
    db::init_db(&conn).map_err(|error| error.to_string())?;
    let file = std::fs::File::open(source).map_err(|error| error.to_string())?;
    crate::gedcom::import_gedcom_with_bases(
        &mut conn,
        std::io::BufReader::new(file),
        source.parent(),
        db_path.parent(),
    )
    .map_err(|error| error.to_string())
}

/// Serializes the database at `db_path` into a UTF-8 GEDCOM file at `dest`.
///
/// Returns the number of bytes written.
pub fn perform_export(
    db_path: &Path,
    dest: &Path,
    options: &ExportOptions,
) -> Result<usize, String> {
    let conn = db::open_connection(db_path).map_err(|error| error.to_string())?;
    db::init_db(&conn).map_err(|error| error.to_string())?;
    let gedcom = export_to_gedcom_with_bases(&conn, options, db_path.parent())
        .map_err(|error| error.to_string())?;
    std::fs::write(dest, gedcom.as_bytes()).map_err(|error| error.to_string())?;
    Ok(gedcom.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
0 HEAD
1 SOUR TEST
0 @I1@ INDI
1 NAME Johan /Ahlberg/
1 SEX M
1 FAMS @F1@
0 @I2@ INDI
1 NAME Maria /Ekdahl/
1 SEX F
1 FAMS @F1@
0 @I3@ INDI
1 NAME Elsa /Ahlberg/
1 SEX F
1 FAMC @F1@
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I2@
1 CHIL @I3@
0 TRLR
";

    fn temp_path(extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "stemma-flow-{}.{}",
            uuid::Uuid::new_v4(),
            extension
        ))
    }

    #[test]
    fn exported_file_imports_into_identical_connections() -> Result<(), String> {
        let source = temp_path("ged");
        let seed_db = temp_path("db");
        let export_db = temp_path("db");
        let export_file = temp_path("ged");

        std::fs::write(&source, SAMPLE).map_err(|error| error.to_string())?;
        let report = perform_import(&seed_db, &source)?;
        assert_eq!((report.people, report.families), (3, 1));

        perform_export(
            &seed_db,
            &export_file,
            &ExportOptions {
                include_living: true,
                include_private: true,
                write_markers: false,
                ..ExportOptions::default()
            },
        )?;
        let reimported = perform_import(&export_db, &export_file)?;
        assert_eq!(
            (
                reimported.people,
                reimported.families,
                reimported.child_links
            ),
            (3, 1, 1),
            "connections must be restored from the exported file"
        );

        let original = connections_of(&seed_db)?;
        let restored = connections_of(&export_db)?;
        assert_eq!(
            original, restored,
            "parent/child and spouse links must match exactly"
        );

        let second_export = temp_path("ged");
        perform_export(
            &export_db,
            &second_export,
            &ExportOptions {
                include_living: true,
                include_private: true,
                write_markers: false,
                ..ExportOptions::default()
            },
        )?;
        let first = std::fs::read(&export_file).map_err(|error| error.to_string())?;
        let again = std::fs::read(&second_export).map_err(|error| error.to_string())?;
        assert_eq!(first, again, "re-export must be byte identical");

        for path in [&source, &seed_db, &export_db, &export_file, &second_export] {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    #[test]
    fn failed_import_reports_error_without_panic() {
        let missing = temp_path("db");
        let result = perform_import(&missing, Path::new("/nonexistent/file.ged"));
        assert!(result.is_err(), "missing input file must fail cleanly");
        let _ = std::fs::remove_file(missing);
    }

    #[test]
    fn poll_applies_events_and_tracks_import_generation() {
        let mut app = StemmaApp::new();
        assert!(!app.poll(), "without a channel there is nothing to apply");
        assert_eq!(app.import_generation(), 0);
        assert_eq!(app.selected_person_id(), None);
        assert_eq!(app.active_db(), None, "starts without a tree");

        app.activate_tree("stemma-flow.db");
        assert_eq!(app.active_db(), Some(Path::new("stemma-flow.db")));
        assert_eq!(
            app.import_generation(),
            0,
            "opening a tree does not rewrite data"
        );

        let (tx, rx) = channel();
        app.rx = Some(rx);
        let _ = tx.send(JobEvent::Imported(ImportReport {
            people: 2,
            families: 1,
            child_links: 1,
            events: 0,
            citations: 0,
            participants: 0,
            warnings: Vec::new(),
        }));
        assert!(app.poll(), "queued events must be applied");
        assert_eq!(app.import_generation(), 1, "imports bump the generation");
        assert!(!app.poll(), "a drained channel applies nothing");

        app.set_selected_person(Some("i1".to_string()));
        assert_eq!(app.selected_person_id(), Some("i1"));
        app.close_tree();
        assert_eq!(app.active_db(), None, "closing returns to the dashboard");
        assert_eq!(app.selected_person_id(), None, "closing clears selection");
    }

    #[test]
    fn tree_ready_event_activates_the_new_database() {
        let mut app = StemmaApp::new();
        app.busy = true;
        app.apply_event(JobEvent::TreeReady {
            report: ImportReport {
                people: 4,
                families: 1,
                child_links: 2,
                events: 0,
                citations: 0,
                participants: 0,
                warnings: Vec::new(),
            },
            path: PathBuf::from("/tmp/fresh.db"),
        });
        assert_eq!(app.active_db(), Some(Path::new("/tmp/fresh.db")));
        assert_eq!(
            app.import_generation(),
            1,
            "the fresh import marks the scene stale"
        );
        assert!(matches!(app.status(), UiStatus::Success(_)));
    }

    #[test]
    fn begin_without_an_active_tree_is_a_noop() {
        let mut app = StemmaApp::new();
        app.begin_import();
        app.begin_export();
        assert!(!app.is_busy(), "jobs need an open tree");
        assert_eq!(app.status(), &UiStatus::Idle);
    }

    #[test]
    fn create_tree_initialises_an_empty_schema() -> Result<(), String> {
        let path = temp_path("db");
        create_tree(&path)?;
        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        let tree = db::load_family_tree(&conn).map_err(|error| error.to_string())?;
        assert!(tree.people.is_empty(), "a fresh tree has no people");
        assert!(tree.families.is_empty(), "a fresh tree has no families");
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn find_everywhere_opens_the_active_tree() -> Result<(), String> {
        let path = temp_path("db");
        create_tree(&path)?;
        let mut app = StemmaApp::new();
        app.activate_tree(&path);
        app.add_person(&PersonDetails::new("Ada", "", "King", "", "F", None, None))?;

        let results = app.find_everywhere("Ada", false)?;
        let people = results
            .groups
            .iter()
            .find(|group| group.kind == crate::db::find::FindKind::Person)
            .expect("person hit");
        assert!(people.hits[0].title.contains("Ada"));
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn apply_event_drives_status_transitions() {
        let mut app = StemmaApp::new();
        assert_eq!(app.status(), &UiStatus::Idle);
        assert!(!app.is_busy());

        app.busy = true;
        app.apply_event(JobEvent::Imported(ImportReport {
            people: 3,
            families: 1,
            child_links: 1,
            events: 0,
            citations: 0,
            participants: 0,
            warnings: Vec::new(),
        }));
        assert!(!app.is_busy());
        assert!(matches!(app.status(), UiStatus::Success(message) if message.contains("3 people")));

        app.busy = true;
        app.apply_event(JobEvent::Cancelled);
        assert!(!app.is_busy());
        assert_eq!(app.status(), &UiStatus::Cancelled);

        app.busy = true;
        app.apply_event(JobEvent::Failed("disk full".to_string()));
        assert_eq!(app.status(), &UiStatus::Error("disk full".to_string()));
    }

    #[test]
    fn geocode_progress_keeps_the_job_running_until_the_report_arrives() {
        let mut app = StemmaApp::new();
        app.busy = true;
        app.geocode_cancel = Some(Arc::new(AtomicBool::new(false)));
        app.geocode_progress = Some((0, 2));

        app.apply_event(JobEvent::GeocodeProgress { done: 1, total: 2 });
        assert!(app.is_busy(), "progress events do not finish the job");
        assert_eq!(app.geocode_progress(), Some((1, 2)));

        app.apply_event(JobEvent::Geocoded(Box::default()));
        assert!(!app.is_busy());
        assert_eq!(app.geocode_progress(), None);
        assert!(app.take_geocode_report().is_some());
        assert!(
            app.take_geocode_report().is_none(),
            "the report is consumed once"
        );
    }

    #[test]
    fn relinking_media_preserves_links_and_albums() -> Result<(), String> {
        let db_path = temp_path("db");
        create_tree(&db_path)?;
        let mut app = StemmaApp::new();
        app.activate_tree(&db_path);

        let place_id = app.save_place(&PlaceDraft {
            name: "Uppsala".to_string(),
            ..PlaceDraft::default()
        })?;
        let media_id = app.save_media(&MediaDraft {
            file_path: "missing/photo.jpg".to_string(),
            caption: "Photo".to_string(),
            ..MediaDraft::default()
        })?;
        app.attach_media(&media_id, &MediaOwner::Place(place_id))?;
        let album_id = app.save_album(&AlbumDraft {
            name: "Trip".to_string(),
            ..AlbumDraft::default()
        })?;
        app.add_media_to_album(&album_id, &media_id)?;

        let media_dir = db_path.with_file_name(format!("media-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&media_dir).map_err(|error| error.to_string())?;
        let file = media_dir.join("photo.jpg");
        std::fs::write(&file, b"photo").map_err(|error| error.to_string())?;
        app.relink_media(&media_id, &file.to_string_lossy())?;

        let record = app.load_media(&media_id)?.expect("media exists");
        assert_eq!(record.caption.as_deref(), Some("Photo"));
        assert_eq!(app.list_media_links(&media_id, false)?.len(), 1);
        assert_eq!(app.albums_for_media(&media_id, false)?.len(), 1);
        let media = app.list_media()?;
        assert_eq!(media.len(), 1);
        assert!(!media[0].missing);
        assert!(media[0].resolved_path.is_file());

        let _ = std::fs::remove_dir_all(media_dir);
        let _ = std::fs::remove_file(&db_path);
        Ok(())
    }

    #[test]
    fn data_clean_preview_and_apply_update_names() -> Result<(), String> {
        let path = temp_path("db");
        create_tree(&path)?;
        let mut app = StemmaApp::new();
        app.activate_tree(&path);
        app.add_person(&PersonDetails::new("ADA", "", "KING", "", "F", None, None))?;

        let plan = app.data_clean_plan()?;
        assert_eq!(plan.names.len(), 1);
        let selection = DataCleanSelection::select_all(&plan);
        let report = app.apply_data_clean(&plan, &selection)?;
        assert_eq!(report.names_changed, 1);
        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        let tree = db::load_family_tree(&conn).map_err(|error| error.to_string())?;
        assert_eq!(tree.people[0].given_name, "Ada");
        assert_eq!(tree.people[0].surname, "King");
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn search_replace_preview_and_apply_update_names() -> Result<(), String> {
        let path = temp_path("db");
        create_tree(&path)?;
        let mut app = StemmaApp::new();
        app.activate_tree(&path);
        app.add_person(&PersonDetails::new("Ann", "", "King", "", "F", None, None))?;

        let request = ReplaceRequest {
            find: "Ann".to_string(),
            replace: "Jane".to_string(),
            match_case: true,
            names: true,
            places: false,
            notes: false,
            filenames: false,
        };
        let plan = app.preview_search_replace(&request, false)?;
        assert_eq!(plan.changes.len(), 1);
        let report = app.apply_search_replace(&plan, &ReplaceSelection::select_all(&plan))?;
        assert_eq!(report.names, 1);

        let conn = db::open_connection(&path).map_err(|error| error.to_string())?;
        let tree = db::load_family_tree(&conn).map_err(|error| error.to_string())?;
        assert_eq!(tree.people[0].given_name, "Jane");
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn maintenance_events_store_results_and_clear_the_block() {
        let mut app = StemmaApp::new();
        app.maintenance = true;
        app.busy = true;
        app.apply_event(JobEvent::Maintenance {
            report: MaintenanceReport {
                kind: MaintenanceKind::Compress,
                healthy: true,
                issues: Vec::new(),
                foreign_key_violations: 0,
                indexes_rebuilt: 0,
                page_count: 10,
                freelist_count: 0,
                size_before: 4096,
                size_after: 2048,
                duration_ms: 3,
            },
            backup_path: Some(PathBuf::from("/tmp/tree-backup.zip")),
        });
        assert!(!app.is_busy());
        assert!(!app.is_maintenance());
        assert!(
            matches!(app.status(), UiStatus::Success(message) if message.contains("Compressed"))
        );
        let result = app.take_maintenance_result().expect("result");
        assert_eq!(result.report.kind, MaintenanceKind::Compress);
        assert_eq!(
            result.backup_path,
            Some(PathBuf::from("/tmp/tree-backup.zip"))
        );
        assert_eq!(app.take_maintenance_result(), None);

        app.maintenance = true;
        app.busy = true;
        app.apply_event(JobEvent::Maintenance {
            report: MaintenanceReport {
                kind: MaintenanceKind::Integrity,
                healthy: false,
                issues: vec!["bad page".to_string()],
                foreign_key_violations: 1,
                indexes_rebuilt: 0,
                page_count: 10,
                freelist_count: 0,
                size_before: 4096,
                size_after: 4096,
                duration_ms: 2,
            },
            backup_path: None,
        });
        assert!(matches!(app.status(), UiStatus::Error(message) if message.contains("Integrity")));

        app.maintenance = true;
        app.busy = true;
        app.apply_event(JobEvent::Failed("database is locked".to_string()));
        assert_eq!(
            app.take_maintenance_error().as_deref(),
            Some("database is locked")
        );

        app.begin_maintenance(MaintenanceKind::Integrity, false);
        assert!(!app.is_maintenance(), "maintenance needs an open tree");
    }

    #[test]
    fn research_log_export_event_reports_success() {
        let mut app = StemmaApp::new();
        app.busy = true;
        app.apply_event(JobEvent::ResearchLogExported {
            path: PathBuf::from("/tmp/research-log.html"),
            bytes: 2048,
            tasks: 4,
        });
        assert!(!app.is_busy());
        assert!(
            matches!(app.status(), UiStatus::Success(message) if message.contains("4 tasks") && message.contains("research-log.html"))
        );
    }

    #[test]
    fn group_roster_export_event_reports_success() {
        let mut app = StemmaApp::new();
        app.busy = true;
        app.apply_event(JobEvent::GroupRosterExported {
            path: PathBuf::from("/tmp/group-roster.html"),
            bytes: 2048,
            members: 3,
        });
        assert!(!app.is_busy());
        assert!(
            matches!(app.status(), UiStatus::Success(message) if message.contains("3 group members") && message.contains("group-roster.html"))
        );
    }

    #[test]
    fn backup_and_restore_events_update_status_and_active_tree() {
        let mut app = StemmaApp::new();
        app.busy = true;
        app.apply_event(JobEvent::BackedUp {
            path: PathBuf::from("/tmp/tree-backup.zip"),
            report: BackupReport {
                media_included: 2,
                media_missing: vec!["/tmp/missing.jpg".to_string()],
                database_bytes: 1024,
            },
        });
        assert!(
            matches!(app.status(), UiStatus::Success(message) if message.contains("2 media") && message.contains("1 media file missing"))
        );

        app.busy = true;
        app.apply_event(JobEvent::Restored {
            path: PathBuf::from("/tmp/restored.db"),
            report: RestoreReport {
                media_restored: 2,
                media_missing: 0,
            },
        });
        assert!(!app.is_busy());
        assert_eq!(app.active_db(), Some(Path::new("/tmp/restored.db")));
        assert!(
            matches!(app.status(), UiStatus::Success(message) if message.contains("Restored tree"))
        );
        assert_eq!(
            app.take_restored_path(),
            Some(PathBuf::from("/tmp/restored.db"))
        );
        assert_eq!(
            app.take_restored_path(),
            None,
            "recent path is consumed once"
        );
    }

    #[test]
    fn import_events_count_warnings_and_expose_the_report_once() {
        let mut app = StemmaApp::new();
        app.busy = true;
        app.apply_event(JobEvent::Imported(ImportReport {
            people: 5,
            families: 2,
            child_links: 0,
            events: 3,
            citations: 1,
            participants: 0,
            warnings: vec!["Line 6: ignored".to_string(), "Line 9: ignored".to_string()],
        }));

        match app.status() {
            UiStatus::Success(message) => {
                assert!(message.contains("5 people"), "{message}");
                assert!(message.contains("2 warnings"), "{message}");
            }
            other => panic!("expected success, got {other:?}"),
        }

        let report = app
            .take_new_import_report()
            .expect("the report waits for the UI");
        assert_eq!(report.warnings.len(), 2);
        assert!(
            app.take_new_import_report().is_none(),
            "the take is one-shot"
        );
    }

    /// Renders the family links as names so two databases with freshly
    /// generated UUIDs can be compared structurally.
    fn connections_of(db_path: &Path) -> Result<String, String> {
        let conn = db::open_connection(db_path).map_err(|error| error.to_string())?;
        let spouses: String = conn
            .query_row(
                "SELECT GROUP_CONCAT(COALESCE(h.given_name, '-') || '<>' || COALESCE(w.given_name, '-'))
                 FROM families f
                 LEFT JOIN people h ON h.id = f.husband_id
                 LEFT JOIN people w ON w.id = f.wife_id",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let children: String = conn
            .query_row(
                "SELECT GROUP_CONCAT(p.given_name)
                 FROM family_children fc
                 JOIN people p ON p.id = fc.child_id",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(format!("{spouses}|{children}"))
    }
}
