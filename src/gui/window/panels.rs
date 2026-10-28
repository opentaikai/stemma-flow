//! Menu bar, status bar and welcome dashboard.

use crate::app::UiStatus;
use crate::gui::welcome;
use crate::gui::welcome::WelcomeAction;
use eframe::Frame;
use egui::Color32;
use egui::Ui;

use super::TreeWindow;

impl TreeWindow {
    pub(super) fn top_panel(&mut self, ui: &mut Ui, frame: &mut Frame) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Family Tree\u{2026}").clicked() {
                    ui.close();
                    self.create_new_tree(frame);
                }
                if ui.button("Open Tree\u{2026}").clicked() {
                    ui.close();
                    self.open_tree(frame);
                }
                if ui.button("Import GEDCOM\u{2026}").clicked() {
                    ui.close();
                    if self.app.active_db().is_some() {
                        self.app.begin_import();
                    } else {
                        self.app.begin_import_new();
                    }
                }
                let can_export = self.app.active_db().is_some() && !self.app.is_busy();
                if ui
                    .add_enabled(can_export, egui::Button::new("Export GEDCOM\u{2026}"))
                    .clicked()
                {
                    ui.close();
                    self.export_options = crate::gedcom::ExportOptions::default();
                    self.show_export = true;
                }
                if !self.recent.is_empty() {
                    self.recent_menu(ui, frame);
                }
                if self.app.active_db().is_some() {
                    ui.separator();
                    if ui.button("Close Tree").clicked() {
                        ui.close();
                        self.app.close_tree();
                    }
                }
                ui.separator();
                if ui.button("Settings\u{2026}").clicked() {
                    ui.close();
                    self.show_settings = true;
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.close();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            if self.app.active_db().is_some() {
                let mut surname_first = self.tree_settings.surname_first;
                let mut changed = None;
                ui.menu_button("View", |ui| {
                    if ui
                        .checkbox(&mut surname_first, "Surname-first names")
                        .changed()
                    {
                        changed = Some(surname_first);
                    }
                });
                if let Some(value) = changed {
                    self.set_surname_first(value);
                }
            }
            if let Some(path) = self.app.active_db() {
                let label = format!("Active Tree: {}", welcome::display_name(path));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(label).weak());
                });
            }
        });
    }

    pub(super) fn recent_menu(&mut self, ui: &mut Ui, frame: &mut Frame) {
        let mut chosen = None;
        ui.menu_button("Recent", |ui| {
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
                ui.label("Ready");
            }
            UiStatus::Running(text) => {
                ui.spinner();
                ui.label(*text);
            }
            UiStatus::Success(message) => {
                ui.colored_label(Color32::from_rgb(34, 197, 94), message);
            }
            UiStatus::Cancelled => {
                ui.colored_label(Color32::from_gray(150), "Cancelled");
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
            WelcomeAction::OpenRecent(path) => self.open_tree_at(path, frame),
        }
    }
}
