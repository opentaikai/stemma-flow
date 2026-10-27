//! Privacy options shown before a GEDCOM export.

use egui::{Context, RichText, Window};

use crate::gedcom::ExportOptions;

/// Renders the export dialog. Returns `Some(true)` for Export, `Some(false)`
/// for Cancel, and `None` while the dialog stays open.
pub fn show(ctx: &Context, options: &mut ExportOptions, open: &mut bool) -> Option<bool> {
    if !*open {
        return None;
    }
    let mut outcome = None;
    let mut visible = true;
    Window::new("Export GEDCOM")
        .id(egui::Id::new("export_options_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut visible)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Living people and private people/facts are excluded by default.",
                )
                .weak()
                .small(),
            );
            ui.separator();
            ui.checkbox(&mut options.include_living, "Include living people");
            ui.checkbox(
                &mut options.include_private,
                "Include private people and facts",
            );
            ui.horizontal(|ui| {
                ui.label("Living-age threshold:");
                ui.add(
                    egui::DragValue::new(&mut options.living_age)
                        .range(80..=130)
                        .suffix(" years"),
                );
            });
            ui.label(
                RichText::new(
                    "The threshold is used only for automatic living detection; manual overrides win.",
                )
                .weak()
                .small(),
            );
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("Export").strong()))
                    .clicked()
                {
                    outcome = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    outcome = Some(false);
                }
            });
        });
    if outcome.is_some() || !visible {
        *open = false;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::RawInput;

    #[test]
    fn dialog_defaults_to_suppression() {
        let ctx = Context::default();
        let mut options = ExportOptions::default();
        let mut open = true;
        let mut outcome = Some(false);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            outcome = show(ui.ctx(), &mut options, &mut open);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the dialog paints");
        assert!(outcome.is_none());
        assert!(open);
        assert!(!options.include_living);
        assert!(!options.include_private);
        assert_eq!(options.living_age, 110);
    }
}
