//! Printable ancestor pedigree chart.
//!
//! Pedigree charts share the photo tree renderer but keep the emphasis on
//! text: every card carries its Ahnentafel number plus birth and death dates
//! and places, and portraits are optional.

use std::path::Path;

use crate::report::photo_tree::{self, PhotoTreePerson, PhotoTreeReport};

/// One numbered ancestor on a pedigree chart.
pub type PedigreePerson = PhotoTreePerson;

/// Everything needed to render an ancestor pedigree chart.
pub type PedigreeReport = PhotoTreeReport;

/// Heading used by pedigree charts.
pub const TITLE: &str = "Pedigree Chart";

/// Default timestamped HTML file name.
pub fn file_name(root_name: &str, timestamp: &str) -> String {
    format!(
        "pedigree-{}-{timestamp}.html",
        photo_tree::sanitize_file_stem(root_name)
    )
}

/// Renders the chart as a standalone, printable HTML document.
pub fn render_html(report: &PedigreeReport) -> String {
    photo_tree::render_html(report)
}

/// Writes the rendered chart to `path`.
pub fn write_html(path: &Path, report: &PedigreeReport) -> Result<usize, String> {
    photo_tree::write_html(path, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::layout::{NodeGeom, TreeScene, UnionGeom};
    use egui::{Pos2, Rect, vec2};

    fn report() -> PedigreeReport {
        PedigreeReport {
            title: TITLE.to_string(),
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            root_name: "Ada King".to_string(),
            generations: 4,
            scene: TreeScene {
                nodes: vec![NodeGeom {
                    person_id: "root".to_string(),
                    display_name: "Ada King".to_string(),
                    bounds: Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(140.0, 56.0)),
                }],
                unions: vec![UnionGeom {
                    family_id: "f1".to_string(),
                    spouse_line: None,
                    union: Pos2::new(80.0, 56.0),
                    trunk_y: 100.0,
                    trunk_lane: 0,
                    parent_rank_y: 0.0,
                    children: vec![("root".to_string(), Pos2::new(80.0, 150.0))],
                }],
                bounds: Rect::from_min_size(Pos2::new(0.0, 0.0), vec2(140.0, 150.0)),
            },
            people: vec![PedigreePerson {
                person_id: "root".to_string(),
                name: "Ada & King".to_string(),
                number: Some(1),
                birth: "1815".to_string(),
                birth_place: "London".to_string(),
                death: "1852".to_string(),
                death_place: "Bath".to_string(),
                photo: None,
            }],
            show_places: true,
            include_photos: false,
        }
    }

    #[test]
    fn renders_numbers_names_and_places() {
        let html = render_html(&report());
        assert!(html.contains("<h1>Pedigree Chart</h1>"), "{html}");
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("class=\"number\">1</text>"));
        assert!(html.contains("b. 1815 \u{00b7} London"));
        assert!(html.contains("d. 1852 \u{00b7} Bath"));
        assert!(html.contains("Family &amp; Friends"));
    }

    #[test]
    fn empty_report_has_a_message() {
        let mut report = report();
        report.scene.nodes.clear();
        report.scene.unions.clear();
        report.people.clear();
        assert!(render_html(&report).contains("No people"));
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "20260101"),
            "pedigree-Ada_King-20260101.html"
        );
    }
}
