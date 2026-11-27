//! Task editor, research-log list and record attachment dialog.

use egui::{Context, RichText, Ui, Window};

use crate::db::tasks::{TaskDraft, TaskOwner, TaskStatus, TaskSummary};
use crate::gui::date_picker::date_edit;
use crate::i18n::{tr, tr_args, trn};

const PRIORITIES: [(Option<i64>, &str); 5] = [
    (None, "tasks.priority_none"),
    (Some(1), "tasks.priority_low"),
    (Some(2), "tasks.priority_normal"),
    (Some(3), "tasks.priority_high"),
    (Some(4), "tasks.priority_urgent"),
];

/// Which tasks the log shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TaskFilter {
    #[default]
    All,
    Open,
    Done,
}

impl TaskFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "tasks.filter_all",
            Self::Open => "tasks.filter_open",
            Self::Done => "tasks.filter_done",
        }
    }
}

/// A task buffered for editing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskEdit {
    pub id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: Option<i64>,
    pub folder: String,
    pub due_date: String,
    pub completed_at: Option<String>,
}

impl TaskEdit {
    pub fn from_summary(summary: &TaskSummary) -> Self {
        Self {
            id: Some(summary.id.clone()),
            title: summary.title.clone(),
            description: summary.description.clone().unwrap_or_default(),
            status: summary.status,
            priority: summary.priority,
            folder: summary.folder.clone().unwrap_or_default(),
            due_date: summary.due_date.clone().unwrap_or_default(),
            completed_at: summary.completed_at.clone(),
        }
    }

    pub fn from_record(record: &crate::db::tasks::TaskRecord) -> Self {
        Self {
            id: Some(record.id.clone()),
            title: record.title.clone(),
            description: record.description.clone().unwrap_or_default(),
            status: record.status,
            priority: record.priority,
            folder: record.folder.clone().unwrap_or_default(),
            due_date: record.due_date.clone().unwrap_or_default(),
            completed_at: record.completed_at.clone(),
        }
    }

    pub fn to_draft(&self) -> TaskDraft {
        TaskDraft {
            id: self.id.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            status: self.status,
            priority: self.priority,
            folder: self.folder.clone(),
            due_date: self.due_date.clone(),
        }
    }
}

/// What the task view asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    New,
    Select(String),
    Save(Box<TaskDraft>),
    Delete(String),
    Print,
    Detach {
        owner_type: String,
        owner_id: String,
    },
}

/// State owned by the window while the Tasks sub-tab is visible.
#[derive(Debug, Default)]
pub struct TasksState {
    pub tasks: Vec<TaskSummary>,
    pub folders: Vec<String>,
    pub selected: Option<String>,
    pub draft: TaskEdit,
    pub links: Vec<crate::db::tasks::TaskLinkSummary>,
    pub filter: String,
    pub status_filter: TaskFilter,
    pub folder_filter: Option<String>,
    pub editing: bool,
    pub error: Option<String>,
    pub loaded: bool,
}

/// Tasks matching the current text, status and folder filters, in list order.
pub fn filtered_tasks(state: &TasksState) -> Vec<&TaskSummary> {
    let filter = state.filter.trim().to_lowercase();
    state
        .tasks
        .iter()
        .filter(|task| match state.status_filter {
            TaskFilter::All => true,
            TaskFilter::Open => task.status == TaskStatus::Open,
            TaskFilter::Done => task.status == TaskStatus::Done,
        })
        .filter(|task| match &state.folder_filter {
            Some(folder) => task.folder.as_deref() == Some(folder.as_str()),
            None => true,
        })
        .filter(|task| {
            filter.is_empty()
                || task.title.to_lowercase().contains(&filter)
                || task
                    .folder
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&filter)
        })
        .collect()
}

