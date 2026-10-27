//! Scene reload, framing and the interactive canvas.

use crate::db;
use crate::db::FamilyTree;
use crate::gui::connectors::paint_connectors;
use crate::gui::layout;
use crate::gui::nodes::hit_test;
use crate::gui::nodes::paint_nodes;
use crate::gui::theme;
use crate::gui::watchlist;
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
                Some((tree, dates, marriages))
            });
        let (tree, dates, marriages) =
            loaded.unwrap_or_else(|| (FamilyTree::default(), HashMap::new(), HashMap::new()));
        self.scene = layout::build_scene(&tree);
        self.rows = watchlist::rows(&tree, &dates);
        self.tree = tree;
        self.dates = dates;
        self.marriages = marriages;
        self.watch.mark_dirty();
        self.seen_db = self.app.active_db().map(Path::to_path_buf);
        let generation = self.app.import_generation();
        let generation_changed = self.seen_generation != generation;
        self.seen_generation = generation;
        if switched {
            self.places = crate::gui::places::PlacesState::default();
            self.sources = crate::gui::sources::SourcesState::default();
            self.citations = crate::gui::citations::CitationsState::default();
            self.repositories = crate::gui::repositories::RepositoriesState::default();
        } else if generation_changed {
            self.places.loaded = false;
            self.repositories.loaded = false;
        }
    }

    /// Scales and centres the whole loaded scene inside `viewport_rect`
    /// once per reload, so even a very wide tree opens fully visible
    /// instead of showing a small slice at 1:1.
    pub(super) fn fit_scene(&mut self, viewport_rect: Rect) {
        self.viewport.fit_to(self.scene.bounds, viewport_rect);
    }

    pub(super) fn canvas(&mut self, ui: &mut Ui) {
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
