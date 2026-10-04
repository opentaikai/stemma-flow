//! Native file picker dialogs for GEDCOM import and export.

use std::path::PathBuf;

use rfd::FileDialog;

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