/// Draws the task list and editor.
pub fn show(ui: &mut Ui, state: &mut TasksState) -> Option<TaskEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        ui.label(tr("common.filter"));
        ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text(tr("tasks.filter_hint")));
        if ui
            .add_enabled(
                !state.filter.is_empty(),
                egui::Button::new(tr("common.clear")),
            )
            .clicked()
        {
            state.filter.clear();
        }
        egui::ComboBox::from_id_salt("task_status_filter")
            .selected_text(tr(state.status_filter.label()))
            .show_ui(ui, |ui| {
                for filter in [TaskFilter::All, TaskFilter::Open, TaskFilter::Done] {
                    ui.selectable_value(&mut state.status_filter, filter, tr(filter.label()));
                }
            });
        let folder_label = state
            .folder_filter
            .clone()
            .unwrap_or_else(|| tr("tasks.all_folders").to_string());
        egui::ComboBox::from_id_salt("task_folder_filter")
            .selected_text(folder_label)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.folder_filter.is_none(), tr("tasks.all_folders"))
                    .clicked()
                {
                    state.folder_filter = None;
                }
                for folder in &state.folders {
                    if ui
                        .selectable_label(
                            state.folder_filter.as_deref() == Some(folder.as_str()),
                            folder,
                        )
                        .clicked()
                    {
                        state.folder_filter = Some(folder.clone());
                    }
                }
            });
        if ui
            .add(egui::Button::new(RichText::new(tr("tasks.new")).strong()))
            .clicked()
        {
            event = Some(TaskEvent::New);
        }
        if ui
            .add(egui::Button::new(crate::i18n::tr("tasks.print")))
            .clicked()
        {
            event = Some(TaskEvent::Print);
        }
    });
    ui.separator();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(470.0);
            let visible = filtered_tasks(state);
            egui::ScrollArea::vertical()
                .id_salt("task_log")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if visible.is_empty() {
                        ui.label(RichText::new(tr("tasks.empty")).weak().small());
                    }
                    for task in visible {
                        let selected = state.selected.as_deref() == Some(task.id.as_str());
                        ui.horizontal(|ui| {
                            if ui.selectable_label(selected, &task.title).clicked() {
                                event = Some(TaskEvent::Select(task.id.clone()));
                            }
                            ui.label(RichText::new(task_status_label(task.status)).weak().small());
                            if let Some(priority) = task.priority {
                                ui.label(
                                    RichText::new(priority_text(Some(priority))).weak().small(),
                                );
                            }
                            if let Some(folder) = &task.folder {
                                ui.label(RichText::new(folder).weak().small());
                            }
                            if let Some(due) = &task.due_date {
                                ui.label(RichText::new(due).weak().small());
                            }
                            ui.label(
                                RichText::new(trn("tasks.links", task.link_count))
                                    .weak()
                                    .small(),
                            );
                        });
                    }
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical()
                .id_salt("task_editor")
                .auto_shrink([false, false])
                .show(ui, |ui| task_editor(ui, state, &mut event));
        });
    });
    event
}

