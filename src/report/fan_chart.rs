//! Printable radial ancestor fan chart.

use std::collections::HashMap;
use std::path::Path;

use crate::model::fan_chart::{FanSector, layout};
use crate::report::html::escape_html;

/// One numbered ancestor in a fan chart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanChartPerson {
    pub number: u32,
    pub name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
}

/// Everything needed to render a fan chart.
#[derive(Debug, Clone, PartialEq)]
pub struct FanChartReport {
    pub tree_name: String,
    pub generated_at: String,
    pub root_name: String,
    pub generations: usize,
    pub span_degrees: f64,
    pub start_angle: f64,
    pub people: Vec<FanChartPerson>,
}

const CENTER: f64 = 500.0;
const RADIUS: f64 = 460.0;
const PADDING: f64 = 24.0;

/// Default timestamped HTML file name.
pub fn file_name(root_name: &str, timestamp: &str) -> String {
    format!(
        "fan-chart-{}-{timestamp}.html",
        sanitize_file_stem(root_name)
    )
}

/// Default timestamped standalone SVG file name.
pub fn svg_file_name(root_name: &str, timestamp: &str) -> String {
    format!(
        "fan-chart-{}-{timestamp}.svg",
        sanitize_file_stem(root_name)
    )
}

/// Renders the chart as a standalone, printable HTML document.
pub fn render_html(report: &FanChartReport) -> String {
    let mut html = String::with_capacity(8192 + report.people.len() * 256);
    html.push_str(&crate::report::html::document_open());
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Fan Chart \u{2014} ");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>Fan Chart</h1><p>");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.generations.to_string());
    html.push_str(" generations \u{00b7} ");
    html.push_str(&format!(
        "{:.0}\u{00b0} from {:.0}\u{00b0}",
        report.span_degrees, report.start_angle
    ));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.people.len().to_string());
    html.push_str(" people</p></header>");

    if report.people.is_empty() {
        html.push_str("<p class=\"empty\">No ancestor data is available.</p>");
        html.push_str("</body></html>");
        return html;
    }

    let sectors = layout(
        report.generations,
        report.span_degrees,
        report.start_angle,
        RADIUS,
    );
    let people: HashMap<u32, &FanChartPerson> = report
        .people
        .iter()
        .map(|person| (person.number, person))
        .collect();
    let root_radius = RADIUS / report.generations.max(1) as f64;
    let bounds = ChartBounds::new(root_radius);
    let view = bounds.view_box(&sectors);

    html.push_str("<main class=\"chart\"><svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"");
    html.push_str(&format!(
        "{:.1} {:.1} {:.1} {:.1}",
        view.min_x, view.min_y, view.width, view.height
    ));
    html.push_str("\" role=\"img\" aria-label=\"Ancestor fan chart\">");
    html.push_str("<g class=\"sectors\">");
    for sector in &sectors {
        let person = people.get(&sector.number);
        let fill = person
            .map(|_| generation_color(sector.generation))
            .unwrap_or("#f1f5f9");
        html.push_str(&format!(
            "<path d=\"{}\" fill=\"{fill}\" />",
            sector_path(sector)
        ));
    }
    html.push_str("</g><g class=\"labels\">");
    for sector in &sectors {
        let Some(person) = people.get(&sector.number) else {
            continue;
        };
        let mid = (sector.start_angle + sector.end_angle) / 2.0;
        let (x, y) = polar(mid, (sector.inner_radius + sector.outer_radius) / 2.0);
        let mut rotation = mid % 360.0;
        if (90.0..270.0).contains(&rotation) {
            rotation += 180.0;
        }
        let font = if sector.generation <= 3 { 12.0 } else { 10.0 };
        let available = (sector.outer_radius - sector.inner_radius - 10.0).max(12.0);
        let max_chars = (available / (font * 0.55)).floor().max(3.0) as usize;
        html.push_str(&format!(
            "<g class=\"label\" transform=\"translate({x:.1} {y:.1}) rotate({rotation:.1})\"><text text-anchor=\"middle\" y=\"-2\" font-size=\"{font:.0}\">{}</text>",
            escape_html(&truncate(&person.name, max_chars))
        ));
        let years = year_range(person.birth_year, person.death_year);
        if !years.is_empty() {
            html.push_str(&format!(
                "<text class=\"years\" text-anchor=\"middle\" y=\"{:.0}\" font-size=\"{:.0}\">{}</text>",
                font + 1.0,
                (font - 1.5).max(8.0),
                escape_html(&years)
            ));
        }
        html.push_str("</g>");
    }
    html.push_str("</g>");

    html.push_str(&format!(
        "<circle cx=\"{CENTER:.1}\" cy=\"{CENTER:.1}\" r=\"{root_radius:.1}\" class=\"root\" />"
    ));
    if let Some(root) = people.get(&1) {
        let max_chars = (root_radius * 2.0 / (14.0 * 0.55)).floor().max(3.0) as usize;
        html.push_str(&format!(
            "<text x=\"{CENTER:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"14\" class=\"root-name\">{}</text>",
            CENTER - 2.0,
            escape_html(&truncate(&root.name, max_chars))
        ));
        let years = year_range(root.birth_year, root.death_year);
        if !years.is_empty() {
            html.push_str(&format!(
                "<text x=\"{CENTER:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"11\" class=\"years\">{}</text>",
                CENTER + 16.0,
                escape_html(&years)
            ));
        }
    }
    html.push_str("</svg></main></body></html>");
    html
}

/// Writes the rendered chart to `path`.
pub fn write_html(path: &Path, report: &FanChartReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::FanChart,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
}

