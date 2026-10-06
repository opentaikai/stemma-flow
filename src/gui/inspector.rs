//! Right-hand person inspector: profile editing plus relational actions.

use std::collections::HashMap;

use egui::{Color32, RichText, Ui};

use crate::db::people::{ParentRole, PersonDetails, VitalDates};
use crate::db::{FamilyTree, MarriageInfo, Person};

const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);

/// What the user asked the shell to do from the inspector panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorAction {
    Save(Box<PersonDetails>),
    AddParent,
    AddSpouse,
    AddChild,
    /// Select `person_id` and re-centre the graph on it.
    Focus(String),
}

/// Buffered profile fields, one snapshot per selected person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorForm {
    pub given_name: String,
    pub middle_name: String,
    pub surname: String,
    pub nickname: String,
    pub gender: String,
    pub birth: String,
    pub death: String,
}

impl Default for InspectorForm {
    fn default() -> Self {
        Self {
            given_name: String::new(),
            middle_name: String::new(),
            surname: String::new(),
            nickname: String::new(),
            gender: "U".to_string(),
            birth: String::new(),
            death: String::new(),
        }
    }
}

impl InspectorForm {
    /// Buffers the stored person plus its vital dates.
    pub fn from_person(person: &Person, dates: &HashMap<String, VitalDates>) -> Self {
        let vitals = dates.get(&person.id);
        Self {
            given_name: person.given_name.clone(),
            middle_name: person.middle_name.clone(),
            surname: person.surname.clone(),
            nickname: person.nickname.clone(),
            gender: person.gender.clone(),
            birth: vitals
                .and_then(|vitals| vitals.birth.clone())
                .unwrap_or_default(),
            death: vitals
                .and_then(|vitals| vitals.death.clone())
                .unwrap_or_default(),
        }
    }

    /// Converts the buffer into write-ready details; blank dates clear.
    pub fn to_details(&self) -> PersonDetails {
        PersonDetails::new(
            self.given_name.clone(),
            self.middle_name.clone(),
            self.surname.clone(),
            self.nickname.clone(),
            self.gender.clone(),
            blank_to_none(&self.birth),
            blank_to_none(&self.death),
        )
    }
}

fn blank_to_none(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// One relative listed in the inspector; `id` drives click-to-focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relative {
    pub id: String,
    pub label: String,
    /// Marriage date and place, set on spouse rows when known.
    pub marriage: Option<MarriageInfo>,
}

/// The selected person's existing family ties, ready to render.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Connections {
    pub parents: Vec<Relative>,
    pub siblings: Vec<Relative>,
    pub spouses: Vec<Relative>,
    pub children: Vec<Relative>,
}

