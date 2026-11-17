//! Printable family group sheet.

use std::path::{Path, PathBuf};

use crate::report::html::{escape_html, image_data_uri};

/// One non-vital fact shown for a spouse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilySheetFact {
    pub label: String,
    pub date: String,
    pub place: String,
    pub value: String,
}

/// A parent or spouse on a family group sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilySheetPerson {
    /// Relationship to the family, for example `Husband` or `Wife`.
    pub role: String,
    pub person_id: String,
    pub name: String,
    pub birth: String,
    pub birth_place: String,
    pub death: String,
    pub death_place: String,
    pub parents: Vec<String>,
    pub facts: Vec<FamilySheetFact>,
    pub photo: Option<PathBuf>,
}

/// A child of the recorded couple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilySheetChild {
    pub person_id: String,
    pub name: String,
    pub birth: String,
    pub birth_place: String,
    pub death: String,
    pub death_place: String,
}

/// Everything needed to render a family group sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyGroupSheetReport {
    pub tree_name: String,
    pub generated_at: String,
    pub family_label: String,
    pub husband: Option<FamilySheetPerson>,
    pub wife: Option<FamilySheetPerson>,
    pub marriage_date: String,
    pub marriage_place: String,
    pub children: Vec<FamilySheetChild>,
}

impl FamilyGroupSheetReport {
    /// Spouses in husband/wife order, skipping missing slots.
    pub fn spouses(&self) -> Vec<&FamilySheetPerson> {
        [self.husband.as_ref(), self.wife.as_ref()]
            .into_iter()
            .flatten()
            .collect()
    }

    /// True when the family has no spouses and no children.
    pub fn is_empty(&self) -> bool {
        self.husband.is_none() && self.wife.is_none() && self.children.is_empty()
    }
}

/// Default timestamped HTML file name.
pub fn file_name(family_label: &str, timestamp: &str) -> String {
    format!(
        "family-sheet-{}-{timestamp}.html",
        sanitize_file_stem(family_label)
    )
}

