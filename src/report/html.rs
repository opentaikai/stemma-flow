//! Shared helpers for standalone report HTML and embedded images.

use std::path::Path;

use base64::Engine as _;

/// Opens a standalone HTML document with the active report language.
pub fn document_open() -> String {
    format!(
        "<!doctype html>\n<html lang=\"{}\"><head><meta charset=\"utf-8\">",
        escape_html(&crate::i18n::current_language())
    )
}

/// Escapes text for safe inclusion in HTML or SVG text.
pub fn escape_html(value: &str) -> String {
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

/// Returns the MIME type for a supported embedded image.
pub fn mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

/// Reads a supported image and returns a data URL.
pub fn image_data_uri(path: &Path) -> Option<String> {
    let mime = mime_type(path)?;
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{encoded}"))
}

/// Human-readable file name used by placeholders.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or("media"))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_language_follows_the_active_catalog() {
        let html = crate::i18n::with_language("de", document_open);
        assert!(html.contains("lang=\"de\""), "{html}");
        let html = crate::i18n::with_language("en", document_open);
        assert!(html.contains("lang=\"en\""), "{html}");
    }

    #[test]
    fn escapes_markup_characters() {
        assert_eq!(
            escape_html("Ada & <King> \"quoted\""),
            "Ada &amp; &lt;King&gt; &quot;quoted&quot;"
        );
    }

    #[test]
    fn embeds_supported_images_and_skips_missing_files() {
        let path = std::env::temp_dir().join(format!("stemma-report-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]))
            .save(&path)
            .expect("png writes");
        let uri = image_data_uri(&path).expect("data uri");
        assert!(uri.starts_with("data:image/png;base64,"));
        assert!(image_data_uri(&path.with_extension("missing")).is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recognizes_supported_mime_types() {
        assert_eq!(mime_type(Path::new("a.JPG")), Some("image/jpeg"));
        assert_eq!(mime_type(Path::new("a.webp")), Some("image/webp"));
        assert_eq!(mime_type(Path::new("a.pdf")), None);
    }
}
