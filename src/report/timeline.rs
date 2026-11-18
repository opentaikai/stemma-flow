//! Printable person and group timelines.

use std::path::Path;

use crate::report::html::escape_html;

/// Whether the timeline covers one person or a group.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TimelineMode {
    #[default]
    Person,
    Group,
}

impl TimelineMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Person => "Person",
            Self::Group => "Group",
        }
    }
}

/// One dated or undated timeline row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    /// Original GEDCOM date; empty for undated facts.
    pub date: String,
    /// Person name, or the members involved for a group timeline.
    pub person: String,
    pub event: String,
    pub detail: String,
    pub place: String,
    pub age: Option<String>,
}

/// Everything needed to render a timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineReport {
    pub mode: TimelineMode,
    pub tree_name: String,
    pub generated_at: String,
    pub subject_name: String,
    pub entries: Vec<TimelineEntry>,
}

/// Default timestamped HTML file name.
pub fn file_name(subject_name: &str, timestamp: &str) -> String {
    format!(
        "timeline-{}-{timestamp}.html",
        sanitize_file_stem(subject_name)
    )
}

/// Renders the timeline as a standalone, printable HTML document.
pub fn render_html(report: &TimelineReport) -> String {
    let mut html = String::with_capacity(4096 + report.entries.len() * 192);
    html.push_str(&crate::report::html::document_open());
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(heading(report)));
    html.push_str(" \u{2014} ");
    html.push_str(&escape_html(&report.subject_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>");
    html.push_str(&escape_html(heading(report)));
    html.push_str("</h1><p class=\"subject\">");
    html.push_str(&escape_html(&report.subject_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.entries.len().to_string());
    html.push_str(" events</p></header>");

    if report.entries.is_empty() {
        let message = match report.mode {
            TimelineMode::Person => "No events are recorded for this person.",
            TimelineMode::Group => "No events are recorded for this group.",
        };
        html.push_str("<p class=\"empty\">");
        html.push_str(message);
        html.push_str("</p></body></html>");
        return html;
    }

    let (dated, undated): (Vec<_>, Vec<_>) = report
        .entries
        .iter()
        .partition(|entry| !entry.date.trim().is_empty());
    if !dated.is_empty() {
        html.push_str("<main><table>");
        render_head(&mut html, report.mode);
        html.push_str("<tbody>");
        for entry in &dated {
            render_row(&mut html, report.mode, entry);
        }
        html.push_str("</tbody></table></main>");
    }
    if !undated.is_empty() {
        html.push_str("<section class=\"undated\"><h2>Undated</h2><table>");
        render_head(&mut html, report.mode);
        html.push_str("<tbody>");
        for entry in &undated {
            render_row(&mut html, report.mode, entry);
        }
        html.push_str("</tbody></table></section>");
    }
    html.push_str("</body></html>");
    html
}

/// Writes the rendered timeline to `path`.
pub fn write_html(path: &Path, report: &TimelineReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::TimelineReport,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
}

fn heading(report: &TimelineReport) -> &'static str {
    match report.mode {
        TimelineMode::Person => "Timeline",
        TimelineMode::Group => "Group Timeline",
    }
}

fn render_head(html: &mut String, mode: TimelineMode) {
    html.push_str("<thead><tr><th>Date</th>");
    if mode == TimelineMode::Group {
        html.push_str("<th>Person</th>");
    }
    html.push_str("<th>Event</th><th>Details</th><th>Place</th><th>Age</th></tr></thead>");
}

fn render_row(html: &mut String, mode: TimelineMode, entry: &TimelineEntry) {
    html.push_str("<tr><td class=\"date\">");
    if entry.date.trim().is_empty() {
        html.push('\u{2014}');
    } else {
        html.push_str(&escape_html(&entry.date));
    }
    html.push_str("</td>");
    if mode == TimelineMode::Group {
        html.push_str("<td class=\"person\">");
        html.push_str(&escape_html(&entry.person));
        html.push_str("</td>");
    }
    html.push_str("<td class=\"event\">");
    html.push_str(&escape_html(&entry.event));
    html.push_str("</td><td>");
    html.push_str(&escape_html(&entry.detail));
    html.push_str("</td><td>");
    html.push_str(&escape_html(&entry.place));
    html.push_str("</td><td>");
    if let Some(age) = &entry.age {
        html.push_str(&escape_html(age));
    }
    html.push_str("</td></tr>");
}

fn sanitize_file_stem(value: &str) -> String {
    let out: String = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let out = out
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if out.is_empty() {
        "timeline".to_string()
    } else {
        out
    }
}

const CSS: &str = r#"
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; margin: 24px; color: #1e293b; background: #f8fafc; }
h1 { margin: 0 0 4px; }
h2 { margin: 0 0 10px; font-size: 1rem; }
p { margin: 2px 0; }
.subject { font-size: 1.1rem; font-weight: 600; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
main, .undated { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 16px; margin-bottom: 18px; page-break-inside: avoid; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; }
td.date { white-space: nowrap; color: #475569; }
td.person, td.event { font-weight: 600; }
tbody tr:nth-child(even) { background: #f8fafc; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } main, .undated { border: none; padding: 0; border-radius: 0; } thead { display: table-header-group; } tr { break-inside: avoid; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(date: &str, person: &str, event: &str) -> TimelineEntry {
        TimelineEntry {
            date: date.to_string(),
            person: person.to_string(),
            event: event.to_string(),
            detail: format!("{event} detail"),
            place: "London".to_string(),
            age: Some("30".to_string()),
        }
    }

    fn report(mode: TimelineMode) -> TimelineReport {
        TimelineReport {
            mode,
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            subject_name: "Ada <King>".to_string(),
            entries: vec![
                entry("12 JAN 1815", "Ada <King>", "Birth"),
                entry("5 JUN 1838", "Ada <King> & Bo", "Marriage"),
                entry("", "Ada <King>", "Occupation"),
            ],
        }
    }

    #[test]
    fn renders_person_timeline_and_undated_section() {
        let html = render_html(&report(TimelineMode::Person));
        assert!(html.contains("<h1>Timeline</h1>"), "{html}");
        assert!(html.contains("Ada &lt;King&gt;"));
        assert!(html.contains("12 JAN 1815"));
        assert!(html.contains("Undated"));
        assert!(html.contains("Occupation"));
        assert!(!html.contains("<th>Person</th>"));
        assert!(html.contains("<th>Age</th>"));
        assert!(html.contains("Family &amp; Friends"));
    }

    #[test]
    fn group_timeline_adds_the_person_column() {
        let html = render_html(&report(TimelineMode::Group));
        assert!(html.contains("<h1>Group Timeline</h1>"));
        assert!(html.contains("<th>Person</th>"));
    }

    #[test]
    fn empty_timelines_name_their_mode() {
        let mut person = report(TimelineMode::Person);
        person.entries.clear();
        assert!(render_html(&person).contains("No events are recorded for this person."));

        let mut group = report(TimelineMode::Group);
        group.entries.clear();
        assert!(render_html(&group).contains("No events are recorded for this group."));
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "20260101"),
            "timeline-Ada_King-20260101.html"
        );
        assert_eq!(
            file_name("  ", "20260101"),
            "timeline-timeline-20260101.html"
        );
    }
}
