//! Family group sheet export options.

use crate::i18n::tr;

use egui::{Context, RichText, Window};

/// One selectable family in the family group sheet dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyChoice {
    pub id: String,
    pub label: String,
    pub detail: String,
}

/// State owned by the family group sheet dialog.
#[derive(Debug, Default)]
pub struct FamilySheetState {
    pub open: bool,
    pub families: Vec<FamilyChoice>,
    pub selected: Option<String>,
}

impl FamilySheetState {
    pub fn open_with(&mut self, families: Vec<FamilyChoice>) {
        self.open = true;
        self.selected = families.first().map(|family| family.id.clone());
        self.families = families;
    }
}

/// Draws the family group sheet options window, returning the chosen family id.
pub fn show(ctx: &Context, state: &mut FamilySheetState) -> Option<String> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("family_sheet.title"))
        .id(egui::Id::unique("family_sheet_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(460.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("family_sheet.description")).weak().small());
            if state.families.is_empty() {
                ui.label(RichText::new(tr("common.no_families")).weak());
                if ui.button(tr("common.close")).clicked() {
                    state.open = false;
                }
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("family_sheet_families")
                .max_height(320.0)
                .show(ui, |ui| {
                    for family in &state.families {
                        let selected = state.selected.as_deref() == Some(family.id.as_str());
                        if ui
                            .selectable_label(
                                selected,
                                RichText::new(format!("{}  {}", family.label, family.detail)),
                            )
                            .clicked()
                        {
                            state.selected = Some(family.id.clone());
                        }
                    }
                });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        state.selected.is_some(),
                        egui::Button::new(RichText::new(tr("common.export")).strong()),
                    )
                    .clicked()
                    && let Some(id) = state.selected.clone()
                {
                    state.open = false;
                    event = Some(id);
                }
                if ui.button(tr("common.cancel")).clicked() {
                    state.open = false;
                }
            });
        });
    if !open {
        state.open = false;
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn choice() -> FamilyChoice {
        FamilyChoice {
            id: "f1".to_string(),
            label: "Ada King & Charles Stone".to_string(),
            detail: "2 children".to_string(),
        }
    }

    #[test]
    fn selects_the_first_family_and_renders() {
        let ctx = Context::default();
        let mut state = FamilySheetState::default();
        state.open_with(vec![choice()]);
        assert_eq!(state.selected.as_deref(), Some("f1"));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }

    #[test]
    fn empty_family_list_has_no_selection() {
        let mut state = FamilySheetState::default();
        state.open_with(Vec::new());
        assert!(state.selected.is_none());
        assert!(state.open);
    }
}
