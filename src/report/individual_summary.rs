//! Printable individual summary.

use std::path::{Path, PathBuf};

use crate::i18n::tr;
use crate::report::html::{escape_html, image_data_uri};

/// One non-vital fact on an individual summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryFact {
    pub label: String,
    pub date: String,
    pub place: String,
    pub value: String,
}

/// A parent, spouse or child listed on an individual summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryRelative {
    pub name: String,
    pub birth: String,
    pub death: String,
    /// Marriage date and place, set on spouse rows when known.
    pub marriage_date: String,
    pub marriage_place: String,
}

/// Everything needed to render one individual summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndividualSummaryReport {
    pub tree_name: String,
    pub generated_at: String,
    pub name: String,
    pub photo: Option<PathBuf>,
    pub birth: String,
    pub birth_place: String,
    pub death: String,
    pub death_place: String,
    pub notes: Option<String>,
    pub facts: Vec<SummaryFact>,
    pub parents: Vec<SummaryRelative>,
    pub spouses: Vec<SummaryRelative>,
    pub children: Vec<SummaryRelative>,
}

/// Default timestamped HTML file name.
pub fn file_name(name: &str, timestamp: &str) -> String {
    format!(
        "individual-summary-{}-{timestamp}.html",
        sanitize_file_stem(name)
    )
}

