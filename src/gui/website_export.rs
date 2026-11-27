//! Static family website export options.

use egui::{Context, RichText, Window};

use crate::db::Person;
use crate::gui::inspector::person_label;
use crate::i18n::tr;

/// State owned by the website export dialog.
#[derive(Debug, Clone, Default)]
pub struct WebsiteExportState {
    pub open: bool,
    pub title: String,
    pub root_id: Option<String>,
    pub include_chart: bool,
}

impl WebsiteExportState {
    pub fn open_with(&mut self, title: String, root_id: Option<String>) {
        self.open = true;
        self.title = title;
        self.root_id = root_id;
        self.include_chart = true;
    }
}

/// What the website dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebsiteExportEvent {
    pub title: String,
    pub root_id: Option<String>,
    pub include_chart: bool,
}

/// Draws the website export options window.
pub fn show(
    ctx: &Context,
    state: &mut WebsiteExportState,
    people: &[Person],
    surname_first: bool,
) -> Option<WebsiteExportEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("website_export.title"))
        .id(egui::Id::unique("website_export_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(tr("website_export.description"))
                    .weak()
                    .small(),
            );
            ui.horizontal(|ui| {
                ui.label(tr("website_export.site_title"));
                ui.text_edit_singleline(&mut state.title);
            });
            ui.horizontal(|ui| {
                ui.label(tr("website_export.chart_root"));
                let current = state
                    .root_id
                    .as_deref()
                    .and_then(|id| people.iter().find(|person| person.id == id))
                    .map(|person| person_label(person, surname_first))
                    .unwrap_or_else(|| tr("website_export.no_chart").to_string());
                egui::ComboBox::from_id_salt("website_root")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                state.root_id.is_none(),
                                tr("website_export.no_chart"),
                            )
                            .clicked()
                        {
                            state.root_id = None;
                        }
                        for person in people {
                            let selected = state.root_id.as_deref() == Some(person.id.as_str());
                            if ui
                                .selectable_label(selected, person_label(person, surname_first))
                                .clicked()
                            {
                                state.root_id = Some(person.id.clone());
                            }
                        }
                    });
            });
            ui.checkbox(&mut state.include_chart, tr("website_export.include_chart"));
            ui.label(
                RichText::new(tr("website_export.privacy_hint"))
                    .weak()
                    .small(),
            );
            ui.label(
                RichText::new(tr("website_export.folder_hint"))
                    .weak()
                    .small(),
            );
            ui.horizontal(|ui| {
                let title = state.title.trim();
                if ui
                    .add_enabled(
                        !title.is_empty(),
                        egui::Button::new(RichText::new(tr("website_export.export")).strong()),
                    )
                    .clicked()
                {
                    state.open = false;
                    event = Some(WebsiteExportEvent {
                        title: title.to_string(),
                        root_id: state.root_id.clone(),
                        include_chart: state.include_chart,
                    });
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
    fn opens_with_defaults_and_renders() {
        let ctx = Context::default();
        let mut state = WebsiteExportState::default();
        state.open_with("Family".to_string(), Some("p1".to_string()));
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let mut person = Person::new("Ada", "King", "F");
            person.id = "p1".to_string();
            assert!(show(ui.ctx(), &mut state, &[person], false).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert!(state.include_chart);
    }
}
