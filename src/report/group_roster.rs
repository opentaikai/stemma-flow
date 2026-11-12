//! Print-ready HTML roster for one named group.

use std::path::Path;

use crate::i18n::{tr, tr_args};
use crate::model::person_search::{GenderFilter, PersonSearchCriteria};

/// One person in the roster.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupRosterMember {
    pub name: String,
    pub birth: String,
    pub death: String,
    pub birth_place: String,
    pub death_place: String,
}

/// Everything needed to render one group roster report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupRosterReport {
    pub tree_name: String,
    pub generated_at: String,
    pub group_name: String,
    pub description: String,
    pub rule: Option<PersonSearchCriteria>,
    pub members: Vec<GroupRosterMember>,
}

/// Default timestamped HTML file name.
pub fn file_name(group_name: &str, timestamp: &str) -> String {
    format!(
        "group-roster-{}-{timestamp}.html",
        sanitize_file_stem(group_name)
    )
}

/// Renders the roster as a standalone, printable HTML document.
pub fn render_html(report: &GroupRosterReport) -> String {
    let mut html = String::with_capacity(2048 + report.members.len() * 256);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(&tr("group_report.title")));
    html.push_str(" — ");
    html.push_str(&escape_html(&report.group_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">");
    html.push_str(&escape_html(&tr("group_report.print")));
    html.push_str("</button><span>");
    html.push_str(&escape_html(&tr("group_report.print_hint")));
    html.push_str("</span></div>");
    html.push_str("<header><h1>");
    html.push_str(&escape_html(&tr_args(
        "group_report.title_named",
        &[("name", report.group_name.clone())],
    )));
    html.push_str("</h1><p>");
    html.push_str(&escape_html(&tr_args(
        "group_report.tree",
        &[("name", report.tree_name.clone())],
    )));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&tr_args(
        "group_report.generated",
        &[("timestamp", report.generated_at.clone())],
    )));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&tr_args(
        "group_report.member_count",
        &[("count", report.members.len().to_string())],
    )));
    html.push_str("</p>");
    if !report.description.trim().is_empty() {
        html.push_str("<p>");
        html.push_str(&escape_html(&report.description));
        html.push_str("</p>");
    }
    if let Some(rule) = &report.rule {
        let lines = rule_lines(rule);
        if !lines.is_empty() {
            html.push_str("<div class=\"rule\"><strong>");
            html.push_str(&escape_html(&tr("group_report.rule")));
            html.push_str("</strong><ul>");
            for line in lines {
                html.push_str("<li>");
                html.push_str(&escape_html(&line));
                html.push_str("</li>");
            }
            html.push_str("</ul></div>");
        }
    }
    html.push_str("</header><main>");
    if report.members.is_empty() {
        html.push_str("<p class=\"empty\">");
        html.push_str(&escape_html(&tr("group_report.empty")));
        html.push_str("</p>");
    } else {
        html.push_str("<table><thead><tr>");
        for key in [
            "group_report.column_name",
            "group_report.column_birth",
            "group_report.column_death",
            "group_report.column_birth_place",
            "group_report.column_death_place",
        ] {
            html.push_str("<th>");
            html.push_str(&escape_html(&tr(key)));
            html.push_str("</th>");
        }
        html.push_str("</tr></thead><tbody>");
        for member in &report.members {
            html.push_str("<tr>");
            for value in [
                &member.name,
                &member.birth,
                &member.death,
                &member.birth_place,
                &member.death_place,
            ] {
                html.push_str("<td>");
                html.push_str(&escape_html(value));
                html.push_str("</td>");
            }
            html.push_str("</tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</main></body></html>");
    html
}

/// Writes a report to `path`, returning the number of bytes written.
pub fn write_html(path: &Path, report: &GroupRosterReport) -> Result<usize, String> {
    let html = render_html(report);
    std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
    Ok(html.len())
}

fn rule_lines(rule: &PersonSearchCriteria) -> Vec<String> {
    let mut lines = Vec::new();
    if !rule.name.trim().is_empty() {
        lines.push(format!("{}: {}", tr("people.search_name"), rule.name));
    }
    if rule.gender != GenderFilter::Any {
        lines.push(format!(
            "{}: {}",
            tr("people.search_gender"),
            gender_label(rule.gender)
        ));
    }
    push_range(
        &mut lines,
        tr("people.search_birth"),
        rule.birth.from,
        rule.birth.to,
    );
    push_range(
        &mut lines,
        tr("people.search_death"),
        rule.death.from,
        rule.death.to,
    );
    if !rule.place.trim().is_empty() {
        lines.push(format!("{}: {}", tr("people.search_place"), rule.place));
    }
    if !rule.fact_type.trim().is_empty() {
        lines.push(format!(
            "{}: {}",
            tr("people.search_fact_type"),
            rule.fact_type
        ));
    }
    if !rule.fact_text.trim().is_empty() {
        lines.push(format!(
            "{}: {}",
            tr("people.search_fact_text"),
            rule.fact_text
        ));
    }
    if !rule.notes.trim().is_empty() {
        lines.push(format!("{}: {}", tr("people.search_notes"), rule.notes));
    }
    lines
}

fn push_range(lines: &mut Vec<String>, label: String, from: Option<i32>, to: Option<i32>) {
    if from.is_none() && to.is_none() {
        return;
    }
    let from = from
        .map(|year| year.to_string())
        .unwrap_or_else(|| "…".to_string());
    let to = to
        .map(|year| year.to_string())
        .unwrap_or_else(|| "…".to_string());
    lines.push(format!("{label}: {from}–{to}"));
}

fn gender_label(gender: GenderFilter) -> String {
    match gender {
        GenderFilter::Any => tr("people.search_gender_any"),
        GenderFilter::Male => tr("inspector.gender_male"),
        GenderFilter::Female => tr("inspector.gender_female"),
        GenderFilter::Unknown => tr("inspector.gender_unknown"),
    }
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn sanitize_file_stem(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            result.push(character);
        } else if !result.ends_with('_') {
            result.push('_');
        }
    }
    let result = result.trim_matches('_');
    if result.is_empty() {
        "group".to_string()
    } else {
        result.to_string()
    }
}

