//! Printable administrative lists and tree statistics.

use std::path::Path;

use crate::report::html::escape_html;

/// Which administrative list is being rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AdministrativeMode {
    #[default]
    Places,
    Sources,
    Repositories,
    Statistics,
}

impl AdministrativeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Places => "Place List",
            Self::Sources => "Source List",
            Self::Repositories => "Repository List",
            Self::Statistics => "Statistics",
        }
    }
}

/// One stored place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceRow {
    pub name: String,
    pub events: i64,
    pub coordinates: String,
}

/// One stored source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    pub title: String,
    pub author: String,
    pub publication: String,
    pub citations: i64,
    pub repositories: String,
}

/// One stored repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryRow {
    pub name: String,
    pub address: String,
    pub sources: i64,
}

/// One label/value metric inside a statistics section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticRow {
    pub label: String,
    pub value: String,
}

/// One titled group of statistics rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatisticSection {
    pub title: String,
    pub rows: Vec<StatisticRow>,
}

/// Everything needed to render one administrative report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdministrativeReport {
    Places {
        tree_name: String,
        generated_at: String,
        rows: Vec<PlaceRow>,
    },
    Sources {
        tree_name: String,
        generated_at: String,
        rows: Vec<SourceRow>,
    },
    Repositories {
        tree_name: String,
        generated_at: String,
        rows: Vec<RepositoryRow>,
    },
    Statistics {
        tree_name: String,
        generated_at: String,
        sections: Vec<StatisticSection>,
    },
}

impl AdministrativeReport {
    pub fn tree_name(&self) -> &str {
        match self {
            Self::Places { tree_name, .. }
            | Self::Sources { tree_name, .. }
            | Self::Repositories { tree_name, .. }
            | Self::Statistics { tree_name, .. } => tree_name,
        }
    }

    pub fn generated_at(&self) -> &str {
        match self {
            Self::Places { generated_at, .. }
            | Self::Sources { generated_at, .. }
            | Self::Repositories { generated_at, .. }
            | Self::Statistics { generated_at, .. } => generated_at,
        }
    }

    pub fn mode(&self) -> AdministrativeMode {
        match self {
            Self::Places { .. } => AdministrativeMode::Places,
            Self::Sources { .. } => AdministrativeMode::Sources,
            Self::Repositories { .. } => AdministrativeMode::Repositories,
            Self::Statistics { .. } => AdministrativeMode::Statistics,
        }
    }

    /// Default timestamped HTML file name.
    pub fn file_name(&self, timestamp: &str) -> String {
        let prefix = match self.mode() {
            AdministrativeMode::Places => "places",
            AdministrativeMode::Sources => "sources",
            AdministrativeMode::Repositories => "repositories",
            AdministrativeMode::Statistics => "statistics",
        };
        format!("{prefix}-{timestamp}.html")
    }

    /// Number of table rows or metric rows in the report.
    pub fn row_count(&self) -> usize {
        match self {
            Self::Places { rows, .. } => rows.len(),
            Self::Sources { rows, .. } => rows.len(),
            Self::Repositories { rows, .. } => rows.len(),
            Self::Statistics { sections, .. } => {
                sections.iter().map(|section| section.rows.len()).sum()
            }
        }
    }

