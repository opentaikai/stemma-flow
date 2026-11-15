//! Printable Ahnentafel list.

use std::path::Path;

use crate::report::html::escape_html;
use crate::report::list_entry::{ListMarriage, event_cell, marriage_cell};

/// One numbered ancestor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AhnentafelEntry {
    pub number: u32,
    pub generation: usize,
    pub name: String,
    pub birth: String,
    pub birth_place: String,
    pub death: String,
    pub death_place: String,
    pub marriages: Vec<ListMarriage>,
}

/// Everything needed to render an Ahnentafel list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AhnentafelListReport {
    pub tree_name: String,
    pub generated_at: String,
    pub root_name: String,
    pub generations: usize,
    pub entries: Vec<AhnentafelEntry>,
}

/// Default timestamped HTML file name.
pub fn file_name(root_name: &str, timestamp: &str) -> String {
    format!(
        "ahnentafel-list-{}-{timestamp}.html",
        sanitize_file_stem(root_name)
    )
}

/// Renders the list as a standalone, printable HTML document.
pub fn render_html(report: &AhnentafelListReport) -> String {
    let mut html = String::with_capacity(4096 + report.entries.len() * 256);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Ahnentafel List \u{2014} ");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>Ahnentafel List</h1><p>");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.generations.to_string());
    html.push_str(" generations \u{00b7} ");
    html.push_str(&report.entries.len().to_string());
    html.push_str(" people</p></header>");

    if report.entries.is_empty() {
        html.push_str("<p class=\"empty\">No ancestors are available for this person.</p>");
        html.push_str("</body></html>");
        return html;
    }

    let mut current_generation = 0;
    for entry in &report.entries {
        if entry.generation != current_generation {
            if current_generation != 0 {
                html.push_str("</tbody></table></section>");
            }
            current_generation = entry.generation;
            html.push_str("<section><h2>Generation ");
            html.push_str(&entry.generation.to_string());
            html.push_str("</h2><table><thead><tr><th>No.</th><th>Name</th><th>Born</th><th>Died</th><th>Marriages</th></tr></thead><tbody>");
        }
        html.push_str("<tr><td class=\"number\">");
        html.push_str(&entry.number.to_string());
        html.push_str("</td><td class=\"name\">");
        html.push_str(&escape_html(&entry.name));
        html.push_str("</td><td>");
        html.push_str(&escape_html(&event_cell(&entry.birth, &entry.birth_place)));
        html.push_str("</td><td>");
        html.push_str(&escape_html(&event_cell(&entry.death, &entry.death_place)));
        html.push_str("</td><td class=\"marriages\">");
        for (index, marriage) in entry.marriages.iter().enumerate() {
            if index > 0 {
                html.push_str("<br>");
            }
            html.push_str(&escape_html(&marriage_cell(marriage)));
        }
        html.push_str("</td></tr>");
    }
    html.push_str("</tbody></table></section>");
    html.push_str("</body></html>");
    html
}

/// Writes the rendered list to `path`.
pub fn write_html(path: &Path, report: &AhnentafelListReport) -> Result<usize, String> {
    let html = render_html(report);
    std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
    Ok(html.len())
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
h2 { margin: 0 0 10px; font-size: 1.05rem; }
p { margin: 2px 0; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
section { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 18px; margin-bottom: 18px; page-break-inside: avoid; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; }
td.number { width: 48px; color: #2563eb; font-weight: 600; }
td.name { font-weight: 600; }
td.marriages { color: #475569; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } section { border: none; padding: 0; border-radius: 0; } thead { display: table-header-group; } tr { break-inside: avoid; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> AhnentafelListReport {
        AhnentafelListReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            root_name: "Ada King".to_string(),
            generations: 4,
            entries: vec![
                AhnentafelEntry {
                    number: 1,
                    generation: 1,
                    name: "Ada & King".to_string(),
                    birth: "1815".to_string(),
                    birth_place: "London".to_string(),
                    death: "1852".to_string(),
                    death_place: "Bath".to_string(),
                    marriages: vec![ListMarriage {
                        spouse: "Charles <Stone>".to_string(),
                        date: "1838".to_string(),
                        place: "Bath".to_string(),
                    }],
                },
                AhnentafelEntry {
                    number: 2,
                    generation: 2,
                    name: "George King".to_string(),
                    birth: "1780".to_string(),
                    birth_place: String::new(),
                    death: String::new(),
                    death_place: String::new(),
                    marriages: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn renders_generations_numbers_vitals_and_marriages() {
        let html = render_html(&report());
        assert!(html.contains("<h1>Ahnentafel List</h1>"), "{html}");
        assert!(html.contains("Generation 1"));
        assert!(html.contains("Generation 2"));
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("1815 \u{00b7} London"));
        assert!(html.contains("m. 1838 \u{00b7} Bath \u{2014} Charles &lt;Stone&gt;"));
        assert!(html.contains("2 people"));
        assert!(html.contains("Family &amp; Friends"));
    }

    #[test]
    fn empty_list_has_a_message() {
        let mut report = report();
        report.entries.clear();
        assert!(render_html(&report).contains("No ancestors"));
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "20260101"),
            "ahnentafel-list-Ada_King-20260101.html"
        );
    }
}
