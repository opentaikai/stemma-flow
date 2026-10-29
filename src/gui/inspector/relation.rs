//! The add-relative pop-up: link an existing person or create a new one.

use std::collections::{HashMap, HashSet};

use crate::db::people::{ParentRole, VitalDates};
use crate::db::{FamilyTree, Person};
use crate::i18n::{tr, tr_args};
use crate::model::duplicates::{DuplicateQuery, score_candidates};

use super::connections::person_label;
use super::duplicates::{self, DuplicateSuggestions};
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
    fn title(self, target: &str, role: ParentRole) -> String {
        let key = match (self, role) {
            (Self::Parent, ParentRole::Father) => "relation.father_title",
            (Self::Parent, ParentRole::Mother) => "relation.mother_title",
            (Self::Spouse, _) => "relation.spouse_title",
            (Self::Child, _) => "relation.child_title",
        };
        tr_args(key, &[("name", target.to_string())])
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
    /// Possible duplicates while typing a new person.
    pub suggestions: DuplicateSuggestions,
}

impl RelationForm {
    /// Opens against `target`'s label; parent forms preset the father role.
    pub fn new(target: &str, kind: RelationKind, target_label: &str) -> Self {
        Self::new_with_role(target, kind, target_label, ParentRole::Father)
    }

    pub fn new_with_role(
        target: &str,
        kind: RelationKind,
        target_label: &str,
        role: ParentRole,
    ) -> Self {
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
            suggestions: DuplicateSuggestions::default(),
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
    surname_first: bool,
) -> Vec<&'a Person> {
    let excluded = existing_relatives(tree, target, kind);
    let needle = query.trim().to_lowercase();
    let mut matches: Vec<&Person> = tree
        .people
        .iter()
        .filter(|person| person.id != target)
        .filter(|person| !excluded.contains(person.id.as_str()))
        .filter(|person| role_compatible(kind, role, person.gender.as_str()))
        .filter(|person| {
            needle.is_empty()
                || person_label(person, surname_first)
                    .to_lowercase()
                    .contains(&needle)
        })
        .collect();
    matches.sort_by_key(|person| person_label(person, surname_first).to_lowercase());
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
    dates: &HashMap<String, VitalDates>,
    surname_first: bool,
) -> Option<RelationOutcome> {
    let mut outcome = None;
    let mut open = true;
    egui::Window::new(form.kind.title(&form.target_label, form.role))
        .id(egui::Id::new("relation_form_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
        .open(&mut open)
        .show(ctx, |ui| {
            if form.kind == RelationKind::Parent {
                ui.horizontal(|ui| {
                    ui.label(tr("relation.role"));
                    if ui
                        .selectable_value(&mut form.role, ParentRole::Father, tr("relation.father"))
                        .clicked()
                    {
                        form.details.gender = ParentRole::Father.gender().to_string();
                    }
                    if ui
                        .selectable_value(&mut form.role, ParentRole::Mother, tr("relation.mother"))
                        .clicked()
                    {
                        form.details.gender = ParentRole::Mother.gender().to_string();
                    }
                });
                ui.separator();
            }

            let candidates = relation_candidates(
                tree,
                &form.target,
                form.kind,
                form.role,
                &form.search,
                surname_first,
            );
            let still_listed = form
                .selected
                .as_ref()
                .is_some_and(|id| candidates.iter().any(|person| &person.id == id));
            if !still_listed {
                form.selected = None;
            }

            ui.label(egui::RichText::new(tr("relation.link_existing")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut form.search).hint_text(tr("relation.search_hint")),
            );
            egui::ScrollArea::vertical()
                .id_salt("relation_candidates")
                .max_height(150.0)
                .show(ui, |ui| {
                    if candidates.is_empty() {
                        ui.label(egui::RichText::new(tr("relation.no_matches")).weak());
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
                    egui::RichText::new(tr_args(
                        "relation.matches_refine",
                        &[("count", candidates.len().to_string())],
                    ))
                    .weak()
                    .small(),
                );
            }
            if ui
                .add_enabled(
                    form.selected.is_some(),
                    egui::Button::new(tr("relation.link")),
                )
                .clicked()
                && let Some(person_id) = form.selected.clone()
            {
                outcome = Some(RelationOutcome::Link(person_id));
            }

            ui.separator();
            egui::CollapsingHeader::new(tr("relation.create_new"))
                .default_open(false)
                .show(ui, |ui| {
                    field_editors(ui, &mut form.details);
                    let query = DuplicateQuery::from_details(&form.details.to_details());
                    let candidates = relation_candidates(
                        tree,
                        &form.target,
                        form.kind,
                        form.role,
                        "",
                        surname_first,
                    );
                    form.suggestions.update(&query, || {
                        score_candidates(&query, dates, candidates.iter().copied())
                    });
                    if let Some(person_id) = duplicates::show(
                        ui,
                        form.suggestions.matches(),
                        query.has_name(),
                        tree,
                        surname_first,
                        &tr("duplicates.link_existing"),
                    ) {
                        outcome = Some(RelationOutcome::Link(person_id));
                    }
                    ui.separator();
                    let can_add = form.details.has_name() && form.details.dates_valid();
                    if ui
                        .add_enabled(can_add, egui::Button::new(tr("common.add")))
                        .clicked()
                    {
                        outcome = Some(RelationOutcome::Submit);
                    }
                });

            ui.separator();
            if ui.button(tr("common.cancel")).clicked() {
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
    fn role_specific_form_presets_the_parent_gender() {
        let father = RelationForm::new_with_role(
            "target",
            RelationKind::Parent,
            "Target",
            ParentRole::Father,
        );
        assert_eq!(father.role, ParentRole::Father);
        assert_eq!(father.details.gender, "M");
        let mother = RelationForm::new_with_role(
            "target",
            RelationKind::Parent,
            "Target",
            ParentRole::Mother,
        );
        assert_eq!(mother.role, ParentRole::Mother);
        assert_eq!(mother.details.gender, "F");
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
            false,
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
            false,
        );
        let ids = candidate_ids(&mother);
        assert!(ids.contains(&"female".to_string()));
        assert!(!ids.contains(&"stranger".to_string()));
    }

    #[test]
    fn new_relative_suggestions_exclude_existing_relatives() {
        let mut tree = tree();
        tree.people
            .push(person("duplicate", "Frank", "Person", "M"));
        let form = RelationForm::new("target", RelationKind::Parent, "Target");
        let mut details = form.details.clone();
        details.given_name = "Frank".to_string();
        details.surname = "Person".to_string();
        let query = DuplicateQuery::from_details(&details.to_details());
        let candidates = relation_candidates(
            &tree,
            "target",
            RelationKind::Parent,
            ParentRole::Father,
            "",
            false,
        );
        let matches = score_candidates(&query, &HashMap::new(), candidates.iter().copied());
        assert_eq!(
            matches
                .iter()
                .map(|matched| matched.person_id.as_str())
                .collect::<Vec<_>>(),
            vec!["duplicate"]
        );
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
            false,
        );
        assert_eq!(candidate_ids(&hits), vec!["stranger".to_string()]);

        let children = relation_candidates(
            &tree,
            "target",
            RelationKind::Child,
            ParentRole::Father,
            "",
            false,
        );
        assert!(!candidate_ids(&children).contains(&"child".to_string()));

        let spouses = relation_candidates(
            &tree,
            "target",
            RelationKind::Spouse,
            ParentRole::Father,
            "",
            false,
        );
        assert!(!candidate_ids(&spouses).contains(&"spouse".to_string()));
    }
}
