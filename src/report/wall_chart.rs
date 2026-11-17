//! Printable large-format ancestor and descendant wall charts.

use std::path::{Path, PathBuf};

use crate::gui::layout::TreeScene;
use crate::report::html::{escape_html, image_data_uri};

/// Which wall chart is being rendered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WallChartMode {
    #[default]
    Ancestors,
    Descendants,
}

impl WallChartMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ancestors => "Ancestor Wall Chart",
            Self::Descendants => "Descendant Wall Chart",
        }
    }
}

/// Supported large-format page sizes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WallPageSize {
    A3,
    #[default]
    A2,
    A1,
    A0,
}

impl WallPageSize {
    pub fn label(self) -> &'static str {
        match self {
            Self::A3 => "A3",
            Self::A2 => "A2",
            Self::A1 => "A1",
            Self::A0 => "A0",
        }
    }

    /// Width and height in millimetres for one orientation.
    pub fn millimetres(self, orientation: WallOrientation) -> (u32, u32) {
        let (width, height) = match self {
            Self::A3 => (420, 297),
            Self::A2 => (594, 420),
            Self::A1 => (841, 594),
            Self::A0 => (1189, 841),
        };
        match orientation {
            WallOrientation::Landscape => (width, height),
            WallOrientation::Portrait => (height, width),
        }
    }
}

/// Print orientation of a wall chart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WallOrientation {
    #[default]
    Landscape,
    Portrait,
}

impl WallOrientation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Landscape => "Landscape",
            Self::Portrait => "Portrait",
        }
    }
}

/// One person shown on a wall chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallChartPerson {
    pub person_id: String,
    pub name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
}

/// Everything needed to render one wall chart.
#[derive(Debug, Clone, PartialEq)]
pub struct WallChartReport {
    pub mode: WallChartMode,
    pub tree_name: String,
    pub generated_at: String,
    pub root_id: String,
    pub root_name: String,
    pub generations: usize,
    pub scene: TreeScene,
    pub people: Vec<WallChartPerson>,
    pub background: Option<PathBuf>,
    /// 0.0 to 1.0.
    pub background_opacity: f64,
    pub page_size: WallPageSize,
    pub orientation: WallOrientation,
}

const PADDING: f32 = 32.0;

/// Default timestamped HTML file name.
pub fn file_name(mode: WallChartMode, root_name: &str, timestamp: &str) -> String {
    let prefix = match mode {
        WallChartMode::Ancestors => "wall-chart-ancestors",
        WallChartMode::Descendants => "wall-chart-descendants",
    };
    format!(
        "{prefix}-{}-{timestamp}.html",
        sanitize_file_stem(root_name)
    )
}

/// Default timestamped standalone SVG file name.
pub fn svg_file_name(mode: WallChartMode, root_name: &str, timestamp: &str) -> String {
    let prefix = match mode {
        WallChartMode::Ancestors => "wall-chart-ancestors",
        WallChartMode::Descendants => "wall-chart-descendants",
    };
    format!("{prefix}-{}-{timestamp}.svg", sanitize_file_stem(root_name))
}

