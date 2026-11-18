//! Printable family history books assembled from report chapters.

use std::collections::HashSet;
use std::path::Path;

use crate::model::print_profile::ReportKind;
use crate::report::html::escape_html;

/// One prepared chapter inside a book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookChapterContent {
    pub title: String,
    /// Trusted HTML produced by the report renderers or escaped text.
    pub body: String,
    pub styles: String,
}

/// Everything needed to render one family history book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookReport {
    pub title: String,
    pub subtitle: String,
    pub author: String,
    pub tree_name: String,
    pub generated_at: String,
    pub chapters: Vec<BookChapterContent>,
}

/// Reduces a complete report document to a chapter body and its styles.
///
/// Toolbars, duplicate page headers and conflicting `@page` rules are
/// removed. The person identity header of an individual summary is kept.
pub fn fragment(html: &str) -> (String, String) {
    let mut styles = Vec::new();
    let mut seen = HashSet::new();
    let mut rest = html;
    while let Some(style_start) = rest.find("<style") {
        let after = &rest[style_start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let content_start = open_end + 1;
        let Some(close) = after[content_start..].find("</style>") else {
            break;
        };
        let css = strip_page_rules(&after[content_start..content_start + close]);
        if !css.trim().is_empty() && seen.insert(css.clone()) {
            styles.push(css);
        }
        rest = &after[content_start + close + "</style>".len()..];
    }

    let mut body = html
        .find("<body")
        .and_then(|start| {
            let after = &html[start..];
            after.find('>').map(|end| &after[end + 1..])
        })
        .and_then(|body| body.find("</body>").map(|end| body[..end].to_string()))
        .unwrap_or_default();
    body = remove_block(&body, "<div class=\"toolbar\">", "</div>");
    if let Some(start) = body.find("<header") {
        let tag_end = body[start..].find('>').map(|offset| start + offset);
        let keep_person = tag_end
            .map(|end| body[start..=end].contains("class=\"person\""))
            .unwrap_or(false);
        if !keep_person && let Some(end) = body[start..].find("</header>") {
            let end = start + end + "</header>".len();
            body.replace_range(start..end, "");
        }
    }
    (body.trim().to_string(), styles.join("\n"))
}

fn remove_block(value: &str, open: &str, close: &str) -> String {
    let Some(start) = value.find(open) else {
        return value.to_string();
    };
    let Some(offset) = value[start..].find(close) else {
        return value.to_string();
    };
    let end = start + offset + close.len();
    let mut out = String::with_capacity(value.len() - (end - start));
    out.push_str(&value[..start]);
    out.push_str(&value[end..]);
    out
}

fn strip_page_rules(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(position) = rest.find("@page") {
        out.push_str(&rest[..position]);
        let after = &rest[position..];
        if after.find('{').is_none() {
            break;
        }
        let mut depth = 0usize;
        let mut end = after.len();
        for (index, character) in after.char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end = index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Default timestamped HTML file name.
pub fn file_name(title: &str, timestamp: &str) -> String {
    format!("family-book-{}-{timestamp}.html", sanitize_file_stem(title))
}

/// Renders the book as a standalone, printable HTML document.
pub fn render_html(report: &BookReport) -> String {
    let mut html = String::with_capacity(8192 + report.chapters.len() * 2048);
    html.push_str(&crate::report::html::document_open());
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>");
    html.push_str(&escape_html(&report.title));
    html.push_str("</title><style>");
    let mut seen = HashSet::new();
    for chapter in &report.chapters {
        if !chapter.styles.trim().is_empty() && seen.insert(chapter.styles.clone()) {
            html.push_str(&chapter.styles);
            html.push('\n');
        }
    }
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");

    html.push_str("<section class=\"cover\"><p class=\"book-kind\">Family History</p><h1>");
    html.push_str(&escape_html(&report.title));
    html.push_str("</h1>");
    if !report.subtitle.trim().is_empty() {
        html.push_str("<p class=\"subtitle\">");
        html.push_str(&escape_html(&report.subtitle));
        html.push_str("</p>");
    }
    if !report.author.trim().is_empty() {
        html.push_str("<p class=\"author\">");
        html.push_str(&escape_html(&report.author));
        html.push_str("</p>");
    }
    html.push_str("<p class=\"muted\">");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str(" \u{00b7} ");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str("</p></section>");

    html.push_str("<section class=\"toc\"><h1>Contents</h1>");
    if report.chapters.is_empty() {
        html.push_str("<p class=\"empty\">No chapters have been added yet.</p>");
    } else {
        html.push_str("<ol>");
        for (index, chapter) in report.chapters.iter().enumerate() {
            html.push_str("<li><a href=\"#chapter-");
            html.push_str(&(index + 1).to_string());
            html.push_str("\">");
            html.push_str(&escape_html(&chapter.title));
            html.push_str("</a></li>");
        }
        html.push_str("</ol>");
    }
    html.push_str("</section>");

    for (index, chapter) in report.chapters.iter().enumerate() {
        html.push_str("<section class=\"chapter\" id=\"chapter-");
        html.push_str(&(index + 1).to_string());
        html.push_str("\"><h1>");
        html.push_str(&escape_html(&chapter.title));
        html.push_str("</h1>");
        html.push_str(&chapter.body);
        html.push_str("</section>");
    }
    html.push_str("</body></html>");
    html
}

/// Writes the rendered book to `path` using the shared HTML/PDF output.
pub fn write_html(path: &Path, report: &BookReport) -> Result<usize, String> {
    let html = render_html(report);
    let html = crate::report::print::decorate(
        html,
        ReportKind::FamilyHistoryBook,
        &report.tree_name,
        &report.generated_at,
    );
    crate::report::output::write(path, &html)
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
        "book".to_string()
    } else {
        out
    }
}

const CSS: &str = r#"
body { font-family: Georgia, "Times New Roman", serif; margin: 24px; color: #1e293b; background: #f8fafc; line-height: 1.5; }
h1 { margin: 0 0 8px; }
p { margin: 2px 0; }
.muted { color: #64748b; font-size: 13px; }
.toolbar { display: flex; gap: 12px; align-items: center; margin-bottom: 20px; font-family: system-ui, sans-serif; }
.toolbar span { color: #64748b; font-size: 13px; }
.cover { min-height: 70vh; display: flex; flex-direction: column; justify-content: center; text-align: center; page-break-after: always; }
.book-kind { text-transform: uppercase; letter-spacing: 0.2em; color: #64748b; font-size: 12px; }
.cover h1 { font-size: 42px; }
.subtitle { font-size: 20px; color: #475569; }
.author { margin-top: 18px; font-size: 16px; }
.toc { page-break-after: always; }
.toc h1 { margin-bottom: 16px; }
.toc ol { padding-left: 24px; }
.toc li { margin: 6px 0; }
.toc a { color: #1e293b; text-decoration: none; }
.chapter { page-break-before: always; }
.chapter > h1 { border-bottom: 2px solid #cbd5e1; padding-bottom: 8px; margin-bottom: 18px; }
.chapter img { max-width: 100%; height: auto; }
.chapter table { width: 100%; border-collapse: collapse; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } @page { size: A4 portrait; margin: 18mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragments_keep_content_styles_and_person_headers() {
        let html = "<html><head><style>@page { margin: 5mm; } .card { color: red; }</style></head>\
                    <body><div class=\"toolbar\"><button>Print</button></div>\
                    <header class=\"person\"><h1>Ada</h1></header><main><p>Body</p></main></body></html>";
        let (body, styles) = fragment(html);
        assert!(body.contains("Ada"));
        assert!(body.contains("Body"));
        assert!(!body.contains("toolbar"));
        assert!(styles.contains(".card { color: red; }"));
        assert!(!styles.contains("@page"));
    }

    #[test]
    fn fragments_drop_non_person_headers() {
        let html = "<body><header><h1>Report</h1></header><main><p>Body</p></main></body>";
        let (body, _) = fragment(html);
        assert!(!body.contains("Report"));
        assert!(body.contains("Body"));
    }

    fn report() -> BookReport {
        BookReport {
            title: "King & Family".to_string(),
            subtitle: "A history".to_string(),
            author: "Ada".to_string(),
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            chapters: vec![
                BookChapterContent {
                    title: "Introduction".to_string(),
                    body: "<p>Hello</p>".to_string(),
                    styles: ".note { color: blue; }".to_string(),
                },
                BookChapterContent {
                    title: "Ada".to_string(),
                    body: "<p>Person</p>".to_string(),
                    styles: ".note { color: blue; }".to_string(),
                },
            ],
        }
    }

    #[test]
    fn renders_cover_toc_chapters_and_deduplicates_styles() {
        let html = render_html(&report());
        assert!(html.contains("King &amp; Family"), "{html}");
        assert!(html.contains("A history"));
        assert!(html.contains("Family"));
        assert!(html.contains("Contents"));
        assert!(html.contains("href=\"#chapter-1\""));
        assert!(html.contains("id=\"chapter-2\""));
        assert_eq!(html.matches(".note { color: blue; }").count(), 1);
        assert!(html.contains("page-break-before"));
    }

    #[test]
    fn empty_books_have_a_toc_message() {
        let mut report = report();
        report.chapters.clear();
        assert!(render_html(&report).contains("No chapters"));
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("King / Family", "20260101"),
            "family-book-King_Family-20260101.html"
        );
    }
}