/// Lists parents, siblings, spouses and children straight off the loaded
/// tree, sorted by display label. Siblings cover every family that
/// contains one of the parents, so remarriage half-siblings appear too.
/// `marriages` supplies the details shown next to spouse names.
pub fn connections(
    tree: &FamilyTree,
    person_id: &str,
    marriages: &HashMap<String, MarriageInfo>,
) -> Connections {
    let mut parent_ids: Vec<String> = Vec::new();
    for (family_id, child_id) in &tree.child_links {
        if child_id != person_id {
            continue;
        }
        let Some(family) = tree.families.iter().find(|family| &family.id == family_id) else {
            continue;
        };
        for slot in [&family.husband_id, &family.wife_id] {
            if let Some(parent_id) = slot
                && parent_id != person_id
                && !parent_ids.contains(parent_id)
            {
                parent_ids.push(parent_id.clone());
            }
        }
    }

    let mut sibling_ids: Vec<String> = Vec::new();
    for parent_id in &parent_ids {
        let their_families = tree.families.iter().filter(|family| {
            family.husband_id.as_ref() == Some(parent_id)
                || family.wife_id.as_ref() == Some(parent_id)
        });
        for family in their_families {
            for (family_id, child_id) in &tree.child_links {
                if family_id == &family.id
                    && child_id != person_id
                    && !sibling_ids.contains(child_id)
                {
                    sibling_ids.push(child_id.clone());
                }
            }
        }
    }

    let mut spouse_ids: Vec<(String, Option<MarriageInfo>)> = Vec::new();
    let mut child_ids: Vec<String> = Vec::new();
    for family in &tree.families {
        let is_spouse = family.husband_id.as_deref() == Some(person_id)
            || family.wife_id.as_deref() == Some(person_id);
        if !is_spouse {
            continue;
        }
        let other = if family.husband_id.as_deref() == Some(person_id) {
            family.wife_id.as_deref()
        } else {
            family.husband_id.as_deref()
        };
        if let Some(spouse_id) = other
            && spouse_id != person_id
            && !spouse_ids.iter().any(|(id, _)| id == spouse_id)
        {
            spouse_ids.push((spouse_id.to_string(), marriages.get(&family.id).cloned()));
        }
        for (family_id, child_id) in &tree.child_links {
            if family_id == &family.id && child_id != person_id && !child_ids.contains(child_id) {
                child_ids.push(child_id.clone());
            }
        }
    }

    Connections {
        parents: by_label(relatives(tree, parent_ids)),
        siblings: by_label(relatives(tree, sibling_ids)),
        spouses: by_label(
            spouse_ids
                .into_iter()
                .filter_map(|(id, marriage)| relative(tree, &id, marriage))
                .collect(),
        ),
        children: by_label(relatives(tree, child_ids)),
    }
}

/// Builds a labelled relative, skipping ids with no stored person.
fn relative(tree: &FamilyTree, id: &str, marriage: Option<MarriageInfo>) -> Option<Relative> {
    let person = tree.people.iter().find(|person| person.id == id)?;
    Some(Relative {
        id: person.id.clone(),
        label: person_label(person),
        marriage,
    })
}

fn relatives(tree: &FamilyTree, ids: Vec<String>) -> Vec<Relative> {
    ids.iter()
        .filter_map(|id| relative(tree, id, None))
        .collect()
}

fn by_label(mut relatives: Vec<Relative>) -> Vec<Relative> {
    relatives.sort_by(|left, right| {
        left.label
            .to_lowercase()
            .cmp(&right.label.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    relatives
}

/// Display label for a stored person; blank names fall back gracefully.
pub fn person_label(person: &Person) -> String {
    let combined = [
        person.given_name.as_str(),
        person.middle_name.as_str(),
        person.surname.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    if combined.is_empty() {
        "(unnamed)".to_string()
    } else {
        combined
    }
}

/// Renders the profile editors shared by the panel and relation pop-up.
pub fn field_editors(ui: &mut Ui, form: &mut InspectorForm) {
    egui::Grid::new("person_profile_fields")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("Given name:");
            ui.text_edit_singleline(&mut form.given_name);
            ui.end_row();

            ui.label("Middle name:");
            ui.text_edit_singleline(&mut form.middle_name);
            ui.end_row();

            ui.label("Surname:");
            ui.text_edit_singleline(&mut form.surname);
            ui.end_row();

            ui.label("Nickname:");
            ui.text_edit_singleline(&mut form.nickname);
            ui.end_row();

            ui.label("Gender:");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut form.gender, "M".to_string(), "Male");
                ui.selectable_value(&mut form.gender, "F".to_string(), "Female");
                ui.selectable_value(&mut form.gender, "U".to_string(), "Unknown");
            });
            ui.end_row();

            ui.label("Birth date:");
            ui.text_edit_singleline(&mut form.birth);
            ui.end_row();

            ui.label("Death date:");
            ui.text_edit_singleline(&mut form.death);
            ui.end_row();
        });
}

