//! Printable duplicate and missing-information cleanup reports.

use std::path::Path;

use crate::report::html::escape_html;

/// Which cleanup report is being rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CleanupMode {
    #[default]
    Duplicates,
    MissingInformation,
}

impl CleanupMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Duplicates => "Duplicate Report",
            Self::MissingInformation => "Missing Information Report",
        }
    }
}

/// One possible duplicate pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateRow {
    pub first_name: String,
    /// Birth/death summary for the first person.
    pub first_vitals: String,
    pub second_name: String,
    /// Birth/death summary for the second person.
    pub second_vitals: String,
    pub score: u8,
    pub reasons: String,
}

/// One open data problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingRow {
    pub person: String,
    pub problem: String,
    pub severity: String,
    pub details: String,
    pub related: String,
}

/// Everything needed to render one cleanup report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupReport {
    Duplicates {
        tree_name: String,
        generated_at: String,
        rows: Vec<DuplicateRow>,
    },
    MissingInformation {
        tree_name: String,
        generated_at: String,
        rows: Vec<MissingRow>,
    },
}

impl CleanupReport {
    pub fn tree_name(&self) -> &str {
        match self {
            Self::Duplicates { tree_name, .. } | Self::MissingInformation { tree_name, .. } => {
                tree_name
            }
        }
    }

    pub fn generated_at(&self) -> &str {
        match self {
            Self::Duplicates { generated_at, .. }
            | Self::MissingInformation { generated_at, .. } => generated_at,
        }
    }

    pub fn mode(&self) -> CleanupMode {
        match self {
            Self::Duplicates { .. } => CleanupMode::Duplicates,
            Self::MissingInformation { .. } => CleanupMode::MissingInformation,
        }
    }

    /// Default timestamped HTML file name.
    pub fn file_name(&self, timestamp: &str) -> String {
        let prefix = match self.mode() {
            CleanupMode::Duplicates => "duplicates",
            CleanupMode::MissingInformation => "missing-information",
        };
        format!("{prefix}-{timestamp}.html")
    }

    /// Number of duplicate pairs or problem rows.
    pub fn row_count(&self) -> usize {
        match self {
            Self::Duplicates { rows, .. } => rows.len(),
            Self::MissingInformation { rows, .. } => rows.len(),
        }
    }

    /// Renders the report as a standalone, printable HTML document.
    pub fn render_html(&self) -> String {
        let mode = self.mode();
        let (tree_name, generated_at) = match self {
            Self::Duplicates {
                tree_name,
                generated_at,
                ..
            }
            | Self::MissingInformation {
                tree_name,
                generated_at,
                ..
            } => (tree_name, generated_at),
        };
        let count = match mode {
            CleanupMode::Duplicates => format!("{} possible pairs", self.row_count()),
            CleanupMode::MissingInformation => format!("{} open problems", self.row_count()),
        };
        let mut html = String::with_capacity(4096 + self.row_count() * 192);
        html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
        html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
        html.push_str("<title>");
        html.push_str(mode.label());
        html.push_str(" \u{2014} ");
        html.push_str(&escape_html(tree_name));
        html.push_str("</title><style>");
        html.push_str(CSS);
        html.push_str("</style></head><body>");
        html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
        html.push_str("<header><h1>");
        html.push_str(mode.label());
        html.push_str("</h1><p>");
        html.push_str(&escape_html(tree_name));
        html.push_str("</p><p class=\"muted\">");
        html.push_str(&escape_html(generated_at));
        html.push_str(" \u{00b7} ");
        html.push_str(&escape_html(&count));
        html.push_str("</p></header><main>");

        match self {
            Self::Duplicates { rows, .. } => {
                if rows.is_empty() {
                    html.push_str("<p class=\"empty\">No possible duplicates were found.</p>");
                } else {
                    html.push_str("<table><thead><tr><th>Person A</th><th>Person B</th><th>Score</th><th>Reasons</th></tr></thead><tbody>");
                    for row in rows {
                        html.push_str("<tr><td>");
                        person_cell(&mut html, &row.first_name, &row.first_vitals);
                        html.push_str("</td><td>");
                        person_cell(&mut html, &row.second_name, &row.second_vitals);
                        html.push_str("</td><td class=\"score\">");
                        html.push_str(&row.score.to_string());
                        html.push_str("</td><td>");
                        html.push_str(&escape_html(&row.reasons));
                        html.push_str("</td></tr>");
                    }
                    html.push_str("</tbody></table>");
                }
            }
            Self::MissingInformation { rows, .. } => {
                if rows.is_empty() {
                    html.push_str("<p class=\"empty\">No open data problems were found.</p>");
                } else {
                    html.push_str("<table><thead><tr><th>Person</th><th>Problem</th><th>Severity</th><th>Details</th><th>Related person</th></tr></thead><tbody>");
                    for row in rows {
                        html.push_str("<tr><td class=\"name\">");
                        html.push_str(&escape_html(&row.person));
                        html.push_str("</td><td>");
                        html.push_str(&escape_html(&row.problem));
                        html.push_str("</td><td class=\"severity\">");
                        html.push_str(&escape_html(&row.severity));
                        html.push_str("</td><td>");
                        html.push_str(&escape_html(&row.details));
                        html.push_str("</td><td>");
                        html.push_str(&escape_html(&row.related));
                        html.push_str("</td></tr>");
                    }
                    html.push_str("</tbody></table>");
                }
            }
        }
        html.push_str("</main></body></html>");
        html
    }

