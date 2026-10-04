//! GEDCOM 5.5.1 import and export engine.

pub mod export;
pub mod import;

pub use export::export_to_gedcom;
pub use import::import_gedcom;

use thiserror::Error;

/// Summary of a completed import, including recoverable parse problems.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub people: usize,
    pub families: usize,
    pub child_links: usize,
    pub events: usize,
    pub warnings: Vec<String>,
}

/// Errors that abort a GEDCOM operation.
///
/// Recoverable parse problems (dangling pointers, malformed lines) are not
/// errors; they are collected as warnings in the import report instead.
#[derive(Debug, Error)]
pub enum GedcomError {
    #[error("I/O error while reading GEDCOM data: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error during GEDCOM transfer: {0}")]
    Db(#[from] rusqlite::Error),
}
