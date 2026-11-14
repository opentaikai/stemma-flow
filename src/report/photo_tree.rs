//! Printable ancestor photo chart.

use std::path::{Path, PathBuf};

use crate::gui::layout::TreeScene;
use crate::report::html::{escape_html, image_data_uri};

/// One person in the photo tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoTreePerson {
    pub person_id: String,
    pub name: String,
    pub birth: String,
    pub death: String,
    pub photo: Option<PathBuf>,
}

/// Everything needed to render an ancestor photo chart.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoTreeReport {
    pub tree_name: String,
    pub generated_at: String,
    pub root_name: String,
    pub generations: usize,
    pub scene: TreeScene,
    pub people: Vec<PhotoTreePerson>,
}

/// Default timestamped HTML file name.
pub fn file_name(root_name: &str, timestamp: &str) -> String {
    format!(
        "photo-tree-{}-{timestamp}.html",
        sanitize_file_stem(root_name)
    )
}

/// Renders the chart as a standalone, printable HTML document.
pub fn render_html(report: &PhotoTreeReport) -> String {
    let mut html = String::with_capacity(8192 + report.scene.nodes.len() * 512);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Photo Tree \u{2014} ");
    html.push_str(&escape_html(&report.root_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>Photo Tree</h1><p>");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.generations.to_string());
    html.push_str(" generations</p></header>");

    if report.scene.nodes.is_empty() {
        html.push_str("<p class=\"empty\">No people are available for this chart.</p>");
        html.push_str("</body></html>");
        return html;
    }

    let bounds = report.scene.bounds;
    let padding = 32.0;
    let width = (bounds.width() + padding * 2.0).max(320.0);
    let height = (bounds.height() + padding * 2.0).max(200.0);
    let map_x = |value: f32| value - bounds.min.x + padding;
    let map_y = |value: f32| value - bounds.min.y + padding;

    html.push_str("<main class=\"chart\"><svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 ");
    html.push_str(&format!("{width:.1} {height:.1}"));
    html.push_str("\" role=\"img\" aria-label=\"Ancestor photo tree\">");
    html.push_str("<g class=\"edges\">");
    for union in &report.scene.unions {
        if let Some([a, b]) = union.spouse_line {
            html.push_str(&line(map_x(a.x), map_y(a.y), map_x(b.x), map_y(b.y)));
        }
        let union_x = map_x(union.union.x);
        let union_y = map_y(union.union.y);
        let trunk_y = map_y(union.trunk_y);
        html.push_str(&line(union_x, union_y, union_x, trunk_y));
        if !union.children.is_empty() {
            let xs: Vec<f32> = union
                .children
                .iter()
                .map(|(_, point)| map_x(point.x))
                .collect();
            let left = xs.iter().copied().fold(f32::INFINITY, f32::min);
            let right = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            html.push_str(&line(left, trunk_y, right, trunk_y));
            for (_, point) in &union.children {
                html.push_str(&line(
                    map_x(point.x),
                    trunk_y,
                    map_x(point.x),
                    map_y(point.y),
                ));
            }
        }
    }
    html.push_str("</g>");

    for (index, node) in report.scene.nodes.iter().enumerate() {
        let person = report
            .people
            .iter()
            .find(|person| person.person_id == node.person_id);
        let x = map_x(node.bounds.min.x);
        let y = map_y(node.bounds.min.y);
        let w = node.bounds.width();
        let h = node.bounds.height();
        html.push_str(&format!(
            "<g class=\"card\"><rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"8\" />"
        ));
        let photo_x = x + 8.0;
        let photo_y = y + 8.0;
        let photo_size = (h - 16.0).clamp(20.0, 40.0);
        if let Some(photo) = person.and_then(|person| person.photo.as_deref())
            && let Some(uri) = image_data_uri(photo)
        {
            html.push_str(&format!(
                "<defs><clipPath id=\"photo-{index}\"><rect x=\"{photo_x:.1}\" y=\"{photo_y:.1}\" width=\"{photo_size:.1}\" height=\"{photo_size:.1}\" rx=\"6\" /></clipPath></defs>"
            ));
            html.push_str(&format!(
                "<image href=\"{}\" x=\"{photo_x:.1}\" y=\"{photo_y:.1}\" width=\"{photo_size:.1}\" height=\"{photo_size:.1}\" preserveAspectRatio=\"xMidYMid slice\" clip-path=\"url(#photo-{index})\" />",
                escape_html(&uri)
            ));
        } else {
            html.push_str(&format!(
                "<rect x=\"{photo_x:.1}\" y=\"{photo_y:.1}\" width=\"{photo_size:.1}\" height=\"{photo_size:.1}\" rx=\"6\" fill=\"#e2e8f0\" />"
            ));
            let initial = person
                .map(|person| first_letter(&person.name))
                .unwrap_or_default();
            html.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" class=\"initial\">{}</text>",
                photo_x + photo_size / 2.0,
                photo_y + photo_size / 2.0 + 4.0,
                escape_html(&initial)
            ));
        }
        let text_x = photo_x + photo_size + 10.0;
        let name = person
            .map(|person| truncate(&person.name, 26))
            .unwrap_or_else(|| node.display_name.clone());
        html.push_str(&format!(
            "<text x=\"{text_x:.1}\" y=\"{:.1}\" class=\"name\">{}</text>",
            y + 22.0,
            escape_html(&name)
        ));
        let dates = person
            .map(|person| dates(&person.birth, &person.death))
            .unwrap_or_default();
        if !dates.is_empty() {
            html.push_str(&format!(
                "<text x=\"{text_x:.1}\" y=\"{:.1}\" class=\"dates\">{}</text>",
                y + 40.0,
                escape_html(&dates)
            ));
        }
        html.push_str("</g>");
    }
    html.push_str("</svg></main>");
    html.push_str("</body></html>");
    html
}

