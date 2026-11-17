//! Printable hourglass and bow-tie ancestor/descendant charts.

use std::collections::HashMap;
use std::path::Path;

use egui::{Pos2, Rect};

use crate::gui::layout::{NodeGeom, TreeScene};
use crate::report::html::escape_html;

/// Which combined chart is being rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CombinedChartMode {
    #[default]
    Hourglass,
    BowTie,
}

impl CombinedChartMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hourglass => "Hourglass Chart",
            Self::BowTie => "Bow-tie Chart",
        }
    }
}

/// One person shown in a combined chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombinedPerson {
    pub person_id: String,
    pub name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
}

/// Everything needed to render an hourglass or bow-tie chart.
#[derive(Debug, Clone, PartialEq)]
pub struct CombinedChartReport {
    pub mode: CombinedChartMode,
    pub tree_name: String,
    pub generated_at: String,
    pub root_id: String,
    pub root_name: String,
    pub ancestor_generations: usize,
    pub descendant_generations: usize,
    pub scene: TreeScene,
    pub people: Vec<CombinedPerson>,
}

const PADDING: f32 = 24.0;

/// Default timestamped HTML file name.
pub fn file_name(mode: CombinedChartMode, root_name: &str, timestamp: &str) -> String {
    let prefix = match mode {
        CombinedChartMode::Hourglass => "hourglass",
        CombinedChartMode::BowTie => "bow-tie",
    };
    format!(
        "{prefix}-{}-{timestamp}.html",
        sanitize_file_stem(root_name)
    )
}

/// Default timestamped standalone SVG file name.
pub fn svg_file_name(mode: CombinedChartMode, root_name: &str, timestamp: &str) -> String {
    let prefix = match mode {
        CombinedChartMode::Hourglass => "hourglass",
        CombinedChartMode::BowTie => "bow-tie",
    };
    format!("{prefix}-{}-{timestamp}.svg", sanitize_file_stem(root_name))
}

/// Renders the chart as a standalone, printable HTML document.
pub fn render_html(report: &CombinedChartReport) -> String {
    let mut html = String::with_capacity(8192 + report.scene.nodes.len() * 384);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(report.mode.label());
    html.push_str(" \u{2014} ");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>");
    html.push_str(report.mode.label());
    html.push_str("</h1><p>");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.ancestor_generations.to_string());
    html.push_str(" ancestor generations \u{00b7} ");
    html.push_str(&report.descendant_generations.to_string());
    html.push_str(" descendant generations \u{00b7} ");
    html.push_str(&report.people.len().to_string());
    html.push_str(" people</p></header>");

    if report.scene.nodes.is_empty() {
        html.push_str("<p class=\"empty\">No people are available for this chart.</p>");
        html.push_str("</body></html>");
        return html;
    }

    let placed = place_nodes(report);
    let people: HashMap<&str, &CombinedPerson> = report
        .people
        .iter()
        .map(|person| (person.person_id.as_str(), person))
        .collect();
    let view = chart_view(report, &placed);
    html.push_str("<main class=\"chart\"><svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"");
    html.push_str(&format!(
        "{:.1} {:.1} {:.1} {:.1}",
        view.min_x, view.min_y, view.width, view.height
    ));
    html.push_str("\" role=\"img\" aria-label=\"");
    html.push_str(report.mode.label());
    html.push_str("\">");
    html.push_str("<g class=\"edges\">");
    for (from, to) in connector_segments(report) {
        html.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" />",
            from.x, from.y, to.x, to.y
        ));
    }
    html.push_str("</g><g class=\"cards\">");
    for node in &report.scene.nodes {
        let Some(bounds) = placed
            .iter()
            .find(|placed| placed.person_id == node.person_id)
            .map(|placed| placed.bounds)
        else {
            continue;
        };
        let root = node.person_id == report.root_id;
        html.push_str("<g class=\"card");
        if root {
            html.push_str(" root");
        }
        html.push_str("\"><rect x=\"");
        html.push_str(&format!("{:.1}", bounds.min.x));
        html.push_str("\" y=\"");
        html.push_str(&format!("{:.1}", bounds.min.y));
        html.push_str("\" width=\"");
        html.push_str(&format!("{:.1}", bounds.width()));
        html.push_str("\" height=\"");
        html.push_str(&format!("{:.1}", bounds.height()));
        html.push_str("\" rx=\"8\" />");
        if let Some(person) = people.get(node.person_id.as_str()) {
            html.push_str("<text class=\"name\" x=\"");
            html.push_str(&format!("{:.1}", bounds.min.x + 10.0));
            html.push_str("\" y=\"");
            html.push_str(&format!("{:.1}", bounds.min.y + 22.0));
            html.push_str("\">");
            html.push_str(&escape_html(&truncate(&person.name, 24)));
            html.push_str("</text>");
            let years = year_range(person.birth_year, person.death_year);
            if !years.is_empty() {
                html.push_str("<text class=\"years\" x=\"");
                html.push_str(&format!("{:.1}", bounds.min.x + 10.0));
                html.push_str("\" y=\"");
                html.push_str(&format!("{:.1}", bounds.min.y + 40.0));
                html.push_str("\">");
                html.push_str(&escape_html(&years));
                html.push_str("</text>");
            }
        }
        html.push_str("</g>");
    }
    html.push_str("</g></svg></main></body></html>");
    html
}

