//! Modal for adding witnesses and shared-event participants.

use egui::{Context, RichText, Window};

use crate::db::FamilyTree;
use crate::db::facts::FactTypeRole;
use crate::db::participants::{EventContext, EventParticipant, ParticipantDraft};
use crate::gui::inspector::person_label;

/// Editable state for the participant window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantForm {
    pub event_id: String,
    pub event_type: String,
    pub event_label: String,
    pub date: Option<String>,
    pub place: Option<String>,
    pub principal: Option<String>,
    pub roles: Vec<FactTypeRole>,
    pub participants: Vec<EventParticipant>,
    pub excluded: Vec<String>,
    pub search: String,
    pub selected_person: Option<String>,
    pub selected_role: Option<String>,
    pub error: Option<String>,
}

impl ParticipantForm {
    pub fn from_context(context: EventContext) -> Self {
        let selected_role = context.roles.first().map(|role| role.role_key.clone());
        Self {
            event_id: context.event_id,
            event_type: context.event_type,
            event_label: context.event_label,
            date: context.date,
            place: context.place,
            principal: context.principal_name,
            roles: context.roles,
            participants: context.participants,
            excluded: context.excluded_people,
            search: String::new(),
            selected_person: None,
            selected_role,
            error: None,
        }
    }

    pub fn to_drafts(&self) -> Vec<ParticipantDraft> {
        self.participants
            .iter()
            .map(|participant| ParticipantDraft {
                person_id: participant.person_id.clone(),
                role_key: participant.role_key.clone(),
            })
            .collect()
    }
}

/// What the participant window asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParticipantOutcome {
    Cancel,
    Save {
        event_id: String,
        participants: Vec<ParticipantDraft>,
    },
}

/// Renders the participant window, returning an outcome when it closes.
pub fn show(
    ctx: &Context,
    form: &mut ParticipantForm,
    tree: &FamilyTree,
) -> Option<ParticipantOutcome> {
    let mut open = true;
    let mut outcome = None;
    Window::new("Witnesses")
        .id(egui::Id::new("event_participants_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(440.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.heading(&form.event_label);
            let details = [form.date.as_deref(), form.place.as_deref()]
                .into_iter()
                .flatten()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" \u{00b7} ");
            if !details.is_empty() {
                ui.label(RichText::new(details).weak().small());
            }
            if let Some(principal) = &form.principal {
                ui.label(RichText::new(format!("Principal: {principal}")).small());
            }
            ui.separator();

            if form.participants.is_empty() {
                ui.label(RichText::new("No witnesses yet.").weak().small());
            }
            let mut remove = None;
            for (index, participant) in form.participants.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&participant.person_name).strong());
                    ui.label(RichText::new(&participant.role_label).weak().small());
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
            }
            if let Some(index) = remove {
                form.participants.remove(index);
            }

            ui.separator();
            if form.roles.is_empty() {
                ui.label(
                    RichText::new(
                        "Add roles for this fact type in Fact Types before adding witnesses.",
                    )
                    .color(ui.visuals().warn_fg_color)
                    .small(),
                );
            } else {
                ui.label(RichText::new("Add person").strong());
                ui.add(egui::TextEdit::singleline(&mut form.search).hint_text("Search people…"));
                let candidates = candidate_people(form, tree);
                if candidates.is_empty() {
                    ui.label(RichText::new("No matching people.").weak().small());
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("participant_people")
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for person in candidates.iter().take(12) {
                                let selected =
                                    form.selected_person.as_deref() == Some(person.id.as_str());
                                if ui
                                    .selectable_label(selected, person_label(person))
                                    .clicked()
                                {
                                    form.selected_person = Some(person.id.clone());
                                }
                            }
                        });
                }
                ui.horizontal(|ui| {
                    let current_role = form
                        .selected_role
                        .as_deref()
                        .and_then(|key| form.roles.iter().find(|role| role.role_key == key))
                        .map(|role| role.label.as_str())
                        .unwrap_or("Select a role");
                    egui::ComboBox::from_id_salt("participant_role")
                        .selected_text(current_role)
                        .show_ui(ui, |ui| {
                            for role in &form.roles {
                                if ui
                                    .selectable_label(
                                        form.selected_role.as_deref()
                                            == Some(role.role_key.as_str()),
                                        &role.label,
                                    )
                                    .clicked()
                                {
                                    form.selected_role = Some(role.role_key.clone());
                                }
                            }
                        });
                    let enabled = form.selected_person.is_some() && form.selected_role.is_some();
                    if ui.add_enabled(enabled, egui::Button::new("Add")).clicked() {
                        add_selected(form, tree);
                    }
                });
            }

            if let Some(error) = &form.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Save").strong()))
                    .clicked()
                {
                    outcome = Some(ParticipantOutcome::Save {
                        event_id: form.event_id.clone(),
                        participants: form.to_drafts(),
                    });
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(ParticipantOutcome::Cancel);
                }
            });
        });
    if !open {
        return Some(ParticipantOutcome::Cancel);
    }
    outcome
}

