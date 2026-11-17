//! PDF rendering through an installed Chromium-based browser or wkhtmltopdf.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use url::Url;

/// One supported external PDF renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfEngine {
    Chromium(String),
    Wkhtmltopdf(String),
}

/// Finds a renderer from the environment or the usual program names.
pub fn detect_engine() -> Result<PdfEngine, String> {
    if let Ok(configured) = std::env::var("STEMMA_PDF_BROWSER")
        && !configured.trim().is_empty()
    {
        let engine = engine_from_program(configured.trim());
        if command_available(program(&engine)) {
            return Ok(engine);
        }
        return Err(format!(
            "STEMMA_PDF_BROWSER points to an unavailable program: {}",
            program(&engine)
        ));
    }
    for candidate in chromium_candidates() {
        if command_available(candidate) {
            return Ok(PdfEngine::Chromium(candidate.to_string()));
        }
    }
    if command_available("wkhtmltopdf") {
        return Ok(PdfEngine::Wkhtmltopdf("wkhtmltopdf".to_string()));
    }
    Err(
        "No PDF renderer found. Install Chromium, Chrome, Edge or wkhtmltopdf, \
         or save the report as HTML and print it to PDF from the browser."
            .to_string(),
    )
}

/// Renders `html` to `path` using the first available renderer.
pub fn write(path: &Path, html: &str) -> Result<usize, String> {
    let engine = detect_engine()?;
    write_with_engine(&engine, path, html)
}

fn write_with_engine(engine: &PdfEngine, path: &Path, html: &str) -> Result<usize, String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let html_path =
        std::env::temp_dir().join(format!("stemma-report-{}.html", uuid::Uuid::new_v4()));
    let _guard = TempHtml(html_path.clone());
    fs::write(&html_path, html.as_bytes()).map_err(|error| error.to_string())?;
    let url = Url::from_file_path(&html_path)
        .map_err(|_| "could not build a file URL for the report".to_string())?;
    match engine {
        PdfEngine::Chromium(program) => {
            let mut output = Command::new(program)
                .args(chromium_args(path, &url, true))
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .map_err(|error| format!("could not run {program}: {error}"))?;
            if !output.status.success() || !path.is_file() {
                output = Command::new(program)
                    .args(chromium_args(path, &url, false))
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|error| format!("could not run {program}: {error}"))?;
            }
            if !output.status.success() || !path.is_file() {
                return Err(render_error(program, &output.stderr));
            }
        }
        PdfEngine::Wkhtmltopdf(program) => {
            let output = Command::new(program)
                .args(wkhtmltopdf_args(&html_path, path))
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output()
                .map_err(|error| format!("could not run {program}: {error}"))?;
            if !output.status.success() || !path.is_file() {
                return Err(render_error(program, &output.stderr));
            }
        }
    }
    fs::metadata(path)
        .map(|metadata| metadata.len() as usize)
        .map_err(|error| error.to_string())
}

fn engine_from_program(program: &str) -> PdfEngine {
    if program.to_ascii_lowercase().contains("wkhtml") {
        PdfEngine::Wkhtmltopdf(program.to_string())
    } else {
        PdfEngine::Chromium(program.to_string())
    }
}

fn program(engine: &PdfEngine) -> &str {
    match engine {
        PdfEngine::Chromium(program) | PdfEngine::Wkhtmltopdf(program) => program,
    }
}

fn command_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn chromium_candidates() -> Vec<&'static str> {
    let mut candidates = vec![
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "microsoft-edge",
        "msedge",
    ];
    if cfg!(target_os = "macos") {
        candidates.extend([
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ]);
    }
    if cfg!(windows) {
        candidates.extend([
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ]);
    }
    candidates
}

fn chromium_args(path: &Path, url: &Url, hide_header_footer: bool) -> Vec<String> {
    let mut args = vec!["--headless=new".to_string(), "--disable-gpu".to_string()];
    if hide_header_footer {
        args.push("--no-pdf-header-footer".to_string());
    }
    args.push(format!("--print-to-pdf={}", path.display()));
    args.push(url.to_string());
    args
}

fn wkhtmltopdf_args(html_path: &Path, path: &Path) -> Vec<String> {
    vec![
        "--enable-local-file-access".to_string(),
        "--quiet".to_string(),
        html_path.to_string_lossy().into_owned(),
        path.to_string_lossy().into_owned(),
    ]
}

fn render_error(program: &str, stderr: &[u8]) -> String {
    let message = String::from_utf8_lossy(stderr).trim().to_string();
    if message.is_empty() {
        format!("{program} could not create the PDF")
    } else {
        format!("{program} could not create the PDF: {message}")
    }
}

struct TempHtml(PathBuf);

impl Drop for TempHtml {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_arguments_include_headless_printing() {
        let url = Url::parse("file:///tmp/report.html").expect("url");
        let args = chromium_args(Path::new("/tmp/out.pdf"), &url, true);
        assert!(args.contains(&"--headless=new".to_string()));
        assert!(args.contains(&"--no-pdf-header-footer".to_string()));
        assert!(args.contains(&"--print-to-pdf=/tmp/out.pdf".to_string()));
        assert_eq!(args.last().map(String::as_str), Some(url.as_str()));
        let retry = chromium_args(Path::new("/tmp/out.pdf"), &url, false);
        assert!(!retry.contains(&"--no-pdf-header-footer".to_string()));
    }

    #[test]
    fn wkhtmltopdf_arguments_enable_local_files() {
        let args = wkhtmltopdf_args(Path::new("/tmp/in.html"), Path::new("/tmp/out.pdf"));
        assert!(args.contains(&"--enable-local-file-access".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("/tmp/out.pdf"));
    }

    #[test]
    fn engine_detection_uses_names_to_pick_the_command() {
        assert_eq!(
            engine_from_program("/usr/bin/wkhtmltopdf"),
            PdfEngine::Wkhtmltopdf("/usr/bin/wkhtmltopdf".to_string())
        );
        assert_eq!(
            engine_from_program("chromium"),
            PdfEngine::Chromium("chromium".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_fake_chromium_writes_a_pdf() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("stemma-pdf-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir");
        let script = dir.join("fake-chromium");
        fs::write(
            &script,
            "#!/bin/sh\nfor arg in \"$@\"; do\n  case \"$arg\" in\n    --print-to-pdf=*) out=\"${arg#--print-to-pdf=}\";;\n  esac\ndone\nprintf '%%PDF-1.4\\n' > \"$out\"\n",
        )
        .expect("script");
        let mut permissions = fs::metadata(&script).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).expect("chmod");

        let dest = dir.join("report.pdf");
        let engine = PdfEngine::Chromium(script.to_string_lossy().into_owned());
        let bytes = write_with_engine(&engine, &dest, "<html><body>Test</body></html>")
            .expect("pdf writes");
        assert!(bytes > 0);
        assert!(
            fs::read_to_string(&dest)
                .expect("pdf")
                .starts_with("%PDF-1.4")
        );
        let _ = fs::remove_dir_all(dir);
    }
}
