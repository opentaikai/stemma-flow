//! Menu bar, status bar and welcome dashboard.

use crate::app::{JobOutcome, RunningJob, StatusMessage, UiStatus};
use crate::db::maintenance::MaintenanceKind;
use crate::gui::tabs;
use crate::gui::welcome;
use crate::gui::welcome::WelcomeAction;
use crate::i18n::{tr, tr_args, trn_args};
use crate::model::print_profile::ReportKind;
use eframe::Frame;
use egui::Color32;
use egui::Ui;
use egui_material_icons::icons::{ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, ICON_STAR};

use super::TreeWindow;

impl TreeWindow {
    pub(super) fn top_panel(&mut self, ui: &mut Ui, frame: &mut Frame) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(tr("menu.file"), |ui| {
                if ui.button(tr("menu.new_tree")).clicked() {
                    ui.close();
                    self.create_new_tree(frame);
                }
                if ui.button(tr("menu.open_tree")).clicked() {
                    ui.close();
                    self.open_tree(frame);
                }
                if ui.button(tr("menu.import_gedcom")).clicked() {
                    ui.close();
                    if self.app.active_db().is_some() {
                        self.app.begin_import();
                    } else {
                        self.app.begin_import_new();
                    }
                }
                let can_export = self.app.active_db().is_some() && !self.app.is_busy();
                if ui
                    .add_enabled(can_export, egui::Button::new(tr("menu.export_gedcom")))
                    .clicked()
                {
                    ui.close();
                    self.export_options = crate::gedcom::ExportOptions::default();
                    self.show_export = true;
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.app.active_db().is_some() && !self.app.is_busy(),
                        egui::Button::new(tr("menu.backup_tree")),
                    )
                    .clicked()
                {
                    ui.close();
                    self.app.begin_backup();
                }
                if ui
                    .add_enabled(
                        !self.app.is_busy(),
                        egui::Button::new(tr("menu.restore_backup")),
                    )
                    .clicked()
                {
                    ui.close();
                    self.app.begin_restore();
                }
                if !self.recent.is_empty() {
                    self.recent_menu(ui, frame);
                }
                if self.app.active_db().is_some() {
                    ui.separator();
                    if ui.button(tr("menu.close_tree")).clicked() {
                        ui.close();
                        self.app.close_tree();
                    }
                }
                ui.separator();
                if ui.button(tr("menu.settings")).clicked() {
                    ui.close();
                    self.show_settings = true;
                }
                ui.separator();
                if ui.button(tr("menu.quit")).clicked() {
                    ui.close();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if self.app.active_db().is_some() {
                let mut surname_first = self.tree_settings.surname_first;
                let mut changed = None;
                ui.menu_button(tr("menu.view"), |ui| {
                    if ui
                        .checkbox(&mut surname_first, tr("menu.surname_first"))
                        .changed()
                    {
                        changed = Some(surname_first);
                    }
                });
                if let Some(value) = changed {
                    self.set_surname_first(value);
                }
            }
            ui.menu_button(tr("tools.menu"), |ui| {
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.find")).clicked()
                {
                    ui.close();
                    self.find.open = true;
                }
                if ui.button(tr("tools.calculator")).clicked() {
                    ui.close();
                    self.open_relationship_calculator();
                }
                if ui.button(tr("tools.date_calculator")).clicked() {
                    ui.close();
                    self.date_calculator.open = true;
                }
                if ui.button(tr("tools.soundex")).clicked() {
                    ui.close();
                    self.soundex.open = true;
                }
                if self.app.active_db().is_some() && ui.button(tr("tools.merge_people")).clicked() {
                    ui.close();
                    self.open_merge_dialog(self.app.selected_person_id().map(str::to_string));
                }
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.database")).clicked()
                {
                    ui.close();
                    self.open_database_tools();
                }
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.dataclean")).clicked()
                {
                    ui.close();
                    self.open_data_clean();
                }
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.placeclean")).clicked()
                {
                    ui.close();
                    self.open_place_clean();
                }
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.geocode")).clicked()
                {
                    ui.close();
                    self.open_geocode();
                }
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.gazetteer")).clicked()
                {
                    ui.close();
                    self.open_gazetteer();
                }
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.media_repair")).clicked()
                {
                    ui.close();
                    self.open_media_repair();
                }
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.share_bundle")).clicked()
                {
                    ui.close();
                    self.open_share_bundle();
                }
                if self.app.active_db().is_some()
                    && !self.app.is_busy()
                    && ui.button(tr("tools.replace")).clicked()
                {
                    ui.close();
                    self.search_replace.open = true;
                }
            });
            ui.menu_button(tr("menu.reports"), |ui| {
                let can_report = self.app.active_db().is_some() && !self.app.is_busy();
                let report = |ui: &mut Ui, label: &str| {
                    ui.add_enabled(can_report, egui::Button::new(label))
                        .clicked()
                };
                if report(ui, &tr("tools.photo_tree")) {
                    ui.close();
                    self.open_photo_tree();
                }
                if report(ui, &tr("tools.scrapbook")) {
                    ui.close();
                    self.open_scrapbook();
                }
                ui.separator();
                if report(ui, &tr("tools.pedigree_chart")) {
                    ui.close();
                    self.open_pedigree_chart();
                }
                if report(ui, &tr("tools.fan_chart")) {
                    ui.close();
                    self.open_fan_chart();
                }
                if report(ui, &tr("tools.hourglass_chart")) {
                    ui.close();
                    self.open_hourglass_chart();
                }
                if report(ui, &tr("tools.wall_chart")) {
                    ui.close();
                    self.open_wall_chart();
                }
                ui.separator();
                if report(ui, &tr("tools.ahnentafel_list")) {
                    ui.close();
                    self.open_ahnentafel_list();
                }
                if report(ui, &tr("tools.descendant_list")) {
                    ui.close();
                    self.open_descendant_list();
                }
                if report(ui, &tr("tools.family_sheet")) {
                    ui.close();
                    self.open_family_sheet();
                }
                if report(ui, &tr("tools.individual_summary")) {
                    ui.close();
                    self.open_individual_summary();
                }
                if report(ui, &tr("tools.individual_list")) {
                    ui.close();
                    self.open_individual_list();
                }
                if report(ui, &tr("tools.kinship_report")) {
                    ui.close();
                    self.open_kinship_report();
                }
                if report(ui, &tr("tools.calendar_reports")) {
                    ui.close();
                    self.open_calendar_report();
                }
                if report(ui, &tr("tools.timeline")) {
                    ui.close();
                    self.open_timeline();
                }
                if report(ui, &tr("tools.administrative_lists")) {
                    ui.close();
                    self.open_administrative_lists();
                }
                if report(ui, &tr("tools.cleanup_reports")) {
                    ui.close();
                    self.open_cleanup_reports();
                }
                ui.separator();
                if report(ui, &tr("tools.family_book")) {
                    ui.close();
                    self.open_family_book();
                }
                if report(ui, &tr("tools.website")) {
                    ui.close();
                    self.open_website_export();
                }
                ui.separator();
                if ui.button(tr("tools.print_settings")).clicked() {
                    ui.close();
                    self.open_print_settings();
                }
            });
            if self.app.active_db().is_some() {
                let items = &self.bookmarks;
                let mut chosen = None;
                let mut removed = None;
                ui.menu_button(tr("bookmarks.menu"), |ui| {
                    if items.is_empty() {
                        ui.label(egui::RichText::new(tr("bookmarks.empty")).weak());
                    }
                    for bookmark in items {
                        ui.horizontal(|ui| {
                            if ui.button(&bookmark.name).clicked() {
                                chosen = Some(bookmark.person_id.clone());
                                ui.close();
                            }
                            if ui
                                .small_button("×")
                                .on_hover_text(tr("bookmarks.remove"))
                                .clicked()
                            {
                                removed = Some(bookmark.person_id.clone());
                            }
                        });
                    }
                });
                if let Some(person_id) = chosen {
                    self.focus_person(&person_id);
                }
                if let Some(person_id) = removed {
                    self.remove_bookmark(&person_id);
                }
            }
            if let Some(path) = self.app.active_db() {
                let label = tr_args("menu.active_tree", &[("name", welcome::display_name(path))]);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(label).weak());
                });
            }
        });
        let rect = ui.max_rect();
        ui.painter().hline(
            rect.x_range(),
            rect.bottom(),
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
    }

    pub(super) fn workspace_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let back = ui
                .add_enabled(
                    self.history.can_back(),
                    egui::Button::new(ICON_CHEVRON_LEFT.rich_text().size(18.0)),
                )
                .on_hover_text(tr("nav.back"));
            if back.clicked() {
                self.navigate_back();
            }
            let forward = ui
                .add_enabled(
                    self.history.can_forward(),
                    egui::Button::new(ICON_CHEVRON_RIGHT.rich_text().size(18.0)),
                )
                .on_hover_text(tr("nav.forward"));
            if forward.clicked() {
                self.navigate_forward();
            }
            let selected = self.app.selected_person_id().is_some();
            let bookmarked = self.selected_is_bookmarked();
            let mark = if bookmarked {
                ICON_STAR
                    .rich_text()
                    .size(18.0)
                    .color(Color32::from_rgb(245, 158, 11))
            } else {
                ICON_STAR.rich_text().size(18.0)
            };
            let tooltip = if bookmarked {
                tr("bookmarks.remove_current")
            } else {
                tr("bookmarks.add_current")
            };
            if ui
                .add_enabled(selected, egui::Button::new(mark))
                .on_hover_text(tooltip)
                .clicked()
            {
                self.toggle_selected_bookmark();
            }
            ui.separator();
            tabs::buttons(ui, &mut self.tab);
        });
    }

    pub(super) fn recent_menu(&mut self, ui: &mut Ui, frame: &mut Frame) {
        let mut chosen = None;
        ui.menu_button(tr("menu.recent"), |ui| {
            for path in self.recent.clone().iter().take(welcome::RECENT_CAP) {
                let name = welcome::display_name(path);
                if ui
                    .button(name)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    chosen = Some(path.clone());
                    ui.close();
                }
            }
        });
        if let Some(path) = chosen {
            self.open_tree_at(path, frame);
        }
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| match self.app.status() {
            UiStatus::Idle => {
                ui.label(tr("status.ready"));
            }
            UiStatus::Running(job) => {
                ui.spinner();
                ui.label(running_text(*job));
            }
            UiStatus::Success(message) => {
                ui.colored_label(Color32::from_rgb(34, 197, 94), status_message_text(message));
            }
            UiStatus::Job(outcome) => {
                let color = if outcome.is_error() {
                    Color32::from_rgb(239, 68, 68)
                } else {
                    Color32::from_rgb(34, 197, 94)
                };
                ui.colored_label(color, job_outcome_text(outcome));
            }
            UiStatus::Cancelled => {
                ui.colored_label(Color32::from_gray(150), tr("status.cancelled"));
            }
            UiStatus::Error(message) => {
                ui.colored_label(Color32::from_rgb(239, 68, 68), message);
            }
        });
    }

    pub(super) fn welcome(&mut self, ui: &mut Ui, frame: &mut Frame) {
        let Some(action) = welcome::show(ui, &self.recent) else {
            return;
        };
        match action {
            WelcomeAction::CreateNew => self.create_new_tree(frame),
            WelcomeAction::OpenTree => self.open_tree(frame),
            WelcomeAction::ImportGedcom => self.app.begin_import_new(),
            WelcomeAction::RestoreBackup => self.app.begin_restore(),
            WelcomeAction::OpenRecent(path) => self.open_tree_at(path, frame),
        }
    }
}

