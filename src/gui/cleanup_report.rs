//! Duplicate and missing-information report options.

use egui::{Context, RichText, Window};

use crate::report::cleanup::CleanupMode;

/// State owned by the cleanup report dialog.
#[derive(Debug, Default)]
pub struct CleanupReportState {
    pub open: bool,
    pub mode: CleanupMode,
}

impl CleanupReportState {
    pub fn open_with(&mut self) {
        self.open = true;
    }
}

/// Draws the cleanup report options window.
pub fn show(ctx: &Context, state: &mut CleanupReportState) -> Option<CleanupMode> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Cleanup Reports")
        .id(egui::Id::unique("cleanup_report_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Find possible duplicates or open missing-information problems for cleanup.",
                )
                .weak()
                .small(),
            );
            ui.horizontal(|ui| {
                ui.label("Report");
                ui.selectable_value(&mut state.mode, CleanupMode::Duplicates, "Duplicates");
                ui.selectable_value(
                    &mut state.mode,
                    CleanupMode::MissingInformation,
                    "Missing information",
                );
            });
            let description = match state.mode {
                CleanupMode::Duplicates => {
                    "Scored candidate pairs with both people's vitals and match reasons."
                }
                CleanupMode::MissingInformation => {
                    "All open data problems; dismissed alerts are excluded."
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
    fn opens_with_duplicates_selected_and_renders() {
        let ctx = Context::default();
        let mut state = CleanupReportState::default();
        state.open_with();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert_eq!(state.mode, CleanupMode::Duplicates);
    }
}