    /// Renders the report as a standalone, printable HTML document.
    pub fn render_html(&self) -> String {
        let mode = self.mode();
        let (tree_name, generated_at) = match self {
            Self::Places {
                tree_name,
                generated_at,
                ..
            }
            | Self::Sources {
                tree_name,
                generated_at,
                ..
            }
            | Self::Repositories {
                tree_name,
                generated_at,
                ..
            }
            | Self::Statistics {
                tree_name,
                generated_at,
                ..
            } => (tree_name, generated_at),
        };
        let count = match mode {
            AdministrativeMode::Statistics => format!("{} metrics", self.row_count()),
            AdministrativeMode::Places => format!("{} places", self.row_count()),
            AdministrativeMode::Sources => format!("{} sources", self.row_count()),
            AdministrativeMode::Repositories => format!("{} repositories", self.row_count()),
        };
        let mut html = String::with_capacity(4096 + self.row_count() * 128);
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
            Self::Places { rows, .. } => {
                if rows.is_empty() {
                    html.push_str("<p class=\"empty\">No places are stored.</p>");
                } else {
                    let body: Vec<Vec<String>> = rows
                        .iter()
                        .map(|row| {
                            vec![
                                row.name.clone(),
                                row.events.to_string(),
                                row.coordinates.clone(),
                            ]
                        })
                        .collect();
                    table(&mut html, &["Name", "Events", "Coordinates"], &body);
                }
            }
            Self::Sources { rows, .. } => {
                if rows.is_empty() {
                    html.push_str("<p class=\"empty\">No sources are stored.</p>");
                } else {
                    let body: Vec<Vec<String>> = rows
                        .iter()
                        .map(|row| {
                            vec![
                                row.title.clone(),
                                row.author.clone(),
                                row.publication.clone(),
                                row.citations.to_string(),
                                row.repositories.clone(),
                            ]
                        })
                        .collect();
                    table(
                        &mut html,
                        &[
                            "Title",
                            "Author",
                            "Publication",
                            "Citations",
                            "Repositories",
                        ],
                        &body,
                    );
                }
            }
            Self::Repositories { rows, .. } => {
                if rows.is_empty() {
                    html.push_str("<p class=\"empty\">No repositories are stored.</p>");
                } else {
                    let body: Vec<Vec<String>> = rows
                        .iter()
                        .map(|row| {
                            vec![
                                row.name.clone(),
                                row.address.clone(),
                                row.sources.to_string(),
                            ]
                        })
                        .collect();
                    table(&mut html, &["Name", "Address", "Sources"], &body);
                }
            }
            Self::Statistics { sections, .. } => {
                if sections.is_empty() {
                    html.push_str("<p class=\"empty\">No statistics are available.</p>");
                }
                for section in sections {
                    html.push_str("<section class=\"statistics\"><h2>");
                    html.push_str(&escape_html(&section.title));
                    html.push_str("</h2>");
                    if section.rows.is_empty() {
                        html.push_str("<p class=\"empty\">No data.</p>");
                    } else {
                        let body: Vec<Vec<String>> = section
                            .rows
                            .iter()
                            .map(|row| vec![row.label.clone(), row.value.clone()])
                            .collect();
                        table(&mut html, &["Metric", "Value"], &body);
                    }
                    html.push_str("</section>");
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
            crate::model::print_profile::ReportKind::AdministrativeLists,
            self.tree_name(),
            self.generated_at(),
        );
        std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
        Ok(html.len())
    }
}

fn table(html: &mut String, headings: &[&str], rows: &[Vec<String>]) {
    html.push_str("<table><thead><tr>");
    for heading in headings {
        html.push_str("<th>");
        html.push_str(&escape_html(heading));
        html.push_str("</th>");
    }
    html.push_str("</tr></thead><tbody>");
    for row in rows {
        html.push_str("<tr>");
        for index in 0..headings.len() {
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

const CSS: &str = r#"
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; margin: 24px; color: #1e293b; background: #f8fafc; }
h1 { margin: 0 0 4px; }
h2 { margin: 0 0 10px; font-size: 1rem; }
p { margin: 2px 0; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
main { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 16px; }
section.statistics + section.statistics { margin-top: 20px; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; }
tbody tr:nth-child(even) { background: #f8fafc; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } main { border: none; padding: 0; border-radius: 0; } thead { display: table-header-group; } tr { break-inside: avoid; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn places() -> AdministrativeReport {
        AdministrativeReport::Places {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            rows: vec![PlaceRow {
                name: "London <City>".to_string(),
                events: 3,
                coordinates: "51.5074, -0.1278".to_string(),
            }],
        }
    }

    fn sources() -> AdministrativeReport {
        AdministrativeReport::Sources {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            rows: vec![SourceRow {
                title: "Parish & register".to_string(),
                author: "St Mary".to_string(),
                publication: "1900".to_string(),
                citations: 2,
                repositories: "Archive".to_string(),
            }],
        }
    }

    fn repositories() -> AdministrativeReport {
        AdministrativeReport::Repositories {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            rows: vec![RepositoryRow {
                name: "Archive".to_string(),
                address: "1 High <St>".to_string(),
                sources: 4,
            }],
        }
    }

    fn statistics() -> AdministrativeReport {
        AdministrativeReport::Statistics {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            sections: vec![StatisticSection {
                title: "Records".to_string(),
                rows: vec![
                    StatisticRow {
                        label: "People".to_string(),
                        value: "12".to_string(),
                    },
                    StatisticRow {
                        label: "Events".to_string(),
                        value: "40".to_string(),
                    },
                ],
            }],
        }
    }

    #[test]
    fn place_lists_render_counts_and_coordinates() {
        let html = places().render_html();
        assert!(html.contains("<h1>Place List</h1>"), "{html}");
        assert!(html.contains("London &lt;City&gt;"));
        assert!(html.contains("51.5074, -0.1278"));
        assert!(html.contains("<th>Events</th>"));
        assert!(html.contains("1 places"));
        assert!(html.contains("Family &amp; Friends"));
    }

    #[test]
    fn source_lists_render_authors_publications_and_repositories() {
        let html = sources().render_html();
        assert!(html.contains("<h1>Source List</h1>"));
        assert!(html.contains("Parish &amp; register"));
        assert!(html.contains("St Mary"));
        assert!(html.contains("Archive"));
        assert!(html.contains("<th>Citations</th>"));
    }

    #[test]
    fn repository_lists_render_addresses_and_counts() {
        let html = repositories().render_html();
        assert!(html.contains("<h1>Repository List</h1>"));
        assert!(html.contains("1 High &lt;St&gt;"));
        assert!(html.contains("<th>Sources</th>"));
    }

    #[test]
    fn statistics_render_sections_and_values() {
        let report = statistics();
        let html = report.render_html();
        assert!(html.contains("<h1>Statistics</h1>"));
        assert!(html.contains("<h2>Records</h2>"));
        assert!(html.contains("<td>People</td><td>12</td>"));
        assert_eq!(report.row_count(), 2);
        assert!(html.contains("2 metrics"));
    }

    #[test]
    fn empty_reports_have_mode_specific_messages() {
        let html = AdministrativeReport::Places {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            rows: Vec::new(),
        }
        .render_html();
        assert!(html.contains("No places are stored."));

        let html = AdministrativeReport::Statistics {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            sections: Vec::new(),
        }
        .render_html();
        assert!(html.contains("No statistics are available."));
    }

    #[test]
    fn file_names_describe_the_mode() {
        assert_eq!(places().file_name("20260101"), "places-20260101.html");
        assert_eq!(sources().file_name("20260101"), "sources-20260101.html");
        assert_eq!(
            repositories().file_name("20260101"),
            "repositories-20260101.html"
        );
        assert_eq!(
            statistics().file_name("20260101"),
            "statistics-20260101.html"
        );
    }
}
