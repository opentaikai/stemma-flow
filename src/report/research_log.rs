//! Print-ready HTML research log.

use std::collections::BTreeMap;
use std::path::Path;

use crate::db::tasks::{TaskStatus, TaskSummary};
use crate::i18n::{tr, tr_args};

/// Everything needed to render one research-log report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchLogReport {
    pub tree_name: String,
    pub generated_at: String,
    pub text_filter: String,
    pub status_filter: String,
    pub folder_filter: Option<String>,
    pub tasks: Vec<TaskSummary>,
}

/// Default timestamped HTML file name.
pub fn file_name(tree_name: &str, timestamp: &str) -> String {
    format!(
        "research-log-{}-{timestamp}.html",
        sanitize_file_stem(tree_name)
    )
}

/// Renders the report as a standalone, printable HTML document.
pub fn render_html(report: &ResearchLogReport) -> String {
    let open = report
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Open)
        .count();
    let done = report.tasks.len().saturating_sub(open);
    let mut html = String::with_capacity(4096 + report.tasks.len() * 512);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(&tr("research_report.title")));
    html.push_str(" — ");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">");
    html.push_str(&escape_html(&tr("research_report.print")));
    html.push_str("</button><span>");
    html.push_str(&escape_html(&tr("research_report.print_hint")));
    html.push_str("</span></div>");
    html.push_str("<header><h1>");
    html.push_str(&escape_html(&tr("research_report.title")));
    html.push_str("</h1><p>");
    html.push_str(&escape_html(&tr_args(
        "research_report.tree",
        &[("name", report.tree_name.clone())],
    )));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&tr_args(
        "research_report.generated",
        &[("timestamp", report.generated_at.clone())],
    )));
    html.push_str("</p><p>");
    html.push_str(&escape_html(&tr_args(
        "research_report.task_count",
        &[("count", report.tasks.len().to_string())],
    )));
    html.push_str(" · ");
    html.push_str(&escape_html(&tr_args(
        "research_report.status_summary",
        &[("open", open.to_string()), ("done", done.to_string())],
    )));
    html.push_str("</p><div class=\"filters\"><strong>");
    html.push_str(&escape_html(&tr("research_report.filters")));
    html.push_str("</strong><br><span>");
    html.push_str(&escape_html(&tr_args(
        "research_report.filter_text",
        &[(
            "value",
            if report.text_filter.is_empty() {
                tr("research_report.none")
            } else {
                report.text_filter.clone()
            },
        )],
    )));
    html.push_str("</span> · <span>");
    html.push_str(&escape_html(&tr_args(
        "research_report.filter_status",
        &[("value", report.status_filter.clone())],
    )));
    html.push_str("</span>");
    if let Some(folder) = &report.folder_filter {
        html.push_str(" · <span>");
        html.push_str(&escape_html(&tr_args(
            "research_report.filter_folder",
            &[("value", folder.clone())],
        )));
        html.push_str("</span>");
    }
    html.push_str("</div></header><main>");

    if report.tasks.is_empty() {
        html.push_str("<p class=\"empty\">");
        html.push_str(&escape_html(&tr("research_report.empty")));
        html.push_str("</p>");
    } else {
        for (folder, tasks) in grouped_tasks(&report.tasks) {
            html.push_str("<section><h2>");
            let title = if folder.is_empty() {
                tr("research_report.no_folder")
            } else {
                folder.clone()
            };
            html.push_str(&escape_html(&title));
            html.push_str("</h2><table><thead><tr>");
            for key in [
                "research_report.column_task",
                "research_report.column_status",
                "research_report.column_priority",
                "research_report.column_due",
                "research_report.column_completed",
                "research_report.column_links",
            ] {
                html.push_str("<th>");
                html.push_str(&escape_html(&tr(key)));
                html.push_str("</th>");
            }
            html.push_str("</tr></thead><tbody>");
            for task in tasks {
                html.push_str("<tr class=\"task\"><td>");
                html.push_str(&escape_html(&task.title));
                html.push_str("</td><td>");
                html.push_str(&escape_html(&status_label(task.status)));
                html.push_str("</td><td>");
                html.push_str(&escape_html(&priority_label(task.priority)));
                html.push_str("</td><td>");
                html.push_str(&escape_html(task.due_date.as_deref().unwrap_or("—")));
                html.push_str("</td><td>");
                html.push_str(&escape_html(task.completed_at.as_deref().unwrap_or("—")));
                html.push_str("</td><td class=\"number\">");
                html.push_str(&task.link_count.to_string());
                html.push_str(
                    "</td></tr><tr class=\"log-row\"><td colspan=\"6\"><div class=\"log\">",
                );
                match task.description.as_deref().map(str::trim) {
                    Some(description) if !description.is_empty() => {
                        html.push_str(&escape_html(description));
                    }
                    _ => {
                        html.push_str("<span class=\"muted\">");
                        html.push_str(&escape_html(&tr("research_report.no_log")));
                        html.push_str("</span>");
                    }
                }
                html.push_str("</div></td></tr>");
            }
            html.push_str("</tbody></table></section>");
        }
    }
    html.push_str("</main></body></html>");
    html
}

