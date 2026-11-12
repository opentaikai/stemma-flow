//! Soundex, Daitch–Mokotoff and Cologne phonetic calculator.

use egui::{Color32, Context, RichText, Window};

use crate::i18n::tr;
use crate::model::phonetics::{self, PhoneticCodes};

/// State of the Soundex Calculator window.
#[derive(Debug, Default)]
pub struct SoundexState {
    pub open: bool,
    pub name: String,
    pub compare: String,
}

impl SoundexState {
    pub fn clear(&mut self) {
        self.name.clear();
        self.compare.clear();
    }
}

/// Draws the phonetic calculator window.
pub fn show(ctx: &Context, state: &mut SoundexState) {
    if !state.open {
        return;
    }
    let mut open = true;
    Window::new(tr("phonetics.title"))
        .id(egui::Id::unique("soundex_calculator_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("phonetics.hint")).weak().small());
            ui.add_space(4.0);
            egui::Grid::new("soundex_fields")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr("phonetics.name"));
                    ui.text_edit_singleline(&mut state.name);
                    ui.end_row();

                    ui.label(tr("phonetics.compare"));
                    ui.text_edit_singleline(&mut state.compare);
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.name.is_empty() || !state.compare.is_empty(),
                        egui::Button::new(tr("phonetics.clear")),
                    )
                    .clicked()
                {
                    state.clear();
                }
                if ui.button(tr("common.close")).clicked() {
                    state.open = false;
                }
            });
            ui.separator();

            let first = phonetic_codes(&state.name);
            let second = phonetic_codes(&state.compare);
            let Some(first) = first else {
                ui.label(RichText::new(tr("phonetics.prompt")).weak());
                return;
            };
            let compare = if state.compare.trim().is_empty() {
                None
            } else {
                second
            };
            egui::Grid::new("soundex_results")
                .num_columns(if compare.is_some() { 4 } else { 2 })
                .striped(true)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label(RichText::new(tr("phonetics.method")).strong());
                    ui.label(RichText::new(tr("phonetics.name")).strong());
                    if compare.is_some() {
                        ui.label(RichText::new(tr("phonetics.compare")).strong());
                        ui.label(RichText::new(tr("phonetics.match")).strong());
                    }
                    ui.end_row();

                    row(
                        ui,
                        tr("phonetics.soundex"),
                        &first.soundex,
                        compare.as_ref().map(|codes| codes.soundex.as_str()),
                        compare.as_ref().map(|codes| {
                            !first.soundex.is_empty() && first.soundex == codes.soundex
                        }),
                    );
                    row(
                        ui,
                        tr("phonetics.daitch_mokotoff"),
                        &first.daitch_mokotoff.join(", "),
                        compare
                            .as_ref()
                            .map(|codes| codes.daitch_mokotoff.join(", "))
                            .as_deref(),
                        compare.as_ref().map(|codes| {
                            first
                                .daitch_mokotoff
                                .iter()
                                .any(|left| codes.daitch_mokotoff.contains(left))
                        }),
                    );
                    row(
                        ui,
                        tr("phonetics.cologne"),
                        &first.cologne,
                        compare.as_ref().map(|codes| codes.cologne.as_str()),
                        compare.as_ref().map(|codes| {
                            !first.cologne.is_empty() && first.cologne == codes.cologne
                        }),
                    );
                });
        });
    if !open {
        state.open = false;
    }
}

fn phonetic_codes(name: &str) -> Option<PhoneticCodes> {
    if name.trim().is_empty() {
        return None;
    }
    Some(phonetics::codes(name))
}

fn row(
    ui: &mut egui::Ui,
    method: String,
    first: &str,
    second: Option<&str>,
    matched: Option<bool>,
) {
    ui.label(method);
    ui.label(if first.is_empty() { "—" } else { first });
    if let Some(second) = second {
        ui.label(if second.is_empty() { "—" } else { second });
        let (text, color) = match matched {
            Some(true) => ("OK", Color32::from_rgb(22, 163, 74)),
            Some(false) => ("—", ui.visuals().weak_text_color()),
            None => ("", ui.visuals().weak_text_color()),
        };
        ui.label(RichText::new(text).color(color).strong());
    }
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn window_renders_headless_and_compares() {
        let ctx = Context::default();
        let mut state = SoundexState {
            open: true,
            name: "Robert".to_string(),
            compare: "Rupert".to_string(),
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui.ctx(), &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);

        let first = phonetic_codes("Robert").expect("codes");
        let second = phonetic_codes("Rupert").expect("codes");
        assert_eq!(first.soundex, second.soundex);
        assert_eq!(first.cologne, second.cologne);
    }

    #[test]
    fn empty_state_prompts_for_a_name() {
        let ctx = Context::default();
        let mut state = SoundexState {
            open: true,
            ..SoundexState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui.ctx(), &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
