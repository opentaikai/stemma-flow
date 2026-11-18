//! Portable folder share bundle options.

use egui::{Context, RichText, Window};

use crate::i18n::tr;

/// State owned by the share bundle dialog.
#[derive(Debug, Clone, Default)]
pub struct ShareBundleState {
    pub open: bool,
    pub name: String,
}

impl ShareBundleState {
    pub fn open_with(&mut self, name: String) {
        self.open = true;
        self.name = name;
    }
}

/// Draws the share bundle options window, returning the bundle name.
pub fn show(ctx: &Context, state: &mut ShareBundleState) -> Option<String> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("share_bundle.title"))
        .id(egui::Id::unique("share_bundle_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("share_bundle.intro")).weak().small());
            ui.horizontal(|ui| {
                ui.label(tr("share_bundle.name_label"));
                ui.text_edit_singleline(&mut state.name);
            });
            ui.label(RichText::new(tr("share_bundle.folder_hint")).weak().small());
            ui.label(RichText::new(tr("share_bundle.privacy")).weak().small());
            ui.horizontal(|ui| {
                let name = state.name.trim();
                if ui
                    .add_enabled(
                        !name.is_empty(),
                        egui::Button::new(RichText::new(tr("share_bundle.create")).strong()),
                    )
                    .clicked()
                {
                    state.open = false;
                    event = Some(name.to_string());
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

    #[test]
    fn opens_with_a_name_and_renders() {
        let ctx = Context::default();
        let mut state = ShareBundleState::default();
        state.open_with("Family Tree".to_string());
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }
}
