//! Research problem-alert list.

use std::collections::HashSet;

use egui::{Color32, RichText, Ui};

use crate::db::Person;
use crate::gui::inspector::person_label;
use crate::i18n::{tr, tr_args};
use crate::model::problems::{ProblemAlert, ProblemSeverity};

const ERROR_COLOR: Color32 = Color32::from_rgb(220, 38, 38);
const WARNING_COLOR: Color32 = Color32::from_rgb(217, 119, 6);
const DISMISSED_COLOR: Color32 = Color32::from_gray(150);

/// Which severities the list shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SeverityFilter {
    #[default]
    All,
    Errors,
    Warnings,
}

impl SeverityFilter {
    fn label(self) -> String {
        tr(match self {
            Self::All => "problems.filter_all",
            Self::Errors => "problems.filter_errors",
            Self::Warnings => "problems.filter_warnings",
        })
    }

    fn accepts(self, severity: ProblemSeverity) -> bool {
        match self {
            Self::All => true,
            Self::Errors => severity == ProblemSeverity::Error,
            Self::Warnings => severity == ProblemSeverity::Warning,
        }
    }
}

/// State owned by the Problems sub-tab.
#[derive(Debug, Default)]
pub struct ProblemsState {
    pub alerts: Vec<ProblemAlert>,
    pub dismissed: HashSet<String>,
    pub filter: String,
    pub severity: SeverityFilter,
    pub show_dismissed: bool,
    pub confirm_reset: bool,
    pub loaded: bool,
    pub error: Option<String>,
}

/// What the Problems pane asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProblemsEvent {
    SelectPerson(String),
    Dismiss(String),
    Restore(String),
    ResetDismissals,
}

/// Draws the problem list.
pub fn show(
    ui: &mut Ui,
    state: &mut ProblemsState,
    people: &[Person],
    surname_first: bool,
) -> Option<ProblemsEvent> {
    let mut event = None;
    let open = state
        .alerts
        .iter()
        .filter(|alert| !state.dismissed.contains(&alert.key))
        .count();
    let dismissed = state.alerts.len().saturating_sub(open);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr_args(
                "problems.summary",
                &[
                    ("total", state.alerts.len().to_string()),
                    ("open", open.to_string()),
                    ("dismissed", dismissed.to_string()),
                ],
            ))
            .strong(),
        );
        if let Some(error) = &state.error {
            ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("problems.filter"));
        ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text(tr("problems.search_hint")));
        if ui
            .add_enabled(
                !state.filter.is_empty(),
                egui::Button::new(tr("common.clear")),
            )
            .clicked()
        {
            state.filter.clear();
        }
        egui::ComboBox::from_id_salt("problem_severity_filter")
            .selected_text(state.severity.label())
            .show_ui(ui, |ui| {
                for filter in [
                    SeverityFilter::All,
                    SeverityFilter::Errors,
                    SeverityFilter::Warnings,
                ] {
                    ui.selectable_value(&mut state.severity, filter, filter.label());
                }
            });
        ui.checkbox(&mut state.show_dismissed, tr("problems.show_dismissed"));
        let has_dismissed = !state.dismissed.is_empty();
        if ui
            .add_enabled(has_dismissed, egui::Button::new(tr("problems.reset")))
            .clicked()
        {
            state.confirm_reset = true;
        }
    });
    if state.confirm_reset {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr("problems.reset_confirm"))
                    .color(ui.visuals().error_fg_color)
                    .small(),
            );
            if ui.button(tr("problems.reset_yes")).clicked() {
                state.confirm_reset = false;
                event = Some(ProblemsEvent::ResetDismissals);
            }
            if ui.button(tr("common.cancel")).clicked() {
                state.confirm_reset = false;
            }
        });
    }
    ui.separator();

    let needle = state.filter.trim().to_lowercase();
    let mut shown = 0usize;
    egui::ScrollArea::vertical()
        .id_salt("problem_alerts")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for alert in &state.alerts {
                let is_dismissed = state.dismissed.contains(&alert.key);
                if is_dismissed && !state.show_dismissed {
                    continue;
                }
                if !state.severity.accepts(alert.severity) {
                    continue;
                }
                let subject = person_name(people, &alert.person_id, surname_first);
                let related = alert
                    .related_person_id
                    .as_deref()
                    .map(|id| person_name(people, id, surname_first));
                let kind = tr(alert.kind.label_key());
                let haystack = format!(
                    "{} {} {} {}",
                    kind,
                    subject,
                    related.as_deref().unwrap_or_default(),
                    alert.details.join(" ")
                )
                .to_lowercase();
                if !needle.is_empty() && !haystack.contains(&needle) {
                    continue;
                }
                shown += 1;
                ui.horizontal_wrapped(|ui| {
                    let severity = tr(alert.severity.label_key());
                    let color = if is_dismissed {
                        DISMISSED_COLOR
                    } else {
                        match alert.severity {
                            ProblemSeverity::Error => ERROR_COLOR,
                            ProblemSeverity::Warning => WARNING_COLOR,
                        }
                    };
                    ui.label(RichText::new(severity).color(color).strong().small());
                    if ui.link(RichText::new(&subject).strong()).clicked() {
                        event = Some(ProblemsEvent::SelectPerson(alert.person_id.clone()));
                    }
                    ui.label(RichText::new(kind).small());
                    if let Some(related) = related {
                        ui.label(
                            RichText::new(tr_args("problems.related", &[("name", related)]))
                                .weak()
                                .small(),
                        );
                    }
                    if !alert.details.is_empty() {
                        ui.label(RichText::new(alert.details.join(" · ")).weak().small());
                    }
                    if is_dismissed {
                        if ui.small_button(tr("problems.restore")).clicked() {
                            event = Some(ProblemsEvent::Restore(alert.key.clone()));
                        }
                    } else if ui.small_button(tr("problems.dismiss")).clicked() {
                        event = Some(ProblemsEvent::Dismiss(alert.key.clone()));
                    }
                });
            }
            if shown == 0 {
                ui.label(RichText::new(tr("problems.empty")).weak());
            }
        });
    event
}

