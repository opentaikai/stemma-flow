//! Per-report print settings dialog.

use egui::{Context, RichText, Window};

use crate::model::print_profile::{PrintFont, PrintProfile, PrintProfiles, ReportKind};

const DEFAULT_MARGIN_MM: u32 = 12;
const DEFAULT_FONT_SIZE_PT: f32 = 10.5;

/// State owned by the print settings dialog.
#[derive(Debug, Clone)]
pub struct PrintSettingsState {
    pub open: bool,
    pub selected: ReportKind,
    pub profiles: PrintProfiles,
    pub header: String,
    pub footer: String,
    pub margin_override: bool,
    pub margin_mm: u32,
    pub font: PrintFont,
    pub font_size_override: bool,
    pub font_size_pt: f32,
}

impl Default for PrintSettingsState {
    fn default() -> Self {
        Self {
            open: false,
            selected: ReportKind::PedigreeChart,
            profiles: PrintProfiles::default(),
            header: String::new(),
            footer: String::new(),
            margin_override: false,
            margin_mm: DEFAULT_MARGIN_MM,
            font: PrintFont::Default,
            font_size_override: false,
            font_size_pt: DEFAULT_FONT_SIZE_PT,
        }
    }
}

impl PrintSettingsState {
    pub fn open_with(&mut self, profiles: PrintProfiles) {
        self.open = true;
        self.selected = profiles
            .reports
            .keys()
            .next()
            .copied()
            .unwrap_or(ReportKind::PedigreeChart);
        self.profiles = profiles;
        self.load_selected();
    }

    fn load_selected(&mut self) {
        let profile = self
            .profiles
            .get(self.selected)
            .cloned()
            .unwrap_or_default();
        self.header = profile.header;
        self.footer = profile.footer;
        self.margin_override = profile.margin_mm.is_some();
        self.margin_mm = profile.margin_mm.unwrap_or(DEFAULT_MARGIN_MM);
        self.font = profile.font;
        self.font_size_override = profile.font_size_pt.is_some();
        self.font_size_pt = profile.font_size_pt.unwrap_or(DEFAULT_FONT_SIZE_PT);
    }

    fn current_profile(&self) -> PrintProfile {
        PrintProfile {
            header: self.header.clone(),
            footer: self.footer.clone(),
            margin_mm: self.margin_override.then_some(self.margin_mm),
            font: self.font,
            font_size_pt: self.font_size_override.then_some(self.font_size_pt),
        }
    }

    fn commit_current(&mut self) {
        let profile = self.current_profile();
        self.profiles.set(self.selected, profile);
    }

    fn select(&mut self, kind: ReportKind) {
        if kind == self.selected {
            return;
        }
        self.commit_current();
        self.selected = kind;
        self.load_selected();
    }
}

/// Draws the print settings window; returns saved profiles when requested.
pub fn show(ctx: &Context, state: &mut PrintSettingsState) -> Option<PrintProfiles> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut saved = None;
    Window::new("Print Settings")
        .id(egui::Id::unique("print_settings_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Customize headers, footers, margins and fonts for each printable report.",
                )
                .weak()
                .small(),
            );
            ui.horizontal(|ui| {
                ui.label("Report");
                egui::ComboBox::from_id_salt("print_settings_report")
                    .selected_text(state.selected.label())
                    .show_ui(ui, |ui| {
                        for kind in ReportKind::ALL {
                            if ui
                                .selectable_label(state.selected == kind, kind.label())
                                .clicked()
                            {
                                state.select(kind);
                            }
                        }
                    });
            });
            ui.separator();
            egui::Grid::new("print_settings_fields")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Header");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.header)
                            .hint_text("{report} \u{00b7} {tree}"),
                    );
                    ui.end_row();

                    ui.label("Footer");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.footer)
                            .hint_text("{date} \u{00b7} {report}"),
                    );
                    ui.end_row();

                    ui.label("Margin");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut state.margin_override, "Override");
                        ui.add_enabled(
                            state.margin_override,
                            egui::DragValue::new(&mut state.margin_mm)
                                .range(8..=40)
                                .suffix(" mm")
                                .speed(1),
                        );
                    });
                    ui.end_row();

                    ui.label("Font");
                    egui::ComboBox::from_id_salt("print_settings_font")
                        .selected_text(state.font.label())
                        .show_ui(ui, |ui| {
                            for font in [
                                PrintFont::Default,
                                PrintFont::SansSerif,
                                PrintFont::Serif,
                                PrintFont::Monospace,
                            ] {
                                ui.selectable_value(&mut state.font, font, font.label());
                            }
                        });
                    ui.end_row();

                    ui.label("Body size");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut state.font_size_override, "Override");
                        ui.add_enabled(
                            state.font_size_override,
                            egui::DragValue::new(&mut state.font_size_pt)
                                .range(6.0..=24.0)
                                .suffix(" pt")
                                .speed(0.5),
                        );
                    });
                    ui.end_row();
                });
            ui.label(
                RichText::new(
                    "Placeholders: {report}, {tree} and {date}. Empty fields keep the report defaults.",
                )
                .weak()
                .small(),
            );
            ui.horizontal(|ui| {
                if ui.button("Reset this report").clicked() {
                    state.profiles.reset(state.selected);
                    state.load_selected();
                }
                if ui
                    .add(egui::Button::new(RichText::new("Save").strong()))
                    .clicked()
                {
                    state.commit_current();
                    state.open = false;
                    saved = Some(state.profiles.clone());
                }
                if ui.button("Cancel").clicked() {
                    state.open = false;
                }
            });
        });
    if !open {
        state.open = false;
    }
    saved
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn opens_with_existing_profiles_and_renders() {
        let ctx = Context::default();
        let mut profiles = PrintProfiles::default();
        profiles.set(
            ReportKind::KinshipReport,
            PrintProfile {
                header: "Kinship".to_string(),
                font: PrintFont::Serif,
                ..PrintProfile::default()
            },
        );
        let mut state = PrintSettingsState::default();
        state.open_with(profiles);
        assert_eq!(state.selected, ReportKind::KinshipReport);
        assert_eq!(state.header, "Kinship");
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }

    #[test]
    fn switching_reports_keeps_edited_profiles() {
        let mut state = PrintSettingsState::default();
        state.open_with(PrintProfiles::default());
        state.header = "Pedigree header".to_string();
        state.select(ReportKind::FanChart);
        state.header = "Fan header".to_string();
        state.select(ReportKind::PedigreeChart);
        assert_eq!(
            state
                .profiles
                .get(ReportKind::FanChart)
                .map(|profile| profile.header.as_str()),
            Some("Fan header")
        );
        assert_eq!(state.header, "Pedigree header");
    }
}
