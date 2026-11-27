//! Printable kinship report for one subject.

use std::path::Path;

use crate::i18n::{tr, tr_args, trn};
use crate::report::html::escape_html;

/// One person's relationship to the report subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KinshipRow {
    pub name: String,
    pub relationship: String,
    pub birth: String,
    pub birth_place: String,
    pub death: String,
    pub death_place: String,
}

/// Everything needed to render a kinship report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KinshipReport {
    pub tree_name: String,
    pub generated_at: String,
    pub subject_name: String,
    pub only_relatives: bool,
    pub rows: Vec<KinshipRow>,
}

/// Default timestamped HTML file name.
pub fn file_name(subject_name: &str, timestamp: &str) -> String {
    format!(
        "kinship-{}-{timestamp}.html",
        sanitize_file_stem(subject_name)
    )
}

/// Renders the report as a standalone, printable HTML document.
pub fn render_html(report: &KinshipReport) -> String {
    let mut html = String::with_capacity(2048 + report.rows.len() * 256);
    html.push_str(&crate::report::html::document_open());
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(tr("kinship_report.title")));
    html.push_str(" \u{2014} ");
    html.push_str(&escape_html(&report.subject_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">");
    html.push_str(&escape_html(tr("kinship_report.print")));
    html.push_str("</button><span>");
    html.push_str(&escape_html(tr("kinship_report.print_hint")));
    html.push_str("</span></div>");
    html.push_str("<header><h1>");
    html.push_str(&escape_html(tr("kinship_report.title")));
    html.push_str("</h1><p>");
    html.push_str(&escape_html(&tr_args(
        "kinship_report.subject",
        &[("name", report.subject_name.clone())],
    )));
    if report.only_relatives {
        html.push_str(" \u{00b7} ");
        html.push_str(&escape_html(tr("kinship_report.only_relatives")));
    }
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&tr_args(
        "kinship_report.tree",
        &[("name", report.tree_name.clone())],
    )));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&tr_args(
        "kinship_report.generated",
        &[("timestamp", report.generated_at.clone())],
    )));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&trn(
        "kinship_report.count",
        report.rows.len() as i64,
    )));
    html.push_str("</p></header><main>");

    if report.rows.is_empty() {
        html.push_str("<p class=\"empty\">");
        html.push_str(&escape_html(tr("kinship_report.empty")));
        html.push_str("</p>");
    } else {
        html.push_str("<table><thead><tr>");
        for key in [
            "kinship_report.column_name",
            "kinship_report.column_relationship",
            "kinship_report.column_born",
            "kinship_report.column_died",
        ] {
            html.push_str("<th>");
            html.push_str(&escape_html(tr(key)));
            html.push_str("</th>");
        }
        html.push_str("</tr></thead><tbody>");
        for row in &report.rows {
            html.push_str("<tr><td class=\"name\">");
            html.push_str(&escape_html(&row.name));
            html.push_str("</td><td>");
            html.push_str(&escape_html(&row.relationship));
            html.push_str("</td><td>");
            html.push_str(&escape_html(&event_cell(&row.birth, &row.birth_place)));
            html.push_str("</td><td>");
            html.push_str(&escape_html(&event_cell(&row.death, &row.death_place)));
            html.push_str("</td></tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</main></body></html>");
    html
}

/// Writes the rendered report to `path`.
pub fn write_html(path: &Path, report: &KinshipReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::KinshipReport,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
}

fn event_cell(date: &str, place: &str) -> String {
    let date = date.trim();
    let place = place.trim();
    match (date.is_empty(), place.is_empty()) {
        (true, true) => String::new(),
        (false, true) => date.to_string(),
        (true, false) => place.to_string(),
        (false, false) => format!("{date} \u{00b7} {place}"),
    }
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
        "person".to_string()
    } else {
        out
    }
}

const CSS: &str = r#"
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; margin: 24px; color: #1e293b; background: #f8fafc; }
h1 { margin: 0 0 4px; }
p { margin: 2px 0; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
main { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 16px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; }
td.name { font-weight: 600; }
tbody tr:nth-child(even) { background: #f8fafc; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } main { border: none; padding: 0; border-radius: 0; } thead { display: table-header-group; } tr { break-inside: avoid; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::set_language;

    fn report() -> KinshipReport {
        KinshipReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            subject_name: "Ada <King>".to_string(),
            only_relatives: true,
            rows: vec![KinshipRow {
                name: "Charles & Stone".to_string(),
                relationship: "Husband".to_string(),
                birth: "1810".to_string(),
                birth_place: "London".to_string(),
                death: String::new(),
                death_place: String::new(),
            }],
        }
    }

    #[test]
    fn renders_subject_rows_and_escaped_vitals() {
        set_language("en");
        let html = render_html(&report());
        assert!(html.contains("Kinship Report"), "{html}");
        assert!(html.contains("Ada &lt;King&gt;"));
        assert!(html.contains("Charles &amp; Stone"));
        assert!(html.contains("Husband"));
        assert!(html.contains("1810 \u{00b7} London"));
        assert!(html.contains("Relatives only"));
        assert!(html.contains("1 person"));
        set_language("en");
    }

    #[test]
    fn empty_report_has_a_message() {
        set_language("en");
        let mut report = report();
        report.rows.clear();
        assert!(render_html(&report).contains("No people"));
        set_language("en");
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "20260101"),
            "kinship-Ada_King-20260101.html"
        );
    }
}