fn sector_path(sector: &FanSector) -> String {
    let (outer_start_x, outer_start_y) = polar(sector.start_angle, sector.outer_radius);
    let (outer_end_x, outer_end_y) = polar(sector.end_angle, sector.outer_radius);
    let (inner_end_x, inner_end_y) = polar(sector.end_angle, sector.inner_radius);
    let (inner_start_x, inner_start_y) = polar(sector.start_angle, sector.inner_radius);
    let large = if sector.end_angle - sector.start_angle > 180.0 {
        1
    } else {
        0
    };
    format!(
        "M {outer_start_x:.2} {outer_start_y:.2} \
         A {outer:.2} {outer:.2} 0 {large} 1 {outer_end_x:.2} {outer_end_y:.2} \
         L {inner_end_x:.2} {inner_end_y:.2} \
         A {inner:.2} {inner:.2} 0 {large} 0 {inner_start_x:.2} {inner_start_y:.2} Z",
        outer = sector.outer_radius,
        inner = sector.inner_radius,
    )
}

fn polar(angle: f64, radius: f64) -> (f64, f64) {
    let radians = angle.to_radians();
    (
        CENTER + radius * radians.cos(),
        CENTER + radius * radians.sin(),
    )
}

fn generation_color(generation: usize) -> &'static str {
    const COLORS: [&str; 6] = [
        "#dbeafe", "#e0f2fe", "#ede9fe", "#fce7f3", "#fef3c7", "#dcfce7",
    ];
    COLORS[generation.saturating_sub(2) % COLORS.len()]
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

struct ChartBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

struct ViewBox {
    min_x: f64,
    min_y: f64,
    width: f64,
    height: f64,
}

impl ChartBounds {
    fn new(root_radius: f64) -> Self {
        let mut bounds = Self {
            min_x: CENTER - root_radius,
            min_y: CENTER - root_radius,
            max_x: CENTER + root_radius,
            max_y: CENTER + root_radius,
        };
        bounds.include(CENTER, CENTER);
        bounds
    }

    fn include(&mut self, x: f64, y: f64) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn view_box(&self, sectors: &[FanSector]) -> ViewBox {
        let mut min_x = self.min_x;
        let mut min_y = self.min_y;
        let mut max_x = self.max_x;
        let mut max_y = self.max_y;
        for sector in sectors {
            for (angle, radius) in [
                (sector.start_angle, sector.inner_radius),
                (sector.start_angle, sector.outer_radius),
                (sector.end_angle, sector.inner_radius),
                (sector.end_angle, sector.outer_radius),
            ] {
                let (x, y) = polar(angle, radius);
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
        ViewBox {
            min_x: min_x - PADDING,
            min_y: min_y - PADDING,
            width: (max_x - min_x) + PADDING * 2.0,
            height: (max_y - min_y) + PADDING * 2.0,
        }
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
.chart { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 16px; }
.chart svg { width: 100%; max-width: 100%; height: auto; display: block; }
.sectors path { stroke: #ffffff; stroke-width: 1.2; }
.label text { fill: #0f172a; font-weight: 600; font-family: system-ui, sans-serif; }
.label .years { fill: #475569; font-weight: 400; }
.root { fill: #1e3a8a; stroke: #ffffff; stroke-width: 2; }
.root-name { fill: #ffffff; font-weight: 700; font-family: system-ui, sans-serif; }
.chart .years { fill: #dbeafe; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } .chart { border: none; padding: 0; border-radius: 0; } @page { size: landscape; margin: 8mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn report(span: f64, start: f64) -> FanChartReport {
        FanChartReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            root_name: "Ada & King".to_string(),
            generations: 3,
            span_degrees: span,
            start_angle: start,
            people: vec![
                FanChartPerson {
                    number: 1,
                    name: "Ada & King".to_string(),
                    birth_year: Some(1815),
                    death_year: Some(1852),
                },
                FanChartPerson {
                    number: 2,
                    name: "John <Stone>".to_string(),
                    birth_year: Some(1780),
                    death_year: None,
                },
                FanChartPerson {
                    number: 3,
                    name: "Mary King".to_string(),
                    birth_year: None,
                    death_year: Some(1860),
                },
            ],
        }
    }

    #[test]
    fn renders_sectors_root_labels_and_blank_slots() {
        let html = render_html(&report(180.0, 180.0));
        assert!(html.contains("<h1>Fan Chart</h1>"), "{html}");
        assert_eq!(html.matches("<path").count(), 6, "numbers 2 through 7");
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("John &lt;Stone&gt;"));
        assert!(html.contains("1780\u{2013}"));
        assert!(html.contains("\u{2013}1860"));
        assert!(
            html.contains("#f1f5f9"),
            "unknown ancestors get blank sectors"
        );
        assert!(html.contains("180\u{00b0} from 180\u{00b0}"));
        assert!(html.contains("class=\"root\""));
        assert!(html.contains("viewBox="));
    }

    #[test]
    fn full_circle_and_rotation_change_the_geometry() {
        let half = render_html(&report(180.0, 180.0));
        let full = render_html(&report(360.0, 0.0));
        assert!(half.contains("180\u{00b0} from 180\u{00b0}"));
        assert!(full.contains("360\u{00b0} from 0\u{00b0}"));
        assert_ne!(
            first_path(&half),
            first_path(&full),
            "orientation changes the outer sector path"
        );
    }

    fn first_path(html: &str) -> &str {
        let start = html.find("<path d=\"").expect("path") + "<path d=\"".len();
        let end = html[start..].find('"').expect("path end") + start;
        &html[start..end]
    }

    #[test]
    fn empty_chart_has_a_message() {
        let mut report = report(180.0, 180.0);
        report.people.clear();
        assert!(render_html(&report).contains("No ancestor data"));
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "20260101"),
            "fan-chart-Ada_King-20260101.html"
        );
    }
}
