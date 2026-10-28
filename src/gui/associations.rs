//! Association editor dialog for the person inspector.

use egui::{Context, RichText, Window};

use crate::db::FamilyTree;
use crate::db::associations::{AssociationDraft, AssociationRecord};
use crate::gui::inspector::person_label;

const MAX_CANDIDATES: usize = 12;

/// Preset relationship labels offered by the editor.
pub const RELATIONSHIP_PRESETS: [&str; 5] =
    ["Friend", "Associate", "Neighbour", "Godparent", "Colleague"];

/// What the association dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssociationOutcome {
    Save(AssociationDraft),
    Cancel,
}

/// Buffered association editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationForm {
    pub id: Option<String>,
    pub person_id: String,
    pub editing_label: Option<String>,
    pub search: String,
    pub selected: Option<String>,
    pub relationship: String,
    pub error: Option<String>,
}

impl AssociationForm {
    pub fn new(person_id: &str) -> Self {
        Self {
            id: None,
            person_id: person_id.to_string(),
            editing_label: None,
            search: String::new(),
            selected: None,
            relationship: "Associate".to_string(),
            error: None,
        }
    }

    pub fn from_record(record: &AssociationRecord) -> Self {
        Self {
            id: Some(record.id.clone()),
            person_id: record.person_id.clone(),
            editing_label: Some(record.associate_name.clone()),
            search: String::new(),
            selected: Some(record.associate_id.clone()),
            relationship: record.relationship.clone(),
            error: None,
        }
    }

    pub fn to_draft(&self) -> AssociationDraft {
        AssociationDraft {
            id: self.id.clone(),
            person_id: self.person_id.clone(),
            associate_id: self.selected.clone().unwrap_or_default(),
            relationship: self.relationship.clone(),
        }
    }
}

/// Shows the association window; `None` keeps it open.
pub fn show(
    ctx: &Context,
    form: &mut AssociationForm,
    tree: &FamilyTree,
    surname_first: bool,
) -> Option<AssociationOutcome> {
    let title = match &form.editing_label {
        Some(name) => format!("Edit association — {name}"),
        None => "Add association".to_string(),
    };
    let mut open = true;
    let mut outcome = None;
    Window::new(title)
        .id(egui::Id::new("association_form_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new("Relationship").strong());
            ui.add(
                egui::TextEdit::singleline(&mut form.relationship)
                    .hint_text("Friend, Associate, Neighbour…"),
            );
            ui.horizontal_wrapped(|ui| {
                for preset in RELATIONSHIP_PRESETS {
                    if ui.small_button(preset).clicked() {
                        form.relationship = preset.to_string();
                    }
                }
            });
            ui.separator();

            if form.editing_label.is_none() {
                ui.label(RichText::new("Associate").strong());
                let candidates = candidates(form, tree, surname_first);
                let still_listed = form
                    .selected
                    .as_ref()
                    .is_some_and(|id| candidates.iter().any(|person| &person.id == id));
                if !still_listed {
                    form.selected = None;
                }
                ui.add(egui::TextEdit::singleline(&mut form.search).hint_text("Search by name…"));
                egui::ScrollArea::vertical()
                    .id_salt("association_candidates")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        if candidates.is_empty() {
                            ui.label(RichText::new("(no matches)").weak());
                        }
                        for person in candidates.iter().take(MAX_CANDIDATES) {
                            let selected = form.selected.as_deref() == Some(person.id.as_str());
                            if ui
                                .selectable_label(selected, person_label(person, surname_first))
                                .clicked()
                            {
                                form.selected = Some(person.id.clone());
                            }
                        }
                    });
                if candidates.len() > MAX_CANDIDATES {
                    ui.label(
                        RichText::new(format!("{} matches — refine the search", candidates.len()))
                            .weak()
                            .small(),
                    );
                }
            } else if let Some(id) = form.selected.clone()
                && let Some(person) = tree.people.iter().find(|person| person.id == id)
            {
                ui.label(RichText::new("Associate").strong());
                ui.label(person_label(person, surname_first));
            }

            if let Some(error) = &form.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
            ui.separator();
            let can_save = form.selected.is_some() && !form.relationship.trim().is_empty();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can_save, egui::Button::new(RichText::new("Save").strong()))
                    .clicked()
                {
                    outcome = Some(AssociationOutcome::Save(form.to_draft()));
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(AssociationOutcome::Cancel);
                }
            });
        });
    if !open {
        return Some(AssociationOutcome::Cancel);
    }
    outcome
}

fn candidates<'a>(
    form: &AssociationForm,
    tree: &'a FamilyTree,
    surname_first: bool,
) -> Vec<&'a crate::db::Person> {
    let needle = form.search.trim().to_lowercase();
    tree.people
        .iter()
        .filter(|person| person.id != form.person_id)
        .filter(|person| {
            needle.is_empty()
                || person_label(person, surname_first)
                    .to_lowercase()
                    .contains(&needle)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{FamilyTree, Person};
    use egui::RawInput;

    fn tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                Person::new("Ada", "King", "F"),
                Person::new("Charles", "Babbage", "M"),
            ],
            families: Vec::new(),
            child_links: Vec::new(),
        }
    }

    #[test]
    fn form_buffers_existing_associations() {
        let record = AssociationRecord {
            id: "a1".to_string(),
            person_id: "p1".to_string(),
            associate_id: "p2".to_string(),
            associate_name: "Charles Babbage".to_string(),
            relationship: "Friend".to_string(),
        };
        let form = AssociationForm::from_record(&record);
        let draft = form.to_draft();
        assert_eq!(draft.id.as_deref(), Some("a1"));
        assert_eq!(draft.associate_id, "p2");
        assert_eq!(draft.relationship, "Friend");
    }

    #[test]
    fn dialog_renders_headless() {
        let ctx = Context::default();
        let mut form = AssociationForm::new("p1");
        let tree = tree();
        let mut outcome = Some(AssociationOutcome::Cancel);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            outcome = show(ui.ctx(), &mut form, &tree, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the dialog paints");
        assert!(outcome.is_none());
    }
}