/// Writes the rendered chart to `path`.
pub fn write_html(path: &Path, report: &CombinedChartReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::HourglassChart,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
}

struct PlacedNode {
    person_id: String,
    bounds: Rect,
}

fn place_nodes(report: &CombinedChartReport) -> Vec<PlacedNode> {
    report
        .scene
        .nodes
        .iter()
        .map(|node| PlacedNode {
            person_id: node.person_id.clone(),
            bounds: match report.mode {
                CombinedChartMode::Hourglass => node.bounds,
                CombinedChartMode::BowTie => transpose_rect(node),
            },
        })
        .collect()
}

fn transpose_rect(node: &NodeGeom) -> Rect {
    let center = node.bounds.center();
    Rect::from_center_size(Pos2::new(center.y, center.x), node.bounds.size())
}

fn transpose_point(point: Pos2) -> Pos2 {
    Pos2::new(point.y, point.x)
}

fn connector_segments(report: &CombinedChartReport) -> Vec<(Pos2, Pos2)> {
    let mut segments = Vec::new();
    for union in &report.scene.unions {
        let map = |point: Pos2| match report.mode {
            CombinedChartMode::Hourglass => point,
            CombinedChartMode::BowTie => transpose_point(point),
        };
        if let Some([left, right]) = union.spouse_line {
            segments.push((map(left), map(right)));
        }
        let union_point = map(union.union);
        let trunk = map(Pos2::new(union.union.x, union.trunk_y));
        segments.push((union_point, trunk));
        if union.children.is_empty() {
            continue;
        }
        let xs: Vec<f32> = union.children.iter().map(|(_, point)| point.x).collect();
        let left = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let right = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        segments.push((
            map(Pos2::new(left, union.trunk_y)),
            map(Pos2::new(right, union.trunk_y)),
        ));
        for (_, point) in &union.children {
            segments.push((map(Pos2::new(point.x, union.trunk_y)), map(*point)));
        }
    }
    segments
}

struct ChartView {
    min_x: f32,
    min_y: f32,
    width: f32,
    height: f32,
}