fn status_message_text(message: &StatusMessage) -> String {
    match message.plural {
        Some(count) => trn_args(message.key, count, &message.args),
        None => tr_args(message.key, &message.args),
    }
}

fn running_text(job: RunningJob) -> String {
    tr(match job {
        RunningJob::ChoosingImportFile => "running.choosing_import_file",
        RunningJob::ChoosingExportDestination => "running.choosing_export_destination",
        RunningJob::ChoosingBackupDestination => "running.choosing_backup_destination",
        RunningJob::ChoosingRestoreSource => "running.choosing_restore_source",
        RunningJob::RunningMaintenance => "running.running_maintenance",
        RunningJob::ChoosingReportDestination => "running.choosing_report_destination",
        RunningJob::ChoosingSvgDestination => "running.choosing_svg_destination",
        RunningJob::ChoosingWebsiteDestination => "running.choosing_website_destination",
        RunningJob::ChoosingShareBundleDestination => "running.choosing_share_bundle_destination",
        RunningJob::Geocoding => "running.geocoding",
    })
}

fn report_status_key(kind: ReportKind) -> &'static str {
    match kind {
        ReportKind::ResearchLog => "status.exported_tasks",
        ReportKind::GroupRoster => "status.exported_members",
        ReportKind::PhotoTree
        | ReportKind::IndividualList
        | ReportKind::FanChart
        | ReportKind::HourglassChart
        | ReportKind::WallChart => "status.exported_people",
        ReportKind::Scrapbook => "status.exported_albums",
        ReportKind::PedigreeChart | ReportKind::AhnentafelList => "status.exported_ancestors",
        ReportKind::FamilyGroupSheet => "status.exported_children",
        ReportKind::IndividualSummary => "status.exported_facts",
        ReportKind::DescendantList => "status.exported_descendants",
        ReportKind::KinshipReport => "status.exported_kinship",
        ReportKind::RelationshipReport => "status.exported_steps",
        ReportKind::CalendarReports | ReportKind::TimelineReport => "status.exported_events",
        ReportKind::AdministrativeLists | ReportKind::CleanupReports => "status.exported_rows",
        ReportKind::FamilyHistoryBook => "status.exported_chapters",
    }
}

