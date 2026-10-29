//! Canvas scene geometry: person boxes, family unions and bounds.

use egui::{Pos2, Rect};

use crate::db::Person;

use super::NAME_MAX_CHARS;

/// A person node positioned in canvas space.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeGeom {
    pub person_id: String,
    pub display_name: String,
    pub bounds: Rect,
}

/// One family's union point and the anchors leading to each child.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionGeom {
    pub family_id: String,
    /// Horizontal bar between the spouses, when both are placed.
    pub spouse_line: Option<[Pos2; 2]>,
    /// Where the drop line to the children starts.
    pub union: Pos2,
    /// Row of the branch line: half a rank gap below the top of the
    /// couple's rank, so the trunk never touches a node box.
    pub trunk_y: f32,
    /// Top of the parent generation row, used to identify multi-rank edges.
    pub parent_rank_y: f32,
    /// `(child_id, top-centre anchor of the child node)`, rowid order.
    pub children: Vec<(String, Pos2)>,
}

/// Pre-computed scene consumed by the paint loop.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeScene {
    pub nodes: Vec<NodeGeom>,
    pub unions: Vec<UnionGeom>,
    /// Canvas-space bounding box of every node, for initial framing.
    pub bounds: Rect,
}

/// Shortens `name` to at most `max_chars` characters with an ellipsis.
pub fn truncate_name(name: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if name.chars().count() <= max_chars {
        return name.to_string();
    }
    let prefix: String = name.chars().take(max_chars - 1).collect();
    let mut out = prefix.trim_end().to_string();
    out.push('…');
    out
}

pub(super) fn person_display_name(person: &Person, surname_first: bool) -> String {
    let full = crate::model::name::format_name(
        &person.name_prefix,
        &person.given_name,
        &person.middle_name,
        &person.surname,
        &person.name_suffix,
        surname_first,
    );
    truncate_name(full.trim(), NAME_MAX_CHARS)
}
