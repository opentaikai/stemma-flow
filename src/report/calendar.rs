//! Printable birthday, anniversary and year-calendar reports.

use std::collections::HashMap;
use std::path::Path;

use chrono::{Datelike, Months, NaiveDate};

use crate::model::calendar::CalendarEvent;
use crate::report::html::escape_html;

/// Which calendar report is being rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CalendarMode {
    #[default]
    Birthdays,
    Anniversaries,
    Year,
}

impl CalendarMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Birthdays => "Birthdays",
            Self::Anniversaries => "Anniversaries",
            Self::Year => "Calendar",
        }
    }
}

/// One row in a birthday or anniversary list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEntry {
    pub date: String,
    pub names: String,
    pub years: Option<u32>,
    pub place: String,
}

/// Everything needed to render one calendar report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarReport {
    pub mode: CalendarMode,
    pub tree_name: String,
    pub generated_at: String,
    /// For example `Upcoming 12 months from 16 NOV 2026` or `Year 2026`.
    pub range_label: String,
    pub year: Option<i32>,
    pub entries: Vec<CalendarEntry>,
    pub events: Vec<CalendarEvent>,
}

/// Default timestamped HTML file name.
pub fn file_name(mode: CalendarMode, year: Option<i32>, timestamp: &str) -> String {
    match mode {
        CalendarMode::Birthdays => format!("birthdays-{timestamp}.html"),
        CalendarMode::Anniversaries => format!("anniversaries-{timestamp}.html"),
        CalendarMode::Year => {
            format!("calendar-{}-{timestamp}.html", year.unwrap_or_default())
        }
    }
}

/// Renders the report as a standalone, printable HTML document.
pub fn render_html(report: &CalendarReport) -> String {
    let mut html = String::with_capacity(4096 + report.entries.len() * 128);
    html.push_str(&crate::report::html::document_open());
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(&heading(report)));
    html.push_str(" \u{2014} ");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>");
    html.push_str(&escape_html(&heading(report)));
    html.push_str("</h1><p>");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.range_label));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&event_count(report).to_string());
    html.push_str(" events</p></header>");

    match report.mode {
        CalendarMode::Birthdays | CalendarMode::Anniversaries => render_list(&mut html, report),
        CalendarMode::Year => render_year(&mut html, report),
    }
    html.push_str("</body></html>");
    html
}

/// Writes the rendered report to `path`.
pub fn write_html(path: &Path, report: &CalendarReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::CalendarReports,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
}

fn heading(report: &CalendarReport) -> String {
    match report.mode {
        CalendarMode::Year => format!("Calendar {}", report.year.unwrap_or_default()),
        mode => mode.label().to_string(),
    }
}

fn event_count(report: &CalendarReport) -> usize {
    match report.mode {
        CalendarMode::Year => report.events.len(),
        _ => report.entries.len(),
    }
}

