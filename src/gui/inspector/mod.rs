//! Right-hand person inspector: profile editing plus relational actions.

mod connections;
mod form;
mod panel;
mod participants;
mod relation;

pub use connections::{
    Connections, FamilyLink, Relative, connections, person_label, reverse_marriage_places,
};
pub use form::InspectorForm;
pub use panel::{field_editors, show};
pub use participants::{ParticipantForm, ParticipantOutcome, show as participant_window};
pub use relation::{RelationForm, RelationKind, RelationOutcome, relation_window};

use egui::Color32;

use crate::db::citations::CitationEdit;
use crate::db::media::MediaOwner;
use crate::db::names::AlternateNameDraft;
use crate::db::notes::NoteOwner;
use crate::db::people::{ParentRole, PersonDetails};
use crate::db::tasks::TaskOwner;

const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);

/// What the user asked the shell to do from the inspector panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorAction {
    Save(Box<PersonDetails>),
    AddParentAs(ParentRole),
    AddSpouse,
    AddChild,
    /// Open the pinned modeless person editor.
    OpenEditor,
    /// Switch to the Groups pane in the Research tab.
    OpenGroups,
    /// Switch the pinned editor to another person.
    SwitchEditor(String),
    /// Select `person_id` and re-centre the graph on it.
    Focus(String),
    /// Close the inspector by clearing the selection.
    Close,
    /// Save buffered citation edits.
    SaveCitations(Box<Vec<CitationEdit>>),
    /// Attach a source to a person or family event.
    AddCitation {
        source_id: String,
        target: String,
        page: Option<String>,
    },
    /// Attach an existing citation to another event.
    AttachCitation {
        citation_id: String,
        target: String,
    },
    /// Remove one citation-to-event link.
    DetachCitation {
        citation_id: String,
        event_id: String,
    },
    /// Remove a citation and all of its links.
    DeleteCitation(String),
    /// Load the reusable citations of a source.
    LoadCitationChoices(String),
    /// Open the witness editor for an event.
    EditParticipants(String),
    /// Mark the person private.
    SetPersonPrivate(bool),
    /// Override living status.
    SetLivingOverride(Option<bool>),
    /// Mark an event private.
    SetEventPrivate {
        event_id: String,
        private: bool,
    },
    /// Open the new-association dialog.
    AddAssociation,
    /// Edit the label of an existing association.
    EditAssociation(String),
    /// Delete one association.
    DeleteAssociation(String),
    /// Open the task attachment dialog for one record.
    ManageTasks {
        owner: TaskOwner,
        title: String,
    },
    /// Open the media attachment dialog for one record.
    ManageMedia {
        owner: MediaOwner,
        title: String,
    },
    /// Open the shared notes dialog for one record.
    OpenNotes {
        owner: NoteOwner,
        title: String,
    },
    /// Save an alternate name.
    SaveAlternateName(Box<AlternateNameDraft>),
    /// Delete an alternate name.
    DeleteAlternateName(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::facts::PersonFact;
    use crate::db::people::{ParentRole, VitalDates};
    use crate::db::privacy::{EventPrivacy, PersonPrivacy};
    use crate::db::{FamilyTree, MarriageInfo, Person};
    use crate::gui::citations::CitationsState;
    use egui::{Context, RawInput};
    use std::collections::HashMap;

    fn person_with_dates() -> (Person, HashMap<String, VitalDates>) {
        let mut person = Person::new("Ada", "King", "F");
        person.middle_name = "Lynn".to_string();
        person.nickname = "Addy".to_string();
        let mut dates = HashMap::new();
        dates.insert(
            person.id.clone(),
            VitalDates {
                birth: Some("1815".to_string()),
                birth_place: Some("London".to_string()),
                birth_event_id: Some("e-birth".to_string()),
                death: Some("1852".to_string()),
                death_place: Some("Cuckfield".to_string()),
                death_event_id: Some("e-death".to_string()),
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

        let mut edited = form.clone();
        edited.given_name = "Augusta".to_string();
        edited.birth = "  ".to_string();
        edited.birth_place = " ".to_string();
        let details = edited.to_details();
        assert_eq!(details.given_name, "Augusta");
        assert_eq!(details.middle_name, "Lynn", "middle name survives saves");
        assert_eq!(details.nickname, "Addy", "nickname survives saves");
        assert_eq!(details.birth_date, None, "blank dates clear the event");
        assert_eq!(details.birth_place, None, "blank places clear the event");
        assert_eq!(details.death_date.as_deref(), Some("1852"));
        assert_eq!(details.death_place.as_deref(), Some("Cuckfield"));
        assert_eq!(details.display_name(), "Augusta Lynn King");
    }

    #[test]
    fn date_validation_flags_bad_input_and_accepts_gedcom_or_iso() {
        let mut form = InspectorForm::default();
        assert!(form.dates_valid(), "blank dates are valid");

        form.birth = "12 jan 1850".to_string();
        form.death = "1852-03-04".to_string();
        assert!(form.dates_valid());
        assert!(form.date_errors().birth.is_none());

        form.death = "31 FEB 1852".to_string();
        assert!(!form.dates_valid());
        let errors = form.date_errors();
        assert!(errors.death.is_some(), "the death field carries the error");
        assert!(errors.birth.is_none());
    }

    #[test]
    fn a_person_needs_a_first_or_last_name() {
        let mut form = InspectorForm::default();
        assert!(!form.has_name(), "a blank form has no name");

        form.given_name = "Ada".to_string();
        assert!(form.has_name(), "a given name is enough");

        form.given_name.clear();
        form.surname = "King".to_string();
        assert!(form.has_name(), "a surname is enough");

        form.surname = "   ".to_string();
        assert!(!form.has_name(), "whitespace alone does not count");
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
                event_id: Some("e-marriage".to_string()),
                date: Some("1910".to_string()),
                place: Some("Uppsala".to_string()),
            },
        );

        let found = connections(&tree, &child.id, &marriages, false);
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
        let mut captured = Some(InspectorAction::AddParentAs(ParentRole::Father));
        let facts = [PersonFact {
            event_id: "e1".to_string(),
            event_type: "OCCUPATION".to_string(),
            label: "Occupation".to_string(),
            date: Some("1870".to_string()),
            place: Some("Uppsala".to_string()),
            value: Some("Teacher".to_string()),
            role: None,
            principal: None,
            principal_id: None,
            participants: Vec::new(),
        }];
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            captured = show(
                ui,
                Some(&person),
                &mut form,
                None,
                Connections::default(),
                &[],
                &[],
                &mut CitationsState::default(),
                &facts,
                None,
                &mut crate::gui::names::NamesState::default(),
                false,
                false,
            );
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the panel paints");
        assert!(captured.is_none(), "no pointer input triggers nothing");

        let mut relation = RelationForm::new(&person.id, RelationKind::Parent, "Ada King");
        let tree = FamilyTree {
            people: vec![person.clone()],
            families: Vec::new(),
            child_links: Vec::new(),
        };
        let mut outcome = Some(RelationOutcome::Submit);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            outcome = relation_window(ui.ctx(), &mut relation, &tree, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the pop-up paints");
        assert_eq!(outcome, None, "no input keeps the window open");
        assert_eq!(relation.role, ParentRole::Father);
        assert_eq!(relation.details.gender, "M", "father presets male");
    }

    #[test]
    fn privacy_card_renders_headless() {
        let (person, dates) = person_with_dates();
        let ctx = Context::default();
        let mut form = InspectorForm::from_person(&person, &dates);
        let privacy = PersonPrivacy {
            private: true,
            living_override: Some(true),
            auto_living: false,
            effective_living: true,
            events: vec![EventPrivacy {
                event_id: "e1".to_string(),
                event_type: "BIRTH".to_string(),
                label: "Birth".to_string(),
                date: Some("1815".to_string()),
                private: false,
                family_event: false,
            }],
        };
        let mut captured = Some(InspectorAction::Close);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            captured = show(
                ui,
                Some(&person),
                &mut form,
                None,
                Connections::default(),
                &[],
                &[],
                &mut CitationsState::default(),
                &[],
                Some(&privacy),
                &mut crate::gui::names::NamesState::default(),
                false,
                false,
            );
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the privacy card paints");
        assert!(captured.is_none(), "no input emits no action");
    }

    #[test]
    fn reverses_marriage_places_for_display() {
        let mut connections = Connections {
            spouses: vec![Relative {
                id: "p2".to_string(),
                label: "Otto King".to_string(),
                marriage: Some(MarriageInfo {
                    event_id: Some("e-marriage".to_string()),
                    date: None,
                    place: Some("Uppsala, Sweden".to_string()),
                }),
            }],
            ..Connections::default()
        };

        reverse_marriage_places(&mut connections);

        assert_eq!(
            connections.spouses[0]
                .marriage
                .as_ref()
                .and_then(|marriage| marriage.place.as_deref()),
            Some("Sweden, Uppsala")
        );
    }
}