/// Draws the inspector body and returns the action to apply, if any.
pub fn show(
    ui: &mut Ui,
    person: Option<&Person>,
    form: &mut InspectorForm,
    connections: Connections,
) -> Option<InspectorAction> {
    let Some(person) = person else {
        ui.label(RichText::new("Selected person not found.").weak());
        return None;
    };

    ui.heading("Person Inspector");
    ui.label(RichText::new(person_label(person)).strong());
    ui.separator();

    ui.label(RichText::new("Profile").strong());
    field_editors(ui, form);

    ui.separator();
    ui.label(RichText::new("Relationships").strong());

    let mut action = None;
    for (title, relatives) in [
        ("Parents", &connections.parents),
        ("Siblings", &connections.siblings),
        ("Spouses", &connections.spouses),
        ("Children", &connections.children),
    ] {
        ui.label(RichText::new(title).weak().size(12.0));
        if relatives.is_empty() {
            ui.label(RichText::new("(none)").weak().size(12.0));
        }
        for relative in relatives {
            let mut text = relative.label.clone();
            if let Some(marriage) = &relative.marriage {
                let facts = [marriage.date.as_deref(), marriage.place.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" \u{00b7} ");
                if !facts.is_empty() {
                    text.push_str(&format!(" \u{00b7} {facts}"));
                }
            }
            let row = egui::Label::new(RichText::new(text).strong())
                .sense(egui::Sense::click())
                .truncate();
            if ui
                .add_sized([ui.available_width(), ui.spacing().interact_size.y], row)
                .clicked()
            {
                action = Some(InspectorAction::Focus(relative.id.clone()));
            }
        }
    }

    ui.horizontal(|ui| {
        if ui.button("+ Add Parent").clicked() {
            action = Some(InspectorAction::AddParent);
        }
        if ui.button("+ Add Spouse").clicked() {
            action = Some(InspectorAction::AddSpouse);
        }
        if ui.button("+ Add Child").clicked() {
            action = Some(InspectorAction::AddChild);
        }
    });

    ui.separator();
    if ui
        .add(egui::Button::new(RichText::new("Save Changes").strong()).fill(ACCENT))
        .clicked()
    {
        action = Some(InspectorAction::Save(Box::new(form.to_details())));
    }
    action
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn person_with_dates() -> (Person, HashMap<String, VitalDates>) {
        let mut person = Person::new("Ada", "King", "F");
        person.middle_name = "Lynn".to_string();
        person.nickname = "Addy".to_string();
        let mut dates = HashMap::new();
        dates.insert(
            person.id.clone(),
            VitalDates {
                birth: Some("1815".to_string()),
                death: Some("1852".to_string()),
                ..Default::default()
            },
        );
        (person, dates)
    }

    #[test]
    fn form_roundtrips_through_person_details() {
        let (person, dates) = person_with_dates();
        let form = InspectorForm::from_person(&person, &dates);
        assert_eq!(form.given_name, "Ada");
        assert_eq!(form.middle_name, "Lynn");
        assert_eq!(form.surname, "King");
        assert_eq!(form.nickname, "Addy");
        assert_eq!(form.gender, "F");
        assert_eq!(form.birth, "1815");
        assert_eq!(form.death, "1852");

        let mut edited = form.clone();
        edited.given_name = "Augusta".to_string();
        edited.birth = "  ".to_string();
        let details = edited.to_details();
        assert_eq!(details.given_name, "Augusta");
        assert_eq!(details.middle_name, "Lynn", "middle name survives saves");
        assert_eq!(details.nickname, "Addy", "nickname survives saves");
        assert_eq!(details.birth_date, None, "blank dates clear the event");
        assert_eq!(details.death_date.as_deref(), Some("1852"));
        assert_eq!(details.display_name(), "Augusta Lynn King");
    }

    #[test]
    fn default_form_gender_is_unknown() {
        assert_eq!(InspectorForm::default().gender, "U");
    }

    #[test]
    fn connections_list_relatives_sorted_by_label() {
        let mut tree = FamilyTree::default();
        let father = Person::new("Papa", "Root", "M");
        let mother = Person::new("Mama", "Root", "F");
        let child = Person::new("Kid", "Root", "U");
        let sibling = Person::new("Sib", "Root", "U");
        let stepmother = Person::new("Stella", "Root", "F");
        let half = Person::new("Haf", "Root", "U");
        let spouse = Person::new("Partner", "Spouse", "F");
        let grandchild = Person::new("Baby", "Spouse", "U");
        let childhood = crate::db::Family {
            id: "f-childhood".to_string(),
            husband_id: Some(father.id.clone()),
            wife_id: Some(mother.id.clone()),
            created_at: "t".to_string(),
        };
        let second = crate::db::Family {
            id: "f-second".to_string(),
            husband_id: Some(father.id.clone()),
            wife_id: Some(stepmother.id.clone()),
            created_at: "t".to_string(),
        };
        let own = crate::db::Family {
            id: "f-own".to_string(),
            husband_id: Some(child.id.clone()),
            wife_id: Some(spouse.id.clone()),
            created_at: "t".to_string(),
        };
        tree.people = vec![
            father,
            mother,
            child.clone(),
            sibling.clone(),
            stepmother,
            half.clone(),
            spouse,
            grandchild.clone(),
        ];
        tree.families = vec![childhood, second, own];
        tree.child_links = vec![
            ("f-childhood".to_string(), child.id.clone()),
            ("f-childhood".to_string(), sibling.id.clone()),
            ("f-second".to_string(), half.id.clone()),
            ("f-own".to_string(), grandchild.id.clone()),
        ];
        let mut marriages = HashMap::new();
        marriages.insert(
            "f-own".to_string(),
            MarriageInfo {
                date: Some("1910".to_string()),
                place: Some("Uppsala".to_string()),
            },
        );

        let found = connections(&tree, &child.id, &marriages);
        assert_eq!(
            found
                .parents
                .iter()
                .map(|relative| relative.label.as_str())
                .collect::<Vec<_>>(),
            ["Mama Root", "Papa Root"],
            "parents sort alphabetically"
        );
        assert_eq!(
            found
                .siblings
                .iter()
                .map(|relative| relative.label.as_str())
                .collect::<Vec<_>>(),
            ["Haf Root", "Sib Root"],
            "remarriage half-siblings join full siblings"
        );
        assert_eq!(
            found
                .children
                .iter()
                .map(|relative| relative.label.as_str())
                .collect::<Vec<_>>(),
            ["Baby Spouse"]
        );
        assert_eq!(found.spouses.len(), 1, "one spouse row");
        let spouse_row = found.spouses.first().expect("spouse row exists");
        assert_eq!(spouse_row.label, "Partner Spouse");
        let marriage = spouse_row.marriage.as_ref().expect("marriage details");
        assert_eq!(marriage.date.as_deref(), Some("1910"));
        assert_eq!(marriage.place.as_deref(), Some("Uppsala"));
    }

    #[test]
    fn inspector_and_relation_windows_render_headless() {
        let (person, dates) = person_with_dates();
        let ctx = Context::default();
        let mut form = InspectorForm::from_person(&person, &dates);
        let mut captured = Some(InspectorAction::AddParent);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            captured = show(ui, Some(&person), &mut form, Connections::default());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the panel paints");
        assert!(captured.is_none(), "no pointer input triggers nothing");

        let mut relation = RelationForm::new(&person.id, RelationKind::Parent, "Ada King");
        let mut outcome = Some(RelationOutcome::Submit);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            outcome = relation_window(ui.ctx(), &mut relation);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the pop-up paints");
        assert_eq!(outcome, None, "no input keeps the window open");
        assert_eq!(relation.role, ParentRole::Father);
        assert_eq!(relation.details.gender, "M", "father presets male");
    }
}