const CSS: &str = r#"
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body { font-family: system-ui, sans-serif; margin: 0; padding: 2rem; line-height: 1.45; }
header { margin-bottom: 1.5rem; }
h1 { margin: 0 0 .35rem; font-size: 1.7rem; }
p { margin: .25rem 0; }
.muted { color: #666; font-size: .9rem; }
.toolbar { display: flex; align-items: center; gap: 1rem; margin-bottom: 1.5rem; }
.toolbar button { padding: .45rem 1rem; font: inherit; cursor: pointer; }
.toolbar span { color: #666; font-size: .85rem; }
.rule { margin-top: .8rem; padding: .7rem .9rem; border: 1px solid #ddd; border-radius: 6px; }
.rule ul { margin: .4rem 0 0; padding-left: 1.2rem; }
table { width: 100%; border-collapse: collapse; font-size: .92rem; }
th, td { border-bottom: 1px solid #ddd; padding: .45rem .5rem; text-align: left; vertical-align: top; }
th { font-weight: 600; }
.empty { color: #666; font-style: italic; }
@media print {
  body { padding: 0; }
  .toolbar { display: none; }
  th, td { border-color: #bbb; }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::person_search::YearRange;

    fn report() -> GroupRosterReport {
        GroupRosterReport {
            tree_name: "King family".to_string(),
            generated_at: "2026-01-02 10:00".to_string(),
            group_name: "London <researchers>".to_string(),
            description: "Parish records".to_string(),
            rule: Some(PersonSearchCriteria {
                place: "London".to_string(),
                birth: YearRange {
                    from: Some(1800),
                    to: Some(1850),
                },
                ..PersonSearchCriteria::default()
            }),
            members: vec![GroupRosterMember {
                name: "Ada & King".to_string(),
                birth: "1815".to_string(),
                death: "1852".to_string(),
                birth_place: "London".to_string(),
                death_place: "London".to_string(),
            }],
        }
    }

    #[test]
    fn renders_escaped_members_and_rule_details() {
        let html = render_html(&report());
        assert!(html.contains("London &lt;researchers&gt;"));
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("1800"));
        assert!(html.contains("London"));
        assert!(html.contains("Parish records"));
    }

    #[test]
    fn file_name_sanitizes_group_names() {
        assert_eq!(
            file_name("London / researchers", "20260102"),
            "group-roster-London_researchers-20260102.html"
        );
    }
}
