//! Media tab, thumbnail cache and reusable attachment dialog.

use std::collections::HashMap;

use egui::{
    Color32, ColorImage, Context, RichText, TextureHandle, TextureOptions, Ui, Vec2, Window,
};

use crate::db::media::{MediaDraft, MediaLinkSummary, MediaOwner, MediaRecord, MediaSummary};

const THUMB_SIZE: f32 = 64.0;
const MAX_CHOICES: usize = 24;

/// Cached image previews keyed by external file path.
#[derive(Default)]
pub struct ThumbnailCache {
    entries: HashMap<String, Option<TextureHandle>>,
}

impl ThumbnailCache {
    fn thumbnail(&mut self, ctx: &Context, path: &str) -> Option<TextureHandle> {
        if let Some(cached) = self.entries.get(path) {
            return cached.clone();
        }
        let texture = image::open(path).ok().map(|image| {
            let thumbnail = image.thumbnail(128, 128);
            let rgba = thumbnail.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = rgba.into_raw();
            let image = ColorImage::from_rgba_unmultiplied(size, &pixels);
            ctx.load_texture(format!("media-thumb:{path}"), image, TextureOptions::LINEAR)
        });
        self.entries.insert(path.to_string(), texture.clone());
        texture
    }
}

/// A place buffered as text while the media editor is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaEdit {
    pub id: Option<String>,
    pub file_path: String,
    pub format: String,
    pub caption: String,
    pub notes: String,
}

impl MediaEdit {
    pub fn from_record(record: &MediaRecord) -> Self {
        Self {
            id: Some(record.id.clone()),
            file_path: record.file_path.clone(),
            format: record.format.clone().unwrap_or_default(),
            caption: record.caption.clone().unwrap_or_default(),
            notes: record.notes.clone().unwrap_or_default(),
        }
    }

    pub fn to_draft(&self) -> MediaDraft {
        MediaDraft {
            id: self.id.clone(),
            file_path: self.file_path.clone(),
            format: self.format.clone(),
            caption: self.caption.clone(),
            notes: self.notes.clone(),
        }
    }
}

/// What the user asked the shell to do from the Media tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaEvent {
    New,
    Select(String),
    Save(Box<MediaDraft>),
    Delete(String),
    Browse,
    Detach {
        owner_type: String,
        owner_id: String,
    },
    Tasks {
        media_id: String,
        title: String,
    },
}

/// State owned by the window while the Media tab is open.
#[derive(Default)]
pub struct MediaState {
    pub media: Vec<MediaSummary>,
    pub selected: Option<String>,
    pub draft: MediaEdit,
    pub links: Vec<MediaLinkSummary>,
    pub filter: String,
    pub editing: bool,
    pub error: Option<String>,
    pub loaded: bool,
    pub thumbnails: ThumbnailCache,
}

