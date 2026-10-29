//! Scene reload, framing and the interactive canvas.

use crate::db;
use crate::db::FamilyTree;
use crate::gui::connectors::paint_connectors;
use crate::gui::inspector;
use crate::gui::layout;
use crate::gui::nodes::hit_test;
use crate::gui::nodes::paint_nodes;
use crate::gui::pedigree;
use crate::gui::theme;
use crate::gui::watchlist;
use crate::model::descendant_view::descendant_tree;
use crate::model::family_view::{candidate_families, family_tree};
use crate::model::pedigree::pedigree_tree;
use egui::Align2;
use egui::FontId;
use egui::PointerButton;
use egui::Rect;
use egui::Sense;
use egui::Ui;
use egui::Vec2;
use std::collections::HashMap;
use std::path::Path;

use super::{HINT_COLOR, HINT_TEXT};

use super::TreeWindow;

#[derive(Debug, Clone, PartialEq, Eq)]
enum CanvasClick {
    Select(Option<String>),
    Edit(String),
}

fn canvas_click_action(
    double_clicked: bool,
    clicked: bool,
    hit: Option<&str>,
) -> Option<CanvasClick> {
    if double_clicked {
        return hit.map(|person_id| CanvasClick::Edit(person_id.to_string()));
    }
    clicked.then(|| CanvasClick::Select(hit.map(str::to_string)))
}

impl TreeWindow {
    /// Rebuilds the scene when a background import bumped the generation
    /// or the active tree switched to another database.
    pub(super) fn sync_scene(&mut self) {
        let active = self.app.active_db().map(Path::to_path_buf);
        if active != self.seen_db || self.app.import_generation() != self.seen_generation {
            self.reload_scene();
        }
    }

    /// One-shot startup apply of the persisted theme/zoom settings.
    pub(super) fn apply_pending_config(&mut self, ctx: &egui::Context) {
        if self.pending_apply {
            theme::apply_config_to_ctx(ctx, &self.config);
            self.pending_apply = false;
        }
    }

    /// Reloads the scene without re-centring the viewport, so edits keep
    /// the user's current pan and zoom.
    pub(super) fn reload_scene_quiet(&mut self) {
        self.reload_data();
    }

    /// Loads people, families, vitals and dates from the active database,
    /// refreshes the watch-list rows and queues a re-centre of the viewport.
    pub(super) fn reload_scene(&mut self) {
        self.reload_data();
        self.pending_framing = true;
    }

    pub(super) fn reload_data(&mut self) {
        let switched = self.seen_db != self.app.active_db().map(Path::to_path_buf);
        let loaded = self
            .app
            .active_db()
            .and_then(|path| db::open_connection(path).ok())
            .and_then(|conn| {
                let tree = db::load_family_tree(&conn).ok()?;
                let dates = db::people::load_vital_dates(&conn).ok()?;
                let marriages = db::load_marriage_details(&conn).ok()?;
                let mut settings = db::settings::load_tree_settings(&conn).ok()?;
                if let Some(root) = settings.root_person_id.as_deref()
                    && !tree.people.iter().any(|person| person.id == root)
                {
                    let _ = db::settings::set_root_person(&conn, None);
                    settings.root_person_id = None;
                }
                let extras = db::people::load_person_extras(&conn).ok()?;
                let facts = db::facts::load_person_facts(&conn, settings.surname_first).ok()?;
                Some((tree, dates, extras, marriages, facts, settings))
            });
        let (tree, dates, extras, marriages, facts, settings) = loaded.unwrap_or_else(|| {
            (
                FamilyTree::default(),
                HashMap::new(),
                HashMap::new(),
                HashMap::new(),
                HashMap::new(),
                crate::db::settings::TreeSettings::default(),
            )
        });
        self.rows = watchlist::rows(
            &tree,
            &dates,
            &extras,
            settings.root_person_id.as_deref(),
            settings.surname_first,
        );
        self.couple_rows =
            crate::model::couples::couples(&tree, &marriages, settings.surname_first);
        self.couples.mark_dirty();
        self.tree = tree;
        self.dates = dates;
        self.marriages = marriages;
        self.person_facts = facts;
        self.tree_settings = settings;
        self.watch.mark_dirty();
        self.seen_db = self.app.active_db().map(Path::to_path_buf);
        let generation = self.app.import_generation();
        let generation_changed = self.seen_generation != generation;
        self.seen_generation = generation;
        if switched {
            self.places = crate::gui::places::PlacesState::default();
            self.names = crate::gui::names::NamesState::default();
            self.fact_types = crate::gui::fact_types::FactTypesState::default();
            self.sources = crate::gui::sources::SourcesState::default();
            self.citations = crate::gui::citations::CitationsState::default();
            self.repositories = crate::gui::repositories::RepositoriesState::default();
            self.pending_participants = None;
            self.pending_new_person = None;
            self.person_privacy = None;
            self.notes = crate::gui::notes::NotesState::default();
            self.person_editor = None;
            self.associations.clear();
            self.associations_owner = None;
            self.pending_association = None;
            self.media = crate::gui::media::MediaState::default();
            self.media_attach = None;
            self.research = crate::gui::research::ResearchState::default();
            self.task_attach = None;
        } else if generation_changed {
            self.places.loaded = false;
            self.fact_types.loaded = false;
            self.repositories.loaded = false;
            self.reload_person_privacy();
        }
        self.rebuild_scene_layout();
    }

