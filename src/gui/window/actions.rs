//! Tree lifecycle actions, watch list and inspector wiring.

use crate::app;
use crate::app::UiStatus;
use crate::db::people::PersonDetails;
use crate::gui::dialogs;
use crate::gui::inspector;
use crate::gui::inspector::InspectorAction;
use crate::gui::inspector::InspectorForm;
use crate::gui::inspector::RelationForm;
use crate::gui::inspector::RelationKind;
use crate::gui::inspector::RelationOutcome;
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
            Ok(()) => self.reload_scene(),
            Err(message) => self.app.set_status(UiStatus::Error(message)),
        }
    }

    /// Renders the inspector side panel for the selected person.
    pub(super) fn person_inspector(&mut self, ui: &mut Ui) {
        self.sync_inspector_form();
        let Some(person_id) = self.app.selected_person_id().map(str::to_string) else {
            return;
        };
        let person = self
            .tree
            .people
            .iter()
            .find(|person| person.id == person_id);
        let connections = inspector::connections(&self.tree, &person_id, &self.marriages);
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
                InspectorAction::Focus(target) => self.focus_person(&target),
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
            Ok(()) => self.reload_scene_quiet(),
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
        match inspector::relation_window(ctx, &mut form) {
            None => self.pending_relation = Some(form),
            Some(RelationOutcome::Cancel) => {}
            Some(RelationOutcome::Submit) => self.submit_relation(form),
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
}
