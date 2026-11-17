//! Administrative list and statistics export options.

use egui::{Context, RichText, Window};

use crate::report::administrative::AdministrativeMode;

/// State owned by the administrative lists dialog.
#[derive(Debug, Default)]
pub struct AdministrativeListsState {
    pub open: bool,
    pub mode: AdministrativeMode,
}

impl AdministrativeListsState {
    pub fn open_with(&mut self) {
        self.open = true;
    }
}

/// Draws the administrative lists options window.
pub fn show(ctx: &Context, state: &mut AdministrativeListsState) -> Option<AdministrativeMode> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Lists & Statistics")
        .id(egui::Id::unique("administrative_lists_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Export stored records and tree-wide counts as printable HTML.")
                    .weak()
                    .small(),
            );
            ui.horizontal(|ui| {
                ui.label("Report");
                ui.selectable_value(&mut state.mode, AdministrativeMode::Places, "Places");
                ui.selectable_value(&mut state.mode, AdministrativeMode::Sources, "Sources");
                ui.selectable_value(
                    &mut state.mode,
                    AdministrativeMode::Repositories,
                    "Repositories",
                );
                ui.selectable_value(
                    &mut state.mode,
                    AdministrativeMode::Statistics,
                    "Statistics",
                );
            });
            let description = match state.mode {
                AdministrativeMode::Places => "Every place with its event count and coordinates.",
                AdministrativeMode::Sources => {
                    "Every source with author, publication, citations and repositories."
                }
                AdministrativeMode::Repositories => {
                    "Every repository with its address and linked source count."
                }
                AdministrativeMode::Statistics => {
                    "Record totals, people completeness and event-type counts."
                }
            };
            ui.label(RichText::new(description).weak().small());
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Export\u{2026}").strong()))
                    .clicked()
                {
                    state.open = false;
                    event = Some(state.mode);
                }
                if ui.button("Cancel").clicked() {
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

    #[test]
    fn opens_with_places_selected_and_renders() {
        let ctx = Context::default();
        let mut state = AdministrativeListsState::default();
        state.open_with();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert_eq!(state.mode, AdministrativeMode::Places);
    }
}
