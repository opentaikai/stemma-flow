//! Database integrity, index rebuild and compression tools.

use std::path::Path;

use egui::{RichText, Ui, Window};

use crate::app::MaintenanceResult;
use crate::db::maintenance::{MaintenanceKind, MaintenanceReport};
use crate::i18n::{tr, tr_args};

const OK_COLOR: egui::Color32 = egui::Color32::from_rgb(34, 197, 94);

/// State of the Database Tools window.
#[derive(Debug, Default)]
pub struct DatabaseToolsState {
    pub open: bool,
    pub pending: Option<MaintenanceKind>,
    pub backup_first: bool,
    pub result: Option<MaintenanceResult>,
    pub error: Option<String>,
}

impl DatabaseToolsState {
    pub fn open_with(&mut self) {
        *self = Self {
            open: true,
            backup_first: true,
            ..Self::default()
        };
    }
}

/// Action requested by the Database Tools window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseToolsEvent {
    Run {
        kind: MaintenanceKind,
        backup_first: bool,
    },
}

/// Draws the window; the caller starts the worker job.
pub fn show(
    ctx: &egui::Context,
    state: &mut DatabaseToolsState,
    db_path: Option<&Path>,
    size: u64,
    busy: bool,
) -> Option<DatabaseToolsEvent> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("db_tools.title"))
        .id(egui::Id::new("database_tools_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .open(&mut open)
        .show(ctx, |ui| {
            if let Some(path) = db_path {
                ui.label(RichText::new(path.display().to_string()).weak().small());
                ui.label(tr_args("db_tools.size", &[("size", format_bytes(size))]));
            } else {
                ui.label(RichText::new(tr("db_tools.no_tree")).weak());
            }
            ui.separator();

            tool_section(
                ui,
                "db_tools.check_title",
                "db_tools.check_hint",
                "db_tools.check_action",
                busy || db_path.is_none(),
                || {
                    event = Some(DatabaseToolsEvent::Run {
                        kind: MaintenanceKind::Integrity,
                        backup_first: false,
                    });
                },
            );

            destructive_section(
                ui,
                state,
                busy || db_path.is_none(),
                MaintenanceKind::RebuildIndexes,
                "db_tools.rebuild_title",
                "db_tools.rebuild_hint",
                "db_tools.rebuild_action",
                "db_tools.rebuild_confirm",
                &mut event,
            );
            ui.separator();
            destructive_section(
                ui,
                state,
                busy || db_path.is_none(),
                MaintenanceKind::Compress,
                "db_tools.compress_title",
                "db_tools.compress_hint",
                "db_tools.compress_action",
                "db_tools.compress_confirm",
                &mut event,
            );

            if let Some(error) = &state.error {
                ui.separator();
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }
            if let Some(result) = &state.result {
                ui.separator();
                report(ui, result);
            }

            ui.separator();
            if ui.button(tr("common.close")).clicked() {
                state.open = false;
            }
        });
    if !open {
        state.open = false;
    }
    event
}

fn tool_section(
    ui: &mut Ui,
    title: &str,
    hint: &str,
    action: &str,
    disabled: bool,
    mut run: impl FnMut(),
) {
    ui.label(RichText::new(tr(title)).strong());
    ui.label(RichText::new(tr(hint)).weak().small());
    if ui
        .add_enabled(!disabled, egui::Button::new(tr(action)))
        .clicked()
    {
        run();
    }
}

#[allow(clippy::too_many_arguments)]
fn destructive_section(
    ui: &mut Ui,
    state: &mut DatabaseToolsState,
    disabled: bool,
    kind: MaintenanceKind,
    title: &str,
    hint: &str,
    action: &str,
    confirm: &str,
    event: &mut Option<DatabaseToolsEvent>,
) {
    ui.separator();
    ui.label(RichText::new(tr(title)).strong());
    ui.label(RichText::new(tr(hint)).weak().small());
    if state.pending == Some(kind) {
        ui.label(
            RichText::new(tr(confirm))
                .color(ui.visuals().error_fg_color)
                .small(),
        );
        ui.checkbox(&mut state.backup_first, tr("db_tools.backup_first"));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !disabled,
                    egui::Button::new(RichText::new(tr("db_tools.run")).strong()),
                )
                .clicked()
            {
                *event = Some(DatabaseToolsEvent::Run {
                    kind,
                    backup_first: state.backup_first,
                });
                state.pending = None;
            }
            if ui.button(tr("common.cancel")).clicked() {
                state.pending = None;
            }
        });
    } else if ui
        .add_enabled(!disabled, egui::Button::new(tr(action)))
        .clicked()
    {
        state.pending = Some(kind);
        state.backup_first = true;
        state.error = None;
    }
}