    /// Rebuilds the canvas scene for the active All or Pedigree mode.
    pub(super) fn rebuild_scene_layout(&mut self) {
        let surname_first = self.tree_settings.surname_first;
        match self.pedigree.mode {
            pedigree::GraphMode::All => {
                self.scene = layout::build_scene(&self.tree, surname_first);
            }
            pedigree::GraphMode::Pedigree => match self.pedigree.root_id.as_deref() {
                Some(root) if self.tree.people.iter().any(|person| person.id == root) => {
                    let filtered =
                        pedigree_tree(&self.tree, root, self.pedigree.generations.levels());
                    self.scene = layout::build_scene(&filtered, surname_first);
                }
                _ => self.scene = super::loading::empty_scene(),
            },
            pedigree::GraphMode::Family => match self.pedigree.family_root_id.as_deref() {
                Some(root) if self.tree.people.iter().any(|person| person.id == root) => {
                    let candidates = candidate_families(&self.tree, root);
                    if self
                        .pedigree
                        .family_id
                        .as_deref()
                        .is_none_or(|id| !candidates.iter().any(|family| family.id == id))
                    {
                        self.pedigree.family_id =
                            candidates.first().map(|family| family.id.clone());
                    }
                    let filtered =
                        family_tree(&self.tree, root, self.pedigree.family_id.as_deref());
                    self.scene = layout::build_scene(&filtered, surname_first);
                }
                _ => self.scene = super::loading::empty_scene(),
            },
            pedigree::GraphMode::Descendants => match self.pedigree.descendant_root_id.as_deref() {
                Some(root) if self.tree.people.iter().any(|person| person.id == root) => {
                    let filtered = descendant_tree(
                        &self.tree,
                        root,
                        self.pedigree.descendant_generations.levels(),
                    );
                    self.scene = layout::build_scene(&filtered, surname_first);
                }
                _ => self.scene = super::loading::empty_scene(),
            },
        }
    }

    /// Scales and centres the whole loaded scene inside `viewport_rect`
    /// once per reload, so even a very wide tree opens fully visible
    /// instead of showing a small slice at 1:1.
    pub(super) fn fit_scene(&mut self, viewport_rect: Rect) {
        self.viewport.fit_to(self.scene.bounds, viewport_rect);
    }

