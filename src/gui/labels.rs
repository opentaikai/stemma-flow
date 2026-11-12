//! Coloured person labels and their memberships.

use egui::{Color32, RichText, Ui};

use crate::db::Person;
use crate::db::labels::{LabelDraft, LabelMember, LabelRecord, LabelSummary};

/// A label buffered for editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelEdit {
    pub id: Option<String>,
    pub name: String,
    pub color: u32,
}

impl Default for LabelEdit {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            color: 0x3b82f6,
        }
    }
}

impl LabelEdit {
    pub fn from_record(record: &LabelRecord) -> Self {
        Self {
            id: Some(record.id.clone()),
            name: record.name.clone(),
            color: record.color,
        }
    }

    pub fn to_draft(&self) -> LabelDraft {
        LabelDraft {
            id: self.id.clone(),
            name: self.name.clone(),
            color: self.color,
        }
    }
}

/// What the labels view asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelEvent {
    New,
    Select(String),
    Save(Box<LabelDraft>),
    Delete(String),
    AddMember(String),
    RemoveMember(String),
}

/// State owned by the window while the Labels sub-tab is visible.
#[derive(Debug, Default)]
pub struct LabelsState {
    pub labels: Vec<LabelSummary>,
    pub selected: Option<String>,
    pub draft: LabelEdit,
    pub members: Vec<LabelMember>,
    pub filter: String,
    pub search: String,
    pub editing: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

/// Draws the label list and editor.
pub fn show(
    ui: &mut Ui,
    state: &mut LabelsState,
    people: &[Person],
    surname_first: bool,
) -> Option<LabelEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text("Label name…"));
        if ui
            .add_enabled(!state.filter.is_empty(), egui::Button::new("Clear"))
            .clicked()
        {
            state.filter.clear();
        }
        if ui
            .add(egui::Button::new(RichText::new("+ New Label").strong()))
            .clicked()
        {
            event = Some(LabelEvent::New);
        }
    });
    ui.separator();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(300.0);
            let needle = state.filter.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("label_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if state.labels.is_empty() {
                        ui.label(RichText::new("No labels yet.").weak().small());
                    }
                    for label in &state.labels {
                        if !needle.is_empty() && !label.name.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let selected = state.selected.as_deref() == Some(label.id.as_str());
                        let text = format!("{} ({} people)", label.name, label.member_count);
                        ui.horizontal(|ui| {
                            crate::gui::symbols::color_swatch(ui, color32(label.color));
                            if ui.selectable_label(selected, text).clicked() {
                                event = Some(LabelEvent::Select(label.id.clone()));
                            }
                        });
                    }
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical()
                .id_salt("label_editor")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    label_editor(ui, state, people, surname_first, &mut event)
                });
        });
    });
    event
}

fn label_editor(
    ui: &mut Ui,
    state: &mut LabelsState,
    people: &[Person],
    surname_first: bool,
    event: &mut Option<LabelEvent>,
) {
    if !state.editing {
        ui.label(RichText::new("Select a label or create a new one.").weak());
        return;
    }
    ui.heading(if state.draft.id.is_some() {
        "Edit label"
    } else {
        "New label"
    });
    egui::Grid::new("label_fields")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Name *").strong());
            ui.text_edit_singleline(&mut state.draft.name);
            ui.end_row();

            ui.label("Colour:");
            let mut color = color32(state.draft.color);
            if ui.color_edit_button_srgba(&mut color).changed() {
                state.draft.color = color_value(color);
            }
            ui.end_row();
        });
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
                !state.draft.name.trim().is_empty(),
                egui::Button::new(RichText::new("Save").strong()),
            )
            .clicked()
        {
            *event = Some(LabelEvent::Save(Box::new(state.draft.to_draft())));
        }
        if let Some(id) = state.draft.id.clone()
            && ui.button("Delete").clicked()
        {
            *event = Some(LabelEvent::Delete(id));
        }
    });

    if state.draft.id.is_none() {
        return;
    }
    ui.separator();
    ui.label(RichText::new(format!("Members ({})", state.members.len())).strong());
    if state.members.is_empty() {
        ui.label(RichText::new("(none)").weak().small());
    }
    for member in &state.members {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&member.name).strong());
            if ui.small_button("Remove").clicked() {
                *event = Some(LabelEvent::RemoveMember(member.person_id.clone()));
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
        .id_salt("label_people")
        .max_height(150.0)
        .show(ui, |ui| {
            if choices.is_empty() {
                ui.label(RichText::new("(no matches)").weak().small());
            }
            for person in choices {
                ui.horizontal(|ui| {
                    ui.label(crate::gui::inspector::person_label(person, surname_first));
                    if ui.small_button("Add").clicked() {
                        *event = Some(LabelEvent::AddMember(person.id.clone()));
                    }
                });
            }
        });
}

pub(crate) fn color32(value: u32) -> Color32 {
    Color32::from_rgb(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}

pub(crate) fn color_value(color: Color32) -> u32 {
    ((color.r() as u32) << 16) | ((color.g() as u32) << 8) | color.b() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Person;
    use egui::{Context, RawInput};

    fn state() -> LabelsState {
        LabelsState {
            labels: vec![LabelSummary {
                id: "l1".to_string(),
                name: "Maternal".to_string(),
                color: 0xff3366,
                member_count: 1,
            }],
            selected: Some("l1".to_string()),
            draft: LabelEdit {
                id: Some("l1".to_string()),
                name: "Maternal".to_string(),
                color: 0xff3366,
            },
            members: vec![LabelMember {
                person_id: "p1".to_string(),
                name: "Ada King".to_string(),
            }],
            editing: true,
            ..LabelsState::default()
        }
    }

    #[test]
    fn label_view_renders_headless() {
        let ctx = Context::default();
        let mut state = state();
        let people = vec![
            Person::new("Ada", "King", "F"),
            Person::new("Charles", "Babbage", "M"),
        ];
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state, &people, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the label view paints");
    }
}