/// Draws the Media tab and returns the action the window should apply.
pub fn show(ui: &mut Ui, state: &mut MediaState) -> Option<MediaEvent> {
    let mut event = None;
    egui::Panel::left("media_list_panel")
        .resizable(true)
        .default_size(360.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .hint_text("Filter by caption, path or format…"),
                );
                if ui
                    .add_enabled(!state.filter.is_empty(), egui::Button::new("Clear"))
                    .clicked()
                {
                    state.filter.clear();
                }
            });
            if ui
                .add(egui::Button::new(RichText::new("+ New Media").strong()))
                .clicked()
            {
                event = Some(MediaEvent::New);
            }
            ui.separator();
            let filter = state.filter.trim().to_lowercase();
            let visible: Vec<MediaSummary> = state
                .media
                .iter()
                .filter(|media| {
                    filter.is_empty()
                        || media
                            .caption
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&filter)
                        || media.file_path.to_lowercase().contains(&filter)
                        || media
                            .format
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&filter)
                })
                .cloned()
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("media_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if visible.is_empty() {
                        ui.label(RichText::new("No media yet.").weak().small());
                    }
                    for media in visible {
                        let selected = state.selected.as_deref() == Some(media.id.as_str());
                        let label = media
                            .caption
                            .clone()
                            .or_else(|| Some(display_path(&media.file_path)))
                            .unwrap_or_else(|| "(no caption)".to_string());
                        ui.horizontal(|ui| {
                            thumbnail(ui, &mut state.thumbnails, &media);
                            let response = ui.selectable_label(selected, label);
                            if response.clicked() {
                                event = Some(MediaEvent::Select(media.id.clone()));
                            }
                            if media.missing {
                                ui.label(
                                    RichText::new("missing")
                                        .color(Color32::from_rgb(239, 68, 68))
                                        .small(),
                                );
                            }
                            ui.label(
                                RichText::new(format!("{} links", media.link_count))
                                    .weak()
                                    .small(),
                            );
                        });
                        ui.separator();
                    }
                });
        });

    egui::CentralPanel::default().show(ui, |ui| {
        if !state.editing {
            ui.label(RichText::new("Select a media record or add a new one.").weak());
            return;
        }
        ui.heading(if state.draft.id.is_some() {
            "Edit media"
        } else {
            "New media"
        });
        if ui.button("Browse…").clicked() {
            event = Some(MediaEvent::Browse);
        }
        egui::Grid::new("media_fields")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(RichText::new("File *").strong());
                ui.text_edit_singleline(&mut state.draft.file_path);
                ui.end_row();

                ui.label("Format:");
                ui.text_edit_singleline(&mut state.draft.format);
                ui.end_row();

                ui.label("Caption:");
                ui.text_edit_singleline(&mut state.draft.caption);
                ui.end_row();

                ui.label("Notes:");
                ui.add(
                    egui::TextEdit::multiline(&mut state.draft.notes)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                ui.end_row();
            });
        if !state.draft.file_path.trim().is_empty()
            && !std::path::Path::new(&state.draft.file_path).is_file()
        {
            ui.label(
                RichText::new("File not found; the path is stored as entered.")
                    .color(Color32::from_rgb(239, 68, 68))
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
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !state.draft.file_path.trim().is_empty(),
                    egui::Button::new(RichText::new("Save").strong()),
                )
                .clicked()
            {
                event = Some(MediaEvent::Save(Box::new(state.draft.to_draft())));
            }
            if let Some(id) = state.draft.id.clone()
                && ui.button("Tasks\u{2026}").clicked()
            {
                let label = if state.draft.caption.trim().is_empty() {
                    state.draft.file_path.trim()
                } else {
                    state.draft.caption.trim()
                };
                event = Some(MediaEvent::Tasks {
                    media_id: id,
                    title: format!("Tasks \u{2014} {label}"),
                });
            }
            if let Some(id) = state.draft.id.clone()
                && ui.button("Delete").clicked()
            {
                event = Some(MediaEvent::Delete(id));
            }
        });
        ui.separator();
        ui.label(RichText::new("Linked to").strong());
        if state.links.is_empty() {
            ui.label(RichText::new("(not attached)").weak().small());
        }
        for link in &state.links {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}:", link.owner_type))
                        .weak()
                        .small(),
                );
                ui.label(&link.label);
                if ui.small_button("Detach").clicked() {
                    event = Some(MediaEvent::Detach {
                        owner_type: link.owner_type.clone(),
                        owner_id: link.owner_id.clone(),
                    });
                }
            });
        }
    });
    event
}

/// State owned by the reusable attach dialog.
pub struct MediaAttachState {
    pub owner: MediaOwner,
    pub title: String,
    pub attached: Vec<MediaSummary>,
    pub choices: Vec<MediaSummary>,
    pub search: String,
    pub error: Option<String>,
    pub thumbnails: ThumbnailCache,
}

/// What the media attachment dialog asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaAttachOutcome {
    Attach(String),
    Detach(String),
    AddFromFile,
    Close,
}

impl MediaAttachState {
    pub fn new(owner: MediaOwner, title: String) -> Self {
        Self {
            owner,
            title,
            attached: Vec::new(),
            choices: Vec::new(),
            search: String::new(),
            error: None,
            thumbnails: ThumbnailCache::default(),
        }
    }
}

