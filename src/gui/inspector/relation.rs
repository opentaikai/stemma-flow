//! The add-relative pop-up and its form.

use crate::db::people::ParentRole;

use super::form::InspectorForm;
use super::panel::field_editors;

/// Which relative the relation pop-up is creating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Parent,
    Spouse,
    Child,
}

impl RelationKind {
    fn title(self, target: &str) -> String {
        match self {
            Self::Parent => format!("Add Parent for {target}"),
            Self::Spouse => format!("Add Spouse for {target}"),
            Self::Child => format!("Add Child for {target}"),
        }
    }
}

/// Buffered state of the add-relative pop-up.
#[derive(Debug, Clone)]
pub struct RelationForm {
    pub target: String,
    pub target_label: String,
    pub kind: RelationKind,
    pub role: ParentRole,
    pub details: InspectorForm,
}

impl RelationForm {
    /// Opens against `target`'s label; parent forms preset the father role.
    pub fn new(target: &str, kind: RelationKind, target_label: &str) -> Self {
        let role = ParentRole::Father;
        let mut details = InspectorForm::default();
        if kind == RelationKind::Parent {
            details.gender = role.gender().to_string();
        }
        Self {
            target: target.to_string(),
            kind,
            role,
            details,
            target_label: target_label.to_string(),
        }
    }
}

/// Result of one pass over the relation pop-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationOutcome {
    Submit,
    Cancel,
}

/// Shows the fixed-position add-relative window; `None` keeps it open.
pub fn relation_window(ctx: &egui::Context, form: &mut RelationForm) -> Option<RelationOutcome> {
    let mut outcome = None;
    let mut open = true;
    egui::Window::new(form.kind.title(&form.target_label))
        .id(egui::Id::new("relation_form_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            if form.kind == RelationKind::Parent {
                ui.horizontal(|ui| {
                    ui.label("Role:");
                    if ui
                        .selectable_value(&mut form.role, ParentRole::Father, "Father")
                        .clicked()
                    {
                        form.details.gender = ParentRole::Father.gender().to_string();
                    }
                    if ui
                        .selectable_value(&mut form.role, ParentRole::Mother, "Mother")
                        .clicked()
                    {
                        form.details.gender = ParentRole::Mother.gender().to_string();
                    }
                });
                ui.separator();
            }
            field_editors(ui, &mut form.details);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    outcome = Some(RelationOutcome::Cancel);
                }
                if ui.button("Add").clicked() {
                    outcome = Some(RelationOutcome::Submit);
                }
            });
        });
    if !open {
        return Some(RelationOutcome::Cancel);
    }
    outcome
}
