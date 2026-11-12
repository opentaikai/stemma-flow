//! Modeless date difference and date arithmetic calculator.

use chrono::Weekday;
use egui::{Context, RichText, Window};

use crate::gui::date_picker::date_edit;
use crate::i18n::{tr, tr_args};
use crate::model::date;
use crate::model::date_calc::{self, DateCalculation, ShiftUnit};

/// Which calculator form is active.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CalculatorMode {
    #[default]
    Difference,
    Shift,
}

impl CalculatorMode {
    fn label_key(self) -> &'static str {
        match self {
            Self::Difference => "datecalc.mode_difference",
            Self::Shift => "datecalc.mode_shift",
        }
    }
}

/// State of the Date Calculator window.
#[derive(Debug, Clone)]
pub struct DateCalculatorState {
    pub open: bool,
    pub mode: CalculatorMode,
    pub from: String,
    pub to: String,
    pub base: String,
    pub amount: String,
    pub unit: ShiftUnit,
    pub subtract: bool,
    pub result: Option<DateCalculation>,
    pub error: Option<String>,
}

impl Default for DateCalculatorState {
    fn default() -> Self {
        Self {
            open: false,
            mode: CalculatorMode::Difference,
            from: String::new(),
            to: String::new(),
            base: String::new(),
            amount: "1".to_string(),
            unit: ShiftUnit::Days,
            subtract: false,
            result: None,
            error: None,
        }
    }
}

impl DateCalculatorState {
    fn clear_result(&mut self) {
        self.result = None;
        self.error = None;
    }

    fn calculate(&mut self) {
        self.result = None;
        self.error = None;
        match self.mode {
            CalculatorMode::Difference => match date_calc::difference(&self.from, &self.to) {
                Ok(difference) => {
                    self.result = Some(DateCalculation::Difference(difference));
                }
                Err(error) => self.error = Some(error),
            },
            CalculatorMode::Shift => match self.amount.trim().parse::<u32>() {
                Ok(amount) => {
                    match date_calc::shift(&self.base, amount, self.unit, self.subtract) {
                        Ok(shifted) => {
                            if let Some(weekday) = date_calc::weekday(&shifted) {
                                self.result = Some(DateCalculation::Shifted {
                                    date: shifted,
                                    weekday,
                                });
                            } else {
                                self.error = Some("date is out of range".to_string());
                            }
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Err(_) => self.error = Some(tr("datecalc.invalid_amount")),
            },
        }
    }
}

/// Draws the calculator window.
pub fn show(ctx: &Context, state: &mut DateCalculatorState) {
    if !state.open {
        return;
    }
    let mut open = true;
    Window::new(tr("datecalc.title"))
        .id(egui::Id::unique("date_calculator_window"))
        .collapsible(false)
        .resizable(false)
        .default_width(500.0)
        .open(&mut open)
        .show(ctx, |ui| {
            let mut changed = false;
            ui.horizontal(|ui| {
                changed |= ui
                    .selectable_value(
                        &mut state.mode,
                        CalculatorMode::Difference,
                        tr(CalculatorMode::Difference.label_key()),
                    )
                    .changed();
                changed |= ui
                    .selectable_value(
                        &mut state.mode,
                        CalculatorMode::Shift,
                        tr(CalculatorMode::Shift.label_key()),
                    )
                    .changed();
            });
            ui.separator();

            match state.mode {
                CalculatorMode::Difference => {
                    egui::Grid::new("datecalc_difference")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(tr("datecalc.from"));
                            changed |= date_edit(ui, &mut state.from, "datecalc_from");
                            ui.end_row();
                            ui.label(tr("datecalc.to"));
                            changed |= date_edit(ui, &mut state.to, "datecalc_to");
                            ui.end_row();
                        });
                }
                CalculatorMode::Shift => {
                    egui::Grid::new("datecalc_shift")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(tr("datecalc.base"));
                            changed |= date_edit(ui, &mut state.base, "datecalc_base");
                            ui.end_row();
                            ui.label(tr("datecalc.amount"));
                            changed |= ui.text_edit_singleline(&mut state.amount).changed();
                            ui.end_row();
                            ui.label(tr("datecalc.unit"));
                            egui::ComboBox::from_id_salt("datecalc_unit")
                                .selected_text(unit_label(state.unit))
                                .show_ui(ui, |ui| {
                                    for unit in
                                        [ShiftUnit::Days, ShiftUnit::Months, ShiftUnit::Years]
                                    {
                                        changed |= ui
                                            .selectable_value(
                                                &mut state.unit,
                                                unit,
                                                unit_label(unit),
                                            )
                                            .changed();
                                    }
                                });
                            ui.end_row();
                            ui.label(tr("datecalc.operation"));
                            ui.horizontal(|ui| {
                                changed |= ui
                                    .radio_value(&mut state.subtract, false, tr("datecalc.add"))
                                    .changed();
                                changed |= ui
                                    .radio_value(&mut state.subtract, true, tr("datecalc.subtract"))
                                    .changed();
                            });
                            ui.end_row();
                        });
                }
            }
            if changed {
                state.clear_result();
            }

            let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(
                        RichText::new(tr("datecalc.calculate")).strong(),
                    ))
                    .clicked()
                    || enter
                {
                    state.calculate();
                }
                if ui.button(tr("common.close")).clicked() {
                    state.open = false;
                }
            });
            ui.separator();
            if let Some(error) = &state.error {
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            } else if let Some(result) = &state.result {
                match result {
                    DateCalculation::Difference(difference) => {
                        ui.label(RichText::new(tr_args(
                            "datecalc.total_days",
                            &[("count", difference.total_days.to_string())],
                        )));
                        ui.label(tr_args(
                            "datecalc.breakdown",
                            &[
                                ("years", difference.years.to_string()),
                                ("months", difference.months.to_string()),
                                ("days", difference.days.to_string()),
                            ],
                        ));
                        if difference.reversed {
                            ui.label(RichText::new(tr("datecalc.reversed")).weak());
                        }
                    }
                    DateCalculation::Shifted { date, weekday } => {
                        ui.label(RichText::new(tr_args(
                            "datecalc.shifted",
                            &[("date", date::format_parts(*date))],
                        )));
                        ui.label(RichText::new(tr_args(
                            "datecalc.weekday",
                            &[("weekday", weekday_label(*weekday))],
                        )));
                    }
                }
            } else {
                ui.label(RichText::new(tr("datecalc.prompt")).weak());
            }
        });
    if !open {
        state.open = false;
    }
}