/// Draws the attachment dialog; `None` keeps it open.
pub fn show_attach(ctx: &Context, state: &mut MediaAttachState) -> Option<MediaAttachOutcome> {
    let mut outcome = None;
    let mut open = true;
    Window::new(&state.title)
        .id(egui::Id::new("media_attach_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(460.0)
        .open(&mut open)
        .show(ctx, |ui| {
            if ui.button("Add from file…").clicked() {
                outcome = Some(MediaAttachOutcome::AddFromFile);
            }
            if state.attached.is_empty() {
                ui.label(RichText::new("No media attached.").weak().small());
            }
            for media in &state.attached {
                ui.horizontal(|ui| {
                    thumbnail(ui, &mut state.thumbnails, media);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(
                                media
                                    .caption
                                    .clone()
                                    .unwrap_or_else(|| display_path(&media.file_path)),
                            )
                            .strong(),
                        );
                        ui.label(RichText::new(&media.file_path).weak().small());
                        if media.missing {
                            ui.label(
                                RichText::new("file missing")
                                    .color(Color32::from_rgb(239, 68, 68))
                                    .small(),
                            );
                        }
                    });
                    if ui.small_button("Detach").clicked() {
                        outcome = Some(MediaAttachOutcome::Detach(media.id.clone()));
                    }
                });
                ui.separator();
            }

            ui.label(RichText::new("Attach existing").strong());
            ui.add(egui::TextEdit::singleline(&mut state.search).hint_text("Search media…"));
            let needle = state.search.trim().to_lowercase();
            let choices: Vec<MediaSummary> = state
                .choices
                .iter()
                .filter(|media| {
                    needle.is_empty()
                        || media
                            .caption
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&needle)
                        || media.file_path.to_lowercase().contains(&needle)
                })
                .take(MAX_CHOICES)
                .cloned()
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("media_choices")
                .max_height(180.0)
                .show(ui, |ui| {
                    if choices.is_empty() {
                        ui.label(RichText::new("(no matches)").weak().small());
                    }
                    for media in choices {
                        ui.horizontal(|ui| {
                            thumbnail(ui, &mut state.thumbnails, &media);
                            ui.label(
                                media
                                    .caption
                                    .clone()
                                    .unwrap_or_else(|| display_path(&media.file_path)),
                            );
                            if ui.small_button("Attach").clicked() {
                                outcome = Some(MediaAttachOutcome::Attach(media.id.clone()));
                            }
                        });
                    }
                });
            if let Some(error) = &state.error {
                ui.label(
                    RichText::new(error)
                        .color(ui.visuals().error_fg_color)
                        .small(),
                );
            }
        });
    if !open {
        return Some(MediaAttachOutcome::Close);
    }
    outcome
}

fn thumbnail(ui: &mut Ui, cache: &mut ThumbnailCache, media: &MediaSummary) {
    if let Some(texture) = cache.thumbnail(ui.ctx(), &media.file_path) {
        ui.add(
            egui::Image::new(&texture)
                .fit_to_exact_size(Vec2::splat(THUMB_SIZE))
                .corner_radius(4.0),
        );
    } else {
        let label = media
            .format
            .clone()
            .or_else(|| {
                std::path::Path::new(&media.file_path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(str::to_ascii_uppercase)
            })
            .unwrap_or_else(|| "FILE".to_string());
        let _ = ui.add_sized(
            [THUMB_SIZE, THUMB_SIZE],
            egui::Label::new(RichText::new(label).weak().small()),
        );
    }
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
    use crate::db::media::MediaSummary;

    fn state() -> MediaState {
        MediaState {
            media: vec![MediaSummary {
                id: "m1".to_string(),
                file_path: "/definitely/missing/photo.jpg".to_string(),
                format: Some("jpg".to_string()),
                caption: Some("Photo".to_string()),
                link_count: 0,
                missing: true,
            }],
            selected: Some("m1".to_string()),
            draft: MediaEdit {
                id: Some("m1".to_string()),
                file_path: "/definitely/missing/photo.jpg".to_string(),
                format: "jpg".to_string(),
                caption: "Photo".to_string(),
                notes: String::new(),
            },
            editing: true,
            ..MediaState::default()
        }
    }

    #[test]
    fn media_tab_renders_headless() {
        let ctx = Context::default();
        let mut state = state();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(ui, &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the media tab paints");
    }

    #[test]
    fn thumbnails_load_images_and_fall_back_for_missing_files() {
        let path =
            std::env::temp_dir().join(format!("stemma-flow-thumb-{}.png", uuid::Uuid::new_v4()));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .expect("png writes");
        let ctx = Context::default();
        let mut cache = ThumbnailCache::default();
        assert!(
            cache
                .thumbnail(&ctx, path.to_str().expect("utf8 path"))
                .is_some(),
            "valid image decodes"
        );
        assert!(
            cache
                .thumbnail(&ctx, "/definitely/missing/photo.jpg")
                .is_none(),
            "missing files fall back"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn attachment_dialog_renders_headless() {
        let ctx = Context::default();
        let mut state = MediaAttachState::new(
            MediaOwner::Person("p1".to_string()),
            "Media — Ada King".to_string(),
        );
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show_attach(ui.ctx(), &mut state);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the attachment dialog paints");
    }
}
