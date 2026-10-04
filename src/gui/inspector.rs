//! Right-hand person inspector: profile editing plus relational actions.

use std::collections::HashMap;

use egui::{Color32, RichText, Ui};

use crate::db::people::{ParentRole, PersonDetails, VitalDates};
use crate::db::{FamilyTree, Person};

const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);

/// What the user asked the shell to do from the inspector panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorAction {
    Save(PersonDetails),
    AddParent,
    AddSpouse,
    AddChild,
}

/// Buffered profile fields, one snapshot per selected person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorForm {
    pub given_name: String,
    pub surname: String,
    pub gender: String,
    pub birth: String,
    pub death: String,
}

impl Default for InspectorForm {
    fn default() -> Self {
        Self {
            given_name: String::new(),
            surname: String::new(),
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
            surname: person.surname.clone(),
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
            self.surname.clone(),
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

/// Counts of the selected person's existing family ties.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Connections {
    pub parents: usize,
    pub spouses: usize,
    pub children: usize,
}

/// Counts parents, spouses and children straight off the loaded tree.
pub fn connections(tree: &FamilyTree, person_id: &str) -> Connections {
    let mut counts = Connections::default();
    for family in &tree.families {
        let is_spouse = family.husband_id.as_deref() == Some(person_id)
            || family.wife_id.as_deref() == Some(person_id);
        if is_spouse {
            counts.spouses += 1;
            counts.children += tree
                .child_links
                .iter()
                .filter(|(family_id, _)| family_id == &family.id)
                .count();
        }
    }
    for (family_id, child_id) in &tree.child_links {
        if child_id != person_id {
            continue;
        }
        let Some(family) = tree.families.iter().find(|family| &family.id == family_id) else {
            continue;
        };
        counts.parents += usize::from(family.husband_id.is_some());
        counts.parents += usize::from(family.wife_id.is_some());
    }
    counts
}

/// Display label for a stored person; blank names fall back gracefully.
pub fn person_label(person: &Person) -> String {
    let combined = format!("{} {}", person.given_name, person.surname);
    let combined = combined.trim();
    if combined.is_empty() {
        "(unnamed)".to_string()
    } else {
        combined.to_string()
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

            ui.label("Surname:");
            ui.text_edit_singleline(&mut form.surname);
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
    ui.label(RichText::new("Connections").strong());
    ui.label(
        RichText::new(format!(
            "{} parents \u{00b7} {} spouses \u{00b7} {} children",
            connections.parents, connections.spouses, connections.children
        ))
        .weak()
        .size(12.0),
    );

    let mut action = None;
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
        action = Some(InspectorAction::Save(form.to_details()));
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
        let person = Person::new("Ada", "King", "F");
        let mut dates = HashMap::new();
        dates.insert(
            person.id.clone(),
            VitalDates {
                birth: Some("1815".to_string()),
                death: Some("1852".to_string()),
            },
        );
        (person, dates)
    }

    #[test]
    fn form_roundtrips_through_person_details() {
        let (person, dates) = person_with_dates();
        let form = InspectorForm::from_person(&person, &dates);
        assert_eq!(form.given_name, "Ada");
        assert_eq!(form.surname, "King");
        assert_eq!(form.gender, "F");
        assert_eq!(form.birth, "1815");
        assert_eq!(form.death, "1852");

        let mut edited = form.clone();
        edited.given_name = "Augusta".to_string();
        edited.birth = "  ".to_string();
        let details = edited.to_details();
        assert_eq!(details.given_name, "Augusta");
        assert_eq!(details.birth_date, None, "blank dates clear the event");
        assert_eq!(details.death_date.as_deref(), Some("1852"));
        assert_eq!(details.display_name(), "Augusta King");
    }

    #[test]
    fn default_form_gender_is_unknown() {
        assert_eq!(InspectorForm::default().gender, "U");
    }

    #[test]
    fn connections_count_parents_spouses_and_children() {
        let mut tree = FamilyTree::default();
        let father = Person::new("Papa", "Root", "M");
        let mother = Person::new("Mama", "Root", "F");
        let child = Person::new("Kid", "Root", "U");
        let spouse = Person::new("Partner", "Spouse", "F");
        let grandchild = Person::new("Baby", "Spouse", "U");
        let couple = crate::db::Family {
            id: "f-childhood".to_string(),
            husband_id: Some(father.id.clone()),
            wife_id: Some(mother.id.clone()),
            created_at: "t".to_string(),
        };
        let own = crate::db::Family {
            id: "f-own".to_string(),
            husband_id: Some(child.id.clone()),
            wife_id: Some(spouse.id.clone()),
            created_at: "t".to_string(),
        };
        let expected = Connections {
            parents: 2,
            spouses: 1,
            children: 1,
        };
        tree.people = vec![father, mother, child.clone(), spouse, grandchild.clone()];
        tree.families = vec![couple, own];
        tree.child_links = vec![
            ("f-childhood".to_string(), child.id.clone()),
            ("f-own".to_string(), grandchild.id.clone()),
        ];

        assert_eq!(connections(&tree, &child.id), expected);
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
