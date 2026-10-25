//! The add-relative pop-up: link an existing person or create a new one.

use std::collections::HashSet;

use crate::db::people::ParentRole;
use crate::db::{FamilyTree, Person};

use super::connections::person_label;
use super::form::InspectorForm;
use super::panel::field_editors;

/// How many search results are listed before asking for a narrower query.
const MAX_CANDIDATES: usize = 100;

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
    /// Text filtering the existing-person picker.
    pub search: String,
    /// Person highlighted in the picker, ready to link.
    pub selected: Option<String>,
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
            target_label: target_label.to_string(),
            kind,
            role,
            details,
            search: String::new(),
            selected: None,
        }
    }
}

/// Result of one pass over the relation pop-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationOutcome {
    /// Create the buffered person and link them.
    Submit,
    Cancel,
    /// Link the person with this id instead of creating one.
    Link(String),
}

/// People who can fill `kind`'s role for `target`, filtered by `query`.
pub(super) fn relation_candidates<'a>(
    tree: &'a FamilyTree,
    target: &str,
    kind: RelationKind,
    role: ParentRole,
    query: &str,
) -> Vec<&'a Person> {
    let excluded = existing_relatives(tree, target, kind);
    let needle = query.trim().to_lowercase();
    let mut matches: Vec<&Person> = tree
        .people
        .iter()
        .filter(|person| person.id != target)
        .filter(|person| !excluded.contains(person.id.as_str()))
        .filter(|person| role_compatible(kind, role, person.gender.as_str()))
        .filter(|person| needle.is_empty() || person_label(person).to_lowercase().contains(&needle))
        .collect();
    matches.sort_by_key(|person| person_label(person).to_lowercase());
    matches
}

/// Ids of people already filling `kind`'s role for `target`.
fn existing_relatives(tree: &FamilyTree, target: &str, kind: RelationKind) -> HashSet<String> {
    let mut ids = HashSet::new();
    for family in &tree.families {
        let target_is_spouse = family.husband_id.as_deref() == Some(target)
            || family.wife_id.as_deref() == Some(target);
        let target_is_child = tree
            .child_links
            .iter()
            .any(|(family_id, child_id)| family_id == &family.id && child_id == target);
        match kind {
            RelationKind::Parent if target_is_child => {
                for parent in [family.husband_id.as_deref(), family.wife_id.as_deref()]
                    .into_iter()
                    .flatten()
                {
                    ids.insert(parent.to_string());
                }
            }
            RelationKind::Spouse if target_is_spouse => {
                for spouse in [family.husband_id.as_deref(), family.wife_id.as_deref()]
                    .into_iter()
                    .flatten()
                {
                    if spouse != target {
                        ids.insert(spouse.to_string());
                    }
                }
            }
            RelationKind::Child if target_is_spouse => {
                for (family_id, child_id) in &tree.child_links {
                    if family_id == &family.id {
                        ids.insert(child_id.clone());
                    }
                }
            }
            _ => {}
        }
    }
    ids
}

fn role_compatible(kind: RelationKind, role: ParentRole, gender: &str) -> bool {
    if kind != RelationKind::Parent {
        return true;
    }
    !matches!(
        (role, gender),
        (ParentRole::Father, "F") | (ParentRole::Mother, "M")
    )
}

