//! Static named groups editor.

use egui::{RichText, Ui};

use crate::db::Person;
use crate::db::groups::{GroupDraft, GroupMember, GroupSummary};
use crate::gui::person_criteria::PersonCriteriaFields;
use crate::gui::watchlist::PersonRow;
use crate::model::person_search::PersonSearchCriteria;

/// A group buffered for editing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupEdit {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub rule_mode: bool,
    pub rule: PersonCriteriaFields,
}

impl GroupEdit {
    pub fn from_record(record: &crate::db::groups::GroupRecord) -> Self {
        let mut rule = PersonCriteriaFields::default();
        if let Some(criteria) = &record.rule {
            rule.load(criteria);
        }
        Self {
            id: Some(record.id.clone()),
            name: record.name.clone(),
            description: record.description.clone().unwrap_or_default(),
            rule_mode: record.rule.is_some(),
            rule,
        }
    }

    pub fn to_draft(&self, rule: Option<PersonSearchCriteria>) -> GroupDraft {
        GroupDraft {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            rule,
        }
    }
}

/// Members matching a dynamic group rule in the current People index.
pub fn rule_members(rows: &[PersonRow], criteria: &PersonSearchCriteria) -> Vec<GroupMember> {
    let mut members: Vec<GroupMember> = rows
        .iter()
        .filter(|row| row.search_document().matches(criteria))
        .map(|row| GroupMember {
            person_id: row.id.clone(),
            name: row.display_name.clone(),
        })
        .collect();
    members.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.person_id.cmp(&right.person_id))
    });
    members
}

/// What the group view asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupEvent {
    New,
    Select(String),
    Save(Box<GroupDraft>),
    Delete(String),
    AddMember(String),
    RemoveMember(String),
    Print,
}

/// State owned by the window while the Groups sub-tab is visible.
#[derive(Debug, Default)]
pub struct GroupsState {
    pub groups: Vec<GroupSummary>,
    pub selected: Option<String>,
    pub draft: GroupEdit,
    pub members: Vec<GroupMember>,
    pub filter: String,
    pub search: String,
    pub editing: bool,
    pub error: Option<String>,
    pub loaded: bool,
    pub rule_members: Vec<GroupMember>,
    pub rule_signature: Option<PersonSearchCriteria>,
}

/// Draws the group list and editor.
pub fn show(
    ui: &mut Ui,
    state: &mut GroupsState,
    people: &[Person],
    surname_first: bool,
    rows: &[PersonRow],
) -> Option<GroupEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text("Group name…"));
        if ui
            .add_enabled(!state.filter.is_empty(), egui::Button::new("Clear"))
            .clicked()
        {
            state.filter.clear();
        }
        if ui
            .add(egui::Button::new(RichText::new("+ New Group").strong()))
            .clicked()
        {
            event = Some(GroupEvent::New);
        }
    });
    ui.separator();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(300.0);
            let needle = state.filter.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("group_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if state.groups.is_empty() {
                        ui.label(RichText::new("No groups yet.").weak().small());
                    }
                    for group in &state.groups {
                        if !needle.is_empty() && !group.name.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let selected = state.selected.as_deref() == Some(group.id.as_str());
                        let label = format!("{} ({} people)", group.name, group.member_count);
                        if ui.selectable_label(selected, label).clicked() {
                            event = Some(GroupEvent::Select(group.id.clone()));
                        }
                    }
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical()
                .id_salt("group_editor")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    group_editor(ui, state, people, surname_first, rows, &mut event)
                });
        });
    });
    event
}

