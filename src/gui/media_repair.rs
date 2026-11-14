//! Modeless broken-media repair dialog.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread;

use egui::{Context, RichText, Window};

use crate::db::media::MediaSummary;
use crate::model::media_repair::{
    AmbiguousMediaMatch, MediaRepairMatch, MediaRepairPlan, find_matches,
};

/// State owned by the media repair dialog.
#[derive(Debug, Default)]
pub struct MediaRepairState {
    pub open: bool,
    pub missing: Vec<MediaSummary>,
    pub filter: String,
    pub preview: Vec<MediaRepairMatch>,
    pub chosen: HashSet<String>,
    pub ambiguous: Vec<AmbiguousMediaMatch>,
    pub unmatched: Vec<String>,
    pub scanning: bool,
    pub scan_error: Option<String>,
    pub error: Option<String>,
    scan: Option<Receiver<Result<MediaRepairPlan, String>>>,
}

impl MediaRepairState {
    pub fn set_missing(&mut self, media: Vec<MediaSummary>) {
        self.open = true;
        self.missing = media.into_iter().filter(|media| media.missing).collect();
        self.filter.clear();
        self.error = None;
        if !self.scanning {
            self.preview.clear();
            self.chosen.clear();
            self.ambiguous.clear();
            self.unmatched.clear();
        }
    }

    pub fn start_scan(&mut self, folder: PathBuf) {
        if self.scanning {
            return;
        }
        let missing = self.missing.clone();
        let (tx, rx) = channel();
        self.scan = Some(rx);
        self.scanning = true;
        self.scan_error = None;
        thread::spawn(move || {
            let _ = tx.send(find_matches(&folder, &missing));
        });
    }

    pub fn poll(&mut self) {
        let Some(rx) = &self.scan else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(plan)) => {
                self.scanning = false;
                self.scan = None;
                self.chosen = plan
                    .matches
                    .iter()
                    .map(|item| item.media_id.clone())
                    .collect();
                self.preview = plan.matches;
                self.ambiguous = plan.ambiguous;
                self.unmatched = plan.unmatched;
                self.scan_error = None;
            }
            Ok(Err(message)) => {
                self.scanning = false;
                self.scan = None;
                self.preview.clear();
                self.chosen.clear();
                self.scan_error = Some(message);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.scanning = false;
                self.scan = None;
                self.scan_error = Some("folder scan failed".to_string());
            }
        }
    }

    pub fn selected_matches(&self) -> Vec<MediaRepairMatch> {
        self.preview
            .iter()
            .filter(|item| self.chosen.contains(&item.media_id))
            .cloned()
            .collect()
    }
}

/// What the repair dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaRepairEvent {
    Locate(String),
    Edit(String),
    SearchFolder,
    Apply(Vec<MediaRepairMatch>),
    Refresh,
}

