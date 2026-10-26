//! Domain models and value parsers shared by the storage and GEDCOM layers.

pub mod date;
pub mod graph;
pub mod source_template;

pub use graph::{GenealogyDb, PersonNode, Relationship};
