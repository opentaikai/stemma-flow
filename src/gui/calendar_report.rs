//! Birthday, anniversary and year-calendar export options.

use egui::{Context, RichText, Window};

use crate::model::date_calc;
use crate::report::calendar::CalendarMode;

/// State owned by the calendar report dialog.
#[derive(Debug, Clone)]
pub struct CalendarReportState {
    pub open: bool,
    pub mode: CalendarMode,
    pub year: i32,
}

impl Default for CalendarReportState {
    fn default() -> Self {
        Self {
            open: false,
            mode: CalendarMode::Birthdays,
            year: date_calc::today().year,
        }
    }
}

impl CalendarReportState {
    pub fn open_with(&mut self) {
        self.open = true;
    }
}

/// What the calendar report dialog asks the shell to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarReportEvent {
    pub mode: CalendarMode,
    pub year: i32,
}

/// Draws the calendar report options window.
pub fn show(ctx: &Context, state: &mut CalendarReportState) -> Option<CalendarReportEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Calendar Reports")
        .id(egui::Id::unique("calendar_report_window"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(
                    "Export upcoming birthdays, marriage anniversaries or a full year calendar.",
                )
                .weak()
                .small(),
            );
            ui.horizontal(|ui| {
                ui.label("Report");
                ui.selectable_value(&mut state.mode, CalendarMode::Birthdays, "Birthdays");
                ui.selectable_value(
                    &mut state.mode,
                    CalendarMode::Anniversaries,
                    "Anniversaries",
                );
                ui.selectable_value(&mut state.mode, CalendarMode::Year, "Year calendar");
            });
            match state.mode {
                CalendarMode::Birthdays => {
                    ui.label(
                        RichText::new(
                            "Birthdays in the next twelve months with the age reached.",
                        )
                        .weak()
                        .small(),
                    );
                }
                CalendarMode::Anniversaries => {
                    ui.label(
                        RichText::new(
                            "Marriage anniversaries in the next twelve months with the years reached.",
                        )
                        .weak()
                        .small(),
                    );
                }
                CalendarMode::Year => {
                    ui.horizontal(|ui| {
                        ui.label("Year");
                        ui.add(
                            egui::DragValue::new(&mut state.year)
                                .range(1..=9999)
                                .speed(1),
                        );
                    });
                    ui.label(
                        RichText::new(
                            "Twelve month grids with births, marriages and deaths.",
                        )
                        .weak()
                        .small(),
                    );
                }
            }
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(
                        RichText::new("Export\u{2026}").strong(),
                    ))
                    .clicked()
                {
                    state.open = false;
                    event = Some(CalendarReportEvent {
                        mode: state.mode,
                        year: state.year,
                    });
                }
                if ui.button("Cancel").clicked() {
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
    fn opens_with_the_current_year_and_renders() {
        let ctx = Context::default();
        let mut state = CalendarReportState::default();
        state.open_with();
        let expected_year = date_calc::today().year;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(state.open);
        assert_eq!(state.mode, CalendarMode::Birthdays);
        assert_eq!(state.year, expected_year);
    }
}