fn person_name(people: &[Person], id: &str, surname_first: bool) -> String {
    people
        .iter()
        .find(|person| person.id == id)
        .map(|person| person_label(person, surname_first))
        .unwrap_or_else(|| tr("problems.unknown_person"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Family, FamilyTree, Person};
    use crate::model::problems::{ProblemAlert, ProblemKind};
    use egui::{Context, RawInput};

    fn tree() -> FamilyTree {
        let mut person = Person::new("Ada", "King", "F");
        person.id = "p1".to_string();
        let mut parent = Person::new("George", "King", "M");
        parent.id = "p2".to_string();
        FamilyTree {
            people: vec![person, parent],
            families: vec![Family {
                id: "f1".to_string(),
                husband_id: Some("p2".to_string()),
                wife_id: None,
                created_at: String::new(),
            }],
            child_links: vec![("f1".to_string(), "p1".to_string())],
        }
    }

    fn alert(dismissed: &mut ProblemsState) {
        dismissed.alerts.push(ProblemAlert {
            key: "death_before_birth|p1||1900|1890".to_string(),
            kind: ProblemKind::DeathBeforeBirth,
            severity: ProblemSeverity::Error,
            person_id: "p1".to_string(),
            related_person_id: None,
            details: vec!["1900".to_string(), "1890".to_string()],
        });
    }

    #[test]
    fn problems_pane_renders_headless() {
        let ctx = Context::default();
        let mut state = ProblemsState::default();
        alert(&mut state);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state, &tree().people, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }

    #[test]
    fn dismissed_alerts_can_be_hidden_and_restored() {
        let ctx = Context::default();
        let mut state = ProblemsState::default();
        alert(&mut state);
        state.dismissed.insert(state.alerts[0].key.clone());
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state, &tree().people, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        state.show_dismissed = true;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui, &mut state, &tree().people, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
