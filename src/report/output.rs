//! Shared report output dispatch for HTML and PDF.

use std::path::Path;

/// Writes report HTML, rendering a PDF when the destination ends in `.pdf`.
pub fn write(path: &Path, html: &str) -> Result<usize, String> {
    if is_pdf_path(path) {
        crate::report::pdf::write(path, html)
    } else {
        std::fs::write(path, html.as_bytes()).map_err(|error| error.to_string())?;
        Ok(html.len())
    }
}

pub fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pdf_destinations_case_insensitively() {
        assert!(is_pdf_path(Path::new("/tmp/report.pdf")));
        assert!(is_pdf_path(Path::new("/tmp/report.PDF")));
        assert!(!is_pdf_path(Path::new("/tmp/report.html")));
        assert!(!is_pdf_path(Path::new("/tmp/report")));
    }

    #[test]
    fn writes_html_without_a_renderer() {
        let path =
            std::env::temp_dir().join(format!("stemma-output-{}.html", uuid::Uuid::new_v4()));
        let bytes = write(&path, "<html>ok</html>").expect("html writes");
        assert_eq!(bytes, "<html>ok</html>".len());
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "<html>ok</html>"
        );
        let _ = std::fs::remove_file(path);
    }
}