/// Renders the chart as a standalone, printable HTML document.
pub fn render_html(report: &WallChartReport) -> String {
    let mut html = String::with_capacity(8192 + report.scene.nodes.len() * 384);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(report.mode.label());
    html.push_str(" \u{2014} ");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    let (width, height) = report.page_size.millimetres(report.orientation);
    html.push_str(&format!(
        "@page {{ size: {width}mm {height}mm; margin: 8mm; }}"
    ));
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
    html.push_str(&report.generations.to_string());
    html.push_str(" generations \u{00b7} ");
    html.push_str(report.page_size.label());
    html.push(' ');
    html.push_str(report.orientation.label());
    html.push_str(" \u{00b7} ");
    html.push_str(&report.people.len().to_string());
    html.push_str(" people</p></header>");

    if report.scene.nodes.is_empty() {
        html.push_str("<p class=\"empty\">No people are available for this chart.</p>");
        html.push_str("</body></html>");
        return html;
    }

    let bounds = report.scene.bounds;
    let min_x = bounds.min.x - PADDING;
    let min_y = bounds.min.y - PADDING;
    let width = bounds.width() + PADDING * 2.0;
    let height = bounds.height() + PADDING * 2.0;
    html.push_str("<main class=\"chart\"><svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"");
    html.push_str(&format!("{min_x:.1} {min_y:.1} {width:.1} {height:.1}"));
    html.push_str("\" role=\"img\" aria-label=\"");
    html.push_str(report.mode.label());
    html.push_str("\">");
    if let Some(background) = report.background.as_deref()
        && let Some(uri) = image_data_uri(background)
    {
        html.push_str(&format!(
            "<image href=\"{}\" x=\"{min_x:.1}\" y=\"{min_y:.1}\" width=\"{width:.1}\" height=\"{height:.1}\" preserveAspectRatio=\"xMidYMid slice\" opacity=\"{:.2}\" />",
            escape_html(&uri),
            report.background_opacity.clamp(0.0, 1.0)
        ));
    }
    html.push_str("<g class=\"edges\">");
    for union in &report.scene.unions {
        if let Some([left, right]) = union.spouse_line {
            html.push_str(&line(left, right));
        }
        html.push_str(&line(
            union.union,
            egui::Pos2::new(union.union.x, union.trunk_y),
        ));
        if union.children.is_empty() {
            continue;
        }
        let xs: Vec<f32> = union.children.iter().map(|(_, point)| point.x).collect();
        let left = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let right = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        html.push_str(&line(
            egui::Pos2::new(left, union.trunk_y),
            egui::Pos2::new(right, union.trunk_y),
        ));
        for (_, point) in &union.children {
            html.push_str(&line(egui::Pos2::new(point.x, union.trunk_y), *point));
        }
    }
    html.push_str("</g><g class=\"cards\">");
    for node in &report.scene.nodes {
        let root = node.person_id == report.root_id;
        html.push_str("<g class=\"card");
        if root {
            html.push_str(" root");
        }
        html.push_str("\"><rect x=\"");
        html.push_str(&format!("{:.1}", node.bounds.min.x));
        html.push_str("\" y=\"");
        html.push_str(&format!("{:.1}", node.bounds.min.y));
        html.push_str("\" width=\"");
        html.push_str(&format!("{:.1}", node.bounds.width()));
        html.push_str("\" height=\"");
        html.push_str(&format!("{:.1}", node.bounds.height()));
        html.push_str("\" rx=\"8\" />");
        if let Some(person) = report
            .people
            .iter()
            .find(|person| person.person_id == node.person_id)
        {
            html.push_str("<text class=\"name\" x=\"");
            html.push_str(&format!("{:.1}", node.bounds.min.x + 10.0));
            html.push_str("\" y=\"");
            html.push_str(&format!("{:.1}", node.bounds.min.y + 22.0));
            html.push_str("\">");
            html.push_str(&escape_html(&truncate(&person.name, 24)));
            html.push_str("</text>");
            let years = year_range(person.birth_year, person.death_year);
            if !years.is_empty() {
                html.push_str("<text class=\"years\" x=\"");
                html.push_str(&format!("{:.1}", node.bounds.min.x + 10.0));
                html.push_str("\" y=\"");
                html.push_str(&format!("{:.1}", node.bounds.min.y + 40.0));
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
pub fn write_html(path: &Path, report: &WallChartReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::WallChart,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
}

fn line(from: egui::Pos2, to: egui::Pos2) -> String {
    format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" />",
        from.x, from.y, to.x, to.y
    )
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
.edges line { stroke: #64748b; stroke-width: 1.4; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } .chart { border: none; padding: 0; border-radius: 0; } .chart svg { width: 100%; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::layout::{NodeGeom, UnionGeom};
    use egui::{Pos2, Rect, vec2};

    fn report(mode: WallChartMode, background: Option<PathBuf>) -> WallChartReport {
        WallChartReport {
            mode,
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            root_id: "root".to_string(),
            root_name: "Ada <King>".to_string(),
            generations: 4,
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
                ],
                unions: vec![UnionGeom {
                    family_id: "f1".to_string(),
                    spouse_line: Some([Pos2::new(70.0, 56.0), Pos2::new(100.0, 56.0)]),
                    union: Pos2::new(170.0, 128.0),
                    trunk_y: 180.0,
                    trunk_lane: 0,
                    parent_rank_y: 100.0,
                    children: vec![("root".to_string(), Pos2::new(170.0, 100.0))],
                }],
                bounds: Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(260.0, 170.0)),
            },
            people: vec![
                WallChartPerson {
                    person_id: "root".to_string(),
                    name: "Ada & King".to_string(),
                    birth_year: Some(1815),
                    death_year: Some(1852),
                },
                WallChartPerson {
                    person_id: "father".to_string(),
                    name: "John <Stone>".to_string(),
                    birth_year: Some(1780),
                    death_year: None,
                },
            ],
            background,
            background_opacity: 0.25,
            page_size: WallPageSize::A2,
            orientation: WallOrientation::Landscape,
        }
    }

    fn png() -> PathBuf {
        let path = std::env::temp_dir().join(format!("stemma-wall-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .expect("png writes");
        path
    }

    #[test]
    fn renders_cards_connectors_years_and_root_accent() {
        let html = render_html(&report(WallChartMode::Ancestors, None));
        assert!(html.contains("<h1>Ancestor Wall Chart</h1>"), "{html}");
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("John &lt;Stone&gt;"));
        assert!(html.contains("1815\u{2013}1852"));
        assert!(html.contains("class=\"card root\""));
        assert!(html.contains("<line"));
        assert!(html.contains("Family &amp; Friends"));
        assert!(html.contains("@page { size: 594mm 420mm; margin: 8mm; }"));
        assert!(!html.contains("data:image/png;base64,"));
    }

    #[test]
    fn backgrounds_are_embedded_dimmed_and_page_sizes_follow_orientation() {
        let background = png();
        let mut report = report(WallChartMode::Descendants, Some(background.clone()));
        report.page_size = WallPageSize::A1;
        report.orientation = WallOrientation::Portrait;
        let html = render_html(&report);
        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("opacity=\"0.25\""));
        assert!(html.contains("<h1>Descendant Wall Chart</h1>"));
        assert!(html.contains("@page { size: 594mm 841mm; margin: 8mm; }"));
        let _ = std::fs::remove_file(background);
    }

    #[test]
    fn empty_scene_has_a_message() {
        let mut report = report(WallChartMode::Ancestors, None);
        report.scene.nodes.clear();
        report.scene.unions.clear();
        assert!(render_html(&report).contains("No people"));
    }

    #[test]
    fn file_names_describe_the_mode() {
        assert_eq!(
            file_name(WallChartMode::Ancestors, "Ada / King", "20260101"),
            "wall-chart-ancestors-Ada_King-20260101.html"
        );
        assert_eq!(
            file_name(WallChartMode::Descendants, "Ada / King", "20260101"),
            "wall-chart-descendants-Ada_King-20260101.html"
        );
    }
}
