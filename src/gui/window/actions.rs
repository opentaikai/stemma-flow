//! Tree lifecycle actions, watch list and inspector wiring.

use crate::app;
use crate::app::UiStatus;
use crate::db::media::{MediaDraft, MediaOwner};
use crate::db::notes::NoteOwner;
use crate::db::people::{ParentRole, PersonDetails};
use crate::db::sources::SourceDraft;
use crate::db::tasks::TaskOwner;
use crate::gui::associations::{self as associations_dialog, AssociationOutcome};
use crate::gui::associations_view::{self as association_view_ui, AssociationViewEvent};
use crate::gui::citations::CitationDraft;
use crate::gui::couples::{self as couples_ui, CouplesEvent};
use crate::gui::dialogs;
use crate::gui::export_dialog;
use crate::gui::fact_types::{self, FactTypesEvent};
use crate::gui::inspector;
use crate::gui::inspector::InspectorAction;
use crate::gui::inspector::InspectorForm;
use crate::gui::inspector::RelationForm;
use crate::gui::inspector::RelationKind;
use crate::gui::inspector::RelationOutcome;
use crate::gui::media::{self as media_ui, MediaAttachOutcome, MediaAttachState, MediaEvent};
use crate::gui::notes::{self as notes_dialog, NotesOutcome};
use crate::gui::pedigree;
use crate::gui::person_editor::{EditorPrompt, PersonEditor};
use crate::gui::places::{self, PlacesEvent};
use crate::gui::repositories::{self, RepositoriesEvent};
use crate::gui::research::{self as research_ui, ResearchEvent};
use crate::gui::sources::{self, SourcesEvent};
use crate::gui::tabs;
use crate::gui::tasks::{self as task_ui, TaskAttachOutcome, TaskAttachState, TaskEvent};
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
            &self.config.people_columns,
            self.tree_settings.root_person_id.as_deref(),
        );
        for event in events {
            self.apply_watch_event(event);
        }
    }

    pub(super) fn apply_watch_event(&mut self, event: WatchEvent) {
        match event {
            WatchEvent::Select(person_id) => self.app.set_selected_person(Some(person_id)),
            WatchEvent::Edit(person_id) => {
                self.app.set_selected_person(Some(person_id.clone()));
                self.open_person_editor(&person_id);
            }
            WatchEvent::AddPerson => self.add_person(),
            WatchEvent::Delete(person_id) => self.delete_person(&person_id),
            WatchEvent::ColumnsChanged(columns) => {
                self.config.people_columns = columns;
                if let Err(error) = self.config.save() {
                    self.app.set_status(UiStatus::Error(error));
                }
            }
            WatchEvent::SetRoot(person_id) => self.set_root_person(Some(&person_id)),
            WatchEvent::ClearRoot => self.set_root_person(None),
        }
    }

    pub(super) fn couples_tab(&mut self, ui: &mut Ui) {
        let events = couples_ui::show(ui, &mut self.couples, &self.couple_rows);
        for event in events {
            match event {
                CouplesEvent::SelectPerson(id) => self.app.set_selected_person(Some(id)),
                CouplesEvent::EditPerson(id) => {
                    self.app.set_selected_person(Some(id.clone()));
                    self.open_person_editor(&id);
                }
                CouplesEvent::OpenFamily(id) => self.open_couple_family(&id),
            }
        }
    }

    pub(super) fn open_couple_family(&mut self, family_id: &str) {
        let Some(family) = self
            .tree
            .families
            .iter()
            .find(|family| family.id == family_id)
        else {
            return;
        };
        let Some(root) = family.husband_id.clone().or_else(|| family.wife_id.clone()) else {
            return;
        };
        self.pedigree.mode = pedigree::GraphMode::Family;
        self.pedigree.family_root_id = Some(root);
        self.pedigree.family_id = Some(family_id.to_string());
        self.tab = tabs::AppTab::Graph;
        self.rebuild_scene_layout();
        self.pending_framing = true;
    }

    pub(super) fn associations_tab(&mut self, ui: &mut Ui) {
        if !self.association_view.loaded {
            match self
                .app
                .list_all_associations(self.tree_settings.surname_first)
            {
                Ok(rows) => {
                    self.association_rows = rows;
                    self.association_view.loaded = true;
                    self.association_view.mark_dirty();
                }
                Err(message) => self.app.set_status(UiStatus::Error(message)),
            }
        }
        let events =
            association_view_ui::show(ui, &mut self.association_view, &self.association_rows);
        for event in events {
            match event {
                AssociationViewEvent::SelectPerson(id) => {
                    self.app.set_selected_person(Some(id));
                }
                AssociationViewEvent::EditPerson(id) => {
                    self.app.set_selected_person(Some(id.clone()));
                    self.open_person_editor(&id);
                }
                AssociationViewEvent::EditAssociation(id) => {
                    if let Some(row) = self.association_rows.iter().find(|row| row.id == id) {
                        self.pending_association = Some(associations_dialog::AssociationForm {
                            id: Some(row.id.clone()),
                            person_id: row.person_id.clone(),
                            editing_label: Some(row.associate_name.clone()),
                            search: String::new(),
                            selected: Some(row.associate_id.clone()),
                            relationship: row.relationship.clone(),
                            error: None,
                        });
                    }
                }
                AssociationViewEvent::Delete(id) => match self.app.delete_association(&id) {
                    Ok(()) => {
                        self.association_view.loaded = false;
                        self.associations_owner = None;
                        self.sync_associations();
                    }
                    Err(message) => self.app.set_status(UiStatus::Error(message)),
                },
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
            SourcesEvent::Media { source_id, title } => {
                self.open_media(MediaOwner::Source(source_id), title);
            }
            SourcesEvent::Tasks { source_id, title } => {
                self.open_tasks(TaskOwner::Source(source_id), title);
            }
            SourcesEvent::Delete(id) => match self.app.delete_source(&id) {
                Ok(()) => {
                    self.research.tasks.loaded = false;
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
                    self.research.tasks.loaded = false;
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
            PlacesEvent::Notes { place_id, title } => {
                self.open_notes(NoteOwner::Place(place_id), title);
            }
            PlacesEvent::Media { place_id, title } => {
                self.open_media(MediaOwner::Place(place_id), title);
            }
            PlacesEvent::Tasks { place_id, title } => {
                self.open_tasks(TaskOwner::Place(place_id), title);
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
        match (
            self.app.load_place(id),
            self.app.place_usage(id, self.tree_settings.surname_first),
        ) {
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

    pub(super) fn research_tab(&mut self, ui: &mut Ui) {
        if !self.research.tasks.loaded {
            self.reload_tasks();
        }
        if !self.research.groups.loaded {
            self.reload_groups();
        }
        if let Some(event) = research_ui::show(
            ui,
            &mut self.research,
            &self.tree.people,
            self.tree_settings.surname_first,
        ) {
            self.apply_research_event(event);
        }
    }

    pub(super) fn reload_tasks(&mut self) {
        match (self.app.list_tasks(), self.app.list_task_folders()) {
            (Ok(tasks), Ok(folders)) => {
                self.research.tasks.tasks = tasks;
                self.research.tasks.folders = folders;
                self.research.tasks.loaded = true;
                self.research.tasks.error = None;
                if let Some(id) = self.research.tasks.selected.clone() {
                    self.load_task_draft(&id);
                }
            }
            (Err(message), _) | (_, Err(message)) => {
                self.research.tasks.error = Some(message);
                self.research.tasks.loaded = true;
            }
        }
    }

    pub(super) fn load_task_draft(&mut self, id: &str) {
        match (
            self.app.load_task(id),
            self.app
                .list_task_links(id, self.tree_settings.surname_first),
        ) {
            (Ok(Some(record)), Ok(links)) => {
                self.research.tasks.selected = Some(id.to_string());
                self.research.tasks.draft = task_ui::TaskEdit::from_record(&record);
                self.research.tasks.links = links;
                self.research.tasks.editing = true;
                self.research.tasks.error = None;
            }
            (Ok(None), _) => {
                self.research.tasks.selected = None;
                self.research.tasks.draft = task_ui::TaskEdit::default();
                self.research.tasks.links.clear();
                self.research.tasks.editing = false;
            }
            (Err(message), _) | (_, Err(message)) => {
                self.research.tasks.error = Some(message);
            }
        }
    }

    pub(super) fn reload_groups(&mut self) {
        match self.app.list_groups() {
            Ok(groups) => {
                self.research.groups.groups = groups;
                self.research.groups.loaded = true;
                self.research.groups.error = None;
                if let Some(id) = self.research.groups.selected.clone() {
                    self.load_group_draft(&id);
                }
            }
            Err(message) => {
                self.research.groups.error = Some(message);
                self.research.groups.loaded = true;
            }
        }
    }

    pub(super) fn load_group_draft(&mut self, id: &str) {
        match (
            self.app.load_group(id),
            self.app
                .list_group_members(id, self.tree_settings.surname_first),
        ) {
            (Ok(Some(record)), Ok(members)) => {
                self.research.groups.selected = Some(id.to_string());
                self.research.groups.draft = task_ui_group_edit(&record);
                self.research.groups.members = members;
                self.research.groups.editing = true;
                self.research.groups.error = None;
            }
            (Ok(None), _) => {
                self.research.groups.selected = None;
                self.research.groups.draft = Default::default();
                self.research.groups.members.clear();
                self.research.groups.editing = false;
            }
            (Err(message), _) | (_, Err(message)) => {
                self.research.groups.error = Some(message);
            }
        }
    }

    pub(super) fn apply_research_event(&mut self, event: ResearchEvent) {
        match event {
            ResearchEvent::Task(event) => self.apply_task_event(event),
            ResearchEvent::Group(event) => self.apply_group_event(event),
        }
    }

    fn apply_task_event(&mut self, event: TaskEvent) {
        match event {
            TaskEvent::New => {
                self.research.tasks.selected = None;
                self.research.tasks.draft = task_ui::TaskEdit::default();
                self.research.tasks.links.clear();
                self.research.tasks.editing = true;
                self.research.tasks.error = None;
            }
            TaskEvent::Select(id) => self.load_task_draft(&id),
            TaskEvent::Save(draft) => match self.app.save_task(&draft) {
                Ok(id) => {
                    self.research.tasks.selected = Some(id);
                    self.research.tasks.loaded = false;
                    self.reload_tasks();
                }
                Err(message) => self.research.tasks.error = Some(message),
            },
            TaskEvent::Delete(id) => match self.app.delete_task(&id) {
                Ok(()) => {
                    self.research.tasks.selected = None;
                    self.research.tasks.draft = task_ui::TaskEdit::default();
                    self.research.tasks.links.clear();
                    self.research.tasks.editing = false;
                    self.research.tasks.loaded = false;
                    self.reload_tasks();
                }
                Err(message) => self.research.tasks.error = Some(message),
            },
            TaskEvent::Detach {
                owner_type,
                owner_id,
            } => {
                let Some(owner) = task_owner(&owner_type, &owner_id) else {
                    return;
                };
                let Some(task_id) = self.research.tasks.selected.clone() else {
                    return;
                };
                match self.app.detach_task(&task_id, &owner) {
                    Ok(()) => {
                        self.research.tasks.loaded = false;
                        self.load_task_draft(&task_id);
                    }
                    Err(message) => self.research.tasks.error = Some(message),
                }
            }
        }
    }

    fn apply_group_event(&mut self, event: crate::gui::groups::GroupEvent) {
        use crate::gui::groups::GroupEvent;
        match event {
            GroupEvent::New => {
                self.research.groups.selected = None;
                self.research.groups.draft = Default::default();
                self.research.groups.members.clear();
                self.research.groups.editing = true;
                self.research.groups.error = None;
            }
            GroupEvent::Select(id) => self.load_group_draft(&id),
            GroupEvent::Save(draft) => match self.app.save_group(&draft) {
                Ok(id) => {
                    self.research.groups.selected = Some(id);
                    self.research.groups.loaded = false;
                    self.reload_groups();
                }
                Err(message) => self.research.groups.error = Some(message),
            },
            GroupEvent::Delete(id) => match self.app.delete_group(&id) {
                Ok(()) => {
                    self.research.groups.selected = None;
                    self.research.groups.draft = Default::default();
                    self.research.groups.members.clear();
                    self.research.groups.editing = false;
                    self.research.groups.loaded = false;
                    self.reload_groups();
                }
                Err(message) => self.research.groups.error = Some(message),
            },
            GroupEvent::AddMember(person_id) => {
                let Some(group_id) = self.research.groups.selected.clone() else {
                    return;
                };
                match self.app.add_group_member(&group_id, &person_id) {
                    Ok(()) => {
                        self.research.groups.loaded = false;
                        self.load_group_draft(&group_id);
                    }
                    Err(message) => self.research.groups.error = Some(message),
                }
            }
            GroupEvent::RemoveMember(person_id) => {
                let Some(group_id) = self.research.groups.selected.clone() else {
                    return;
                };
                match self.app.remove_group_member(&group_id, &person_id) {
                    Ok(()) => {
                        self.research.groups.loaded = false;
                        self.load_group_draft(&group_id);
                    }
                    Err(message) => self.research.groups.error = Some(message),
                }
            }
        }
    }

    pub(super) fn open_tasks(&mut self, owner: TaskOwner, title: String) {
        let mut state = TaskAttachState::new(owner, title);
        self.load_task_attach(&mut state);
        self.task_attach = Some(state);
    }

    fn load_task_attach(&self, state: &mut TaskAttachState) {
        match (
            self.app.list_owner_tasks(&state.owner),
            self.app.list_tasks(),
        ) {
            (Ok(attached), Ok(all)) => {
                let attached_ids: std::collections::HashSet<&str> =
                    attached.iter().map(|task| task.id.as_str()).collect();
                state.choices = all
                    .into_iter()
                    .filter(|task| !attached_ids.contains(task.id.as_str()))
                    .collect();
                state.attached = attached;
                state.error = None;
            }
            (Err(message), _) | (_, Err(message)) => state.error = Some(message),
        }
    }

    pub(super) fn show_task_attach(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.task_attach.take() else {
            return;
        };
        match task_ui::show_attach(ctx, &mut state) {
            None => self.task_attach = Some(state),
            Some(TaskAttachOutcome::Close) => {}
            Some(TaskAttachOutcome::Attach(task_id)) => {
                match self.app.attach_task(&task_id, &state.owner) {
                    Ok(()) => {
                        self.research.tasks.loaded = false;
                        self.load_task_attach(&mut state);
                        self.task_attach = Some(state);
                    }
                    Err(message) => {
                        state.error = Some(message);
                        self.task_attach = Some(state);
                    }
                }
            }
            Some(TaskAttachOutcome::Detach(task_id)) => {
                match self.app.detach_task(&task_id, &state.owner) {
                    Ok(()) => {
                        self.research.tasks.loaded = false;
                        self.load_task_attach(&mut state);
                        self.task_attach = Some(state);
                    }
                    Err(message) => {
                        state.error = Some(message);
                        self.task_attach = Some(state);
                    }
                }
            }
        }
    }

    pub(super) fn media_tab(&mut self, ui: &mut Ui) {
        if !self.media.loaded {
            self.reload_media();
        }
        if let Some(event) = media_ui::show(ui, &mut self.media) {
            self.apply_media_event(event);
        }
    }

    pub(super) fn reload_media(&mut self) {
        match self.app.list_media() {
            Ok(list) => {
                self.media.media = list;
                self.media.loaded = true;
                self.media.error = None;
                if let Some(id) = self.media.selected.clone() {
                    self.load_media_draft(&id);
                }
            }
            Err(message) => {
                self.media.error = Some(message);
                self.media.loaded = true;
            }
        }
    }

    pub(super) fn load_media_draft(&mut self, id: &str) {
        match (
            self.app.load_media(id),
            self.app
                .list_media_links(id, self.tree_settings.surname_first),
        ) {
            (Ok(Some(record)), Ok(links)) => {
                self.media.selected = Some(id.to_string());
                self.media.draft = media_ui::MediaEdit::from_record(&record);
                self.media.links = links;
                self.media.editing = true;
                self.media.error = None;
            }
            (Ok(None), _) => {
                self.media.selected = None;
                self.media.draft = media_ui::MediaEdit::default();
                self.media.links.clear();
                self.media.editing = false;
            }
            (Err(message), _) | (_, Err(message)) => self.media.error = Some(message),
        }
    }

    pub(super) fn apply_media_event(&mut self, event: MediaEvent) {
        match event {
            MediaEvent::New => {
                self.media.selected = None;
                self.media.draft = media_ui::MediaEdit::default();
                self.media.links.clear();
                self.media.editing = true;
                self.media.error = None;
            }
            MediaEvent::Select(id) => self.load_media_draft(&id),
            MediaEvent::Save(draft) => match self.app.save_media(&draft) {
                Ok(id) => {
                    self.media.selected = Some(id);
                    self.media.loaded = false;
                    self.reload_media();
                }
                Err(message) => self.media.error = Some(message),
            },
            MediaEvent::Delete(id) => match self.app.delete_media(&id) {
                Ok(()) => {
                    self.research.tasks.loaded = false;
                    self.media.selected = None;
                    self.media.draft = media_ui::MediaEdit::default();
                    self.media.links.clear();
                    self.media.editing = false;
                    self.media.loaded = false;
                    self.reload_media();
                }
                Err(message) => self.media.error = Some(message),
            },
            MediaEvent::Browse => {
                if let Some(path) = dialogs::pick_media_path() {
                    let draft = media_draft_for_path(&path);
                    self.media.draft.file_path = draft.file_path;
                    self.media.draft.format = draft.format;
                    if self.media.draft.caption.trim().is_empty() {
                        self.media.draft.caption = draft.caption;
                    }
                }
            }
            MediaEvent::Tasks { media_id, title } => {
                self.open_tasks(TaskOwner::Media(media_id), title);
            }
            MediaEvent::Detach {
                owner_type,
                owner_id,
            } => {
                let Some(owner) = media_owner(&owner_type, &owner_id) else {
                    return;
                };
                let Some(media_id) = self.media.selected.clone() else {
                    return;
                };
                match self.app.detach_media(&media_id, &owner) {
                    Ok(()) => {
                        self.media.loaded = false;
                        self.load_media_draft(&media_id);
                    }
                    Err(message) => self.media.error = Some(message),
                }
            }
        }
    }

    pub(super) fn open_person_editor(&mut self, person_id: &str) {
        if self
            .person_editor
            .as_ref()
            .is_some_and(|editor| editor.person_id == person_id)
        {
            return;
        }
        if let Some(editor) = self.person_editor.as_mut()
            && editor.is_dirty()
        {
            editor.prompt = Some(EditorPrompt::Switch(person_id.to_string()));
            return;
        }
        if let Some(editor) = self.build_person_editor(person_id) {
            self.person_editor = Some(editor);
        }
    }

    fn build_person_editor(&self, person_id: &str) -> Option<PersonEditor> {
        let person = self
            .tree
            .people
            .iter()
            .find(|person| person.id == person_id)?;
        let form = crate::gui::inspector::InspectorForm::from_person(person, &self.dates);
        let mut names = crate::gui::names::NamesState {
            person_id: Some(person_id.to_string()),
            loaded: true,
            ..crate::gui::names::NamesState::default()
        };
        match self.app.alternate_names(person_id) {
            Ok(list) => names.names = list,
            Err(message) => names.error = Some(message),
        }
        let mut citations = crate::gui::citations::CitationsState {
            person_id: Some(person_id.to_string()),
            loaded: true,
            ..crate::gui::citations::CitationsState::default()
        };
        self.fill_editor_citations(&mut citations, person_id);
        let associations = self
            .app
            .list_associations(person_id, self.tree_settings.surname_first)
            .unwrap_or_default();
        let privacy = self.app.person_privacy(person_id).ok().flatten();
        let groups = self
            .app
            .list_groups_for_person(person_id)
            .unwrap_or_default();
        Some(PersonEditor {
            person_id: person_id.to_string(),
            saved_form: form.clone(),
            form,
            names,
            citations,
            associations,
            associations_owner: Some(person_id.to_string()),
            privacy,
            groups,
            prompt: None,
            prompt_error: None,
        })
    }

    fn fill_editor_citations(
        &self,
        citations: &mut crate::gui::citations::CitationsState,
        person_id: &str,
    ) {
        citations.error = None;
        citations.dirty = false;
        citations.pending_delete = None;
        citations.choices.clear();
        citations.choices_source = None;
        match (
            self.app.list_person_citations(person_id),
            self.app.list_sources(),
            self.app.citation_targets(person_id),
        ) {
            (Ok(records), Ok(sources), Ok(targets)) => {
                citations.citations = CitationDraft::from_records(&records);
                citations.sources = sources;
                citations.targets = targets;
                if !citations
                    .targets
                    .iter()
                    .any(|target| target.key == citations.add_target)
                {
                    citations.add_target = citations
                        .targets
                        .first()
                        .map(|target| target.key.clone())
                        .unwrap_or_else(|| "BIRTH".to_string());
                }
                if let Some(source_id) = citations.add_source.as_deref()
                    && !citations
                        .sources
                        .iter()
                        .any(|source| source.id == source_id)
                {
                    citations.add_source = None;
                }
            }
            (Err(message), _, _) | (_, Err(message), _) | (_, _, Err(message)) => {
                citations.error = Some(message);
            }
        }
    }

    fn refresh_editor_names(&self, editor: &mut PersonEditor) {
        match self.app.alternate_names(&editor.person_id) {
            Ok(names) => editor.names.names = names,
            Err(message) => editor.names.error = Some(message),
        }
    }

    fn refresh_editor_associations(&mut self, editor: &mut PersonEditor) {
        match self
            .app
            .list_associations(&editor.person_id, self.tree_settings.surname_first)
        {
            Ok(associations) => editor.associations = associations,
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    fn refresh_editor_privacy(&self, editor: &mut PersonEditor) {
        editor.privacy = self.app.person_privacy(&editor.person_id).ok().flatten();
        editor.groups = self
            .app
            .list_groups_for_person(&editor.person_id)
            .unwrap_or_default();
    }

    fn save_editor_profile(&mut self, editor: &mut PersonEditor) -> Result<(), String> {
        let details = editor.form.to_details();
        self.app.edit_person(&editor.person_id, &details)?;
        editor.saved_form = editor.form.clone();
        self.reload_scene_quiet();
        self.refresh_editor_privacy(editor);
        Ok(())
    }

    fn resolve_editor_prompt(&mut self, editor: &mut PersonEditor) -> bool {
        let Some(prompt) = editor.prompt.take() else {
            return true;
        };
        match prompt {
            EditorPrompt::Close => false,
            EditorPrompt::Switch(person_id) => match self.build_person_editor(&person_id) {
                Some(next) => {
                    *editor = next;
                    self.app.set_selected_person(Some(person_id));
                    true
                }
                None => false,
            },
        }
    }

    pub(super) fn apply_editor_action(
        &mut self,
        editor: &mut PersonEditor,
        action: InspectorAction,
    ) {
        let person_id = editor.person_id.clone();
        match action {
            InspectorAction::Save(details) => match self.app.edit_person(&person_id, &details) {
                Ok(()) => {
                    editor.saved_form = editor.form.clone();
                    self.reload_scene_quiet();
                    self.refresh_editor_privacy(editor);
                }
                Err(message) => self.app.set_status(UiStatus::Error(message)),
            },
            InspectorAction::Close => editor.prompt = Some(EditorPrompt::Close),
            InspectorAction::OpenEditor => {}
            InspectorAction::SetRoot => self.set_root_person(Some(&person_id)),
            InspectorAction::ClearRoot => self.set_root_person(None),
            InspectorAction::SwitchEditor(target) => {
                if target != person_id {
                    if editor.is_dirty() {
                        editor.prompt = Some(EditorPrompt::Switch(target));
                    } else if let Some(next) = self.build_person_editor(&target) {
                        *editor = next;
                        self.app.set_selected_person(Some(target));
                    }
                }
            }
            InspectorAction::OpenGroups => {
                self.tab = crate::gui::tabs::AppTab::Research;
                self.research.tab = crate::gui::research::ResearchTab::Groups;
            }
            InspectorAction::Focus(target) => self.focus_person(&target),
            InspectorAction::AddParentAs(role) => {
                self.open_relation_with_role(&person_id, RelationKind::Parent, role)
            }
            InspectorAction::AddSpouse => self.open_relation(&person_id, RelationKind::Spouse),
            InspectorAction::AddChild => self.open_relation(&person_id, RelationKind::Child),
            InspectorAction::EditParticipants(event_id) => self.open_participants(&event_id),
            InspectorAction::ManageMedia { owner, title } => self.open_media(owner, title),
            InspectorAction::ManageTasks { owner, title } => self.open_tasks(owner, title),
            InspectorAction::OpenNotes { owner, title } => self.open_notes(owner, title),
            InspectorAction::SaveCitations(edits) => {
                let mut failure = None;
                for edit in edits.iter() {
                    if let Err(message) = self.app.save_citation(edit) {
                        failure = Some(message);
                        break;
                    }
                }
                if let Some(message) = failure {
                    editor.citations.error = Some(message);
                } else {
                    self.fill_editor_citations(&mut editor.citations, &person_id);
                }
            }
            InspectorAction::AddCitation {
                source_id,
                target,
                page,
            } => match self
                .app
                .add_citation(&source_id, &person_id, &target, page.as_deref())
            {
                Ok(_) => {
                    editor.citations.add_page.clear();
                    self.fill_editor_citations(&mut editor.citations, &person_id);
                }
                Err(message) => editor.citations.error = Some(message),
            },
            InspectorAction::AttachCitation {
                citation_id,
                target,
            } => match self.app.attach_citation(&citation_id, &person_id, &target) {
                Ok(_) => self.fill_editor_citations(&mut editor.citations, &person_id),
                Err(message) => editor.citations.error = Some(message),
            },
            InspectorAction::DetachCitation {
                citation_id,
                event_id,
            } => match self.app.detach_citation(&citation_id, &event_id) {
                Ok(()) => self.fill_editor_citations(&mut editor.citations, &person_id),
                Err(message) => editor.citations.error = Some(message),
            },
            InspectorAction::DeleteCitation(id) => match self.app.delete_citation(&id) {
                Ok(()) => {
                    editor.citations.pending_delete = None;
                    self.fill_editor_citations(&mut editor.citations, &person_id);
                }
                Err(message) => editor.citations.error = Some(message),
            },
            InspectorAction::LoadCitationChoices(source_id) => {
                editor.citations.add_source = Some(source_id.clone());
                match self.app.source_citations(&source_id) {
                    Ok(choices) => {
                        editor.citations.choices = choices;
                        editor.citations.choices_source = Some(source_id);
                    }
                    Err(message) => editor.citations.error = Some(message),
                }
            }
            InspectorAction::SetPersonPrivate(private) => {
                match self.app.set_person_private(&person_id, private) {
                    Ok(()) => self.refresh_editor_privacy(editor),
                    Err(message) => self.app.set_status(UiStatus::Error(message)),
                }
            }
            InspectorAction::SetLivingOverride(value) => {
                match self.app.set_living_override(&person_id, value) {
                    Ok(()) => self.refresh_editor_privacy(editor),
                    Err(message) => self.app.set_status(UiStatus::Error(message)),
                }
            }
            InspectorAction::SetEventPrivate { event_id, private } => {
                match self.app.set_event_private(&event_id, private) {
                    Ok(()) => self.refresh_editor_privacy(editor),
                    Err(message) => self.app.set_status(UiStatus::Error(message)),
                }
            }
            InspectorAction::SaveAlternateName(draft) => {
                match self.app.save_alternate_name(&person_id, &draft) {
                    Ok(_) => {
                        editor.names.editing = false;
                        editor.names.draft = crate::db::names::AlternateNameDraft::default();
                        self.refresh_editor_names(editor);
                    }
                    Err(message) => editor.names.error = Some(message),
                }
            }
            InspectorAction::DeleteAlternateName(id) => match self.app.delete_alternate_name(&id) {
                Ok(()) => self.refresh_editor_names(editor),
                Err(message) => editor.names.error = Some(message),
            },
            InspectorAction::AddAssociation => {
                self.pending_association =
                    Some(associations_dialog::AssociationForm::new(&person_id));
            }
            InspectorAction::EditAssociation(id) => {
                if let Some(record) = editor.associations.iter().find(|record| record.id == id) {
                    self.pending_association =
                        Some(associations_dialog::AssociationForm::from_record(record));
                }
            }
            InspectorAction::DeleteAssociation(id) => match self.app.delete_association(&id) {
                Ok(()) => self.refresh_editor_associations(editor),
                Err(message) => self.app.set_status(UiStatus::Error(message)),
            },
        }
    }

    pub(super) fn show_person_editor(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.person_editor.take() else {
            return;
        };
        let Some(person) = self
            .tree
            .people
            .iter()
            .find(|person| person.id == editor.person_id)
            .cloned()
        else {
            return;
        };
        let person_id = editor.person_id.clone();
        let mut connections = inspector::connections(
            &self.tree,
            &person_id,
            &self.marriages,
            self.tree_settings.surname_first,
        );
        if self.config.reverse_place_names {
            inspector::reverse_marriage_places(&mut connections);
        }
        let facts = self
            .person_facts
            .get(&person_id)
            .cloned()
            .unwrap_or_default();
        let associations = editor.associations.clone();
        let groups = editor.groups.clone();
        let dates = self.dates.get(&person_id).cloned();
        let root_relationship = self.root_relationship(&person_id);
        let is_root = self.tree_settings.root_person_id.as_deref() == Some(person_id.as_str());
        let surname_first = self.tree_settings.surname_first;
        let mut action = None;
        let mut open = true;
        egui::Window::new(format!(
            "Edit Person — {}",
            inspector::person_label(&person, surname_first)
        ))
        .id(egui::Id::new("person_editor_window"))
        .resizable(true)
        .default_width(760.0)
        .default_height(720.0)
        .min_width(620.0)
        .open(&mut open)
        .show(ctx, |ui| {
            action = inspector::show(
                ui,
                Some(&person),
                &mut editor.form,
                dates.as_ref(),
                connections,
                &associations,
                &groups,
                &mut editor.citations,
                &facts,
                editor.privacy.as_ref(),
                &mut editor.names,
                root_relationship.as_deref(),
                is_root,
                surname_first,
                true,
            );
        });
        if !open {
            if editor.is_dirty() {
                editor.prompt = Some(EditorPrompt::Close);
            } else {
                return;
            }
        }
        if let Some(action) = action {
            self.apply_editor_action(&mut editor, action);
        }
        if editor.prompt.is_some() {
            let mut outcome = None;
            egui::Window::new(crate::i18n::tr("editor.unsaved_title"))
                .id(egui::Id::new("person_editor_prompt"))
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(crate::i18n::tr("editor.unsaved_hint"));
                    if let Some(error) = &editor.prompt_error {
                        ui.label(
                            egui::RichText::new(error)
                                .color(ui.visuals().error_fg_color)
                                .small(),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::tr("editor.save")).clicked() {
                            outcome = Some("save");
                        }
                        if ui.button(crate::i18n::tr("editor.discard")).clicked() {
                            outcome = Some("discard");
                        }
                        if ui.button(crate::i18n::tr("common.cancel")).clicked() {
                            outcome = Some("cancel");
                        }
                    });
                });
            let mut keep = true;
            match outcome {
                Some("save") => match self.save_editor_profile(&mut editor) {
                    Ok(()) => keep = self.resolve_editor_prompt(&mut editor),
                    Err(message) => editor.prompt_error = Some(message),
                },
                Some("discard") => keep = self.resolve_editor_prompt(&mut editor),
                Some("cancel") => {
                    editor.prompt = None;
                    editor.prompt_error = None;
                }
                _ => {}
            }
            if keep {
                self.person_editor = Some(editor);
            }
        } else {
            self.person_editor = Some(editor);
        }
    }

    fn root_relationship(&self, person_id: &str) -> Option<String> {
        let root = self.tree_settings.root_person_id.as_deref()?;
        if root == person_id {
            return None;
        }
        crate::model::relationship::relationship_to_root(&self.tree, root, person_id)
            .map(|relationship| relationship.label())
    }

    pub(super) fn set_root_person(&mut self, person_id: Option<&str>) {
        match self.app.set_root_person(person_id) {
            Ok(()) => {
                self.tree_settings.root_person_id = person_id.map(str::to_string);
                self.reload_data();
            }
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    pub(super) fn open_media(&mut self, owner: MediaOwner, title: String) {
        let mut state = MediaAttachState::new(owner, title);
        self.load_media_attach(&mut state);
        self.media_attach = Some(state);
    }

    fn load_media_attach(&self, state: &mut MediaAttachState) {
        match (
            self.app.list_owner_media(&state.owner),
            self.app.list_media(),
        ) {
            (Ok(attached), Ok(all)) => {
                let attached_ids: std::collections::HashSet<&str> =
                    attached.iter().map(|media| media.id.as_str()).collect();
                state.choices = all
                    .into_iter()
                    .filter(|media| !attached_ids.contains(media.id.as_str()))
                    .collect();
                state.attached = attached;
                state.error = None;
            }
            (Err(message), _) | (_, Err(message)) => state.error = Some(message),
        }
    }

    pub(super) fn show_media_attach(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.media_attach.take() else {
            return;
        };
        let outcome = media_ui::show_attach(ctx, &mut state);
        match outcome {
            None => self.media_attach = Some(state),
            Some(MediaAttachOutcome::Close) => {}
            Some(MediaAttachOutcome::Attach(media_id)) => {
                match self.app.attach_media(&media_id, &state.owner) {
                    Ok(()) => {
                        self.media.loaded = false;
                        self.load_media_attach(&mut state);
                        self.media_attach = Some(state);
                    }
                    Err(message) => {
                        state.error = Some(message);
                        self.media_attach = Some(state);
                    }
                }
            }
            Some(MediaAttachOutcome::Detach(media_id)) => {
                match self.app.detach_media(&media_id, &state.owner) {
                    Ok(()) => {
                        self.media.loaded = false;
                        self.load_media_attach(&mut state);
                        self.media_attach = Some(state);
                    }
                    Err(message) => {
                        state.error = Some(message);
                        self.media_attach = Some(state);
                    }
                }
            }
            Some(MediaAttachOutcome::AddFromFile) => {
                if let Some(path) = dialogs::pick_media_path() {
                    let draft = media_draft_for_path(&path);
                    match self.app.save_media(&draft) {
                        Ok(media_id) => {
                            if let Err(message) = self.app.attach_media(&media_id, &state.owner) {
                                state.error = Some(message);
                            } else {
                                self.media.loaded = false;
                                self.load_media_attach(&mut state);
                            }
                        }
                        Err(message) => state.error = Some(message),
                    }
                }
                self.media_attach = Some(state);
            }
        }
    }

    pub(super) fn fact_types_tab(&mut self, ui: &mut Ui) {
        if !self.fact_types.loaded {
            self.reload_fact_types();
        }
        if let Some(event) = fact_types::show(ui, &mut self.fact_types) {
            self.apply_fact_types_event(event);
        }
    }

    pub(super) fn apply_fact_types_event(&mut self, event: FactTypesEvent) {
        match event {
            FactTypesEvent::New => {
                self.fact_types.selected = None;
                self.fact_types.draft = crate::db::facts::FactTypeDraft::default();
                self.fact_types.builtin = false;
                self.fact_types.auto_tag = true;
                self.fact_types.add_role.clear();
                self.fact_types.editing = true;
                self.fact_types.error = None;
            }
            FactTypesEvent::Select(key) => self.load_fact_type_draft(&key),
            FactTypesEvent::Save(draft) => {
                let result = if self.fact_types.builtin {
                    match draft.key.clone() {
                        Some(key) => self.app.save_fact_roles(&key, &draft.roles).map(|()| key),
                        None => Err("built-in fact type has no key".to_string()),
                    }
                } else {
                    self.app.save_fact_type(&draft)
                };
                match result {
                    Ok(key) => {
                        self.fact_types.selected = Some(key);
                        self.fact_types.editing = true;
                        self.fact_types.error = None;
                        self.reload_fact_types();
                        self.places.loaded = false;
                        self.reload_scene_quiet();
                    }
                    Err(message) => self.fact_types.error = Some(message),
                }
            }
            FactTypesEvent::Delete(key) => match self.app.delete_fact_type(&key) {
                Ok(()) => {
                    self.fact_types.selected = None;
                    self.fact_types.draft = crate::db::facts::FactTypeDraft::default();
                    self.fact_types.editing = false;
                    self.fact_types.error = None;
                    self.reload_fact_types();
                    self.places.loaded = false;
                    self.reload_scene_quiet();
                }
                Err(message) => self.fact_types.error = Some(message),
            },
        }
    }

    pub(super) fn reload_fact_types(&mut self) {
        match self.app.list_fact_types() {
            Ok(types) => {
                self.fact_types.types = types;
                self.fact_types.loaded = true;
                self.fact_types.error = None;
                if let Some(key) = self.fact_types.selected.clone() {
                    self.load_fact_type_draft(&key);
                }
            }
            Err(message) => {
                self.fact_types.error = Some(message);
                self.fact_types.loaded = true;
            }
        }
    }

    pub(super) fn load_fact_type_draft(&mut self, key: &str) {
        match self.app.load_fact_type(key) {
            Ok(Some(record)) => {
                self.fact_types.selected = Some(key.to_string());
                self.fact_types.builtin = record.is_builtin;
                self.fact_types.auto_tag = false;
                self.fact_types.add_role.clear();
                self.fact_types.draft = fact_types::draft_from_record(&record);
                self.fact_types.editing = true;
                self.fact_types.error = None;
            }
            Ok(None) => {
                self.fact_types.selected = None;
                self.fact_types.draft = crate::db::facts::FactTypeDraft::default();
                self.fact_types.builtin = false;
                self.fact_types.editing = false;
            }
            Err(message) => self.fact_types.error = Some(message),
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
                self.research.tasks.loaded = false;
                self.research.groups.loaded = false;
                self.reload_scene();
            }
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    /// Renders the inspector side panel for the selected person.
    pub(super) fn person_inspector(&mut self, ui: &mut Ui) {
        self.sync_inspector_form();
        self.sync_citations();
        self.sync_alternate_names();
        self.sync_associations();
        let Some(person_id) = self.app.selected_person_id().map(str::to_string) else {
            return;
        };
        let person = self
            .tree
            .people
            .iter()
            .find(|person| person.id == person_id);
        let mut connections = inspector::connections(
            &self.tree,
            &person_id,
            &self.marriages,
            self.tree_settings.surname_first,
        );
        if self.config.reverse_place_names {
            inspector::reverse_marriage_places(&mut connections);
        }
        let associations = self.associations.clone();
        let root_relationship = self.root_relationship(&person_id);
        let is_root = self.tree_settings.root_person_id.as_deref() == Some(person_id.as_str());
        let facts = self
            .person_facts
            .get(&person_id)
            .cloned()
            .unwrap_or_default();
        if let Some(action) = inspector::show(
            ui,
            person,
            &mut self.inspector,
            self.dates.get(&person_id),
            connections,
            &associations,
            &[],
            &mut self.citations,
            &facts,
            self.person_privacy.as_ref(),
            &mut self.names,
            root_relationship.as_deref(),
            is_root,
            self.tree_settings.surname_first,
            false,
        ) {
            self.apply_inspector_action(action, &person_id);
        }
    }

    /// Applies an action emitted by the inspector for `person_id`.
    pub(super) fn apply_inspector_action(&mut self, action: InspectorAction, person_id: &str) {
        match action {
            InspectorAction::Save(details) => self.edit_person(&details),
            InspectorAction::AddParentAs(role) => {
                self.open_relation_with_role(person_id, RelationKind::Parent, role)
            }
            InspectorAction::AddSpouse => self.open_relation(person_id, RelationKind::Spouse),
            InspectorAction::AddChild => self.open_relation(person_id, RelationKind::Child),
            InspectorAction::OpenEditor => self.open_person_editor(person_id),
            InspectorAction::SetRoot => self.set_root_person(Some(person_id)),
            InspectorAction::ClearRoot => self.set_root_person(None),
            InspectorAction::SwitchEditor(target) => self.open_person_editor(&target),
            InspectorAction::OpenGroups => {
                self.tab = tabs::AppTab::Research;
                self.research.tab = crate::gui::research::ResearchTab::Groups;
            }
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
                    self.research.tasks.loaded = false;
                    self.citations.pending_delete = None;
                    self.citations.loaded = false;
                }
                Err(message) => self.citations.error = Some(message),
            },
            InspectorAction::LoadCitationChoices(source_id) => {
                self.citations.add_source = Some(source_id);
                self.load_citation_choices();
            }
            InspectorAction::EditParticipants(event_id) => self.open_participants(&event_id),
            InspectorAction::SetPersonPrivate(private) => {
                match self.app.set_person_private(person_id, private) {
                    Ok(()) => self.reload_person_privacy(),
                    Err(message) => self.app.set_status(UiStatus::Error(message)),
                }
            }
            InspectorAction::SetLivingOverride(value) => {
                match self.app.set_living_override(person_id, value) {
                    Ok(()) => self.reload_person_privacy(),
                    Err(message) => self.app.set_status(UiStatus::Error(message)),
                }
            }
            InspectorAction::SetEventPrivate { event_id, private } => {
                match self.app.set_event_private(&event_id, private) {
                    Ok(()) => self.reload_person_privacy(),
                    Err(message) => self.app.set_status(UiStatus::Error(message)),
                }
            }
            InspectorAction::AddAssociation => {
                self.pending_association =
                    Some(associations_dialog::AssociationForm::new(person_id));
            }
            InspectorAction::EditAssociation(id) => {
                if let Some(record) = self.associations.iter().find(|record| record.id == id) {
                    self.pending_association =
                        Some(associations_dialog::AssociationForm::from_record(record));
                }
            }
            InspectorAction::DeleteAssociation(id) => match self.app.delete_association(&id) {
                Ok(()) => {
                    self.associations_owner = None;
                    self.sync_associations();
                }
                Err(message) => self.app.set_status(UiStatus::Error(message)),
            },
            InspectorAction::ManageMedia { owner, title } => self.open_media(owner, title),
            InspectorAction::ManageTasks { owner, title } => self.open_tasks(owner, title),
            InspectorAction::OpenNotes { owner, title } => self.open_notes(owner, title),
            InspectorAction::SaveAlternateName(draft) => {
                match self.app.save_alternate_name(person_id, &draft) {
                    Ok(_) => {
                        self.names.loaded = false;
                        self.names.editing = false;
                        self.names.draft = crate::db::names::AlternateNameDraft::default();
                    }
                    Err(message) => self.names.error = Some(message),
                }
            }
            InspectorAction::DeleteAlternateName(id) => match self.app.delete_alternate_name(&id) {
                Ok(()) => self.names.loaded = false,
                Err(message) => self.names.error = Some(message),
            },
        }
    }

    pub(super) fn open_notes(&mut self, owner: NoteOwner, title: String) {
        match self.app.list_notes(&owner) {
            Ok(notes) => self.notes.open(owner, title, notes),
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    pub(super) fn show_notes(&mut self, ctx: &egui::Context) {
        let Some(outcome) = notes_dialog::show(ctx, &mut self.notes) else {
            return;
        };
        let Some(owner) = self.notes.target.clone() else {
            return;
        };
        let result = match outcome {
            NotesOutcome::Save { id, body } => match id {
                Some(id) => self.app.update_note(&id, &body),
                None => self.app.add_note(&owner, &body),
            },
            NotesOutcome::Delete(id) => self.app.delete_note(&id),
        };
        match result {
            Ok(()) => {
                self.notes.finish_edit();
                match self.app.list_notes(&owner) {
                    Ok(notes) => self.notes.replace_notes(notes),
                    Err(message) => self.notes.error = Some(message),
                }
                match owner {
                    NoteOwner::Place(_) => self.places.loaded = false,
                    NoteOwner::Citation(_) => self.citations.loaded = false,
                    NoteOwner::Event(_) => self.reload_scene_quiet(),
                    NoteOwner::Person(_) | NoteOwner::Family(_) => {}
                }
            }
            Err(message) => self.notes.error = Some(message),
        }
    }

    pub(super) fn sync_associations(&mut self) {
        let current = self.app.selected_person_id().map(str::to_string);
        if self.associations_owner == current {
            return;
        }
        self.associations_owner = current.clone();
        let Some(person_id) = current else {
            self.associations.clear();
            return;
        };
        match self
            .app
            .list_associations(&person_id, self.tree_settings.surname_first)
        {
            Ok(associations) => self.associations = associations,
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    pub(super) fn show_association_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.pending_association.take() else {
            return;
        };
        match associations_dialog::show(
            ctx,
            &mut form,
            &self.tree,
            self.tree_settings.surname_first,
        ) {
            None => self.pending_association = Some(form),
            Some(AssociationOutcome::Cancel) => {}
            Some(AssociationOutcome::Save(draft)) => match self.app.save_association(&draft) {
                Ok(_) => {
                    self.associations_owner = None;
                    self.association_view.loaded = false;
                    self.sync_associations();
                }
                Err(message) => {
                    form.error = Some(message);
                    self.pending_association = Some(form);
                }
            },
        }
    }

    pub(super) fn sync_alternate_names(&mut self) {
        let current = self.app.selected_person_id().map(str::to_string);
        if self.names.loaded && self.names.person_id == current {
            return;
        }
        self.names.person_id = current.clone();
        self.names.loaded = true;
        self.names.error = None;
        let Some(person_id) = current else {
            self.names.names.clear();
            return;
        };
        match self.app.alternate_names(&person_id) {
            Ok(names) => self.names.names = names,
            Err(message) => self.names.error = Some(message),
        }
    }

    pub(super) fn reload_person_privacy(&mut self) {
        self.person_privacy = if let Some(id) = self.inspected.as_deref() {
            match self.app.person_privacy(id) {
                Ok(privacy) => privacy,
                Err(message) => {
                    self.app.set_status(UiStatus::Error(message));
                    None
                }
            }
        } else {
            None
        };
    }

    pub(super) fn open_participants(&mut self, event_id: &str) {
        match self
            .app
            .load_event_context(event_id, self.tree_settings.surname_first)
        {
            Ok(Some(context)) => {
                self.pending_participants = Some(inspector::ParticipantForm::from_context(context));
            }
            Ok(None) => self
                .app
                .set_status(UiStatus::Error(format!("no event with id {event_id}"))),
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    pub(super) fn show_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_export {
            return;
        }
        if export_dialog::show(ctx, &mut self.export_options, &mut self.show_export) == Some(true) {
            self.app.begin_export_with_options(self.export_options);
        }
    }

    pub(super) fn show_participant_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.pending_participants.take() else {
            return;
        };
        match inspector::participant_window(
            ctx,
            &mut form,
            &self.tree,
            self.tree_settings.surname_first,
        ) {
            None => self.pending_participants = Some(form),
            Some(inspector::ParticipantOutcome::Cancel) => {}
            Some(inspector::ParticipantOutcome::Save {
                event_id,
                participants,
            }) => match self.app.save_event_participants(&event_id, &participants) {
                Ok(()) => self.reload_scene_quiet(),
                Err(message) => {
                    form.error = Some(message);
                    self.pending_participants = Some(form);
                }
            },
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
        self.reload_person_privacy();
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
                self.reload_person_privacy();
            }
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    /// Applies the surname-first preference to the active tree and refreshes
    /// every label derived from person names.
    pub(super) fn set_surname_first(&mut self, surname_first: bool) {
        match self.app.set_surname_first(surname_first) {
            Ok(()) => {
                self.tree_settings.surname_first = surname_first;
                self.associations_owner = None;
                self.places.loaded = false;
                self.research.tasks.loaded = false;
                self.research.groups.loaded = false;
                self.pending_participants = None;
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
            .map(|person| inspector::person_label(person, self.tree_settings.surname_first))
            .unwrap_or_else(|| "(unnamed)".to_string());
        self.pending_relation = Some(RelationForm::new(target, kind, &target_label));
    }

    pub(super) fn open_relation_with_role(
        &mut self,
        target: &str,
        kind: RelationKind,
        role: crate::db::people::ParentRole,
    ) {
        let target_label = self
            .tree
            .people
            .iter()
            .find(|person| person.id == target)
            .map(|person| inspector::person_label(person, self.tree_settings.surname_first))
            .unwrap_or_else(|| "(unnamed)".to_string());
        self.pending_relation = Some(inspector::RelationForm::new_with_role(
            target,
            kind,
            &target_label,
            role,
        ));
    }

    pub(super) fn show_relation_window(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.pending_relation.take() else {
            return;
        };
        match inspector::relation_window(
            ctx,
            &mut form,
            &self.tree,
            self.tree_settings.surname_first,
        ) {
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

fn media_owner(owner_type: &str, id: &str) -> Option<MediaOwner> {
    match owner_type {
        "person" => Some(MediaOwner::Person(id.to_string())),
        "family" => Some(MediaOwner::Family(id.to_string())),
        "fact" => Some(MediaOwner::Event(id.to_string())),
        "source" => Some(MediaOwner::Source(id.to_string())),
        "place" => Some(MediaOwner::Place(id.to_string())),
        _ => None,
    }
}

fn media_draft_for_path(path: &std::path::Path) -> MediaDraft {
    MediaDraft {
        id: None,
        file_path: path.to_string_lossy().to_string(),
        format: path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase(),
        caption: path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string(),
        notes: String::new(),
    }
}

fn task_owner(owner_type: &str, id: &str) -> Option<TaskOwner> {
    match owner_type {
        "person" => Some(TaskOwner::Person(id.to_string())),
        "family" => Some(TaskOwner::Family(id.to_string())),
        "fact" => Some(TaskOwner::Event(id.to_string())),
        "source" => Some(TaskOwner::Source(id.to_string())),
        "citation" => Some(TaskOwner::Citation(id.to_string())),
        "place" => Some(TaskOwner::Place(id.to_string())),
        "media" => Some(TaskOwner::Media(id.to_string())),
        _ => None,
    }
}

fn task_ui_group_edit(record: &crate::db::groups::GroupRecord) -> crate::gui::groups::GroupEdit {
    crate::gui::groups::GroupEdit::from_record(record)
}
