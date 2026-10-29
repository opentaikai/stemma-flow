//! Menu bar, status bar and welcome dashboard.

use crate::app::UiStatus;
use crate::gui::welcome;
use crate::gui::welcome::WelcomeAction;
use crate::i18n::{tr, tr_args};
use eframe::Frame;
use egui::Color32;
use egui::Ui;

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
                if ui.button(tr("tools.calculator")).clicked() {
                    ui.close();
                    self.open_relationship_calculator();
                }
                if self.app.active_db().is_some() && ui.button(tr("tools.merge_people")).clicked() {
                    ui.close();
                    self.open_merge_dialog(self.app.selected_person_id().map(str::to_string));
                }
            });
            if let Some(path) = self.app.active_db() {
                let label = tr_args("menu.active_tree", &[("name", welcome::display_name(path))]);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(label).weak());
                });
            }
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
            UiStatus::Running(text) => {
                ui.spinner();
                ui.label(*text);
            }
            UiStatus::Success(message) => {
                ui.colored_label(Color32::from_rgb(34, 197, 94), message);
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
