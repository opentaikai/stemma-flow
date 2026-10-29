//! Tools dialog for calculating any two people's relationship.

use egui::{Context, RichText, Ui, Window};

use crate::db::FamilyTree;
use crate::gui::inspector::person_label;
use crate::i18n::{tr, tr_args};
use crate::model::relationship::{PathKind, RelationshipResult, relationship_between};

/// State for the relationship calculator dialog.
#[derive(Debug, Default)]
pub struct RelationshipCalculatorState {
    pub open: bool,
    pub from: Option<String>,
    pub to: Option<String>,
    pub from_search: String,
    pub to_search: String,
    pub result: Option<RelationshipResult>,
    pub error: Option<String>,
}

impl RelationshipCalculatorState {
    pub fn open_prefilled(&mut self, root: Option<&str>, selected: Option<&str>) {
        self.open = true;
        self.from = root
            .map(str::to_string)
            .or_else(|| selected.map(str::to_string));
        self.to = selected
            .filter(|selected| self.from.as_deref() != Some(*selected))
            .map(str::to_string);
        self.from_search.clear();
        self.to_search.clear();
        self.recompute(None);
    }

    fn recompute(&mut self, tree: Option<&FamilyTree>) {
        self.result = match (tree, self.from.as_deref(), self.to.as_deref()) {
            (Some(tree), Some(from), Some(to)) => relationship_between(tree, from, to),
            _ => None,
        };
    }
}

/// Draws the calculator window.
pub fn show(
    ctx: &Context,
    state: &mut RelationshipCalculatorState,
    tree: &FamilyTree,
    surname_first: bool,
) -> bool {
    if !state.open {
        return false;
    }
    let mut open = true;
    let mut changed = false;
    Window::new(tr("calculator.title"))
        .id(egui::Id::new("relationship_calculator"))
        .collapsible(false)
        .resizable(true)
        .default_width(680.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    person_picker(
                        ui,
                        &tr("calculator.first"),
                        &mut state.from,
                        &mut state.from_search,
                        tree,
                        surname_first,
                    );
                });
                ui.separator();
                ui.vertical(|ui| {
                    person_picker(
                        ui,
                        &tr("calculator.second"),
                        &mut state.to,
                        &mut state.to_search,
                        tree,
                        surname_first,
                    );
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tr("calculator.swap")).clicked() {
                    std::mem::swap(&mut state.from, &mut state.to);
                    std::mem::swap(&mut state.from_search, &mut state.to_search);
                    changed = true;
                }
                if ui.button(tr("common.clear")).clicked() {
                    state.from = None;
                    state.to = None;
                    state.from_search.clear();
                    state.to_search.clear();
                    state.result = None;
                }
            });
            state.recompute(Some(tree));
            ui.separator();
            if let Some(result) = &state.result {
                ui.heading(result.kinship.label());
                if let Some(common) = &result.common_ancestor {
                    ui.label(tr_args(
                        "calculator.common_ancestor",
                        &[("name", person_name(tree, common, surname_first))],
                    ));
                }
                let path = state
                    .from
                    .as_deref()
                    .map(|from| path_text(tree, result, from, surname_first))
                    .unwrap_or_default();
                if !path.is_empty() {
                    ui.label(RichText::new(path).weak());
                }
            } else if state.from.is_some() && state.to.is_some() {
                ui.label(RichText::new(tr("calculator.no_connection")).weak());
            } else {
                ui.label(RichText::new(tr("calculator.choose_two")).weak());
            }
            if let Some(error) = &state.error {
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }
            ui.separator();
            if ui.button(tr("common.close")).clicked() {
                state.open = false;
            }
        });
    if !open {
        state.open = false;
    }
    changed
}

fn person_picker(
    ui: &mut Ui,
    label: &str,
    selected: &mut Option<String>,
    search: &mut String,
    tree: &FamilyTree,
    surname_first: bool,
) {
    ui.label(RichText::new(label).strong());
    let selected_label = selected
        .as_deref()
        .map(|id| person_name(tree, id, surname_first))
        .unwrap_or_else(|| tr("calculator.none"));
    ui.label(selected_label);
    ui.add(egui::TextEdit::singleline(search).hint_text(tr("calculator.search")));
    let needle = search.trim().to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt(("calculator", label))
        .max_height(180.0)
        .show(ui, |ui| {
            for person in tree.people.iter().filter(|person| {
                needle.is_empty()
                    || person_label(person, surname_first)
                        .to_lowercase()
                        .contains(&needle)
            }) {
                if ui
                    .selectable_label(
                        selected.as_deref() == Some(person.id.as_str()),
                        person_label(person, surname_first),
                    )
                    .clicked()
                {
                    *selected = Some(person.id.clone());
                }
            }
        });
}

fn path_text(
    tree: &FamilyTree,
    result: &RelationshipResult,
    from: &str,
    surname_first: bool,
) -> String {
    if result.path.is_empty() {
        return String::new();
    }
    let mut parts = vec![person_name(tree, from, surname_first)];
    for step in &result.path {
        parts.push(format!(
            "{} {}",
            path_kind(step.kind),
            person_name(tree, &step.to_person_id, surname_first)
        ));
    }
    parts.join(" → ")
}

fn path_kind(kind: PathKind) -> String {
    tr(match kind {
        PathKind::Parent => "calculator.parent_step",
        PathKind::Child => "calculator.child_step",
        PathKind::Spouse => "calculator.spouse_step",
    })
}

fn person_name(tree: &FamilyTree, id: &str, surname_first: bool) -> String {
    tree.people
        .iter()
        .find(|person| person.id == id)
        .map(|person| person_label(person, surname_first))
        .unwrap_or_else(|| tr("calculator.none"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Family, Person};
    use egui::{Context, RawInput};

    fn tree() -> FamilyTree {
        let mut root = Person::new("Ada", "King", "F");
        root.id = "root".to_string();
        let mut father = Person::new("Charles", "Babbage", "M");
        father.id = "father".to_string();
        FamilyTree {
            people: vec![root, father],
            families: vec![Family {
                id: "f1".to_string(),
                husband_id: Some("father".to_string()),
                wife_id: None,
                created_at: String::new(),
            }],
            child_links: vec![("f1".to_string(), "root".to_string())],
        }
    }

    #[test]
    fn calculator_renders_headless() {
        let ctx = Context::default();
        let mut state = RelationshipCalculatorState {
            open: true,
            from: Some("root".to_string()),
            to: Some("father".to_string()),
            ..RelationshipCalculatorState::default()
        };
        let tree = tree();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui.ctx(), &mut state, &tree, false);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert_eq!(
            state.result.as_ref().expect("result").kinship.label(),
            "Father"
        );
    }
}