fn candidate_people<'a>(
    form: &ParticipantForm,
    tree: &'a FamilyTree,
) -> Vec<&'a crate::db::Person> {
    let needle = form.search.trim().to_lowercase();
    tree.people
        .iter()
        .filter(|person| {
            !form.excluded.iter().any(|id| id == &person.id)
                && !form
                    .participants
                    .iter()
                    .any(|participant| participant.person_id == person.id)
                && (needle.is_empty() || person_label(person).to_lowercase().contains(&needle))
        })
        .collect()
}

fn add_selected(form: &mut ParticipantForm, tree: &FamilyTree) {
    let Some(person_id) = form.selected_person.clone() else {
        return;
    };
    let Some(role_key) = form.selected_role.clone() else {
        return;
    };
    let Some(role) = form.roles.iter().find(|role| role.role_key == role_key) else {
        return;
    };
    let Some(person) = tree.people.iter().find(|person| person.id == person_id) else {
        return;
    };
    if form.excluded.iter().any(|id| id == &person_id)
        || form
            .participants
            .iter()
            .any(|participant| participant.person_id == person_id)
    {
        return;
    }
    form.participants.push(EventParticipant {
        person_id,
        person_name: person_label(person),
        role_label: role.label.clone(),
        role_key,
        sort_order: form.participants.len() as i64,
    });
    form.selected_person = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Person;
    use egui::RawInput;

    fn context() -> EventContext {
        EventContext {
            event_id: "e1".to_string(),
            event_type: "BIRTH".to_string(),
            event_label: "Birth".to_string(),
            date: Some("1850".to_string()),
            place: Some("Uppsala".to_string()),
            principal_id: Some("p1".to_string()),
            principal_name: Some("Ada King".to_string()),
            family_id: None,
            excluded_people: vec!["p1".to_string()],
            roles: vec![FactTypeRole {
                role_key: "GODPARENT".to_string(),
                label: "Godparent".to_string(),
                sort_order: 0,
            }],
            participants: Vec::new(),
        }
    }

    fn tree() -> FamilyTree {
        FamilyTree {
            people: vec![
                Person::new("Ada", "King", "F"),
                Person::new("Bo", "King", "M"),
            ],
            families: Vec::new(),
            child_links: Vec::new(),
        }
    }

    #[test]
    fn adds_and_removes_participants() {
        let tree = tree();
        let mut form = ParticipantForm::from_context(context());
        form.selected_person = Some(tree.people[1].id.clone());
        form.selected_role = Some("GODPARENT".to_string());

        add_selected(&mut form, &tree);

        assert_eq!(form.participants.len(), 1);
        assert_eq!(form.participants[0].person_name, "Bo King");
        assert_eq!(form.to_drafts()[0].role_key, "GODPARENT");
        form.participants.clear();
        assert!(form.to_drafts().is_empty());
    }

    #[test]
    fn excludes_the_principal_and_existing_participants() {
        let tree = tree();
        let mut form = ParticipantForm::from_context(context());
        form.excluded = vec![tree.people[0].id.clone()];
        form.selected_person = Some(tree.people[0].id.clone());
        add_selected(&mut form, &tree);
        assert!(form.participants.is_empty(), "the principal is excluded");
        let candidates = candidate_people(&form, &tree);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].given_name, "Bo");
    }

    #[test]
    fn participant_window_renders_headless() {
        let ctx = Context::default();
        let tree = tree();
        let mut form = ParticipantForm::from_context(context());
        let mut outcome = Some(ParticipantOutcome::Cancel);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            outcome = show(ui.ctx(), &mut form, &tree);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the window paints");
        assert!(outcome.is_none());
    }
}
