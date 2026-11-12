//! Reusable text field with an anchored calendar popup.

use chrono::{Datelike, Months, NaiveDate};
use egui::{Popup, RichText, Ui};

use crate::i18n::tr;
use crate::model::date::{self, DateParts};
use crate::model::date_calc;

/// Draws a date text field with a calendar button. Returns whether the text changed.
pub fn date_edit(ui: &mut Ui, text: &mut String, id_salt: impl egui::AsIdSalt) -> bool {
    let base_id = ui.make_persistent_id(id_salt);
    let state_id = base_id.with("calendar_month");
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui
            .add(egui::TextEdit::singleline(text).hint_text("e.g. 12 JAN 1850"))
            .changed();
        let button = ui
            .small_button(tr("date_picker.button"))
            .on_hover_text(tr("date_picker.tooltip"));
        if button.clicked() {
            let initial = initial_month(text);
            ui.data_mut(|data| data.insert_temp(state_id, initial));
        }
        Popup::from_toggle_button_response(&button).show(|ui| {
            let initial = initial_month(text);
            let mut calendar = ui.data_mut(|data| *data.get_temp_mut_or(state_id, initial));
            calendar_popup(ui, text, &mut calendar, &mut changed);
            ui.data_mut(|data| data.insert_temp(state_id, calendar));
        });
    });
    changed
}

fn initial_month(text: &str) -> (i32, u32) {
    if let Some(parts) = current_parts(text) {
        return (parts.year, parts.month.unwrap_or(1));
    }
    let today = date_calc::today();
    (today.year, today.month.unwrap_or(1))
}

fn current_parts(text: &str) -> Option<DateParts> {
    let canonical = date::canonicalize(text).ok()?;
    date::exact_parts(&canonical)
}

fn calendar_popup(ui: &mut Ui, text: &mut String, calendar: &mut (i32, u32), changed: &mut bool) {
    let (mut year, mut month) = *calendar;
    ui.horizontal(|ui| {
        if ui.small_button("<").clicked() {
            (year, month) = previous_month(year, month);
        }
        let title = DateParts {
            year,
            month: Some(month),
            day: None,
        };
        ui.label(RichText::new(date::format_parts(title)).strong());
        if ui.small_button(">").clicked() {
            (year, month) = next_month(year, month);
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("date_picker.year"));
        let mut input = year;
        if ui
            .add(egui::DragValue::new(&mut input).range(1..=9999))
            .changed()
        {
            year = input;
        }
    });
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        ui.label(RichText::new(tr("date_picker.invalid_month")).color(ui.visuals().error_fg_color));
        *calendar = (year, month);
        return;
    };
    let offset = first.weekday().num_days_from_monday() as usize;
    let days = first
        .checked_add_months(Months::new(1))
        .map(|next| (next - first).num_days() as u32)
        .unwrap_or(0);
    let selected = current_parts(text)
        .filter(|parts| parts.year == year && parts.month == Some(month) && parts.day.is_some());

    const CELL_WIDTH: f32 = 30.0;
    const CELL_HEIGHT: f32 = 22.0;
    egui::Grid::new("date_picker_calendar")
        .num_columns(7)
        .spacing([2.0, 2.0])
        .show(ui, |ui| {
            for key in [
                "date_picker.monday",
                "date_picker.tuesday",
                "date_picker.wednesday",
                "date_picker.thursday",
                "date_picker.friday",
                "date_picker.saturday",
                "date_picker.sunday",
            ] {
                ui.allocate_ui_with_layout(
                    egui::vec2(CELL_WIDTH, CELL_HEIGHT),
                    egui::Layout::centered_and_justified(egui::Direction::TopDown),
                    |ui| {
                        ui.label(RichText::new(tr(key)).small());
                    },
                );
            }
            ui.end_row();

            let mut day = 1;
            for week in 0..6 {
                for slot in 0..7 {
                    let index = week * 7 + slot;
                    if index < offset || day > days {
                        ui.add_sized([CELL_WIDTH, CELL_HEIGHT], egui::Label::new(""));
                        continue;
                    }
                    let is_selected = selected.is_some_and(|parts| parts.day == Some(day));
                    if ui
                        .add_sized(
                            [CELL_WIDTH, CELL_HEIGHT],
                            egui::Button::new(day.to_string()).selected(is_selected),
                        )
                        .clicked()
                    {
                        *text = date::format_parts(DateParts {
                            year,
                            month: Some(month),
                            day: Some(day),
                        });
                        *changed = true;
                        Popup::close_all(ui.ctx());
                    }
                    day += 1;
                }
                ui.end_row();
            }
        });

    ui.separator();
    ui.horizontal(|ui| {
        if ui.small_button(tr("date_picker.today")).clicked() {
            *text = date::format_parts(date_calc::today());
            *changed = true;
            Popup::close_all(ui.ctx());
        }
        if ui.small_button(tr("date_picker.this_month")).clicked() {
            *text = date::format_parts(DateParts {
                year,
                month: Some(month),
                day: None,
            });
            *changed = true;
            Popup::close_all(ui.ctx());
        }
        if ui.small_button(tr("date_picker.this_year")).clicked() {
            *text = year.to_string();
            *changed = true;
            Popup::close_all(ui.ctx());
        }
        if ui.small_button(tr("date_picker.clear")).clicked() {
            text.clear();
            *changed = true;
            Popup::close_all(ui.ctx());
        }
    });
    *calendar = (year, month);
}

fn previous_month(year: i32, month: u32) -> (i32, u32) {
    if month <= 1 {
        (year.saturating_sub(1).max(1), 12)
    } else {
        (year, month - 1)
    }
}

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month >= 12 {
        (year.saturating_add(1).min(9999), 1)
    } else {
        (year, month + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn renders_date_field_headless() {
        let ctx = Context::default();
        let mut text = "12 JAN 1850".to_string();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(!date_edit(ui, &mut text, "birth_date"));
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }

    #[test]
    fn parses_partial_dates_for_initial_month() {
        assert_eq!(initial_month("JAN 1850"), (1850, 1));
        assert_eq!(initial_month("1850"), (1850, 1));
        assert_eq!(initial_month("1850-07-03"), (1850, 7));
    }

    #[test]
    fn calendar_grid_renders_a_non_monday_month() {
        let ctx = Context::default();
        let mut text = "1 FEB 2023".to_string();
        let mut calendar = (2023, 2);
        let mut changed = false;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            calendar_popup(ui, &mut text, &mut calendar, &mut changed);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(!changed, "rendering a month does not change the text");
    }

    #[test]
    fn month_navigation_wraps() {
        assert_eq!(previous_month(1850, 1), (1849, 12));
        assert_eq!(next_month(1850, 12), (1851, 1));
    }
}
