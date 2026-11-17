//! Native file picker dialogs for GEDCOM import and export.

use std::path::{Path, PathBuf};

use rfd::FileDialog;

use crate::model::report_format::ReportFormat;

/// Opens the native open dialog filtered to GEDCOM files.
///
/// Returns `None` when the user cancels the dialog.
pub fn pick_gedcom_import_path() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("GEDCOM File", &["ged", "gedcom"])
        .pick_file()
}

/// Opens the native save dialog pre-filled with `family_tree.ged`.
///
/// Returns `None` when the user cancels the dialog.
pub fn prompt_gedcom_export_path() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("GEDCOM File", &["ged", "gedcom"])
        .set_file_name("family_tree.ged")
        .save_file()
}

/// Opens the native save dialog for a new SQLite tree database.
///
/// Returns `None` when the user cancels the dialog.
pub fn prompt_new_tree_path() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("SQLite Database", &["db", "sqlite"])
        .set_file_name("family_tree.db")
        .save_file()
}

/// Opens the native open dialog for an existing SQLite tree database.
///
/// Returns `None` when the user cancels the dialog.
pub fn pick_tree_open_path() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("SQLite Database", &["db", "sqlite"])
        .pick_file()
}

/// Opens the native picker for an external media file.
pub fn pick_media_path() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter(
            "Media",
            &[
                "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "pdf", "mp3", "mp4",
                "wav", "mov",
            ],
        )
        .pick_file()
}

/// Opens the native folder picker used for media repair searches.
pub fn pick_folder() -> Option<PathBuf> {
    FileDialog::new().pick_folder()
}

/// Opens the save dialog for a new timestamped backup archive.
pub fn prompt_backup_path(default_name: &str, directory: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = FileDialog::new()
        .add_filter("Stemma Flow Backup", &["zip"])
        .set_file_name(default_name);
    if let Some(directory) = directory {
        dialog = dialog.set_directory(directory);
    }
    dialog.save_file().map(|path| ensure_extension(path, "zip"))
}

/// Opens the picker for an existing backup archive.
pub fn pick_backup_restore_path() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("Stemma Flow Backup", &["zip"])
        .pick_file()
}

/// Opens the save dialog for the new database created by a restore.
pub fn prompt_restore_target(default_name: &str, directory: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = FileDialog::new()
        .add_filter("SQLite Database", &["db", "sqlite"])
        .set_file_name(default_name);
    if let Some(directory) = directory {
        dialog = dialog.set_directory(directory);
    }
    dialog.save_file().map(|path| ensure_extension(path, "db"))
}

fn ensure_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|value| !value.eq_ignore_ascii_case(extension))
    {
        path.set_extension(extension);
    }
    path
}

/// Opens the save dialog for a report in the preferred format.
pub fn prompt_report_path(
    default_name: &str,
    directory: Option<&Path>,
    format: ReportFormat,
) -> Option<PathBuf> {
    let default_name = report_file_name(default_name, format);
    let (label, extensions) = match format {
        ReportFormat::Html => ("HTML Report", &["html", "htm"][..]),
        ReportFormat::Pdf => ("PDF Report", &["pdf"][..]),
    };
    let mut dialog = FileDialog::new()
        .add_filter(label, extensions)
        .set_file_name(default_name);
    if let Some(directory) = directory {
        dialog = dialog.set_directory(directory);
    }
    dialog
        .save_file()
        .map(|path| ensure_extension(path, format.extension()))
}

fn report_file_name(default_name: &str, format: ReportFormat) -> String {
    let mut path = PathBuf::from(default_name);
    path.set_extension(format.extension());
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(default_name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_names_follow_the_selected_format() {
        assert_eq!(
            report_file_name("pedigree-Ada-20260101.html", ReportFormat::Pdf),
            "pedigree-Ada-20260101.pdf"
        );
        assert_eq!(
            report_file_name("pedigree-Ada-20260101.pdf", ReportFormat::Html),
            "pedigree-Ada-20260101.html"
        );
        assert_eq!(report_file_name("report", ReportFormat::Pdf), "report.pdf");
    }

    #[test]
    fn extensions_are_enforced() {
        assert_eq!(
            ensure_extension(PathBuf::from("report.txt"), "pdf"),
            PathBuf::from("report.pdf")
        );
        assert_eq!(
            ensure_extension(PathBuf::from("report.PDF"), "pdf"),
            PathBuf::from("report.PDF")
        );
    }
}