/// Writes a report to `path`, returning the number of bytes written.
pub fn write_html(path: &Path, report: &ResearchLogReport) -> Result<usize, String> {
    let html = render_html(report);
    std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
    Ok(html.len())
}

fn grouped_tasks(tasks: &[TaskSummary]) -> Vec<(String, Vec<&TaskSummary>)> {
    let mut groups: BTreeMap<String, Vec<&TaskSummary>> = BTreeMap::new();
    for task in tasks {
        groups
            .entry(task.folder.clone().unwrap_or_default())
            .or_default()
            .push(task);
    }
    let mut folders: Vec<String> = groups.keys().cloned().collect();
    folders.sort_by(|left, right| {
        let left_empty = left.trim().is_empty();
        let right_empty = right.trim().is_empty();
        left_empty
            .cmp(&right_empty)
            .then_with(|| left.to_lowercase().cmp(&right.to_lowercase()))
    });
    folders
        .into_iter()
        .filter_map(|folder| groups.remove(&folder).map(|tasks| (folder, tasks)))
        .collect()
}

fn status_label(status: TaskStatus) -> String {
    tr(match status {
        TaskStatus::Open => "research_report.status_open",
        TaskStatus::Done => "research_report.status_done",
    })
}

fn priority_label(priority: Option<i64>) -> String {
    tr(match priority {
        None => "research_report.priority_none",
        Some(1) => "research_report.priority_low",
        Some(2) => "research_report.priority_normal",
        Some(3) => "research_report.priority_high",
        _ => "research_report.priority_urgent",
    })
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
        "family_tree".to_string()
    } else {
        result.to_string()
    }
}

/// Escapes text for use in HTML element content and attribute values.
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
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

const CSS: &str = r#"
:root { color-scheme: light; }
* { box-sizing: border-box; }
body { margin: 0; padding: 20px; color: #171717; background: #fff;
       font: 10pt/1.4 system-ui, -apple-system, "Segoe UI", sans-serif; }