/// Writes the rendered report to `path`.
pub fn write_html(path: &Path, report: &PhotoTreeReport) -> Result<usize, String> {
    let html = render_html(report);
    std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
    Ok(html.len())
}

fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> String {
    format!("<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" />")
}

fn dates(birth: &str, death: &str) -> String {
    match (birth.trim(), death.trim()) {
        ("", "") => String::new(),
        (birth, "") => birth.to_string(),
        ("", death) => death.to_string(),
        (birth, death) => format!("{birth} \u{2013} {death}"),
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

fn first_letter(value: &str) -> String {
    value
        .chars()
        .find(|character| character.is_alphabetic())
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
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
        "tree".to_string()
    } else {
        out
    }
}

const CSS: &str = r#"
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; margin: 24px; color: #1e293b; background: #f8fafc; }
header h1 { margin: 0 0 4px; }
header p { margin: 2px 0; }
.muted { color: #64748b; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
.chart { background: white; padding: 16px; border-radius: 12px; overflow: auto; }
.chart svg { width: 100%; max-width: 100%; height: auto; }
.card rect { fill: #ffffff; stroke: #94a3b8; stroke-width: 1.2; }
.card .name { font-size: 12px; font-weight: 600; fill: #0f172a; }
.card .dates { font-size: 10px; fill: #475569; }
.card .initial { font-size: 14px; font-weight: 600; fill: #475569; }
.edges line { stroke: #94a3b8; stroke-width: 1.2; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } .chart { padding: 0; } @page { size: landscape; margin: 10mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::html::display_name;
    use egui::{Pos2, Rect, vec2};

    fn report() -> PhotoTreeReport {
        let path = std::env::temp_dir().join(format!("stemma-photo-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .expect("png writes");
        PhotoTreeReport {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            root_name: "Ada & King".to_string(),
            generations: 4,
            scene: TreeScene {
                nodes: vec![crate::gui::layout::NodeGeom {
                    person_id: "p1".to_string(),
                    display_name: "Ada".to_string(),
                    bounds: Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(140.0, 56.0)),
                }],
                unions: vec![crate::gui::layout::UnionGeom {
                    family_id: "f1".to_string(),
                    spouse_line: Some([Pos2::new(70.0, 56.0), Pos2::new(90.0, 56.0)]),
                    union: Pos2::new(80.0, 56.0),
                    trunk_y: 100.0,
                    trunk_lane: 0,
                    parent_rank_y: 0.0,
                    children: vec![("p2".to_string(), Pos2::new(80.0, 150.0))],
                }],
                bounds: Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(140.0, 150.0)),
            },
            people: vec![PhotoTreePerson {
                person_id: "p1".to_string(),
                name: "Ada & King".to_string(),
                birth: "1815".to_string(),
                death: "1852".to_string(),
                photo: Some(path),
            }],
        }
    }

    #[test]
    fn renders_cards_connectors_and_images() {
        let report = report();
        let html = render_html(&report);
        assert!(html.contains("Ada &amp; King"), "{html}");
        assert!(html.contains("1815"));
        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("<line"));
        assert!(html.contains("Photo Tree"));
        if let Some(path) = report.people[0].photo.as_ref() {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn empty_scene_has_a_message() {
        let mut report = report();
        report.scene.nodes.clear();
        report.scene.unions.clear();
        let html = render_html(&report);
        assert!(html.contains("No people"));
        if let Some(path) = report.people[0].photo.as_ref() {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "20260101"),
            "photo-tree-Ada_King-20260101.html"
        );
    }

    #[test]
    fn dates_and_truncation() {
        assert_eq!(dates("1815", "1852"), "1815 \u{2013} 1852");
        assert_eq!(dates("", ""), "");
        assert_eq!(truncate("abcdef", 4), "abc\u{2026}");
        assert_eq!(display_name(Path::new("/tmp/a.jpg")), "a.jpg");
    }
}
