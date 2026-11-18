//! Printable list of people, matching the current People view.

use std::path::Path;

use crate::report::html::escape_html;

/// Everything needed to render one individual list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndividualListReport {
    pub tree_name: String,
    pub generated_at: String,
    /// Active simple filter or advanced-search description.
    pub filter: Vec<String>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Default timestamped HTML file name.
pub fn file_name(timestamp: &str) -> String {
    format!("individual-list-{timestamp}.html")
}

/// Renders the list as a standalone, printable HTML document.
pub fn render_html(report: &IndividualListReport) -> String {
    let mut html = String::with_capacity(2048 + report.rows.len() * report.columns.len() * 24);
    html.push_str(&crate::report::html::document_open());
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Individual List \u{2014} ");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>Individual List</h1><p>");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.rows.len().to_string());
    html.push_str(" people</p>");
    if !report.filter.is_empty() {
        html.push_str("<ul class=\"filter\">");
        for line in &report.filter {
            html.push_str("<li>");
            html.push_str(&escape_html(line));
            html.push_str("</li>");
        }
        html.push_str("</ul>");
    }
    html.push_str("</header><main>");

    if report.rows.is_empty() {
        html.push_str("<p class=\"empty\">No people match this list.</p>");
    } else {
        html.push_str("<table><thead><tr>");
        for column in &report.columns {
            html.push_str("<th>");
            html.push_str(&escape_html(column));
            html.push_str("</th>");
        }
        html.push_str("</tr></thead><tbody>");
        for row in &report.rows {
            html.push_str("<tr>");
            for index in 0..report.columns.len() {
                html.push_str("<td>");
                html.push_str(&escape_html(
                    row.get(index).map(String::as_str).unwrap_or(""),
                ));
                html.push_str("</td>");
            }
            html.push_str("</tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</main></body></html>");
    html
}

/// Writes the rendered list to `path`.
pub fn write_html(path: &Path, report: &IndividualListReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::IndividualList,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
}

const CSS: &str = r#"
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; margin: 24px; color: #1e293b; background: #f8fafc; }
h1 { margin: 0 0 4px; }
p { margin: 2px 0; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
.filter { margin: 10px 0 0; padding-left: 20px; color: #475569; font-size: 13px; }
main { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 16px; }
table { width: 100%; border-collapse: collapse; font-size: 12px; }
th, td { text-align: left; padding: 5px 7px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; }
tbody tr:nth-child(even) { background: #f8fafc; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } main { border: none; padding: 0; border-radius: 0; } thead { display: table-header-group; } tr { break-inside: avoid; } @page { size: landscape; margin: 10mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> IndividualListReport {
        IndividualListReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            filter: vec!["Place: London".to_string()],
            columns: vec!["Name".to_string(), "Birth".to_string(), "Death".to_string()],
            rows: vec![
                vec![
                    "Ada & King".to_string(),
                    "1815".to_string(),
                    "1852".to_string(),
                ],
                vec!["Bea <Stone>".to_string(), "1840".to_string(), String::new()],
            ],
        }
    }

    #[test]
    fn renders_columns_rows_and_filter() {
        let html = render_html(&report());
        assert!(html.contains("<h1>Individual List</h1>"));
        assert!(html.contains("Family &amp; Friends"));
        assert!(html.contains("<th>Name</th>"));
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("Bea &lt;Stone&gt;"));
        assert!(html.contains("Place: London"));
        assert!(html.contains("2 people"));
    }

    #[test]
    fn empty_list_has_a_message() {
        let mut report = report();
        report.rows.clear();
        report.filter.clear();
        let html = render_html(&report);
        assert!(html.contains("No people match this list."));
        assert!(!html.contains("class=\"filter\""));
    }

    #[test]
    fn short_rows_do_not_overflow() {
        let mut report = report();
        report.columns.push("Notes".to_string());
        let html = render_html(&report);
        assert!(html.contains("Ada &amp; King</td><td>1815</td><td>1852</td><td></td>"));
    }

    #[test]
    fn file_name_is_timestamped() {
        assert_eq!(file_name("20260101"), "individual-list-20260101.html");
    }
}
