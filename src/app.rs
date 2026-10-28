//! Application state and background job orchestration for import/export.
//!
//! Native file dialogs and file I/O run on a spawned worker thread so the
//! egui render loop stays responsive; results travel back through an
//! `mpsc` channel that [`StemmaApp::poll`] drains every frame.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use crate::db;
use crate::db::citations::{
    self as citation_store, CitationEdit, CitationRecord, CitationTarget, SourceCitation,
};
use crate::db::facts::{
    self as fact_store, FactRoleDraft, FactTypeDraft, FactTypeRecord, FactTypeSummary,
};
use crate::db::names::{self as name_store, AlternateName, AlternateNameDraft};
use crate::db::notes::{self as note_store, Note, NoteOwner};
use crate::db::participants::{self as participant_store, EventContext, ParticipantDraft};
use crate::db::people::{self, ParentRole, PersonDetails};
use crate::db::places::{self as place_store, PlaceDraft, PlaceRecord, PlaceSummary, PlaceUsage};
use crate::db::privacy::{self as privacy_store, DEFAULT_LIVING_AGE, PersonPrivacy};
use crate::db::repositories::{
    self as repository_store, RepositoryDraft, RepositoryRecord, RepositorySummary,
};
use crate::db::settings::{self as settings_store, TreeSettings};
use crate::db::sources::{
    self as source_store, SourceDraft, SourceMergeReport, SourceRecord, SourceSummary,
};
use crate::gedcom::{ExportOptions, ImportReport, export_to_gedcom_with_options, import_gedcom};
use crate::gui::dialogs;
use crate::model::source_template::SourceTemplate;

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
    Cancelled,
    Failed(String),
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
        self.busy = false;
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
            JobEvent::Cancelled => UiStatus::Cancelled,
            JobEvent::Failed(message) => UiStatus::Error(message),
        };
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
        let event = self.run_with(db_path);
        let _ = tx.send(event);
    }

    fn run_with(self, db_path: Option<PathBuf>) -> JobEvent {
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
    import_gedcom(&mut conn, std::io::BufReader::new(file)).map_err(|error| error.to_string())
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
    let gedcom =
        export_to_gedcom_with_options(&conn, options).map_err(|error| error.to_string())?;
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