fn group_editor(
    ui: &mut Ui,
    state: &mut GroupsState,
    people: &[Person],
    surname_first: bool,
    rows: &[PersonRow],
    event: &mut Option<GroupEvent>,
) {
    if !state.editing {
        ui.label(RichText::new("Select a group or create a new one.").weak());
        return;
    }
    ui.heading(if state.draft.id.is_some() {
        "Edit group"
    } else {
        "New group"
    });
    egui::Grid::new("group_fields")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Name *").strong());
            ui.text_edit_singleline(&mut state.draft.name);
            ui.end_row();

            ui.label("Description:");
            ui.add(
                egui::TextEdit::multiline(&mut state.draft.description)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.end_row();
        });
    ui.horizontal(|ui| {
        ui.radio_value(&mut state.draft.rule_mode, false, "Manual members");
        ui.radio_value(&mut state.draft.rule_mode, true, "Rule-based members");
    });

    let mut rule = None;
    let mut rule_problem = None;
    if state.draft.rule_mode {
        state.draft.rule.show(ui, "group_rule");
        match state.draft.rule.criteria() {
            Some(criteria) if !criteria.is_empty() => {
                if state.rule_signature.as_ref() != Some(&criteria) {
                    state.rule_members = rule_members(rows, &criteria);
                    state.rule_signature = Some(criteria.clone());
                }
                rule = Some(criteria);
            }
            Some(_) => {
                state.rule_members.clear();
                state.rule_signature = None;
                rule_problem = Some("Add at least one criterion to this rule.");
            }
            None => {
                state.rule_members.clear();
                state.rule_signature = None;
                rule_problem = Some("Fix the rule criteria before saving.");
            }
        }
    } else {
        state.rule_members.clear();
        state.rule_signature = None;
    }

    if let Some(problem) = rule_problem {
        ui.label(
            RichText::new(problem)
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
    if let Some(error) = &state.error {
        ui.label(
            RichText::new(error)
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !state.draft.name.trim().is_empty() && (!state.draft.rule_mode || rule.is_some()),
                egui::Button::new(RichText::new("Save").strong()),
            )
            .clicked()
        {
            *event = Some(GroupEvent::Save(Box::new(
                state.draft.to_draft(rule.clone()),
            )));
        }
        if let Some(id) = state.draft.id.clone()
            && ui.button("Delete").clicked()
        {
            *event = Some(GroupEvent::Delete(id));
        }
        if state.draft.id.is_some()
            && ui
                .add_enabled(
                    !state.draft.rule_mode || rule.is_some(),
                    egui::Button::new("Print group…"),
                )
                .clicked()
        {
            *event = Some(GroupEvent::Print);
        }
    });

    if state.draft.id.is_none() {
        return;
    }
    ui.separator();
    if state.draft.rule_mode {
        ui.label(RichText::new(format!("Rule matches ({})", state.rule_members.len())).strong());
        if state.rule_members.is_empty() {
            ui.label(RichText::new("(no matches)").weak().small());
        }
        for member in &state.rule_members {
            ui.label(RichText::new(&member.name).strong());
        }
        return;
    }

    ui.label(RichText::new(format!("Members ({})", state.members.len())).strong());
    if state.members.is_empty() {
        ui.label(RichText::new("(none)").weak().small());
    }
    for member in &state.members {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&member.name).strong());
            if ui.small_button("Remove").clicked() {
                *event = Some(GroupEvent::RemoveMember(member.person_id.clone()));
            }
        });
    }
    ui.separator();
    ui.label(RichText::new("Add person").strong());
    ui.add(egui::TextEdit::singleline(&mut state.search).hint_text("Search people…"));
    let needle = state.search.trim().to_lowercase();
    let choices: Vec<&Person> = people
        .iter()
        .filter(|person| {
            !state
                .members
                .iter()
                .any(|member| member.person_id == person.id)
        })
        .filter(|person| {
            needle.is_empty()
                || crate::gui::inspector::person_label(person, surname_first)
                    .to_lowercase()
                    .contains(&needle)
        })
        .take(12)
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("group_people")
        .max_height(150.0)
        .show(ui, |ui| {
            if choices.is_empty() {
                ui.label(RichText::new("(no matches)").weak().small());
            }
            for person in choices {
                ui.horizontal(|ui| {
                    ui.label(crate::gui::inspector::person_label(person, surname_first));
                    if ui.small_button("Add").clicked() {
                        *event = Some(GroupEvent::AddMember(person.id.clone()));
                    }
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Person;
    use egui::{Context, RawInput};

    fn state() -> GroupsState {
        GroupsState {
            groups: vec![GroupSummary {
                id: "g1".to_string(),
                name: "Friends".to_string(),
                description: None,
                rule: None,
                member_count: 1,
            }],
            selected: Some("g1".to_string()),
            draft: GroupEdit {
                id: Some("g1".to_string()),
                name: "Friends".to_string(),
                description: String::new(),
                rule_mode: false,
                rule: PersonCriteriaFields::default(),
            },
            members: vec![GroupMember {
                person_id: "p1".to_string(),
                name: "Ada King".to_string(),
            }],
            editing: true,
            ..GroupsState::default()
        }
    }

    #[test]
    fn rule_members_match_the_people_index() {
        use crate::db::FamilyTree;
        use crate::model::person_search::PersonSearchCriteria;
        use std::collections::HashMap;

        let mut tree = FamilyTree::default();
        tree.people.push(Person::new("Ada", "King", "F"));
        tree.people.push(Person::new("Charles", "Babbage", "M"));
        let rows = crate::gui::watchlist::rows(
            &tree,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            None,
            false,
        );
        let members = rule_members(
            &rows,
            &PersonSearchCriteria {
                name: "ada".to_string(),
                ..PersonSearchCriteria::default()
            },
        );
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "Ada King");
    }

    #[test]
    fn group_view_renders_headless() {
        let ctx = Context::default();
        let mut state = state();
        let people = vec![
            Person::new("Ada", "King", "F"),
            Person::new("Charles", "Babbage", "M"),
        ];
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state, &people, false, &[]);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the group view paints");
    }
}
