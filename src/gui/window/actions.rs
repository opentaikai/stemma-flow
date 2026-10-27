//! Tree lifecycle actions, watch list and inspector wiring.

use crate::app;
use crate::app::UiStatus;
use crate::db::people::{ParentRole, PersonDetails};
use crate::db::sources::SourceDraft;
use crate::gui::citations::CitationDraft;
use crate::gui::dialogs;
use crate::gui::inspector;
use crate::gui::inspector::InspectorAction;
use crate::gui::inspector::InspectorForm;
use crate::gui::inspector::RelationForm;
use crate::gui::inspector::RelationKind;
use crate::gui::inspector::RelationOutcome;
use crate::gui::places::{self, PlacesEvent};
use crate::gui::repositories::{self, RepositoriesEvent};
use crate::gui::sources::{self, SourcesEvent};
use crate::gui::tabs;
use crate::gui::watchlist;
use crate::gui::watchlist::WatchEvent;
use crate::gui::welcome;
use eframe::Frame;
use egui::Ui;
use std::path::PathBuf;

use super::RECENT_TREES_KEY;
use super::loading::{load_error, pan_centering};

use super::TreeWindow;

impl TreeWindow {
    /// Inline open flow: pick an existing database file and switch to it.
    pub(super) fn open_tree(&mut self, frame: &mut Frame) {
        let Some(path) = dialogs::pick_tree_open_path() else {
            self.app.set_status(UiStatus::Cancelled);
            return;
        };
        self.open_tree_at(path, frame);
    }