    pub(super) fn canvas(&mut self, ui: &mut Ui) {
        let selected = self.app.selected_person_id().map(str::to_string);
        let surname_first = self.tree_settings.surname_first;
        let person_label = |id: &str| {
            self.tree
                .people
                .iter()
                .find(|person| person.id == id)
                .map(|person| inspector::person_label(person, surname_first))
        };
        let pedigree_root_label = self.pedigree.root_id.as_deref().and_then(person_label);
        let family_root_label = self
            .pedigree
            .family_root_id
            .as_deref()
            .and_then(person_label);
        let descendant_root_label = self
            .pedigree
            .descendant_root_id
            .as_deref()
            .and_then(person_label);
        let family_options: Vec<(String, String)> = self
            .pedigree
            .family_root_id
            .as_deref()
            .map(|root| {
                candidate_families(&self.tree, root)
                    .into_iter()
                    .map(|family| {
                        let label = if family.husband_id.as_deref() == Some(root)
                            || family.wife_id.as_deref() == Some(root)
                        {
                            let other = if family.husband_id.as_deref() == Some(root) {
                                family.wife_id.as_deref()
                            } else {
                                family.husband_id.as_deref()
                            };
                            other
                                .and_then(person_label)
                                .map(|name| crate::i18n::tr_args("family.with", &[("name", name)]))
                                .unwrap_or_else(|| crate::i18n::tr("family.family"))
                        } else {
                            let parents = [family.husband_id.as_deref(), family.wife_id.as_deref()]
                                .into_iter()
                                .flatten()
                                .filter_map(person_label)
                                .collect::<Vec<_>>();
                            crate::i18n::tr_args(
                                "family.parents",
                                &[("names", parents.join(" & "))],
                            )
                        };
                        (family.id.clone(), label)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if pedigree::show_toolbar(
            ui,
            &mut self.pedigree,
            selected.as_deref(),
            pedigree_root_label.as_deref(),
            family_root_label.as_deref(),
            &family_options,
            descendant_root_label.as_deref(),
        ) {
            self.rebuild_scene_layout();
            self.pending_framing = true;
        }
        ui.separator();
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), Sense::click_and_drag());
        let viewport_rect = response.rect;
        if viewport_rect.width() <= 0.0 || viewport_rect.height() <= 0.0 {
            return;
        }

        if self.pending_framing {
            self.fit_scene(viewport_rect);
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

        let hit = response.interact_pointer_pos().and_then(|pos| {
            let canvas_pos = self.viewport.screen_to_canvas(pos, viewport_rect);
            hit_test(&self.scene, canvas_pos).map(|node| node.person_id.clone())
        });
        match canvas_click_action(
            response.double_clicked(),
            response.clicked(),
            hit.as_deref(),
        ) {
            Some(CanvasClick::Edit(person_id)) => {
                self.app.set_selected_person(Some(person_id.clone()));
                self.open_person_editor(&person_id);
            }
            Some(CanvasClick::Select(person_id)) => self.app.set_selected_person(person_id),
            None => {}
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
            let hint = match self.pedigree.mode {
                pedigree::GraphMode::Pedigree => crate::i18n::tr("pedigree.no_root"),
                pedigree::GraphMode::Family => crate::i18n::tr("family.no_root"),
                pedigree::GraphMode::Descendants => crate::i18n::tr("descendants.no_root"),
                pedigree::GraphMode::All => HINT_TEXT.to_string(),
            };
            painter.text(
                viewport_rect.center(),
                Align2::CENTER_CENTER,
                hint,
                FontId::proportional(16.0),
                HINT_COLOR,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_edits_a_hit_node_and_suppresses_single_selection() {
        assert_eq!(
            canvas_click_action(true, true, Some("p1")),
            Some(CanvasClick::Edit("p1".to_string()))
        );
        assert_eq!(canvas_click_action(true, false, None), None);
    }

    #[test]
    fn single_click_still_selects_or_clears() {
        assert_eq!(
            canvas_click_action(false, true, Some("p1")),
            Some(CanvasClick::Select(Some("p1".to_string())))
        );
        assert_eq!(
            canvas_click_action(false, true, None),
            Some(CanvasClick::Select(None))
        );
        assert_eq!(canvas_click_action(false, false, Some("p1")), None);
    }
}