    /// Writes the rendered report to `path`.
    pub fn write_html(&self, path: &Path) -> Result<usize, String> {
        let html = self.render_html();
        let html = crate::report::print::decorate(
            html,
            crate::model::print_profile::ReportKind::CleanupReports,
            self.tree_name(),
            self.generated_at(),
        );
        std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
        Ok(html.len())
    }
}

fn person_cell(html: &mut String, name: &str, vitals: &str) {
    html.push_str("<div class=\"name\">");
    html.push_str(&escape_html(name));
    html.push_str("</div>");
    if !vitals.trim().is_empty() {
        html.push_str("<div class=\"vitals\">");
        html.push_str(&escape_html(vitals));
        html.push_str("</div>");
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
td.name, .name { font-weight: 600; }
.vitals { color: #64748b; font-size: 11px; font-weight: 400; }
td.score { font-weight: 700; color: #2563eb; }
td.severity { font-weight: 600; }
tbody tr:nth-child(even) { background: #f8fafc; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } main { border: none; padding: 0; border-radius: 0; } thead { display: table-header-group; } tr { break-inside: avoid; } @page { size: landscape; margin: 10mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates() -> CleanupReport {
        CleanupReport::Duplicates {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            rows: vec![DuplicateRow {
                first_name: "Ada <King>".to_string(),
                first_vitals: "b. 1815 \u{00b7} London".to_string(),
                second_name: "Ada King".to_string(),
                second_vitals: "b. 1815".to_string(),
                score: 99,
                reasons: "Exact name, Same birth year".to_string(),
            }],
        }
    }

    fn missing() -> CleanupReport {
        CleanupReport::MissingInformation {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            rows: vec![MissingRow {
                person: "Bo & Tester".to_string(),
                problem: "Missing birth date".to_string(),
                severity: "Warning".to_string(),
                details: "No date".to_string(),
                related: "Ada King".to_string(),
            }],
        }
    }

    #[test]
    fn duplicate_rows_render_both_people_vitals_and_reasons() {
        let html = duplicates().render_html();
        assert!(html.contains("<h1>Duplicate Report</h1>"), "{html}");
        assert!(html.contains("Ada &lt;King&gt;"));
        assert!(html.contains("b. 1815 \u{00b7} London"));
        assert!(html.contains("99"));
        assert!(html.contains("Exact name, Same birth year"));
        assert!(html.contains("1 possible pairs"));
        assert!(html.contains("Family &amp; Friends"));
    }

    #[test]
    fn missing_rows_render_problem_details_and_related_people() {
        let html = missing().render_html();
        assert!(html.contains("<h1>Missing Information Report</h1>"));
        assert!(html.contains("Bo &amp; Tester"));
        assert!(html.contains("Missing birth date"));
        assert!(html.contains("Warning"));
        assert!(html.contains("Ada King"));
        assert!(html.contains("1 open problems"));
    }

    #[test]
    fn empty_reports_have_messages() {
        let html = CleanupReport::Duplicates {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            rows: Vec::new(),
        }
        .render_html();
        assert!(html.contains("No possible duplicates"));

        let html = CleanupReport::MissingInformation {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            rows: Vec::new(),
        }
        .render_html();
        assert!(html.contains("No open data problems"));
    }

    #[test]
    fn file_names_describe_the_mode() {
        assert_eq!(
            duplicates().file_name("20260101"),
            "duplicates-20260101.html"
        );
        assert_eq!(
            missing().file_name("20260101"),
            "missing-information-20260101.html"
        );
        assert_eq!(duplicates().row_count(), 1);
    }
}