fn job_outcome_text(outcome: &JobOutcome) -> String {
    match outcome {
        JobOutcome::Imported {
            people,
            families,
            events,
            citations,
            participants,
            warnings,
        } => tr_args(
            "status.imported",
            &[
                ("people", people.to_string()),
                ("families", families.to_string()),
                ("events", events.to_string()),
                ("citations", citations.to_string()),
                ("participants", participants.to_string()),
                ("warnings", warnings.to_string()),
            ],
        ),
        JobOutcome::ExportedGedcom { path, bytes } => tr_args(
            "status.exported_gedcom",
            &[
                ("bytes", bytes.to_string()),
                ("path", path.display().to_string()),
            ],
        ),
        JobOutcome::BackedUp {
            path,
            media_included,
            media_missing,
        } => {
            let path = path.display().to_string();
            let media = media_included.to_string();
            match media_missing {
                0 => tr_args("status.backed_up", &[("media", media), ("path", path)]),
                1 => tr_args(
                    "status.backed_up_missing_one",
                    &[("media", media), ("path", path)],
                ),
                count => tr_args(
                    "status.backed_up_missing_other",
                    &[
                        ("media", media),
                        ("path", path),
                        ("missing", count.to_string()),
                    ],
                ),
            }
        }
        JobOutcome::Restored {
            path,
            media_missing,
        } => {
            let path = path.display().to_string();
            match media_missing {
                0 => tr_args("status.restored", &[("path", path)]),
                1 => tr_args("status.restored_missing_one", &[("path", path)]),
                count => tr_args(
                    "status.restored_missing_other",
                    &[("path", path), ("missing", count.to_string())],
                ),
            }
        }
        JobOutcome::Maintenance {
            kind,
            healthy,
            issues,
            foreign_keys,
            indexes_rebuilt,
            size_before,
            size_after,
        } => match kind {
            MaintenanceKind::Integrity if !healthy => tr_args(
                "status.integrity_failed",
                &[
                    ("issues", issues.to_string()),
                    ("foreign_keys", foreign_keys.to_string()),
                ],
            ),
            MaintenanceKind::Integrity => tr("status.integrity_passed"),
            MaintenanceKind::RebuildIndexes => tr_args(
                "status.indexes_rebuilt",
                &[("count", indexes_rebuilt.to_string())],
            ),
            MaintenanceKind::Compress => tr_args(
                "status.compressed",
                &[
                    ("before", size_before.to_string()),
                    ("after", size_after.to_string()),
                ],
            ),
        },
        JobOutcome::ReportExported {
            kind,
            path,
            bytes,
            count,
        } => tr_args(
            report_status_key(*kind),
            &[
                ("count", count.to_string()),
                ("bytes", bytes.to_string()),
                ("path", path.display().to_string()),
            ],
        ),
        JobOutcome::SvgExported { path, bytes } => tr_args(
            "status.exported_svg",
            &[
                ("bytes", bytes.to_string()),
                ("path", path.display().to_string()),
            ],
        ),
        JobOutcome::WebsiteExported {
            path,
            bytes,
            pages,
            media,
        } => tr_args(
            "status.exported_website",
            &[
                ("pages", pages.to_string()),
                ("media", media.to_string()),
                ("bytes", bytes.to_string()),
                ("path", path.display().to_string()),
            ],
        ),
        JobOutcome::ShareBundleExported {
            path,
            media,
            total_bytes,
            database_bytes,
        } => tr_args(
            "status.shared_bundle",
            &[
                ("media", media.to_string()),
                ("total_bytes", total_bytes.to_string()),
                ("database_bytes", database_bytes.to_string()),
                ("path", path.display().to_string()),
            ],
        ),
        JobOutcome::Geocoded { found } => {
            tr_args("status.geocoded", &[("count", found.to_string())])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_outcomes_substitute_all_placeholders() {
        let imported = job_outcome_text(&JobOutcome::Imported {
            people: 5,
            families: 2,
            events: 3,
            citations: 1,
            participants: 0,
            warnings: 2,
        });
        assert!(imported.contains('5'));
        assert!(!imported.contains('{'), "{imported}");

        let backed_up = job_outcome_text(&JobOutcome::BackedUp {
            path: "/tmp/tree-backup.zip".into(),
            media_included: 2,
            media_missing: 1,
        });
        assert!(backed_up.contains('2'));
        assert!(backed_up.contains('1'));
        assert!(backed_up.contains("tree-backup.zip"));
        assert!(!backed_up.contains('{'), "{backed_up}");

        let bundle = job_outcome_text(&JobOutcome::ShareBundleExported {
            path: "/tmp/family-share".into(),
            media: 3,
            total_bytes: 4096,
            database_bytes: 1024,
        });
        assert!(bundle.contains('3'));
        assert!(bundle.contains("4096"));
        assert!(bundle.contains("1024"));
        assert!(bundle.contains("family-share"));
        assert!(!bundle.contains('{'), "{bundle}");
    }

    #[test]
    fn status_messages_render_arguments_and_plurals() {
        let added = status_message_text(&StatusMessage {
            key: "status.person_added",
            args: vec![("name", "Ada King".to_string())],
            plural: None,
        });
        assert!(added.contains("Ada King"), "{added}");
        assert!(!added.contains('{'), "{added}");

        let replaced = status_message_text(&StatusMessage {
            key: "status.fields_replaced",
            args: Vec::new(),
            plural: Some(3),
        });
        assert!(replaced.contains('3'), "{replaced}");
        assert!(!replaced.contains('{'), "{replaced}");
    }

    #[test]
    fn every_running_job_has_localized_text() {
        let jobs = [
            RunningJob::ChoosingImportFile,
            RunningJob::ChoosingExportDestination,
            RunningJob::ChoosingBackupDestination,
            RunningJob::ChoosingRestoreSource,
            RunningJob::RunningMaintenance,
            RunningJob::ChoosingReportDestination,
            RunningJob::ChoosingSvgDestination,
            RunningJob::ChoosingWebsiteDestination,
            RunningJob::ChoosingShareBundleDestination,
            RunningJob::Geocoding,
        ];
        for job in jobs {
            let text = running_text(job);
            assert!(!text.is_empty(), "{job:?}");
            assert!(!text.contains('{'), "{job:?}: {text}");
        }
    }
}
