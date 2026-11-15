//! Individual list export options.

use egui::{Context, RichText, Window};

/// State owned by the individual list dialog.
#[derive(Debug, Default)]
pub struct IndividualListState {
    pub open: bool,
}

/// What the open list currently contains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndividualListPreview {
    pub count: usize,
    pub columns: usize,
    pub filter: Vec<String>,
}

/// Draws the individual list options window.
pub fn show(
    ctx: &Context,
    state: &mut IndividualListState,
    preview: &IndividualListPreview,
) -> Option<()> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut export = false;
    Window::new("Individual List")
        .id(egui::Id::unique("individual_list_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Export the current People list as printable HTML, keeping its filter, sort order and columns.",
                )
                .weak()
                .small(),
            );
            ui.label(format!(
                "{} people \u{00b7} {} columns",
                preview.count, preview.columns
            ));
            if !preview.filter.is_empty() {
                ui.label(RichText::new("Active filter").strong().small());
                for line in &preview.filter {
                    ui.label(RichText::new(line).weak().small());
                }
            }
            if preview.count == 0 {
                ui.label(RichText::new("The current list is empty.").weak());
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        preview.count > 0,
                        egui::Button::new(RichText::new("Export HTML\u{2026}").strong()),
                    )
                    .clicked()
                {
                    state.open = false;
                    export = true;
                }
                if ui.button("Cancel").clicked() {
                    state.open = false;
                }
            });
        });
    if !open {
        state.open = false;
    }
    export.then_some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn shows_the_current_list_summary() {
        let ctx = Context::default();
        let mut state = IndividualListState { open: true };
        let preview = IndividualListPreview {
            count: 3,
            columns: 4,
            filter: vec!["Place: London".to_string()],
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state, &preview).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }
}
