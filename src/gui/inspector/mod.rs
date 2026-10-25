//! Right-hand person inspector: profile editing plus relational actions.

mod connections;
mod form;
mod panel;
mod relation;

pub use connections::{Connections, Relative, connections, person_label};
pub use form::InspectorForm;
pub use panel::{field_editors, show};
pub use relation::{RelationForm, RelationKind, RelationOutcome, relation_window};

use egui::Color32;

use crate::db::people::PersonDetails;

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
    /// Close the inspector by clearing the selection.
    Close,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::people::{ParentRole, VitalDates};
    use crate::db::{FamilyTree, MarriageInfo, Person};
    use egui::{Context, RawInput};
    use std::collections::HashMap;

    fn person_with_dates() -> (Person, HashMap<String, VitalDates>) {
        let mut person = Person::new("Ada", "King", "F");
        person.middle_name = "Lynn".to_string();
        person.nickname = "Addy".to_string();
        person.notes = "Nearest relation of the mayor.".to_string();
        let mut dates = HashMap::new();
        dates.insert(
            person.id.clone(),
            VitalDates {
                birth: Some("1815".to_string()),
                birth_place: Some("London".to_string()),
                death: Some("1852".to_string()),
                death_place: Some("Cuckfield".to_string()),
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
        assert_eq!(form.birth_place, "London");
        assert_eq!(form.death, "1852");
        assert_eq!(form.death_place, "Cuckfield");
        assert_eq!(form.notes, "Nearest relation of the mayor.");

        let mut edited = form.clone();
        edited.given_name = "Augusta".to_string();
        edited.birth = "  ".to_string();
        edited.birth_place = " ".to_string();
        edited.notes = "  Edited.  ".to_string();
        let details = edited.to_details();
        assert_eq!(details.given_name, "Augusta");
        assert_eq!(details.middle_name, "Lynn", "middle name survives saves");
        assert_eq!(details.nickname, "Addy", "nickname survives saves");
        assert_eq!(details.birth_date, None, "blank dates clear the event");
        assert_eq!(details.birth_place, None, "blank places clear the event");
        assert_eq!(details.death_date.as_deref(), Some("1852"));
        assert_eq!(details.death_place.as_deref(), Some("Cuckfield"));
        assert_eq!(details.notes, "Edited.", "notes are trimmed");
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