fn chart_view(report: &CombinedChartReport, placed: &[PlacedNode]) -> ChartView {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for node in placed {
        min_x = min_x.min(node.bounds.min.x);
        min_y = min_y.min(node.bounds.min.y);
        max_x = max_x.max(node.bounds.max.x);
        max_y = max_y.max(node.bounds.max.y);
    }
    for (from, to) in connector_segments(report) {
        for point in [from, to] {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
    }
    ChartView {
        min_x: min_x - PADDING,
        min_y: min_y - PADDING,
        width: (max_x - min_x) + PADDING * 2.0,
        height: (max_y - min_y) + PADDING * 2.0,
    }
}

fn year_range(birth: Option<i32>, death: Option<i32>) -> String {
    match (birth, death) {
        (Some(birth), Some(death)) => format!("{birth}\u{2013}{death}"),
        (Some(birth), None) => format!("{birth}\u{2013}"),
        (None, Some(death)) => format!("\u{2013}{death}"),
        (None, None) => String::new(),
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut shortened: String = value.chars().take(max.saturating_sub(1)).collect();
    shortened.push('\u{2026}');
    shortened
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
.chart { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 16px; }
.chart svg { width: 100%; max-width: 100%; height: auto; display: block; }
.card rect { fill: #ffffff; stroke: #94a3b8; stroke-width: 1.2; }
.card.root rect { fill: #eff6ff; stroke: #2563eb; stroke-width: 2; }
.card .name { font-size: 12px; font-weight: 600; fill: #0f172a; }
.card .years { font-size: 10px; fill: #475569; }
.edges line { stroke: #94a3b8; stroke-width: 1.2; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } .chart { border: none; padding: 0; border-radius: 0; } @page { size: landscape; margin: 10mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::layout::UnionGeom;
    use egui::vec2;

    fn report(mode: CombinedChartMode) -> CombinedChartReport {
        CombinedChartReport {
            mode,
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            root_id: "root".to_string(),
            root_name: "Ada <King>".to_string(),
            ancestor_generations: 3,
            descendant_generations: 3,
            scene: TreeScene {
                nodes: vec![
                    NodeGeom {
                        person_id: "root".to_string(),
                        display_name: "Ada King".to_string(),
                        bounds: Rect::from_min_size(Pos2::new(100.0, 100.0), vec2(140.0, 56.0)),
                    },
                    NodeGeom {
                        person_id: "father".to_string(),
                        display_name: "John Stone".to_string(),
                        bounds: Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(140.0, 56.0)),
                    },
                    NodeGeom {
                        person_id: "child".to_string(),
                        display_name: "Bea Stone".to_string(),
                        bounds: Rect::from_min_size(Pos2::new(200.0, 250.0), vec2(140.0, 56.0)),
                    },
                ],
                unions: vec![UnionGeom {
                    family_id: "f1".to_string(),
                    spouse_line: Some([Pos2::new(70.0, 56.0), Pos2::new(100.0, 56.0)]),
                    union: Pos2::new(170.0, 128.0),
                    trunk_y: 180.0,
                    trunk_lane: 0,
                    parent_rank_y: 100.0,
                    children: vec![("child".to_string(), Pos2::new(270.0, 250.0))],
                }],
                bounds: Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(340.0, 306.0)),
            },
            people: vec![
                CombinedPerson {
                    person_id: "root".to_string(),
                    name: "Ada & King".to_string(),
                    birth_year: Some(1815),
                    death_year: Some(1852),
                },
                CombinedPerson {
                    person_id: "father".to_string(),
                    name: "John <Stone>".to_string(),
                    birth_year: Some(1780),
                    death_year: None,
                },
                CombinedPerson {
                    person_id: "child".to_string(),
                    name: "Bea Stone".to_string(),
                    birth_year: Some(1840),
                    death_year: None,
                },
            ],
        }
    }

    #[test]
    fn hourglass_renders_cards_connectors_and_years() {
        let html = render_html(&report(CombinedChartMode::Hourglass));
        assert!(html.contains("<h1>Hourglass Chart</h1>"), "{html}");
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("John &lt;Stone&gt;"));
        assert!(html.contains("1815\u{2013}1852"));
        assert!(html.contains("class=\"card root\""));
        assert!(html.contains("<line"));
        assert!(html.contains("Family &amp; Friends"));
        assert!(html.contains("3 ancestor generations"));
        assert!(html.contains("3 descendant generations"));
    }

    #[test]
    fn bow_tie_renders_a_transposed_view() {
        let hourglass = render_html(&report(CombinedChartMode::Hourglass));
        let bow_tie = render_html(&report(CombinedChartMode::BowTie));
        assert!(bow_tie.contains("<h1>Bow-tie Chart</h1>"));
        assert!(bow_tie.contains("Ada &amp; King"));
        assert_ne!(hourglass, bow_tie);
        let root_rect = bow_tie
            .split("<rect x=\"")
            .nth(1)
            .and_then(|part| part.split('"').next())
            .expect("rect");
        assert_ne!(root_rect, "100.0", "root x coordinate is transposed");
    }

    #[test]
    fn empty_scene_has_a_message() {
        let mut report = report(CombinedChartMode::Hourglass);
        report.scene.nodes.clear();
        report.scene.unions.clear();
        assert!(render_html(&report).contains("No people"));
    }

    #[test]
    fn file_names_describe_the_mode() {
        assert_eq!(
            file_name(CombinedChartMode::Hourglass, "Ada / King", "20260101"),
            "hourglass-Ada_King-20260101.html"
        );
        assert_eq!(
            file_name(CombinedChartMode::BowTie, "Ada / King", "20260101"),
            "bow-tie-Ada_King-20260101.html"
        );
    }
}