fn render_list(html: &mut String, report: &CalendarReport) {
    let (name_heading, years_heading, empty) = match report.mode {
        CalendarMode::Anniversaries => (
            "Couple",
            "Years",
            "No anniversaries fall in the next 12 months.",
        ),
        _ => ("Name", "Turns", "No birthdays fall in the next 12 months."),
    };
    html.push_str("<main>");
    if report.entries.is_empty() {
        html.push_str("<p class=\"empty\">");
        html.push_str(&escape_html(empty));
        html.push_str("</p>");
    } else {
        html.push_str("<table><thead><tr><th>Date</th><th>");
        html.push_str(name_heading);
        html.push_str("</th><th>");
        html.push_str(years_heading);
        html.push_str("</th><th>Place</th></tr></thead><tbody>");
        for entry in &report.entries {
            html.push_str("<tr><td>");
            html.push_str(&escape_html(&entry.date));
            html.push_str("</td><td class=\"name\">");
            html.push_str(&escape_html(&entry.names));
            html.push_str("</td><td>");
            if let Some(years) = entry.years {
                html.push_str(&years.to_string());
            }
            html.push_str("</td><td>");
            html.push_str(&escape_html(&entry.place));
            html.push_str("</td></tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</main>");
}

fn render_year(html: &mut String, report: &CalendarReport) {
    let year = report.year.unwrap_or_default();
    html.push_str("<p class=\"legend\"><span>b. Birth</span><span>m. Marriage</span><span>d. Death</span></p>");
    if report.events.is_empty() {
        html.push_str("<p class=\"empty\">No dated events are available for this year.</p>");
        return;
    }
    html.push_str("<main class=\"months\">");
    for month in 1..=12 {
        let Some(name) = month_name(year, month) else {
            continue;
        };
        html.push_str("<section class=\"month\"><h2>");
        html.push_str(&escape_html(&name));
        html.push_str("</h2><table><thead><tr>");
        for weekday in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
            html.push_str("<th>");
            html.push_str(weekday);
            html.push_str("</th>");
        }
        html.push_str("</tr></thead><tbody>");
        let mut by_day: HashMap<u32, Vec<&CalendarEvent>> = HashMap::new();
        for event in &report.events {
            if event.month == month {
                by_day.entry(event.day).or_default().push(event);
            }
        }
        for events in by_day.values_mut() {
            events.sort_by(|left, right| {
                left.kind
                    .rank()
                    .cmp(&right.kind.rank())
                    .then_with(|| left.people.join(" ").cmp(&right.people.join(" ")))
            });
        }
        for week in month_grid(year, month) {
            html.push_str("<tr>");
            for day in week {
                match day {
                    Some(day) => {
                        html.push_str("<td class=\"day\"><div class=\"num\">");
                        html.push_str(&day.to_string());
                        html.push_str("</div>");
                        if let Some(events) = by_day.get(&day) {
                            for event in events {
                                html.push_str("<div class=\"event\">");
                                html.push_str(&escape_html(&event.short_label()));
                                html.push_str("</div>");
                            }
                        }
                        html.push_str("</td>");
                    }
                    None => html.push_str("<td class=\"day blank\"></td>"),
                }
            }
            html.push_str("</tr>");
        }
        html.push_str("</tbody></table></section>");
    }
    html.push_str("</main>");
}

/// Monday-first week rows for one month, padded with empty leading/trailing cells.
fn month_grid(year: i32, month: u32) -> Vec<Vec<Option<u32>>> {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return Vec::new();
    };
    let days = first
        .checked_add_months(Months::new(1))
        .map(|next| (next - first).num_days() as u32)
        .unwrap_or(31);
    let offset = first.weekday().num_days_from_monday() as usize;
    let mut weeks = Vec::new();
    let mut week = vec![None; 7];
    let mut slot = offset;
    for day in 1..=days {
        week[slot] = Some(day);
        slot += 1;
        if slot == 7 {
            weeks.push(week);
            week = vec![None; 7];
            slot = 0;
        }
    }
    if slot > 0 {
        weeks.push(week);
    }
    weeks
}

fn month_name(year: i32, month: u32) -> Option<String> {
    NaiveDate::from_ymd_opt(year, month, 1).map(|date| date.format("%B").to_string())
}

