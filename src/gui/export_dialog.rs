//! Privacy options shown before a GEDCOM export.

use egui::{Context, RichText, Window};

use crate::gedcom::ExportOptions;
use crate::i18n::tr;

/// Renders the export dialog. Returns `Some(true)` for Export, `Some(false)`
/// for Cancel, and `None` while the dialog stays open.
pub fn show(ctx: &Context, options: &mut ExportOptions, open: &mut bool) -> Option<bool> {
    if !*open {
        return None;
    }
    let mut outcome = None;
    let mut visible = true;
    Window::new(tr("export.title"))
        .id(egui::Id::new("export_options_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut visible)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("export.privacy_hint")).weak().small());
            ui.separator();
            ui.checkbox(&mut options.include_living, tr("export.include_living"));
            ui.checkbox(&mut options.include_private, tr("export.include_private"));
            ui.horizontal(|ui| {
                ui.label(tr("export.threshold"));
                ui.add(
                    egui::DragValue::new(&mut options.living_age)
                        .range(80..=130)
                        .suffix(format!(" {}", tr("export.years"))),
                );
            });
            ui.label(RichText::new(tr("export.threshold_hint")).weak().small());
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(
                        RichText::new(tr("export.export")).strong(),
                    ))
                    .clicked()
                {
                    outcome = Some(true);
                }
                if ui.button(tr("common.cancel")).clicked() {
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