fn unit_label(unit: ShiftUnit) -> String {
    tr(match unit {
        ShiftUnit::Days => "datecalc.days",
        ShiftUnit::Months => "datecalc.months",
        ShiftUnit::Years => "datecalc.years",
    })
}

fn weekday_label(weekday: Weekday) -> String {
    tr(match weekday {
        Weekday::Mon => "date_picker.monday",
        Weekday::Tue => "date_picker.tuesday",
        Weekday::Wed => "date_picker.wednesday",
        Weekday::Thu => "date_picker.thursday",
        Weekday::Fri => "date_picker.friday",
        Weekday::Sat => "date_picker.saturday",
        Weekday::Sun => "date_picker.sunday",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn difference_and_shift_calculate() {
        let mut state = DateCalculatorState {
            from: "1 JAN 2000".to_string(),
            to: "1 JAN 2001".to_string(),
            ..DateCalculatorState::default()
        };
        state.calculate();
        assert!(matches!(
            state.result,
            Some(DateCalculation::Difference(ref difference)) if difference.total_days == 366
        ));

        state.mode = CalculatorMode::Shift;
        state.base = "29 FEB 2024".to_string();
        state.amount = "1".to_string();
        state.unit = ShiftUnit::Years;
        state.calculate();
        assert!(matches!(
            state.result,
            Some(DateCalculation::Shifted { ref date, .. })
                if date::format_parts(*date) == "28 FEB 2025"
        ));
    }

    #[test]
    fn invalid_amount_reports_an_error() {
        let mut state = DateCalculatorState {
            mode: CalculatorMode::Shift,
            base: "1 JAN 2000".to_string(),
            amount: "many".to_string(),
            ..DateCalculatorState::default()
        };
        state.calculate();
        assert!(state.result.is_none());
        assert!(state.error.is_some());
    }

    #[test]
    fn window_renders_headless() {
        let ctx = Context::default();
        let mut state = DateCalculatorState {
            open: true,
            ..DateCalculatorState::default()
        };
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(ui.ctx(), &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
    }
}
