//! Preferred file format for exported reports.

use serde::{Deserialize, Serialize};

/// File format used when saving a report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportFormat {
    #[default]
    Html,
    Pdf,
}

impl ReportFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Html => "HTML",
            Self::Pdf => "PDF",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_expose_labels_and_extensions() {
        assert_eq!(ReportFormat::default(), ReportFormat::Html);
        assert_eq!(ReportFormat::Html.extension(), "html");
        assert_eq!(ReportFormat::Pdf.extension(), "pdf");
        assert_eq!(ReportFormat::Pdf.label(), "PDF");
    }

    #[test]
    fn formats_round_trip_through_json() {
        let text = serde_json::to_string(&ReportFormat::Pdf).expect("serialize");
        let parsed: ReportFormat = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, ReportFormat::Pdf);
    }
}
