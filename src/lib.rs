//! Local-first genealogy application.
//!
//! Production code carries two compile-enforced promises: no `unsafe` (set in
//! `Cargo.toml`) and no panicking primitives. Tests are exempt from the latter
//! because a failing test should abort its case, not return a `Result`.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod app;
pub mod backup;
pub mod config;
pub mod db;
pub mod gedcom;
pub mod geocode;
pub mod gui;
pub mod i18n;
pub mod media_path;
pub mod model;
pub mod report;