fn report(ui: &mut Ui, result: &MaintenanceResult) {
    if let Some(path) = &result.backup_path {
        ui.label(
            RichText::new(tr_args(
                "db_tools.backup_created",
                &[("path", path.display().to_string())],
            ))
            .weak()
            .small(),
        );
    }
    match &result.report {
        MaintenanceReport {
            kind: MaintenanceKind::Integrity,
            healthy,
            issues,
            foreign_key_violations,
            page_count,
            freelist_count,
            duration_ms,
            ..
        } => {
            if *healthy {
                ui.label(
                    RichText::new(tr("db_tools.healthy"))
                        .color(OK_COLOR)
                        .strong(),
                );
            } else {
                ui.label(
                    RichText::new(tr_args(
                        "db_tools.unhealthy",
                        &[
                            ("issues", issues.len().to_string()),
                            ("foreign_keys", foreign_key_violations.to_string()),
                        ],
                    ))
                    .color(ui.visuals().error_fg_color)
                    .strong(),
                );
            }
            ui.label(
                RichText::new(tr_args(
                    "db_tools.check_summary",
                    &[
                        ("pages", page_count.to_string()),
                        ("free", freelist_count.to_string()),
                        ("duration", duration_ms.to_string()),
                    ],
                ))
                .weak()
                .small(),
            );
            if !issues.is_empty() {
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .id_salt("db_tools_integrity_issues")
                    .show(ui, |ui| {
                        for issue in issues {
                            ui.label(
                                RichText::new(issue)
                                    .color(ui.visuals().error_fg_color)
                                    .small(),
                            );
                        }
                    });
            }
        }
        MaintenanceReport {
            kind: MaintenanceKind::RebuildIndexes,
            indexes_rebuilt,
            size_before,
            size_after,
            duration_ms,
            ..
        } => {
            ui.label(
                RichText::new(tr_args(
                    "db_tools.rebuilt",
                    &[("count", indexes_rebuilt.to_string())],
                ))
                .color(OK_COLOR)
                .strong(),
            );
            ui.label(
                RichText::new(tr_args(
                    "db_tools.size_change",
                    &[
                        ("before", format_bytes(*size_before)),
                        ("after", format_bytes(*size_after)),
                        ("duration", duration_ms.to_string()),
                    ],
                ))
                .weak()
                .small(),
            );
        }
        MaintenanceReport {
            kind: MaintenanceKind::Compress,
            size_before,
            size_after,
            duration_ms,
            ..
        } => {
            let saved = size_before.saturating_sub(*size_after);
            ui.label(
                RichText::new(tr_args(
                    "db_tools.compressed",
                    &[("saved", format_bytes(saved))],
                ))
                .color(OK_COLOR)
                .strong(),
            );
            ui.label(
                RichText::new(tr_args(
                    "db_tools.size_change",
                    &[
                        ("before", format_bytes(*size_before)),
                        ("after", format_bytes(*size_after)),
                        ("duration", duration_ms.to_string()),
                    ],
                ))
                .weak()
                .small(),
            );
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    #[test]
    fn formats_sizes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn dialog_renders_headless_and_requires_confirmation() {
        let ctx = Context::default();
        let mut state = DatabaseToolsState::default();
        state.open_with();
        state.pending = Some(MaintenanceKind::Compress);
        let mut event = None;
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            event = show(
                ui.ctx(),
                &mut state,
                Some(Path::new("/tmp/tree.db")),
                1024,
                false,
            );
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert!(event.is_none(), "confirmation needs an explicit run");
        assert_eq!(state.pending, Some(MaintenanceKind::Compress));
        assert!(state.backup_first);
    }

    #[test]
    fn result_report_renders() {
        let ctx = Context::default();
        let mut state = DatabaseToolsState::default();
        state.open_with();
        state.result = Some(MaintenanceResult {
            report: MaintenanceReport {
                kind: MaintenanceKind::Compress,
                healthy: true,
                issues: Vec::new(),
                foreign_key_violations: 0,
                indexes_rebuilt: 0,
                page_count: 10,
                freelist_count: 0,
                size_before: 4096,
                size_after: 2048,
                duration_ms: 5,
            },
            backup_path: None,
        });
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            show(
                ui.ctx(),
                &mut state,
                Some(Path::new("/tmp/tree.db")),
                1024,
                false,
            );
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
