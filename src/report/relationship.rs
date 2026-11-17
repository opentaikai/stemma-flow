//! Printable relationship report between two people.

use std::path::Path;

use crate::i18n::{tr, tr_args};
use crate::report::html::escape_html;

/// One step in the shortest path from the first to the second person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipHop {
    /// Localized step label, for example `parent of`.
    pub relation: String,
    pub person: String,
}

/// Everything needed to render a relationship report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipReport {
    pub tree_name: String,
    pub generated_at: String,
    pub from_name: String,
    pub to_name: String,
    pub kinship: String,
    pub common_ancestor: Option<String>,
    pub hops: Vec<RelationshipHop>,
}

/// Default timestamped HTML file name.
pub fn file_name(from_name: &str, to_name: &str, timestamp: &str) -> String {
    format!(
        "relationship-{}-{}-{timestamp}.html",
        sanitize_file_stem(from_name),
        sanitize_file_stem(to_name)
    )
}

/// Renders the report as a standalone, printable HTML document.
pub fn render_html(report: &RelationshipReport) -> String {
    let mut html = String::with_capacity(2048 + report.hops.len() * 128);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(&tr("relationship_report.title")));
    html.push_str(" \u{2014} ");
    html.push_str(&escape_html(&report.from_name));
    html.push_str(" \u{2192} ");
    html.push_str(&escape_html(&report.to_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">");
    html.push_str(&escape_html(&tr("relationship_report.print")));
    html.push_str("</button><span>");
    html.push_str(&escape_html(&tr("relationship_report.print_hint")));
    html.push_str("</span></div>");
    html.push_str("<header><h1>");
    html.push_str(&escape_html(&tr("relationship_report.title")));
    html.push_str("</h1><p class=\"pair\">");
    html.push_str(&escape_html(&report.from_name));
    html.push_str(" \u{2192} ");
    html.push_str(&escape_html(&report.to_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&tr_args(
        "relationship_report.tree",
        &[("name", report.tree_name.clone())],
    )));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&tr_args(
        "relationship_report.generated",
        &[("timestamp", report.generated_at.clone())],
    )));
    html.push_str("</p></header><main>");

    html.push_str("<section class=\"result\"><h2>");
    html.push_str(&escape_html(&tr("relationship_report.relationship")));
    html.push_str("</h2><p class=\"kinship\">");
    html.push_str(&escape_html(&report.kinship));
    html.push_str("</p>");
    if let Some(ancestor) = &report.common_ancestor {
        html.push_str("<p class=\"muted\">");
        html.push_str(&escape_html(&tr_args(
            "relationship_report.common_ancestor",
            &[("name", ancestor.clone())],
        )));
        html.push_str("</p>");
    }
    html.push_str("</section>");

    html.push_str("<section><h2>");
    html.push_str(&escape_html(&tr("relationship_report.path")));
    html.push_str("</h2>");
    if report.hops.is_empty() {
        html.push_str("<p class=\"empty\">");
        html.push_str(&escape_html(&tr("relationship_report.empty_path")));
        html.push_str("</p>");
    } else {
        html.push_str("<ol class=\"path\">");
        for hop in &report.hops {
            html.push_str("<li><span class=\"relation\">");
            html.push_str(&escape_html(&hop.relation));
            html.push_str("</span> <span class=\"person\">");
            html.push_str(&escape_html(&hop.person));
            html.push_str("</span></li>");
        }
        html.push_str("</ol>");
    }
    html.push_str("</section></main></body></html>");
    html
}

/// Writes the rendered report to `path`.
pub fn write_html(path: &Path, report: &RelationshipReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::RelationshipReport,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
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
h2 { margin: 0 0 8px; font-size: 1rem; }
p { margin: 2px 0; }
.pair { font-size: 1.15rem; font-weight: 600; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
main { display: grid; gap: 18px; }
section { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 18px; page-break-inside: avoid; }
.kinship { font-size: 1.5rem; font-weight: 700; margin: 2px 0 6px; }
.path { margin: 0; padding-left: 22px; font-size: 13px; }
.path li { padding: 4px 0; }
.relation { color: #64748b; }
.person { font-weight: 600; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } section { border: none; padding: 0; border-radius: 0; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::set_language;

    fn report() -> RelationshipReport {
        RelationshipReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            from_name: "Ada <King>".to_string(),
            to_name: "Bea & Stone".to_string(),
            kinship: "Daughter".to_string(),
            common_ancestor: Some("George <King>".to_string()),
            hops: vec![
                RelationshipHop {
                    relation: "child of".to_string(),
                    person: "Bea & Stone".to_string(),
                },
                RelationshipHop {
                    relation: "parent of".to_string(),
                    person: "George <King>".to_string(),
                },
            ],
        }
    }

    #[test]
    fn renders_pair_kinship_ancestor_and_path() {
        set_language("en");
        let html = render_html(&report());
        assert!(html.contains("Relationship Report"), "{html}");
        assert!(html.contains("Ada &lt;King&gt; \u{2192} Bea &amp; Stone"));
        assert!(html.contains("Daughter"));
        assert!(html.contains("George &lt;King&gt;"));
        assert!(html.contains("<ol class=\"path\">"));
        assert!(html.contains("child of"));
        set_language("en");
    }

    #[test]
    fn empty_path_has_a_message() {
        set_language("en");
        let mut report = report();
        report.hops.clear();
        report.common_ancestor = None;
        let html = render_html(&report);
        assert!(html.contains("No path"));
        assert!(!html.contains("Common ancestor"));
        set_language("en");
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "Bea Stone", "20260101"),
            "relationship-Ada_King-Bea_Stone-20260101.html"
        );
    }
}
