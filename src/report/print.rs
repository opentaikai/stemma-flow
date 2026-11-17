//! Print-only header, footer, margin and font decoration for report HTML.

use std::cell::RefCell;

use crate::model::print_profile::{PrintProfile, PrintProfiles, ReportKind};
use crate::report::html::escape_html;

thread_local! {
    static PROFILES: RefCell<PrintProfiles> = RefCell::new(PrintProfiles::default());
}

/// Publishes the profiles used by report writers on this worker thread.
pub fn set_profiles(profiles: PrintProfiles) {
    PROFILES.with(|current| *current.borrow_mut() = profiles);
}

pub fn clear_profiles() {
    PROFILES.with(|current| *current.borrow_mut() = PrintProfiles::default());
}

/// Applies the current thread's profile for `kind` to a rendered document.
pub fn decorate(html: String, kind: ReportKind, tree: &str, date: &str) -> String {
    let profile = PROFILES.with(|profiles| profiles.borrow().get(kind).cloned());
    match profile {
        Some(profile) => apply(html, &profile, kind, tree, date),
        None => html,
    }
}

/// Applies one explicit print profile to a rendered document.
pub fn apply(
    html: String,
    profile: &PrintProfile,
    kind: ReportKind,
    tree: &str,
    date: &str,
) -> String {
    if profile.is_default() {
        return html;
    }
    let mut style = String::from("<style id=\"stemma-print-options\">");
    if let Some(margin) = profile.margin_mm {
        style.push_str(&format!("@page {{ margin: {margin}mm; }}"));
    }
    if let Some(family) = profile.font.css_family() {
        style.push_str(&format!(
            "body, p, li, td, th {{ font-family: {family}; }} svg text {{ font-family: {family}; }}"
        ));
    }
    if let Some(size) = profile.font_size_pt {
        style.push_str(&format!(
            "body {{ font-size: {size}pt; }} p, li, td, th {{ font-size: {size}pt; }}"
        ));
    }
    let header = template(&profile.header, kind, tree, date);
    let footer = template(&profile.footer, kind, tree, date);
    if !header.is_empty() || !footer.is_empty() {
        style.push_str(
            ".stemma-print-header, .stemma-print-footer { display: none; } \
             @media print { \
               .stemma-print-header, .stemma-print-footer { \
                 display: block; position: fixed; left: 0; right: 0; \
                 text-align: center; font-size: 8pt; color: #475569; \
               } \
               .stemma-print-header { top: 2mm; } \
               .stemma-print-footer { bottom: 2mm; } \
             }",
        );
    }
    style.push_str("</style>");

    let mut html = html.replacen("</head>", &format!("{style}</head>"), 1);
    if !header.is_empty() {
        html = html.replacen(
            "<body>",
            &format!("<body><div class=\"stemma-print-header\">{header}</div>"),
            1,
        );
    }
    if !footer.is_empty() {
        html = html.replacen(
            "</body>",
            &format!("<div class=\"stemma-print-footer\">{footer}</div></body>"),
            1,
        );
    }
    html
}

fn template(text: &str, kind: ReportKind, tree: &str, date: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&escape_html(&rest[..open]));
        let after = &rest[open..];
        let Some(close) = after.find('}') else {
            out.push_str(&escape_html(after));
            return out;
        };
        let token = &after[1..close];
        match token {
            "report" => out.push_str(&escape_html(kind.label())),
            "tree" => out.push_str(&escape_html(tree)),
            "date" => out.push_str(&escape_html(date)),
            _ => out.push_str(&escape_html(&after[..=close])),
        }
        rest = &after[close + 1..];
    }
    out.push_str(&escape_html(rest));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::print_profile::PrintFont;

    fn document() -> String {
        "<!doctype html><html><head><style>body { color: black; }</style></head>\
         <body><h1>Report</h1></body></html>"
            .to_string()
    }

    #[test]
    fn default_profiles_leave_html_unchanged() {
        let html = document();
        assert_eq!(
            apply(
                html.clone(),
                &PrintProfile::default(),
                ReportKind::PedigreeChart,
                "Family",
                "today"
            ),
            html
        );
    }

    #[test]
    fn inserts_styles_header_footer_placeholders_and_escapes() {
        let profile = PrintProfile {
            header: "Family & {tree}".to_string(),
            footer: "{report} \u{00b7} {date} <{unknown}>".to_string(),
            margin_mm: Some(15),
            font: PrintFont::Serif,
            font_size_pt: Some(10.5),
        };
        let html = apply(
            document(),
            &profile,
            ReportKind::FamilyGroupSheet,
            "King <family>",
            "2026-01-01",
        );
        assert!(html.contains("<style id=\"stemma-print-options\">"));
        assert!(html.contains("@page { margin: 15mm; }"));
        assert!(html.contains("font-family: Georgia"));
        assert!(html.contains("font-size: 10.5pt"));
        assert!(html.contains("Family &amp; King &lt;family&gt;"));
        assert!(html.contains("Family Group Sheet \u{00b7} 2026-01-01 &lt;{unknown}&gt;"));
        assert!(html.contains("class=\"stemma-print-header\""));
        assert!(html.contains("class=\"stemma-print-footer\""));
    }

    #[test]
    fn malformed_documents_do_not_panic() {
        let profile = PrintProfile {
            header: "Header".to_string(),
            ..PrintProfile::default()
        };
        let html = apply(
            "no document tags".to_string(),
            &profile,
            ReportKind::PhotoTree,
            "Family",
            "today",
        );
        assert_eq!(html, "no document tags");
    }

    #[test]
    fn report_writers_apply_the_thread_profile() {
        std::thread::spawn(|| {
            let mut profiles = PrintProfiles::default();
            profiles.set(
                ReportKind::GroupRoster,
                PrintProfile {
                    header: "{tree} roster".to_string(),
                    margin_mm: Some(18),
                    ..PrintProfile::default()
                },
            );
            set_profiles(profiles);
            let path =
                std::env::temp_dir().join(format!("stemma-print-{}.html", uuid::Uuid::new_v4()));
            let report = crate::report::group_roster::GroupRosterReport {
                tree_name: "Family & Friends".to_string(),
                generated_at: "now".to_string(),
                ..crate::report::group_roster::GroupRosterReport::default()
            };
            crate::report::group_roster::write_html(&path, &report).expect("write");
            let html = std::fs::read_to_string(&path).expect("read");
            assert!(html.contains("Family &amp; Friends roster"), "{html}");
            assert!(html.contains("@page { margin: 18mm; }"));
            let _ = std::fs::remove_file(path);
            clear_profiles();
        })
        .join()
        .expect("thread completes");
    }

    #[test]
    fn thread_profiles_decorate_matching_reports() {
        std::thread::spawn(|| {
            let mut profiles = PrintProfiles::default();
            profiles.set(
                ReportKind::KinshipReport,
                PrintProfile {
                    header: "Kinship".to_string(),
                    ..PrintProfile::default()
                },
            );
            set_profiles(profiles);
            let html = decorate(document(), ReportKind::KinshipReport, "Family", "today");
            assert!(html.contains("Kinship"));
            let other = decorate(document(), ReportKind::PhotoTree, "Family", "today");
            assert!(!other.contains("Kinship"));
            clear_profiles();
        })
        .join()
        .expect("thread completes");
    }
}
