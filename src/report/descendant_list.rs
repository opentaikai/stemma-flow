//! Printable descendant list in indented or direct-line form.

use std::path::Path;

use crate::model::descendant_list::DescendantNumbering;
use crate::report::html::escape_html;
use crate::report::list_entry::{ListMarriage, event_cell, marriage_cell};

/// Presentation of a descendant list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DescendantListMode {
    /// Nested full descendancy with one indentation level per generation.
    #[default]
    Indented,
    /// Only the blood line from the root to one chosen descendant.
    Direct,
}

impl DescendantListMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Indented => "Indented",
            Self::Direct => "Direct line",
        }
    }
}

/// One numbered descendant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescendantEntry {
    pub number: String,
    pub depth: usize,
    pub name: String,
    pub birth: String,
    pub birth_place: String,
    pub death: String,
    pub death_place: String,
    pub marriages: Vec<ListMarriage>,
}

/// Everything needed to render a descendant list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescendantListReport {
    pub tree_name: String,
    pub generated_at: String,
    pub root_name: String,
    /// Blood-line end point, set for direct lists.
    pub target_name: Option<String>,
    pub generations: usize,
    pub mode: DescendantListMode,
    pub numbering: DescendantNumbering,
    pub entries: Vec<DescendantEntry>,
}

/// Default timestamped HTML file name.
pub fn file_name(root_name: &str, timestamp: &str) -> String {
    format!(
        "descendant-list-{}-{timestamp}.html",
        sanitize_file_stem(root_name)
    )
}

/// Renders the list as a standalone, printable HTML document.
pub fn render_html(report: &DescendantListReport) -> String {
    let mut html = String::with_capacity(4096 + report.entries.len() * 256);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Descendant List \u{2014} ");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>Descendant List</h1><p>");
    html.push_str(&escape_html(&report.root_name));
    if let Some(target) = &report.target_name {
        html.push_str(" \u{2192} ");
        html.push_str(&escape_html(target));
    }
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(report.mode.label());
    html.push_str(" \u{00b7} ");
    html.push_str(report.numbering.label());
    if report.mode == DescendantListMode::Indented {
        html.push_str(" \u{00b7} ");
        html.push_str(&report.generations.to_string());
        html.push_str(" generations");
    }
    html.push_str(" \u{00b7} ");
    html.push_str(&report.entries.len().to_string());
    html.push_str(" people</p></header>");

    if report.entries.is_empty() {
        html.push_str("<p class=\"empty\">No descendants are available for this person.</p>");
        html.push_str("</body></html>");
        return html;
    }

    html.push_str("<main>");
    for entry in &report.entries {
        let indent = match report.mode {
            DescendantListMode::Indented => entry.depth * 22,
            DescendantListMode::Direct => 0,
        };
        html.push_str("<article class=\"entry\" style=\"margin-left:");
        html.push_str(&indent.to_string());
        html.push_str("px\"><div class=\"head\"><span class=\"number\">");
        html.push_str(&escape_html(&entry.number));
        html.push_str("</span><span class=\"name\">");
        html.push_str(&escape_html(&entry.name));
        html.push_str("</span></div>");
        let birth = event_cell(&entry.birth, &entry.birth_place);
        let death = event_cell(&entry.death, &entry.death_place);
        if !birth.is_empty() || !death.is_empty() {
            html.push_str("<div class=\"details\">");
            if !birth.is_empty() {
                html.push_str("<span>b. ");
                html.push_str(&escape_html(&birth));
                html.push_str("</span>");
            }
            if !death.is_empty() {
                html.push_str("<span>d. ");
                html.push_str(&escape_html(&death));
                html.push_str("</span>");
            }
            html.push_str("</div>");
        }
        for marriage in &entry.marriages {
            html.push_str("<div class=\"marriage\">");
            html.push_str(&escape_html(&marriage_cell(marriage)));
            html.push_str("</div>");
        }
        html.push_str("</article>");
    }
    html.push_str("</main></body></html>");
    html
}

/// Writes the rendered list to `path`.
pub fn write_html(path: &Path, report: &DescendantListReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::DescendantList,
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
p { margin: 2px 0; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
main { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 18px; }
.entry { padding: 7px 0 7px 10px; border-left: 2px solid #e2e8f0; margin-bottom: 4px; page-break-inside: avoid; }
.head { display: flex; gap: 10px; align-items: baseline; }
.number { color: #2563eb; font-weight: 600; font-size: 12px; min-width: 42px; }
.name { font-weight: 600; font-size: 14px; }
.details { display: flex; gap: 14px; flex-wrap: wrap; margin-left: 52px; color: #475569; font-size: 12px; }
.marriage { margin-left: 52px; color: #475569; font-size: 12px; font-style: italic; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } main { border: none; padding: 0; border-radius: 0; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn report(mode: DescendantListMode) -> DescendantListReport {
        DescendantListReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            root_name: "Ada King".to_string(),
            target_name: (mode == DescendantListMode::Direct).then(|| "Bea <Stone>".to_string()),
            generations: 4,
            mode,
            numbering: DescendantNumbering::DAboville,
            entries: vec![
                DescendantEntry {
                    number: "1".to_string(),
                    depth: 0,
                    name: "Ada King".to_string(),
                    birth: "1815".to_string(),
                    birth_place: "London".to_string(),
                    death: String::new(),
                    death_place: String::new(),
                    marriages: vec![ListMarriage {
                        spouse: "Charles Stone".to_string(),
                        date: "1838".to_string(),
                        place: "Bath".to_string(),
                    }],
                },
                DescendantEntry {
                    number: "1.1".to_string(),
                    depth: 1,
                    name: "Bea <Stone>".to_string(),
                    birth: "1840".to_string(),
                    birth_place: String::new(),
                    death: "1900".to_string(),
                    death_place: "Bath".to_string(),
                    marriages: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn renders_numbers_depths_vitals_and_marriages() {
        let html = render_html(&report(DescendantListMode::Indented));
        assert!(html.contains("<h1>Descendant List</h1>"), "{html}");
        assert!(html.contains("Indented"));
        assert!(html.contains("d'Aboville"));
        assert!(html.contains("4 generations"));
        assert!(html.contains("b. 1815 \u{00b7} London"));
        assert!(html.contains("m. 1838 \u{00b7} Bath \u{2014} Charles Stone"));
        assert!(html.contains("Bea &lt;Stone&gt;"));
        assert!(html.contains("margin-left:22px"));
        assert!(html.contains("Family &amp; Friends"));
    }

    #[test]
    fn direct_mode_drops_indentation_and_shows_the_target() {
        let html = render_html(&report(DescendantListMode::Direct));
        assert!(html.contains("Direct line"));
        assert!(html.contains("\u{2192} Bea &lt;Stone&gt;"));
        assert!(!html.contains("margin-left:22px"));
        assert!(!html.contains("4 generations"));
    }

    #[test]
    fn empty_list_has_a_message() {
        let mut report = report(DescendantListMode::Indented);
        report.entries.clear();
        assert!(render_html(&report).contains("No descendants"));
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "20260101"),
            "descendant-list-Ada_King-20260101.html"
        );
    }
}