    /// Inline create flow: pick a destination, initialise the schema and
    /// switch to the canvas within the same frame.
    pub(super) fn create_new_tree(&mut self, frame: &mut Frame) {
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
    pub(super) fn open_tree_at(&mut self, path: PathBuf, frame: &mut Frame) {
        if let Some(reason) = load_error(&path) {
            self.app.set_status(UiStatus::Error(format!(
                "Cannot open {}: {reason}",
                path.display()
            )));
            self.recent.retain(|entry| *entry != path);
            self.save_recent(frame);
            return;
        }
        self.app.activate_tree(path.clone());
        self.note_recent(path, frame);
    }

    pub(super) fn note_recent(&mut self, path: PathBuf, frame: &mut Frame) {
        welcome::push_recent(&mut self.recent, path, welcome::RECENT_CAP);
        self.save_recent(frame);
    }

    pub(super) fn save_recent(&mut self, frame: &mut Frame) {
        if let Some(storage) = frame.storage_mut() {
            eframe::set_value(storage, RECENT_TREES_KEY, &self.recent);
        }
    }

    /// Renders the Watch List tab and applies the grid's events.
    pub(super) fn watch_list(&mut self, ui: &mut Ui) {
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

    pub(super) fn sources_tab(&mut self, ui: &mut Ui) {
        if !self.sources.loaded {
            self.reload_sources();
        }
        if let Some(event) = sources::show(ui, &mut self.sources) {
            self.apply_sources_event(event);
        }
    }

    pub(super) fn apply_sources_event(&mut self, event: SourcesEvent) {
        match event {
            SourcesEvent::New => self.new_source(),
            SourcesEvent::Select(id) => self.load_source_draft(&id),
            SourcesEvent::Save(draft) => match self.app.save_source(&draft) {
                Ok(id) => {
                    self.sources.selected = Some(id);
                    self.sources.error = None;
                    self.citations.loaded = false;
                    self.repositories.loaded = false;
                    self.reload_sources();
                }
                Err(message) => self.sources.error = Some(message),
            },
            SourcesEvent::Delete(id) => match self.app.delete_source(&id) {
                Ok(()) => {
                    self.sources.selected = None;
                    self.sources.draft = SourceDraft::default();
                    self.sources.merge_open = false;
                    self.sources.merge_target = None;
                    self.citations.loaded = false;
                    self.repositories.loaded = false;
                    self.reload_sources();
                }
                Err(message) => self.sources.error = Some(message),
            },
            SourcesEvent::Merge {
                source_id,
                target_id,
            } => match self.app.merge_sources(&source_id, &target_id) {
                Ok(_) => {
                    self.sources.selected = Some(target_id);
                    self.sources.merge_open = false;
                    self.sources.merge_target = None;
                    self.citations.loaded = false;
                    self.repositories.loaded = false;
                    self.reload_sources();
                }
                Err(message) => self.sources.error = Some(message),
            },
        }
    }

    pub(super) fn places_tab(&mut self, ui: &mut Ui) {
        if !self.places.loaded {
            self.reload_places();
        }
        if let Some(event) = places::show(ui, &mut self.places, self.config.reverse_place_names) {
            self.apply_places_event(event);
        }
    }

    pub(super) fn apply_places_event(&mut self, event: PlacesEvent) {
        match event {
            PlacesEvent::New => {
                self.places.selected = None;
                self.places.draft = places::PlaceEdit::default();
                self.places.usage.clear();
                self.places.editing = true;
                self.places.error = None;
            }
            PlacesEvent::Select(id) => self.load_place_draft(&id),
            PlacesEvent::Save(draft) => match self.app.save_place(&draft) {
                Ok(id) => {
                    self.places.selected = Some(id);
                    self.places.editing = true;
                    self.places.error = None;
                    self.reload_places();
                    self.reload_scene_quiet();
                    self.inspected = None;
                }
                Err(message) => self.places.error = Some(message),
            },
            PlacesEvent::Delete(id) => match self.app.delete_place(&id) {
                Ok(()) => {
                    self.places.selected = None;
                    self.places.draft = places::PlaceEdit::default();
                    self.places.usage.clear();
                    self.places.editing = false;
                    self.places.error = None;
                    self.reload_places();
                    self.reload_scene_quiet();
                    self.inspected = None;
                }
                Err(message) => self.places.error = Some(message),
            },
            PlacesEvent::SelectPerson(id) => {
                self.app.set_selected_person(Some(id));
            }
        }
    }

    pub(super) fn reload_places(&mut self) {
        match self.app.list_places() {
            Ok(list) => {
                self.places.places = list;
                self.places.loaded = true;
                self.places.error = None;
                if let Some(id) = self.places.selected.clone() {
                    self.load_place_draft(&id);
                }
            }
            Err(message) => {
                self.places.error = Some(message);
                self.places.loaded = true;
            }
        }
    }

    pub(super) fn load_place_draft(&mut self, id: &str) {
        match (self.app.load_place(id), self.app.place_usage(id)) {
            (Ok(Some(record)), Ok(usage)) => {
                self.places.selected = Some(id.to_string());
                self.places.draft = places::draft_from_record(&record);
                self.places.usage = usage;
                self.places.editing = true;
                self.places.error = None;
            }
            (Ok(None), _) => {
                self.places.selected = None;
                self.places.draft = places::PlaceEdit::default();
                self.places.usage.clear();
                self.places.editing = false;
            }
            (Err(message), _) | (_, Err(message)) => self.places.error = Some(message),
        }
    }

    pub(super) fn repositories_tab(&mut self, ui: &mut Ui) {
        if !self.repositories.loaded {
            self.reload_repositories();
        }
        if let Some(event) = repositories::show(ui, &mut self.repositories) {
            self.apply_repositories_event(event);
        }
    }

    pub(super) fn apply_repositories_event(&mut self, event: RepositoriesEvent) {
        match event {
            RepositoriesEvent::New => {
                self.repositories.selected = None;
                self.repositories.draft = crate::db::repositories::RepositoryDraft::default();
                self.repositories.add_source = None;
                self.repositories.add_call_number.clear();
                self.repositories.add_media_type.clear();
                self.repositories.editing = true;
                self.repositories.error = None;
            }
            RepositoriesEvent::Select(id) => self.load_repository_draft(&id),
            RepositoriesEvent::Save(draft) => match self.app.save_repository(&draft) {
                Ok(id) => {
                    self.repositories.selected = Some(id);
                    self.repositories.add_source = None;
                    self.repositories.add_call_number.clear();
                    self.repositories.add_media_type.clear();
                    self.repositories.editing = true;
                    self.repositories.error = None;
                    self.reload_repositories();
                }
                Err(message) => self.repositories.error = Some(message),
            },
            RepositoriesEvent::Delete(id) => match self.app.delete_repository(&id) {
                Ok(()) => {
                    self.repositories.selected = None;
                    self.repositories.draft = crate::db::repositories::RepositoryDraft::default();
                    self.repositories.editing = false;
                    self.repositories.error = None;
                    self.reload_repositories();
                }
                Err(message) => self.repositories.error = Some(message),
            },
        }
    }

    pub(super) fn reload_repositories(&mut self) {
        match (self.app.list_repositories(), self.app.list_sources()) {
            (Ok(list), Ok(sources)) => {
                self.repositories.repositories = list;
                self.repositories.sources = sources;
                self.repositories.loaded = true;
                self.repositories.error = None;
                if let Some(id) = self.repositories.selected.clone() {
                    self.load_repository_draft(&id);
                }
            }
            (Err(message), _) | (_, Err(message)) => {
                self.repositories.error = Some(message);
                self.repositories.loaded = true;
            }
        }
    }

    pub(super) fn load_repository_draft(&mut self, id: &str) {
        match self.app.load_repository(id) {
            Ok(Some(record)) => {
                self.repositories.selected = Some(id.to_string());
                self.repositories.draft = repositories::draft_from_record(&record);
                self.repositories.add_source = None;
                self.repositories.add_call_number.clear();
                self.repositories.add_media_type.clear();
                self.repositories.editing = true;
                self.repositories.error = None;
            }
            Ok(None) => {
                self.repositories.selected = None;
                self.repositories.draft = crate::db::repositories::RepositoryDraft::default();
                self.repositories.editing = false;
            }
            Err(message) => self.repositories.error = Some(message),
        }
    }

    pub(super) fn reload_sources(&mut self) {
        let templates = self.app.source_templates();
        let list = self.app.list_sources();
        match (templates, list) {
            (Ok(templates), Ok(list)) => {
                self.sources.templates = templates;
                self.sources.list = list;
                self.sources.loaded = true;
                self.sources.error = None;
                if let Some(id) = self.sources.selected.clone() {
                    self.load_source_draft(&id);
                }
            }
            (Err(message), _) | (_, Err(message)) => {
                self.sources.error = Some(message);
                self.sources.loaded = true;
            }
        }
    }

    pub(super) fn load_source_draft(&mut self, id: &str) {
        self.sources.merge_open = false;
        self.sources.merge_target = None;
        match self.app.load_source(id) {
            Ok(Some(record)) => {
                self.sources.selected = Some(id.to_string());
                self.sources.draft = SourceDraft::from_record(&record);
                self.sources.preview.clear();
            }
            Ok(None) => {
                self.sources.selected = None;
                self.sources.draft = SourceDraft::default();
            }
            Err(message) => self.sources.error = Some(message),
        }
    }

    pub(super) fn new_source(&mut self) {
        self.sources.selected = None;
        self.sources.merge_open = false;
        self.sources.merge_target = None;
        let template_key = self
            .sources
            .templates
            .first()
            .map(|template| template.key.clone());
        self.sources.draft = SourceDraft {
            template_key,
            ..SourceDraft::default()
        };
        self.sources.preview.clear();
        self.sources.error = None;
    }

    /// Instant add: inserts an unnamed person and focuses it.
    pub(super) fn add_person(&mut self) {
        match self.app.add_person(&PersonDetails::unnamed()) {
            Ok(person_id) => {
                self.app.set_selected_person(Some(person_id));
                self.reload_scene();
            }
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    pub(super) fn delete_person(&mut self, person_id: &str) {
        match self.app.delete_person(person_id) {
            Ok(()) => {
                self.places.loaded = false;
                self.reload_scene();
            }
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    /// Renders the inspector side panel for the selected person.
    pub(super) fn person_inspector(&mut self, ui: &mut Ui) {
        self.sync_inspector_form();
        self.sync_citations();
        let Some(person_id) = self.app.selected_person_id().map(str::to_string) else {
            return;
        };
        let person = self
            .tree
            .people
            .iter()
            .find(|person| person.id == person_id);
        let mut connections = inspector::connections(&self.tree, &person_id, &self.marriages);
        if self.config.reverse_place_names {
            inspector::reverse_marriage_places(&mut connections);
        }
        if let Some(action) = inspector::show(
            ui,
            person,
            &mut self.inspector,
            connections,
            &mut self.citations,
        ) {
            self.apply_inspector_action(action, &person_id);
        }
    }

    /// Applies an action emitted by the inspector for `person_id`.
    pub(super) fn apply_inspector_action(&mut self, action: InspectorAction, person_id: &str) {
        match action {
            InspectorAction::Save(details) => self.edit_person(&details),
            InspectorAction::AddParent => self.open_relation(person_id, RelationKind::Parent),
            InspectorAction::AddSpouse => self.open_relation(person_id, RelationKind::Spouse),
            InspectorAction::AddChild => self.open_relation(person_id, RelationKind::Child),
            InspectorAction::Focus(target) => self.focus_person(&target),
            InspectorAction::Close => self.app.set_selected_person(None),
            InspectorAction::SaveCitations(edits) => {
                let mut failure = None;
                for edit in edits.iter() {
                    if let Err(message) = self.app.save_citation(edit) {
                        failure = Some(message);
                        break;
                    }
                }
                if let Some(message) = failure {
                    self.citations.error = Some(message);
                } else {
                    self.citations.dirty = false;
                    self.citations.loaded = false;
                }
            }
            InspectorAction::AddCitation {
                source_id,
                target,
                page,
            } => match self
                .app
                .add_citation(&source_id, person_id, &target, page.as_deref())
            {
                Ok(_) => {
                    self.citations.add_page.clear();
                    self.citations.loaded = false;
                }
                Err(message) => self.citations.error = Some(message),
            },
            InspectorAction::AttachCitation {
                citation_id,
                target,
            } => match self.app.attach_citation(&citation_id, person_id, &target) {
                Ok(_) => self.citations.loaded = false,
                Err(message) => self.citations.error = Some(message),
            },
            InspectorAction::DetachCitation {
                citation_id,
                event_id,
            } => match self.app.detach_citation(&citation_id, &event_id) {
                Ok(()) => self.citations.loaded = false,
                Err(message) => self.citations.error = Some(message),
            },
            InspectorAction::DeleteCitation(id) => match self.app.delete_citation(&id) {
                Ok(()) => {
                    self.citations.pending_delete = None;
                    self.citations.loaded = false;
                }
                Err(message) => self.citations.error = Some(message),
            },
            InspectorAction::LoadCitationChoices(source_id) => {
                self.citations.add_source = Some(source_id);
                self.load_citation_choices();
            }
        }
    }

    /// Loads citations, sources and attachment targets for the selected person.
    pub(super) fn sync_citations(&mut self) {
        let current = self.app.selected_person_id().map(str::to_string);
        if self.citations.loaded && self.citations.person_id == current {
            return;
        }
        self.citations.person_id = current.clone();
        self.citations.loaded = true;
        self.citations.error = None;
        self.citations.dirty = false;
        self.citations.pending_delete = None;
        self.citations.choices.clear();
        self.citations.choices_source = None;
        let Some(person_id) = current else {
            self.citations.citations.clear();
            self.citations.sources.clear();
            self.citations.targets.clear();
            return;
        };
        match (
            self.app.list_person_citations(&person_id),
            self.app.list_sources(),
            self.app.citation_targets(&person_id),
        ) {
            (Ok(citations), Ok(sources), Ok(targets)) => {
                self.citations.citations = CitationDraft::from_records(&citations);
                self.citations.sources = sources;
                self.citations.targets = targets;
                if !self
                    .citations
                    .targets
                    .iter()
                    .any(|target| target.key == self.citations.add_target)
                {
                    self.citations.add_target = self
                        .citations
                        .targets
                        .first()
                        .map(|target| target.key.clone())
                        .unwrap_or_else(|| "BIRTH".to_string());
                }
                if let Some(source_id) = self.citations.add_source.as_deref()
                    && !self
                        .citations
                        .sources
                        .iter()
                        .any(|source| source.id == source_id)
                {
                    self.citations.add_source = None;
                }
                self.load_citation_choices();
            }
            (Err(message), _, _) | (_, Err(message), _) | (_, _, Err(message)) => {
                self.citations.error = Some(message);
            }
        }
    }

    /// Loads the citations of the source selected for reuse.
    pub(super) fn load_citation_choices(&mut self) {
        let Some(source_id) = self.citations.add_source.clone() else {
            self.citations.choices.clear();
            self.citations.choices_source = None;
            return;
        };
        match self.app.source_citations(&source_id) {
            Ok(choices) => {
                self.citations.choices = choices;
                self.citations.choices_source = Some(source_id);
            }
            Err(message) => {
                self.citations.choices.clear();
                self.citations.choices_source = None;
                self.citations.error = Some(message);
            }
        }
    }

    /// Selects `person_id`, jumps to the graph and centres the viewport
    /// on its node, so a relative click always lands on the right place.
    pub(super) fn focus_person(&mut self, person_id: &str) {
        self.app.set_selected_person(Some(person_id.to_string()));
        self.tab = tabs::AppTab::Graph;
        if let Some(node) = self
            .scene
            .nodes
            .iter()
            .find(|node| node.person_id == person_id)
        {
            self.viewport.pan = pan_centering(node.bounds.center(), self.viewport.zoom);
        }
    }

    /// Refills the buffer whenever the selection changes.
    pub(super) fn sync_inspector_form(&mut self) {
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
    pub(super) fn edit_person(&mut self, details: &PersonDetails) {
        let Some(person_id) = self.app.selected_person_id().map(str::to_string) else {
            return;
        };
        match self.app.edit_person(&person_id, details) {
            Ok(()) => {
                self.places.loaded = false;
                self.reload_scene_quiet();
            }
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    pub(super) fn open_relation(&mut self, target: &str, kind: RelationKind) {
        let target_label = self
            .tree
            .people
            .iter()
            .find(|person| person.id == target)
            .map(inspector::person_label)
            .unwrap_or_else(|| "(unnamed)".to_string());
        self.pending_relation = Some(RelationForm::new(target, kind, &target_label));
    }

    pub(super) fn show_relation_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.pending_relation.take() else {
            return;
        };
        match inspector::relation_window(ctx, &mut form, &self.tree) {
            None => self.pending_relation = Some(form),
            Some(RelationOutcome::Cancel) => {}
            Some(RelationOutcome::Submit) => self.submit_relation(form),
            Some(RelationOutcome::Link(person_id)) => {
                self.link_existing_relation(&form.target, form.kind, &person_id, form.role);
            }
        }
    }

    pub(super) fn submit_relation(&mut self, form: RelationForm) {
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

    /// Links an existing person as the target's parent, spouse or child.
    pub(super) fn link_existing_relation(
        &mut self,
        target: &str,
        kind: RelationKind,
        person_id: &str,
        role: ParentRole,
    ) {
        let result = match kind {
            RelationKind::Parent => self.app.link_existing_parent(target, person_id, role),
            RelationKind::Spouse => self.app.link_existing_spouse(target, person_id),
            RelationKind::Child => self.app.link_existing_child(target, person_id),
        };
        match result {
            Ok(()) => self.reload_scene(),
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }
}