fn task_editor(ui: &mut Ui, state: &mut TasksState, event: &mut Option<TaskEvent>) {
    if !state.editing {
        ui.label(RichText::new(tr("tasks.select_prompt")).weak());
        return;
    }
    ui.heading(if state.draft.id.is_some() {
        tr("tasks.edit_title")
    } else {
        tr("tasks.new_title")
    });
    egui::Grid::new("task_fields")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label(RichText::new(tr("tasks.title_required")).strong());
            ui.text_edit_singleline(&mut state.draft.title);
            ui.end_row();

            ui.label(tr("tasks.status"));
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut state.draft.status,
                    TaskStatus::Open,
                    tr("tasks.status_open"),
                );
                ui.selectable_value(
                    &mut state.draft.status,
                    TaskStatus::Done,
                    tr("tasks.status_done"),
                );
            });
            ui.end_row();

            ui.label(tr("tasks.priority"));
            egui::ComboBox::from_id_salt("task_priority")
                .selected_text(priority_text(state.draft.priority))
                .show_ui(ui, |ui| {
                    for (value, label) in PRIORITIES {
                        ui.selectable_value(&mut state.draft.priority, value, label);
                    }
                });
            ui.end_row();

            ui.label(tr("tasks.folder"));
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut state.draft.folder);
                egui::ComboBox::from_id_salt("task_folder_choices")
                    .selected_text("…")
                    .show_ui(ui, |ui| {
                        for folder in &state.folders {
                            if ui.button(folder).clicked() {
                                state.draft.folder = folder.clone();
                            }
                        }
                    });
            });
            ui.end_row();

            ui.label(tr("tasks.due_date"));
            date_edit(ui, &mut state.draft.due_date, "task_due_date");
            ui.end_row();
        });
    if let Some(completed) = &state.draft.completed_at {
        ui.label(
            RichText::new(tr_args(
                "tasks.completed",
                &[("date", completed.to_string())],
            ))
            .weak()
            .small(),
        );
    }
    ui.label(tr("tasks.research_log"));
    ui.add(
        egui::TextEdit::multiline(&mut state.draft.description)
            .desired_rows(8)
            .desired_width(f32::INFINITY),
    );
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
                !state.draft.title.trim().is_empty(),
                egui::Button::new(RichText::new(tr("common.save")).strong()),
            )
            .clicked()
        {
            *event = Some(TaskEvent::Save(Box::new(state.draft.to_draft())));
        }
        if let Some(id) = state.draft.id.clone()
            && ui.button(tr("common.delete")).clicked()
        {
            *event = Some(TaskEvent::Delete(id));
        }
    });
    ui.separator();
    ui.label(RichText::new(tr("tasks.linked_records")).strong());
    if state.links.is_empty() {
        ui.label(RichText::new(tr("inspector.none")).weak().small());
    }
    for link in &state.links {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr_args(
                    "tasks.owner_line",
                    &[("kind", owner_type_label(&link.owner_type).to_string())],
                ))
                .weak()
                .small(),
            );
            ui.label(&link.label);
            if ui.small_button(tr("tasks.detach")).clicked() {
                *event = Some(TaskEvent::Detach {
                    owner_type: link.owner_type.clone(),
                    owner_id: link.owner_id.clone(),
                });
            }
        });
    }
}

pub fn priority_label(priority: i64) -> String {
    PRIORITIES
        .iter()
        .find(|(value, _)| *value == Some(priority))
        .map(|(_, key)| tr(key).to_string())
        .unwrap_or_else(|| priority.to_string())
}

fn priority_text(priority: Option<i64>) -> String {
    match priority {
        Some(value) => priority_label(value),
        None => tr("tasks.priority_none").to_string(),
    }
}

fn task_status_label(status: TaskStatus) -> &'static str {
    tr(match status {
        TaskStatus::Open => "tasks.status_open",
        TaskStatus::Done => "tasks.status_done",
    })
}

fn owner_type_label(kind: &str) -> &'static str {
    tr(match kind {
        "person" => "tasks.owner.person",
        "family" => "tasks.owner.family",
        "event" => "tasks.owner.event",
        "source" => "tasks.owner.source",
        "citation" => "tasks.owner.citation",
        "place" => "tasks.owner.place",
        "media" => "tasks.owner.media",
        _ => "tasks.owner.other",
    })
}

/// State owned by the reusable task attachment dialog.
pub struct TaskAttachState {
    pub owner: TaskOwner,
    pub title: String,
    pub attached: Vec<TaskSummary>,
    pub choices: Vec<TaskSummary>,
    pub search: String,
    pub error: Option<String>,
}

/// What the task attachment dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskAttachOutcome {
    Attach(String),
    Detach(String),
    Close,
}

impl TaskAttachState {
    pub fn new(owner: TaskOwner, title: String) -> Self {
        Self {
            owner,
            title,
            attached: Vec::new(),
            choices: Vec::new(),
            search: String::new(),
            error: None,
        }
    }
}

