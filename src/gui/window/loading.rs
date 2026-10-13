//! Validating and loading trees, plus small canvas helpers.

use std::path::Path;

use egui::{Pos2, Rect, Vec2, vec2};

use crate::db;
use crate::gui::layout::TreeScene;

pub(super) fn empty_scene() -> TreeScene {
    TreeScene {
        nodes: Vec::new(),
        unions: Vec::new(),
        bounds: Rect::from_min_size(Pos2::ZERO, vec2(0.0, 0.0)),
    }
}

/// Why the file at `path` cannot be opened as a family tree, or `None`
/// when it loads. Opening runs the schema migration (`db::init_db`)
/// first so databases from older builds keep working; the upgrade is
/// best effort, so an already-current schema still loads from a
/// read-only file. Only files that already carry a `people` table are
/// considered — an unrelated SQLite file must neither open nor get our
/// schema created inside it.
pub(super) fn load_error(path: &Path) -> Option<String> {
    let conn = match db::open_connection(path) {
        Ok(conn) => conn,
        Err(error) => return Some(error.to_string()),
    };
    let is_tree = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'people'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !is_tree {
        return Some("it has no people table".to_string());
    }
    let _ = db::init_db(&conn);
    db::load_family_tree(&conn)
        .err()
        .map(|error| error.to_string())
}

/// True when `path` is a readable database carrying the stemma-flow schema.
pub(super) fn loadable(path: &Path) -> bool {
    load_error(path).is_none()
}

/// Pan that places `target` at the centre of the viewport (inverse of
/// [`ViewportState::canvas_to_screen`]).
pub(super) fn pan_centering(target: Pos2, zoom: f32) -> Vec2 {
    Vec2::new(-target.x * zoom, -target.y * zoom)
}