/// Shows the fixed-position relation window; `None` keeps it open.
pub fn relation_window(
    ctx: &egui::Context,
    form: &mut RelationForm,
    tree: &FamilyTree,
) -> Option<RelationOutcome> {
    let mut outcome = None;
    let mut open = true;
    egui::Window::new(form.kind.title(&form.target_label))
        .id(egui::Id::new("relation_form_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
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

            let candidates =
                relation_candidates(tree, &form.target, form.kind, form.role, &form.search);
            let still_listed = form
                .selected
                .as_ref()
                .is_some_and(|id| candidates.iter().any(|person| &person.id == id));
            if !still_listed {
                form.selected = None;
            }

            ui.label(egui::RichText::new("Link an existing person").strong());
            ui.add(
                egui::TextEdit::singleline(&mut form.search).hint_text("Search by name\u{2026}"),
            );
            egui::ScrollArea::vertical()
                .id_salt("relation_candidates")
                .max_height(150.0)
                .show(ui, |ui| {
                    if candidates.is_empty() {
                        ui.label(egui::RichText::new("(no matches)").weak());
                    }
                    for person in candidates.iter().take(MAX_CANDIDATES) {
                        let selected = form.selected.as_deref() == Some(person.id.as_str());
                        if ui
                            .selectable_label(selected, person_label(person))
                            .clicked()
                        {
                            form.selected = Some(person.id.clone());
                        }
                    }
                });
            if candidates.len() > MAX_CANDIDATES {
                ui.label(
                    egui::RichText::new(format!(
                        "{} matches \u{2014} refine the search",
                        candidates.len()
                    ))
                    .weak()
                    .small(),
                );
            }
            if ui
                .add_enabled(form.selected.is_some(), egui::Button::new("Link"))
                .clicked()
                && let Some(person_id) = form.selected.clone()
            {
                outcome = Some(RelationOutcome::Link(person_id));
            }

            ui.separator();
            egui::CollapsingHeader::new("Create a new person")
                .default_open(false)
                .show(ui, |ui| {
                    field_editors(ui, &mut form.details);
                    ui.separator();
                    let can_add = form.details.has_name() && form.details.dates_valid();
                    if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                        outcome = Some(RelationOutcome::Submit);
                    }
                });

            ui.separator();
            if ui.button("Cancel").clicked() {
                outcome = Some(RelationOutcome::Cancel);
            }
        });
    if !open {
        return Some(RelationOutcome::Cancel);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Family, Person};

    fn person(id: &str, given: &str, surname: &str, gender: &str) -> Person {
        let mut person = Person::new(given, surname, gender);
        person.id = id.to_string();
        person
    }

    fn family(id: &str, husband: Option<&str>, wife: Option<&str>) -> Family {
        Family {
            id: id.to_string(),
            husband_id: husband.map(str::to_string),
            wife_id: wife.map(str::to_string),
            created_at: String::new(),
        }
    }

    fn tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                person("target", "Target", "Person", "U"),
                person("father", "Frank", "Person", "M"),
                person("mother", "Mary", "Person", "F"),
                person("spouse", "Sarah", "Person", "F"),
                person("child", "Chris", "Person", "U"),
                person("stranger", "Zoe", "Stranger", "M"),
                person("female", "Yara", "Stranger", "F"),
            ],
            families: vec![
                family("f1", Some("father"), Some("mother")),
                family("f2", Some("target"), Some("spouse")),
            ],
            child_links: vec![
                ("f1".to_string(), "target".to_string()),
                ("f2".to_string(), "child".to_string()),
            ],
        }
    }

    fn candidate_ids(people: &[&Person]) -> Vec<String> {
        people.iter().map(|person| person.id.clone()).collect()
    }

    #[test]
    fn parent_candidates_exclude_existing_parents_and_filter_by_role() {
        let tree = tree();
        let father = relation_candidates(
            &tree,
            "target",
            RelationKind::Parent,
            ParentRole::Father,
            "",
        );
        let ids = candidate_ids(&father);
        assert!(ids.contains(&"stranger".to_string()));
        for excluded in ["target", "father", "mother", "spouse", "female"] {
            assert!(!ids.contains(&excluded.to_string()), "{excluded} listed");
        }

        let mother = relation_candidates(
            &tree,
            "target",
            RelationKind::Parent,
            ParentRole::Mother,
            "",
        );
        let ids = candidate_ids(&mother);
        assert!(ids.contains(&"female".to_string()));
        assert!(!ids.contains(&"stranger".to_string()));
    }

    #[test]
    fn search_is_case_insensitive_and_existing_roles_are_excluded() {
        let tree = tree();
        let hits = relation_candidates(
            &tree,
            "target",
            RelationKind::Child,
            ParentRole::Father,
            "zoe stranger",
        );
        assert_eq!(candidate_ids(&hits), vec!["stranger".to_string()]);

        let children =
            relation_candidates(&tree, "target", RelationKind::Child, ParentRole::Father, "");
        assert!(!candidate_ids(&children).contains(&"child".to_string()));

        let spouses = relation_candidates(
            &tree,
            "target",
            RelationKind::Spouse,
            ParentRole::Father,
            "",
        );
        assert!(!candidate_ids(&spouses).contains(&"spouse".to_string()));
    }
}