const CSS: &str = r#"
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; margin: 24px; color: #1e293b; background: #f8fafc; }
h1 { margin: 0 0 4px; }
h2 { margin: 0 0 6px; font-size: 0.95rem; }
p { margin: 2px 0; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
main { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 16px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; }
td.name { font-weight: 600; }
.months { display: grid; grid-template-columns: repeat(auto-fit, minmax(290px, 1fr)); gap: 16px; background: none; border: none; padding: 0; }
.month { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 12px; page-break-inside: avoid; }
.month th { text-align: center; padding: 2px; border: none; font-size: 10px; }
.month td { padding: 2px; border: 1px solid #e2e8f0; width: 14.28%; height: 52px; }
.month td.blank { background: #f8fafc; }
.month .num { color: #64748b; font-size: 10px; }
.month .event { font-size: 9px; line-height: 1.2; color: #1e293b; }
.legend { display: flex; gap: 18px; color: #64748b; font-size: 12px; margin: 4px 0 14px; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } main { border: none; padding: 0; border-radius: 0; } .months { grid-template-columns: repeat(3, 1fr); } .month { border-color: #bbb; } @page { size: landscape; margin: 8mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::calendar::CalendarEventKind;

    fn list_report(mode: CalendarMode) -> CalendarReport {
        CalendarReport {
            mode,
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            range_label: "Upcoming 12 months from 16 NOV 2026".to_string(),
            year: None,
            entries: vec![CalendarEntry {
                date: "12 JAN 2027".to_string(),
                names: "Ada & King".to_string(),
                years: Some(216),
                place: "London".to_string(),
            }],
            events: Vec::new(),
        }
    }

    fn year_report() -> CalendarReport {
        CalendarReport {
            mode: CalendarMode::Year,
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            range_label: "Year 2024".to_string(),
            year: Some(2024),
            entries: Vec::new(),
            events: vec![
                CalendarEvent {
                    kind: CalendarEventKind::Birth,
                    month: 2,
                    day: 29,
                    year: 2000,
                    people: vec!["Leap & Baby".to_string()],
                    place: String::new(),
                },
                CalendarEvent {
                    kind: CalendarEventKind::Marriage,
                    month: 6,
                    day: 5,
                    year: 1838,
                    people: vec!["Ada".to_string(), "Bo".to_string()],
                    place: String::new(),
                },
            ],
        }
    }

    #[test]
    fn renders_birthday_lists_with_ages_and_places() {
        let html = render_html(&list_report(CalendarMode::Birthdays));
        assert!(html.contains("<h1>Birthdays</h1>"), "{html}");
        assert!(html.contains("12 JAN 2027"));
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("216"));
        assert!(html.contains("London"));
        assert!(html.contains("Turns"));
        assert!(html.contains("<th>Name</th>"));
    }

    #[test]
    fn renders_anniversary_lists_with_couple_and_years() {
        let html = render_html(&list_report(CalendarMode::Anniversaries));
        assert!(html.contains("<h1>Anniversaries</h1>"));
        assert!(html.contains("<th>Couple</th>"));
        assert!(html.contains("<th>Years</th>"));
    }

    #[test]
    fn renders_all_year_months_and_event_cells() {
        let html = render_html(&year_report());
        assert!(html.contains("<h1>Calendar 2024</h1>"), "{html}");
        assert!(html.contains("January"));
        assert!(html.contains("February"));
        assert!(html.contains("December"));
        assert!(html.contains("b. Leap &amp; Baby (2000)"));
        assert!(html.contains("m. Ada &amp; Bo (1838)"));
        assert!(html.contains("Mon"));
        assert!(html.contains("b. Birth"));
    }

    #[test]
    fn month_grid_pads_weeks_and_keeps_leap_days() {
        let february = month_grid(2024, 2);
        assert_eq!(february.len(), 5);
        assert_eq!(february[0][3], Some(1), "1 February 2024 is a Thursday");
        assert!(february.iter().flatten().any(|day| *day == Some(29)));
        let march = month_grid(2026, 3);
        assert_eq!(march[0][6], Some(1), "1 March 2026 is a Sunday");
    }

    #[test]
    fn empty_modes_have_messages() {
        let mut birthdays = list_report(CalendarMode::Birthdays);
        birthdays.entries.clear();
        assert!(render_html(&birthdays).contains("No birthdays"));

        let mut anniversaries = list_report(CalendarMode::Anniversaries);
        anniversaries.entries.clear();
        assert!(render_html(&anniversaries).contains("No anniversaries"));

        let mut year = year_report();
        year.events.clear();
        assert!(render_html(&year).contains("No dated events"));
    }

    #[test]
    fn file_names_describe_the_mode() {
        assert_eq!(
            file_name(CalendarMode::Birthdays, None, "20260101"),
            "birthdays-20260101.html"
        );
        assert_eq!(
            file_name(CalendarMode::Anniversaries, None, "20260101"),
            "anniversaries-20260101.html"
        );
        assert_eq!(
            file_name(CalendarMode::Year, Some(2024), "20260101"),
            "calendar-2024-20260101.html"
        );
    }
}