/// Renders the sheet as a standalone, printable HTML document.
pub fn render_html(report: &FamilyGroupSheetReport) -> String {
    let mut html = String::with_capacity(8192 + report.children.len() * 256);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Family Group Sheet \u{2014} ");
    html.push_str(&escape_html(&report.family_label));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>Family Group Sheet</h1><p>");
    html.push_str(&escape_html(&report.family_label));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.children.len().to_string());
    html.push_str(" children</p></header>");

    if report.is_empty() {
        html.push_str("<p class=\"empty\">This family has no members.</p>");
        html.push_str("</body></html>");
        return html;
    }

    if !report.spouses().is_empty() {
        html.push_str("<section class=\"couple\">");
        for person in report.spouses() {
            render_person(&mut html, person);
        }
        html.push_str("</section>");
    }

    html.push_str("<section class=\"marriage\"><h2>Marriage</h2>");
    let marriage = event_cell(&report.marriage_date, &report.marriage_place);
    if marriage.is_empty() {
        html.push_str("<p class=\"empty\">No marriage event recorded.</p>");
    } else {
        html.push_str("<p>");
        html.push_str(&escape_html(&marriage));
        html.push_str("</p>");
    }
    html.push_str("</section>");

    html.push_str("<section class=\"children\"><h2>Children (");
    html.push_str(&report.children.len().to_string());
    html.push_str(")</h2>");
    if report.children.is_empty() {
        html.push_str("<p class=\"empty\">No children recorded.</p>");
    } else {
        html.push_str("<table><thead><tr><th>#</th><th>Name</th><th>Birth</th><th>Death</th></tr></thead><tbody>");
        for (index, child) in report.children.iter().enumerate() {
            html.push_str("<tr><td class=\"index\">");
            html.push_str(&(index + 1).to_string());
            html.push_str("</td><td class=\"name\">");
            html.push_str(&escape_html(&child.name));
            html.push_str("</td><td>");
            html.push_str(&escape_html(&event_cell(&child.birth, &child.birth_place)));
            html.push_str("</td><td>");
            html.push_str(&escape_html(&event_cell(&child.death, &child.death_place)));
            html.push_str("</td></tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</section>");
    html.push_str("</body></html>");
    html
}

fn render_person(html: &mut String, person: &FamilySheetPerson) {
    html.push_str("<article class=\"person\"><div class=\"identity\">");
    if let Some(photo) = person.photo.as_deref()
        && let Some(uri) = image_data_uri(photo)
    {
        html.push_str(&format!(
            "<img class=\"portrait\" src=\"{}\" alt=\"{}\">",
            escape_html(&uri),
            escape_html(&person.name)
        ));
    } else {
        html.push_str("<div class=\"portrait placeholder\">");
        html.push_str(&escape_html(&initial(&person.name)));
        html.push_str("</div>");
    }
    html.push_str("<div><p class=\"role\">");
    html.push_str(&escape_html(&person.role));
    html.push_str("</p><h2>");
    html.push_str(&escape_html(&person.name));
    html.push_str("</h2></div></div>");

    html.push_str("<dl class=\"vitals\">");
    push_vital(html, "Birth", &person.birth, &person.birth_place);
    push_vital(html, "Death", &person.death, &person.death_place);
    if !person.parents.is_empty() {
        html.push_str("<div><dt>Parents</dt><dd>");
        for (index, parent) in person.parents.iter().enumerate() {
            if index > 0 {
                html.push_str(" \u{00b7} ");
            }
            html.push_str(&escape_html(parent));
        }
        html.push_str("</dd></div>");
    }
    html.push_str("</dl>");

    if !person.facts.is_empty() {
        html.push_str("<table class=\"facts\"><thead><tr><th>Fact</th><th>Date</th><th>Place</th><th>Details</th></tr></thead><tbody>");
        for fact in &person.facts {
            html.push_str("<tr><td>");
            html.push_str(&escape_html(&fact.label));
            html.push_str("</td><td>");
            html.push_str(&escape_html(&fact.date));
            html.push_str("</td><td>");
            html.push_str(&escape_html(&fact.place));
            html.push_str("</td><td>");
            html.push_str(&escape_html(&fact.value));
            html.push_str("</td></tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</article>");
}

fn push_vital(html: &mut String, label: &str, date: &str, place: &str) {
    html.push_str("<div><dt>");
    html.push_str(label);
    html.push_str("</dt><dd>");
    let value = event_cell(date, place);
    if value.is_empty() {
        html.push_str("<span class=\"unknown\">Unknown</span>");
    } else {
        html.push_str(&escape_html(&value));
    }
    html.push_str("</dd></div>");
}

/// Writes the rendered sheet to `path`.
pub fn write_html(path: &Path, report: &FamilyGroupSheetReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::FamilyGroupSheet,
        &report.tree_name,
        &report.generated_at,
    );
    std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
    Ok(html.len())
}

fn event_cell(date: &str, place: &str) -> String {
    let date = date.trim();
    let place = place.trim();
    match (date.is_empty(), place.is_empty()) {
        (true, true) => String::new(),
        (false, true) => date.to_string(),
        (true, false) => place.to_string(),
        (false, false) => format!("{date} \u{00b7} {place}"),
    }
}

fn initial(name: &str) -> String {
    name.chars()
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
        "family".to_string()
    } else {
        out
    }
}

const CSS: &str = r#"
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; margin: 24px; color: #1e293b; background: #f8fafc; }
header h1 { margin: 0 0 4px; }
header p { margin: 2px 0; }
h2 { margin: 0 0 10px; }
.muted { color: #64748b; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
.couple { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 20px; margin-bottom: 20px; }
.person, .marriage, .children { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 18px; }
.person { page-break-inside: avoid; }
.identity { display: flex; align-items: center; gap: 14px; margin-bottom: 12px; }
.portrait { width: 64px; height: 64px; border-radius: 10px; object-fit: cover; background: #e2e8f0; flex: none; }
.portrait.placeholder { display: flex; align-items: center; justify-content: center; font-size: 22px; font-weight: 600; color: #475569; }
.role { margin: 0; color: #64748b; font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; }
.identity h2 { margin: 2px 0 0; }
.vitals { display: grid; grid-template-columns: max-content 1fr; gap: 4px 14px; margin: 0 0 14px; }
.vitals div { display: contents; }
.vitals dt { color: #64748b; font-size: 13px; }
.vitals dd { margin: 0; font-size: 13px; }
.unknown { color: #94a3b8; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; }
td.index { width: 32px; color: #64748b; }
td.name { font-weight: 600; }
.marriage { margin-bottom: 20px; page-break-inside: avoid; }
.children { page-break-inside: auto; }
.children tr { break-inside: avoid; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } .person, .marriage, .children { border: none; padding: 0; border-radius: 0; } .couple, .marriage { margin-bottom: 16px; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn person(role: &str, id: &str, name: &str, photo: Option<PathBuf>) -> FamilySheetPerson {
        FamilySheetPerson {
            role: role.to_string(),
            person_id: id.to_string(),
            name: name.to_string(),
            birth: "1815".to_string(),
            birth_place: "London".to_string(),
            death: "1852".to_string(),
            death_place: "Bath".to_string(),
            parents: vec!["George King".to_string(), "Mary <Anne>".to_string()],
            facts: vec![FamilySheetFact {
                label: "Occupation".to_string(),
                date: "1840".to_string(),
                place: "London".to_string(),
                value: "Teacher & clerk".to_string(),
            }],
            photo,
        }
    }

    fn child(id: &str, name: &str) -> FamilySheetChild {
        FamilySheetChild {
            person_id: id.to_string(),
            name: name.to_string(),
            birth: "1840".to_string(),
            birth_place: "London".to_string(),
            death: String::new(),
            death_place: String::new(),
        }
    }

    fn report() -> FamilyGroupSheetReport {
        let path = std::env::temp_dir().join(format!("stemma-sheet-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .expect("png writes");
        FamilyGroupSheetReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            family_label: "Ada King & Charles <Stone>".to_string(),
            husband: Some(person("Husband", "p1", "Charles <Stone>", None)),
            wife: Some(person("Wife", "p2", "Ada & King", Some(path))),
            marriage_date: "1838".to_string(),
            marriage_place: "Bath".to_string(),
            children: vec![child("c1", "Bea Stone"), child("c2", "Dan Stone")],
        }
    }

    #[test]
    fn renders_spouses_marriage_children_and_facts() {
        let report = report();
        let html = render_html(&report);
        assert!(html.contains("<h1>Family Group Sheet</h1>"), "{html}");
        assert!(html.contains("Charles &lt;Stone&gt;"));
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("Family &amp; Friends"));
        assert!(html.contains("1838 \u{00b7} Bath"));
        assert!(html.contains("Bea Stone"));
        assert!(html.contains("Teacher &amp; clerk"));
        assert!(html.contains("Mary &lt;Anne&gt;"));
        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("2 children"));
        if let Some(path) = report.wife.as_ref().and_then(|wife| wife.photo.as_ref()) {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn single_parent_family_renders_one_spouse() {
        let mut report = report();
        report.husband = None;
        report.marriage_date.clear();
        report.marriage_place.clear();
        let html = render_html(&report);
        assert!(html.contains("Wife"));
        assert!(!html.contains("Husband"));
        assert!(html.contains("No marriage event recorded."));
    }

    #[test]
    fn unknown_vitals_and_missing_children_are_explained() {
        let mut report = report();
        report.children.clear();
        if let Some(wife) = report.wife.as_mut() {
            wife.birth.clear();
            wife.birth_place.clear();
        }
        let html = render_html(&report);
        assert!(html.contains("No children recorded."));
        assert!(html.contains("Unknown"));
    }

    #[test]
    fn empty_family_has_a_message() {
        let report = FamilyGroupSheetReport {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            family_label: "Family".to_string(),
            husband: None,
            wife: None,
            marriage_date: String::new(),
            marriage_place: String::new(),
            children: Vec::new(),
        };
        assert!(render_html(&report).contains("no members"));
        assert!(report.is_empty());
        assert!(report.spouses().is_empty());
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada King & Charles Stone", "20260101"),
            "family-sheet-Ada_King_Charles_Stone-20260101.html"
        );
        assert_eq!(
            file_name("  ", "20260101"),
            "family-sheet-family-20260101.html"
        );
    }
}