/// Draws the task attachment dialog; `None` keeps it open.
pub fn show_attach(ctx: &Context, state: &mut TaskAttachState) -> Option<TaskAttachOutcome> {
    let mut outcome = None;
    let mut open = true;
    Window::new(&state.title)
        .id(egui::Id::unique("task_attach_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(460.0)
        .open(&mut open)
        .show(ctx, |ui| {
            if state.attached.is_empty() {
                ui.label(RichText::new(tr("tasks.none_attached")).weak().small());
            }
            for task in &state.attached {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&task.title).strong());
                    ui.label(RichText::new(task_status_label(task.status)).weak().small());
                    if let Some(due) = &task.due_date {
                        ui.label(RichText::new(due).weak().small());
                    }
                    if ui.small_button(tr("tasks.detach")).clicked() {
                        outcome = Some(TaskAttachOutcome::Detach(task.id.clone()));
                    }
                });
            }
            ui.separator();
            ui.label(RichText::new(tr("tasks.attach_existing")).strong());
            ui.add(
                egui::TextEdit::singleline(&mut state.search).hint_text(tr("tasks.search_hint")),
            );
            let needle = state.search.trim().to_lowercase();
            let choices: Vec<TaskSummary> = state
                .choices
                .iter()
                .filter(|task| {
                    needle.is_empty()
                        || task.title.to_lowercase().contains(&needle)
                        || task
                            .folder
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&needle)
                })
                .take(24)
                .cloned()
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("task_choices")
                .max_height(180.0)
                .show(ui, |ui| {
                    if choices.is_empty() {
                        ui.label(RichText::new(tr("common.no_matches")).weak().small());
                    }
                    for task in choices {
                        ui.horizontal(|ui| {
                            ui.label(&task.title);
                            ui.label(RichText::new(task_status_label(task.status)).weak().small());
                            if ui.small_button(tr("tasks.attach")).clicked() {
                                outcome = Some(TaskAttachOutcome::Attach(task.id.clone()));
                            }
                        });
                    }
                });
            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
        });
    if !open {
        return Some(TaskAttachOutcome::Close);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::RawInput;

    fn summary() -> TaskSummary {
        TaskSummary {
            id: "t1".to_string(),
            title: "Find the birth record".to_string(),
            description: Some("Checked the parish register.".to_string()),
            status: TaskStatus::Open,
            priority: Some(3),
            folder: Some("Parish".to_string()),
            due_date: Some("2026-01-01".to_string()),
            completed_at: None,
            link_count: 1,
        }
    }

    #[test]
    fn filtered_tasks_apply_text_status_and_folder_filters() {
        let mut open = summary();
        open.id = "open".to_string();
        let mut done = summary();
        done.id = "done".to_string();
        done.status = TaskStatus::Done;
        done.folder = Some("Archive".to_string());
        let mut other = summary();
        other.id = "other".to_string();
        other.title = "Find the death record".to_string();
        other.folder = None;
        let mut state = TasksState {
            tasks: vec![open, done, other],
            ..TasksState::default()
        };

        assert_eq!(filtered_tasks(&state).len(), 3);
        state.status_filter = TaskFilter::Done;
        assert_eq!(
            filtered_tasks(&state)
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            ["done"]
        );
        state.status_filter = TaskFilter::All;
        state.folder_filter = Some("Parish".to_string());
        assert_eq!(
            filtered_tasks(&state)
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            ["open"]
        );
        state.folder_filter = None;
        state.filter = "death".to_string();
        assert_eq!(
            filtered_tasks(&state)
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            ["other"]
        );
    }

    #[test]
    fn task_view_renders_headless() {
        let ctx = Context::default();
        let mut state = TasksState {
            tasks: vec![summary()],
            selected: Some("t1".to_string()),
            draft: TaskEdit::from_summary(&summary()),
            editing: true,
            ..TasksState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the task view paints");
    }

    #[test]
    fn attachment_dialog_renders_headless() {
        let ctx = Context::default();
        let mut state = TaskAttachState::new(
            TaskOwner::Person("p1".to_string()),
            "Tasks — Ada King".to_string(),
        );
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show_attach(ui.ctx(), &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the task dialog paints");
    }
}
