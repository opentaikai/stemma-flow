//! Ranked duplicate suggestions shared by the add-person dialogs.

use egui::{RichText, Ui};

use crate::db::FamilyTree;
use crate::i18n::{tr, tr_args};
use crate::model::duplicates::{DuplicateMatch, DuplicateQuery};

use super::connections::person_label;

/// Cached matches, recomputed only when the buffered query changes.
#[derive(Debug, Clone, Default)]
pub struct DuplicateSuggestions {
    computed: Option<DuplicateQuery>,
    matches: Vec<DuplicateMatch>,
}

impl DuplicateSuggestions {
    /// Recomputes matches when `query` differs from the last pass.
    pub fn update(
        &mut self,
        query: &DuplicateQuery,
        compute: impl FnOnce() -> Vec<DuplicateMatch>,
    ) {
        if !query.has_name() {
            self.computed = None;
            self.matches.clear();
            return;
        }
        if self.computed.as_ref() != Some(query) {
            self.matches = compute();
            self.computed = Some(query.clone());
        }
    }

    pub fn matches(&self) -> &[DuplicateMatch] {
        &self.matches
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Draws the suggestion list and returns the person chosen for reuse.
pub fn show(
    ui: &mut Ui,
    matches: &[DuplicateMatch],
    query_has_name: bool,
    tree: &FamilyTree,
    surname_first: bool,
    action_label: &str,
) -> Option<String> {
    if !query_has_name {
        return None;
    }
    if matches.is_empty() {
        ui.label(RichText::new(tr("duplicates.none")).weak().small());
        return None;
    }
    ui.label(RichText::new(tr("duplicates.heading")).strong());
    let mut selected = None;
    for matched in matches {
        let Some(person) = tree
            .people
            .iter()
            .find(|person| person.id == matched.person_id)
        else {
            continue;
        };
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(6)
            .corner_radius(4)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(person_label(person, surname_first)).strong());
                    ui.label(
                        RichText::new(tr_args(
                            "duplicates.score",
                            &[("score", matched.score.to_string())],
                        ))
                        .weak()
                        .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(action_label).clicked() {
                            selected = Some(matched.person_id.clone());
                        }
                    });
                });
                if !matched.reasons.is_empty() {
                    let reasons = matched
                        .reasons
                        .iter()
                        .map(|reason| reason.label())
                        .collect::<Vec<_>>()
                        .join(" · ");
                    ui.label(RichText::new(reasons).weak().small());
                }
            });
    }
    selected
}