/// Draws the repair dialog.
pub fn show(ctx: &Context, state: &mut MediaRepairState) -> Option<MediaRepairEvent> {
    state.poll();
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new("Repair missing media")
        .id(egui::Id::unique("media_repair_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(780.0)
        .open(&mut open)
        .show(ctx, |ui| {
            if state.missing.is_empty() {
                ui.label(RichText::new("All media files are present.").weak());
                if ui.button("Close").clicked() {
                    state.open = false;
                }
                return;
            }

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} media files are missing.", state.missing.len()))
                        .strong(),
                );
                if ui.button("Refresh").clicked() {
                    event = Some(MediaRepairEvent::Refresh);
                }
                if ui
                    .add_enabled(
                        !state.scanning,
                        egui::Button::new(RichText::new("Search folder\u{2026}").strong()),
                    )
                    .clicked()
                {
                    event = Some(MediaRepairEvent::SearchFolder);
                }
                if state.scanning {
                    ui.spinner();
                    ui.label(RichText::new("Scanning\u{2026}").weak());
                }
            });
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .hint_text("Filter by caption or path\u{2026}"),
                );
            });
            ui.separator();

            let needle = state.filter.trim().to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("media_repair_missing")
                .max_height(300.0)
                .show(ui, |ui| {
                    for media in &state.missing {
                        let caption = media
                            .caption
                            .clone()
                            .unwrap_or_else(|| display_path(&media.file_path));
                        if !needle.is_empty()
                            && !caption.to_lowercase().contains(&needle)
                            && !media.file_path.to_lowercase().contains(&needle)
                        {
                            continue;
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("MISSING")
                                    .color(ui.visuals().error_fg_color)
                                    .small()
                                    .strong(),
                            );
                            ui.label(RichText::new(caption).strong());
                            ui.label(RichText::new(&media.file_path).weak().small());
                            ui.label(
                                RichText::new(format!("{} links", media.link_count))
                                    .weak()
                                    .small(),
                            );
                            if ui.small_button("Locate\u{2026}").clicked() {
                                event = Some(MediaRepairEvent::Locate(media.id.clone()));
                            }
                            if ui.small_button("Edit").clicked() {
                                event = Some(MediaRepairEvent::Edit(media.id.clone()));
                            }
                        });
                        ui.label(
                            RichText::new(format!("Resolved: {}", media.resolved_path.display()))
                                .weak()
                                .small(),
                        );
                        ui.separator();
                    }
                });

            if let Some(error) = &state.scan_error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }

            if !state.preview.is_empty() {
                ui.separator();
                ui.label(RichText::new("Folder matches").strong());
                let mut chosen = state.chosen.clone();
                egui::ScrollArea::vertical()
                    .id_salt("media_repair_preview")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for item in &state.preview {
                            let mut checked = chosen.contains(&item.media_id);
                            ui.horizontal_wrapped(|ui| {
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        chosen.insert(item.media_id.clone());
                                    } else {
                                        chosen.remove(&item.media_id);
                                    }
                                }
                                ui.label(RichText::new(&item.old_path).weak());
                                ui.label(crate::gui::symbols::ARROW_RIGHT);
                                ui.label(RichText::new(&item.new_path).strong());
                            });
                        }
                    });
                state.chosen = chosen;
                let selected = state.selected_matches();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !selected.is_empty(),
                            egui::Button::new(RichText::new("Apply selected").strong()),
                        )
                        .clicked()
                    {
                        event = Some(MediaRepairEvent::Apply(selected));
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} unique, {} ambiguous, {} not found",
                            state.preview.len(),
                            state.ambiguous.len(),
                            state.unmatched.len()
                        ))
                        .weak()
                        .small(),
                    );
                });
            }
        });
    if !open {
        state.open = false;
    }
    event
}

fn display_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn media() -> MediaSummary {
        MediaSummary {
            id: "m1".to_string(),
            file_path: "photos/photo.jpg".to_string(),
            resolved_path: PathBuf::from("/tmp/photos/photo.jpg"),
            format: Some("jpg".to_string()),
            caption: Some("Photo".to_string()),
            link_count: 1,
            missing: true,
        }
    }

    #[test]
    fn only_missing_media_is_listed() {
        let mut present = media();
        present.id = "m2".to_string();
        present.missing = false;
        let mut state = MediaRepairState::default();
        state.set_missing(vec![media(), present]);
        assert!(state.open);
        assert_eq!(state.missing.len(), 1);
        assert_eq!(state.missing[0].id, "m1");
    }

    #[test]
    fn preview_selection_feeds_apply() {
        let mut state = MediaRepairState {
            preview: vec![MediaRepairMatch {
                media_id: "m1".to_string(),
                old_path: "old/photo.jpg".to_string(),
                new_path: "/new/photo.jpg".to_string(),
            }],
            ..MediaRepairState::default()
        };
        state.chosen.insert("m1".to_string());
        assert_eq!(state.selected_matches().len(), 1);
        state.chosen.clear();
        assert!(state.selected_matches().is_empty());
    }

    #[test]
    fn window_renders_headless() {
        let ctx = Context::default();
        let mut state = MediaRepairState::default();
        state.set_missing(vec![media()]);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
