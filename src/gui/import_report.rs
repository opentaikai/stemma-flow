//! The `Import Report` window: the unmapped tags collected by the last
//! GEDCOM import, listed for review without blocking the import itself.

use egui::{Context, Id, Label, ScrollArea, TextStyle, Window};

use crate::gedcom::ImportReport;

/// Renders the report window while `open` is true.
///
/// Rows are virtualized with [`ScrollArea::show_rows`] because a real
/// WikiTree export logs thousands of warnings; every row shows one message
/// truncated to the window width with the full text on hover.
pub fn show(ctx: &Context, report: &ImportReport, open: &mut bool) {
    Window::new("Import Report")
        .id(Id::new("import_report_window"))
        .open(open)
        .default_width(560.0)
        .default_height(360.0)
        .show(ctx, |ui| {
            ui.label(format!(
                "{} people, {} families, {} events, {} citations — {} warnings",
                report.people,
                report.families,
                report.events,
                report.citations,
                report.warnings.len()
            ));
            ui.separator();
            let row_height = ui.text_style_height(&TextStyle::Body) + ui.spacing().item_spacing.y;
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, row_height, report.warnings.len(), |ui, row_range| {
                    for index in row_range {
                        let Some(warning) = report.warnings.get(index) else {
                            continue;
                        };
                        ui.add(Label::new(warning.as_str()).truncate())
                            .on_hover_text(warning);
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use egui::{Context, RawInput};

    use super::show;
    use crate::gedcom::ImportReport;

    fn report_with(warnings: usize) -> ImportReport {
        ImportReport {
            people: 1,
            families: 1,
            events: 2,
            citations: 0,
            warnings: (0..warnings)
                .map(|index| format!("Line {index}: Tag recognized/unsupported or ignored"))
                .collect(),
            ..ImportReport::default()
        }
    }

    #[test]
    fn paints_the_summary_and_warning_rows() {
        let ctx = Context::default();
        let report = report_with(3);
        let mut open = true;

        let mut output = ctx.run_ui(RawInput::default(), |_ui| {
            show(&ctx, &report, &mut open);
        });
        output.textures_delta.clear();

        assert!(!output.shapes.is_empty(), "the window paints");
        assert!(open, "the window stays open");
    }

    #[test]
    fn a_closed_window_paints_nothing() {
        let ctx = Context::default();
        let report = report_with(3);
        let mut open = false;

        let mut output = ctx.run_ui(RawInput::default(), |_ui| {
            show(&ctx, &report, &mut open);
        });
        output.textures_delta.clear();

        assert!(output.shapes.is_empty(), "nothing to draw");
        assert!(!open, "stays closed");
    }

    #[test]
    fn thousands_of_warnings_render_without_panicking() {
        let ctx = Context::default();
        let report = report_with(5_000);
        let mut open = true;

        let mut output = ctx.run_ui(RawInput::default(), |_ui| {
            show(&ctx, &report, &mut open);
        });
        output.textures_delta.clear();

        assert!(!output.shapes.is_empty(), "the window paints");
    }
}
