//! Printable album scrapbook pages.

use std::path::{Path, PathBuf};

use crate::report::html::{display_name, escape_html, image_data_uri};

/// One photo entry in a scrapbook album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapbookPhoto {
    pub path: PathBuf,
    pub caption: Option<String>,
    pub format: Option<String>,
    pub missing: bool,
}

/// One album page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapbookAlbum {
    pub name: String,
    pub description: Option<String>,
    pub person_name: Option<String>,
    pub photos: Vec<ScrapbookPhoto>,
}

/// Everything needed to render a scrapbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapbookReport {
    pub tree_name: String,
    pub generated_at: String,
    pub albums: Vec<ScrapbookAlbum>,
}

/// Default timestamped HTML file name.
pub fn file_name(tree_name: &str, timestamp: &str) -> String {
    format!(
        "scrapbook-{}-{timestamp}.html",
        sanitize_file_stem(tree_name)
    )
}

/// Renders the scrapbook as a standalone, printable HTML document.
pub fn render_html(report: &ScrapbookReport) -> String {
    let mut html = String::with_capacity(8192 + report.albums.len() * 1024);
    html.push_str("<!doctype html>\n<html><head><meta charset=\"utf-8\">");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    html.push_str("<title>Scrapbook \u{2014} ");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str("</title><style>");
    html.push_str(CSS);
    html.push_str("</style></head><body>");
    html.push_str("<div class=\"toolbar\"><button type=\"button\" onclick=\"window.print()\">Print</button><span>Print or save as PDF from the browser.</span></div>");
    html.push_str("<header><h1>Scrapbook</h1><p>");
    html.push_str(&escape_html(&report.tree_name));
    html.push_str("</p><p class=\"muted\">");
    html.push_str(&escape_html(&report.generated_at));
    html.push_str(" \u{00b7} ");
    html.push_str(&report.albums.len().to_string());
    html.push_str(" albums</p></header>");

    if report.albums.is_empty() {
        html.push_str("<p class=\"empty\">No albums are available.</p>");
        html.push_str("</body></html>");
        return html;
    }

    for album in &report.albums {
        html.push_str("<section class=\"album\"><h2>");
        html.push_str(&escape_html(&album.name));
        html.push_str("</h2>");
        if let Some(person) = &album.person_name {
            html.push_str("<p class=\"person\">");
            html.push_str(&escape_html(person));
            html.push_str("</p>");
        }
        if let Some(description) = &album.description
            && !description.trim().is_empty()
        {
            html.push_str("<p class=\"description\">");
            html.push_str(&escape_html(description));
            html.push_str("</p>");
        }
        if album.photos.is_empty() {
            html.push_str("<p class=\"empty\">No media in this album.</p>");
        } else {
            html.push_str("<div class=\"grid\">");
            for photo in &album.photos {
                html.push_str("<figure>");
                if !photo.missing
                    && let Some(uri) = image_data_uri(&photo.path)
                {
                    html.push_str(&format!(
                        "<img src=\"{}\" alt=\"{}\" loading=\"lazy\">",
                        escape_html(&uri),
                        escape_html(&photo.caption.clone().unwrap_or_default())
                    ));
                } else {
                    html.push_str("<div class=\"missing\"><strong>Missing media</strong><br>");
                    html.push_str(&escape_html(&display_name(&photo.path)));
                    html.push_str("</div>");
                }
                let caption = photo
                    .caption
                    .clone()
                    .filter(|caption| !caption.trim().is_empty())
                    .unwrap_or_else(|| display_name(&photo.path));
                html.push_str("<figcaption>");
                html.push_str(&escape_html(&caption));
                if let Some(format) = &photo.format
                    && !format.trim().is_empty()
                {
                    html.push_str("<span class=\"format\">");
                    html.push_str(&escape_html(format));
                    html.push_str("</span>");
                }
                html.push_str("</figcaption></figure>");
            }
            html.push_str("</div>");
        }
        html.push_str("</section>");
    }
    html.push_str("</body></html>");
    html
}

/// Writes the rendered scrapbook to `path`.
pub fn write_html(path: &Path, report: &ScrapbookReport) -> Result<usize, String> {
    let html = render_html(report);
    std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
    Ok(html.len())
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
.album { background: white; padding: 18px; border-radius: 12px; margin-bottom: 22px; page-break-after: always; }
.album:last-of-type { page-break-after: auto; }
.album h2 { margin: 0; }
.person { color: #475569; margin: 4px 0; }
.description { color: #334155; margin: 8px 0 14px; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; }
figure { margin: 0; background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 10px; overflow: hidden; }
figure img { display: block; width: 100%; height: 180px; object-fit: cover; background: #e2e8f0; }
figcaption { padding: 8px 10px; font-size: 13px; }
figcaption .format { display: block; color: #64748b; font-size: 11px; margin-top: 2px; }
.missing { height: 180px; display: flex; flex-direction: column; align-items: center; justify-content: center; color: #64748b; font-size: 12px; text-align: center; padding: 8px; }
.empty { color: #64748b; }
@media print { .toolbar { display: none; } body { background: white; margin: 0; } .album { padding: 0; margin: 0; box-shadow: none; } figure { break-inside: avoid; } @page { size: portrait; margin: 12mm; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> ScrapbookReport {
        let path =
            std::env::temp_dir().join(format!("stemma-scrapbook-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .expect("png writes");
        ScrapbookReport {
            tree_name: "Family & Friends".to_string(),
            generated_at: "now".to_string(),
            albums: vec![ScrapbookAlbum {
                name: "Trip".to_string(),
                description: Some("Summer <trip>".to_string()),
                person_name: Some("Ada & King".to_string()),
                photos: vec![
                    ScrapbookPhoto {
                        path,
                        caption: Some("Photo".to_string()),
                        format: Some("png".to_string()),
                        missing: false,
                    },
                    ScrapbookPhoto {
                        path: PathBuf::from("/missing/photo.jpg"),
                        caption: None,
                        format: Some("jpg".to_string()),
                        missing: true,
                    },
                ],
            }],
        }
    }

    #[test]
    fn renders_albums_photos_and_placeholders() {
        let report = report();
        let html = render_html(&report);
        assert!(html.contains("Family &amp; Friends"), "{html}");
        assert!(html.contains("Summer &lt;trip&gt;"));
        assert!(html.contains("Ada &amp; King"));
        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("Missing media"));
        assert!(html.contains("Photo"));
        if let Some(path) = report.albums[0].photos.first().map(|photo| &photo.path) {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn empty_scrapbook_has_a_message() {
        let report = ScrapbookReport {
            tree_name: "Family".to_string(),
            generated_at: "now".to_string(),
            albums: Vec::new(),
        };
        assert!(render_html(&report).contains("No albums"));
    }

    #[test]
    fn file_names_are_sanitized() {
        assert_eq!(
            file_name("Family / Tree", "20260101"),
            "scrapbook-Family_Tree-20260101.html"
        );
    }
}