/// Renders the summary as a standalone, printable HTML document.
pub fn render_html(report: &IndividualSummaryReport) -> String {
    let mut html = String::with_capacity(8192 + report.facts.len() * 256);
    html.push_str(&crate::report::html::document_open());
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(tr("report.individual_summary.title")));
    html.push_str(" \u{2014} ");
    html.push_str(&escape_html(&report.name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str(&format!(
        "<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">{}</button><span>{}</span></div>",
        escape_html(tr("report.common.print")),
        escape_html(tr("report.common.print_hint")),
    ));

    html.push_str("<header class=\"person\">");
    if let Some(photo) = report.photo.as_deref()
        && let Some(uri) = image_data_uri(photo)
    {
        html.push_str(&format!(
            "<img class=\"portrait\" src=\"{}\" alt=\"{}\">",
            escape_html(&uri),
            escape_html(&report.name)
        ));
    } else {
        html.push_str("<div class=\"portrait placeholder\">");
        html.push_str(&escape_html(&initial(&report.name)));
        html.push_str("</div>");
    }
    html.push_str(&format!(
        "<div><p class=\"kind\">{}</p><h1>",
        escape_html(tr("report.individual_summary.title")),
    ));
    html.push_str(&escape_html(&report.name));
    html.push_str("</h1><p class=\"muted\">");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str("</p></div></header>");

    html.push_str("<main>");
    html.push_str(&format!(
        "<section><h2>{}</h2><dl class=\"vitals\">",
        escape_html(tr("report.individual_summary.vital_data")),
    ));
    push_vital(
        &mut html,
        tr("report.common.birth"),
        &report.birth,
        &report.birth_place,
    );
    push_vital(
        &mut html,
        tr("report.common.death"),
        &report.death,
        &report.death_place,
    );
    html.push_str("</dl></section>");

    html.push_str(&format!(
        "<section><h2>{}</h2>",
        escape_html(tr("report.common.facts")),
    ));
    if report.facts.is_empty() {
        html.push_str(&format!(
            "<p class=\"empty\">{}</p>",
            escape_html(tr("report.individual_summary.no_facts")),
        ));
    } else {
        html.push_str(&format!(
            "<table><thead><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr></thead><tbody>",
            escape_html(tr("report.common.fact")),
            escape_html(tr("report.common.date")),
            escape_html(tr("report.common.place")),
            escape_html(tr("report.common.details")),
        ));
        for fact in &report.facts {
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
    html.push_str("</section>");

    relative_section(
        &mut html,
        tr("report.common.parents"),
        &report.parents,
        false,
    );
    relative_section(
        &mut html,
        tr("report.common.spouses"),
        &report.spouses,
        true,
    );
    relative_section(
        &mut html,
        tr("report.common.children"),
        &report.children,
        false,
    );

    if let Some(notes) = report
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|notes| !notes.is_empty())
    {
        html.push_str(&format!(
            "<section class=\"notes\"><h2>{}</h2>",
            escape_html(tr("report.common.notes")),
        ));
        for paragraph in notes.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
            html.push_str("<p>");
            html.push_str(&escape_html(paragraph).replace('\n', "<br>"));
            html.push_str("</p>");
        }
        html.push_str("</section>");
    }
    html.push_str("</main></body></html>");
    html
}

fn relative_section(
    html: &mut String,
    title: &str,
    relatives: &[SummaryRelative],
    marriages: bool,
) {
    html.push_str("<section><h2>");
    html.push_str(title);
    html.push_str("</h2>");
    if relatives.is_empty() {
        html.push_str("<p class=\"empty\">No ");
        html.push_str(&escape_html(&title.to_lowercase()));
        html.push_str(" recorded.</p></section>");
        return;
    }
    html.push_str("<ul class=\"relatives\">");
    for relative in relatives {
        html.push_str("<li><span class=\"name\">");
        html.push_str(&escape_html(&relative.name));
        html.push_str("</span>");
        let dates = dates(&relative.birth, &relative.death);
        if !dates.is_empty() {
            html.push_str("<span class=\"dates\">");
            html.push_str(&escape_html(&dates));
            html.push_str("</span>");
        }
        if marriages {
            let marriage = event_cell(&relative.marriage_date, &relative.marriage_place);
            if !marriage.is_empty() {
                html.push_str("<span class=\"marriage\">m. ");
                html.push_str(&escape_html(&marriage));
                html.push_str("</span>");
            }
        }
        html.push_str("</li>");
    }
    html.push_str("</ul></section>");
}

fn push_vital(html: &mut String, label: &str, date: &str, place: &str) {
    html.push_str("<div><dt>");
    html.push_str(label);
    html.push_str("</dt><dd>");
    let value = event_cell(date, place);
    if value.is_empty() {
        html.push_str(&format!(
            "<span class=\"unknown\">{}</span>",
            escape_html(tr("report.common.unknown")),
        ));
    } else {
        html.push_str(&escape_html(&value));
    }
    html.push_str("</dd></div>");
}

/// Writes the rendered summary to `path`.
pub fn write_html(path: &Path, report: &IndividualSummaryReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        crate::model::print_profile::ReportKind::IndividualSummary,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
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

fn dates(birth: &str, death: &str) -> String {
    match (birth.trim(), death.trim()) {
        ("", "") => String::new(),
        (birth, "") => format!("b. {birth}"),
        ("", death) => format!("d. {death}"),
        (birth, death) => format!("{birth} \u{2013} {death}"),
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
        "person".to_string()
    } else {
        out
    }
}

const CSS: &str = r#"
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; margin: 24px; color: #1e293b; background: #f8fafc; }
h1 { margin: 2px 0 4px; }
h2 { margin: 0 0 10px; font-size: 1.05rem; }
header.person { display: flex; align-items: center; gap: 18px; margin-bottom: 22px; }
.kind { margin: 0; color: #64748b; font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; }
.muted { color: #64748b; font-size: 13px; margin: 0; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; }
.toolbar span { color: #64748b; font-size: 13px; }
.portrait { width: 84px; height: 84px; border-radius: 12px; object-fit: cover; background: #e2e8f0; flex: none; }
.portrait.placeholder { display: flex; align-items: center; justify-content: center; font-size: 30px; font-weight: 600; color: #475569; }
main section { background: white; border: 1px solid #e2e8f0; border-radius: 12px; padding: 18px; margin-bottom: 18px; page-break-inside: avoid; }
.vitals { display: grid; grid-template-columns: max-content 1fr; gap: 4px 14px; margin: 0; }
.vitals div { display: contents; }
.vitals dt { color: #64748b; font-size: 13px; }
.vitals dd { margin: 0; font-size: 13px; }
.unknown { color: #94a3b8; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #e2e8f0; vertical-align: top; }
th { color: #475569; font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; }
.relatives { list-style: none; margin: 0; padding: 0; }
.relatives li { display: flex; flex-wrap: wrap; gap: 4px 14px; padding: 6px 0; border-bottom: 1px solid #e2e8f0; font-size: 13px; }
.relatives li:last-child { border-bottom: none; }
.relatives .name { font-weight: 600; }
.relatives .dates, .relatives .marriage { color: #475569; }
.notes p { margin: 0 0 10px; font-size: 13px; line-height: 1.5; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } main section { border: none; padding: 0; border-radius: 0; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn relative(name: &str, birth: &str, death: &str) -> SummaryRelative {
        SummaryRelative {
            name: name.to_string(),
            birth: birth.to_string(),
            death: death.to_string(),
            marriage_date: "1838".to_string(),
            marriage_place: "Bath".to_string(),
        }
    }

    fn report() -> IndividualSummaryReport {
        let path =
            std::env::temp_dir().join(format!("stemma-summary-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .expect("png writes");
        IndividualSummaryReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            name: "Ada & King".to_string(),
            photo: Some(path),
            birth: "1815".to_string(),
            birth_place: "London".to_string(),
            death: "1852".to_string(),
            death_place: "Bath".to_string(),
            notes: Some("First line\nSecond line\n\nSecond <paragraph>".to_string()),
            facts: vec![SummaryFact {
                label: "Occupation".to_string(),
                date: "1840".to_string(),
                place: "London".to_string(),
                value: "Teacher & clerk".to_string(),
            }],
            parents: vec![relative("George <King>", "1780", "1840")],
            spouses: vec![relative("Charles Stone", "1810", "1880")],
            children: vec![relative("Bea Stone", "1840", "")],
        }
    }

    #[test]
    fn renders_person_facts_relatives_and_notes() {
        let report = report();
        let html = render_html(&report);
        assert!(html.contains("<h1>Ada &amp; King</h1>"), "{html}");
        assert!(html.contains("1815 \u{00b7} London"));
        assert!(html.contains("1852 \u{00b7} Bath"));
        assert!(html.contains("Teacher &amp; clerk"));
        assert!(html.contains("George &lt;King&gt;"));
        assert!(html.contains("m. 1838 \u{00b7} Bath"));
        assert!(html.contains("Bea Stone"));
        assert!(html.contains("Second &lt;paragraph&gt;"));
        assert!(html.contains("First line<br>Second line"));
        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("Family &amp; Friends"));
        if let Some(path) = report.photo.as_ref() {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn empty_sections_are_explained() {
        let mut report = report();
        report.facts.clear();
        report.parents.clear();
        report.children.clear();
        report.notes = None;
        report.birth.clear();
        report.birth_place.clear();
        let html = render_html(&report);
        assert!(html.contains("No additional facts recorded."));
        assert!(html.contains("No parents recorded."));
        assert!(html.contains("No children recorded."));
        assert!(html.contains("Unknown"));
        assert!(!html.contains("<h2>Notes</h2>"));
        if let Some(path) = report.photo.as_ref() {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Ada / King", "20260101"),
            "individual-summary-Ada_King-20260101.html"
        );
        assert_eq!(
            file_name("  ", "20260101"),
            "individual-summary-person-20260101.html"
        );
    }
}