.toolbar { display: flex; align-items: center; gap: 12px; padding: 8px 0 16px;
           border-bottom: 1px solid #ddd; margin-bottom: 18px; }
.toolbar button { font: inherit; padding: 6px 16px; }
.toolbar span, .muted { color: #666; }
header { margin-bottom: 18px; }
h1 { font-size: 20pt; margin: 0 0 6px; }
h2 { font-size: 13pt; margin: 22px 0 6px; border-bottom: 1px solid #ccc;
     padding-bottom: 3px; }
p { margin: 4px 0; }
.filters { margin-top: 10px; font-size: 9pt; }
table { width: 100%; border-collapse: collapse; table-layout: fixed; margin-bottom: 10px; }
th, td { border: 1px solid #bbb; padding: 4px 6px; text-align: left; vertical-align: top; }
th { background: #f2f2f2; font-size: 9pt; }
th:nth-child(1) { width: 28%; }
th:nth-child(2) { width: 9%; }
th:nth-child(3) { width: 10%; }
th:nth-child(4) { width: 14%; }
th:nth-child(5) { width: 14%; }
th:nth-child(6) { width: 8%; }
td.number { text-align: right; }
tr.task td { font-weight: 600; }
tr.log-row td { border-top: 0; padding-top: 2px; }
.log { white-space: pre-wrap; font-weight: normal; font-size: 9.5pt; }
.empty { padding: 20px; background: #f7f7f7; border: 1px solid #ddd; }
@media print {
  @page { margin: 16mm; }
  body { padding: 0; font-size: 9.5pt; }
  .toolbar { display: none; }
  thead { display: table-header-group; }
  tr.task, tr.log-row { break-inside: avoid; page-break-inside: avoid; }
  section { break-inside: auto; }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tasks::TaskSummary;

    fn task(id: &str, title: &str, folder: Option<&str>, log: Option<&str>) -> TaskSummary {
        TaskSummary {
            id: id.to_string(),
            title: title.to_string(),
            description: log.map(str::to_string),
            status: TaskStatus::Open,
            priority: Some(3),
            folder: folder.map(str::to_string),
            due_date: Some("2026-01-01".to_string()),
            completed_at: None,
            link_count: 2,
        }
    }

    fn report(tasks: Vec<TaskSummary>) -> ResearchLogReport {
        ResearchLogReport {
            tree_name: "Family Tree".to_string(),
            generated_at: "2026-11-09-120000".to_string(),
            text_filter: "birth <record>".to_string(),
            status_filter: "Open".to_string(),
            folder_filter: Some("Parish & Archive".to_string()),
            tasks,
        }
    }

    #[test]
    fn report_groups_folders_and_renders_log_text() {
        let html = render_html(&report(vec![
            task(
                "t1",
                "Find birth",
                Some("Parish"),
                Some("Page <12>\nChecked & cited."),
            ),
            task("t2", "Find death", None, None),
            task("t3", "Find marriage", Some("Archive"), Some("Index found.")),
        ]));
        assert!(html.contains("find birth") || html.contains("Find birth"));
        assert!(html.contains("Parish"), "folder section is rendered");
        assert!(html.contains("No folder"), "unfiled tasks are grouped last");
        assert!(html.contains("Page &lt;12&gt;"));
        assert!(html.contains("Checked &amp; cited."));
        assert!(html.contains("No research notes"));
        assert!(html.contains("window.print()"));
        assert!(html.contains("@media print"));
        assert!(html.contains("Parish &amp; Archive"));
        assert!(!html.contains("Page <12>"), "raw HTML must be escaped");
    }

    #[test]
    fn empty_report_is_valid_html() {
        let html = render_html(&report(Vec::new()));
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("No tasks match the current filters."));
        assert!(html.contains("0 tasks"));
    }

    #[test]
    fn file_names_are_timestamped_and_safe() {
        assert_eq!(
            file_name("Family Tree / 2026", "20261109-120000"),
            "research-log-Family_Tree_2026-20261109-120000.html"
        );
        assert_eq!(
            file_name("", "20261109-120000"),
            "research-log-family_tree-20261109-120000.html"
        );
    }

    #[test]
    fn write_html_creates_the_report_file() {
        let path = std::env::temp_dir().join(format!(
            "stemma-flow-research-log-{}.html",
            uuid::Uuid::new_v4()
        ));
        let bytes =
            write_html(&path, &report(vec![task("t1", "Find", None, Some("Log"))])).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert_eq!(bytes, text.len());
        assert!(text.contains("Find"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn escaping_covers_all_markup_characters() {
        assert_eq!(
            escape_html("<a href='x'>&\"</a>"),
            "&lt;a href=&#39;x&#39;&gt;&amp;&quot;&lt;/a&gt;"
        );
    }
}
