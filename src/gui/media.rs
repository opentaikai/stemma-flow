//! Media tab, thumbnail cache and reusable attachment dialog.

use std::collections::HashMap;
use std::path::Path;

use egui::{
    Color32, ColorImage, Context, RichText, TextureHandle, TextureOptions, Ui, Vec2, Window,
};

use crate::db::Person;
use crate::db::albums::{AlbumDraft, AlbumSummary};
use crate::db::media::{MediaDraft, MediaLinkSummary, MediaOwner, MediaRecord, MediaSummary};
use crate::gui::inspector::person_label;

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

/// An album buffered for editing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlbumEdit {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub person_id: Option<String>,
}

impl AlbumEdit {
    pub fn from_summary(album: &AlbumSummary) -> Self {
        Self {
            id: Some(album.id.clone()),
            name: album.name.clone(),
            description: album.description.clone().unwrap_or_default(),
            person_id: album.person_id.clone(),
        }
    }

    pub fn to_draft(&self) -> AlbumDraft {
        AlbumDraft {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            person_id: self.person_id.clone(),
        }
    }
}

/// Payload used while dragging media rows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MediaDrag {
    Media(String),
    AlbumEntry { album_id: String, media_id: String },
}

/// What the user asked the shell to do from the Media tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaEvent {
    New,
    Select(String),
    Save(Box<MediaDraft>),
    Delete(String),
    Browse,
    RepairMissing,
    Detach {
        owner_type: String,
        owner_id: String,
    },
    Tasks {
        media_id: String,
        title: String,
    },
    SelectAlbum(Option<String>),
    NewAlbum,
    EditAlbum(String),
    SaveAlbum(Box<AlbumDraft>),
    DeleteAlbum(String),
    AddToAlbum {
        album_id: String,
        media_id: String,
    },
    RemoveFromAlbum {
        album_id: String,
        media_id: String,
    },
    MoveInAlbum {
        album_id: String,
        media_id: String,
        before_id: Option<String>,
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
    pub albums: Vec<AlbumSummary>,
    pub albums_loaded: bool,
    pub selected_album: Option<String>,
    pub album_media: Vec<MediaSummary>,
    pub album_edit: AlbumEdit,
    pub album_form: bool,
    pub album_person_search: String,
    pub album_confirm_delete: bool,
    pub album_error: Option<String>,
    pub media_albums: Vec<AlbumSummary>,
}

/// Draws the Media tab and returns the action the window should apply.
pub fn show(
    ui: &mut Ui,
    state: &mut MediaState,
    people: &[Person],
    surname_first: bool,
    db_path: Option<&Path>,
) -> Option<MediaEvent> {
    let mut event = None;
    egui::Panel::left("media_list_panel")
        .resizable(true)
        .default_size(360.0)
        .show(ui, |ui| {
            albums_section(ui, state, people, surname_first, &mut event);
            ui.separator();
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
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(RichText::new("+ New Media").strong()))
                    .clicked()
                {
                    event = Some(MediaEvent::New);
                }
                if ui.button("Repair missing\u{2026}").clicked() {
                    event = Some(MediaEvent::RepairMissing);
                }
            });
            ui.separator();
            let filter = state.filter.trim().to_lowercase();
            let source = if state.selected_album.is_some() {
                &state.album_media
            } else {
                &state.media
            };
            let visible: Vec<MediaSummary> = source
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
                        let label = if state.selected_album.is_some() {
                            "No media in this album."
                        } else {
                            "No media yet."
                        };
                        ui.label(RichText::new(label).weak().small());
                    }
                    let album_id = state.selected_album.clone();
                    for media in visible {
                        let selected = state.selected.as_deref() == Some(media.id.as_str());
                        let label = media
                            .caption
                            .clone()
                            .or_else(|| Some(display_path(&media.file_path)))
                            .unwrap_or_else(|| "(no caption)".to_string());
                        let payload = match &album_id {
                            Some(album_id) => MediaDrag::AlbumEntry {
                                album_id: album_id.clone(),
                                media_id: media.id.clone(),
                            },
                            None => MediaDrag::Media(media.id.clone()),
                        };
                        let mut clicked = false;
                        let mut remove = false;
                        let response = ui
                            .dnd_drag_source(
                                egui::Id::unique(("media_drag", media.id.as_str())),
                                payload,
                                |ui| {
                                    ui.horizontal(|ui| {
                                        thumbnail(ui, &mut state.thumbnails, &media);
                                        clicked = ui.selectable_label(selected, label).clicked();
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
                                        if album_id.is_some() && ui.small_button("Remove").clicked()
                                        {
                                            remove = true;
                                        }
                                    });
                                },
                            )
                            .response;
                        if clicked {
                            event = Some(MediaEvent::Select(media.id.clone()));
                        }
                        if remove && let Some(album_id) = &album_id {
                            event = Some(MediaEvent::RemoveFromAlbum {
                                album_id: album_id.clone(),
                                media_id: media.id.clone(),
                            });
                        }
                        if let Some(dropped) = response.dnd_release_payload::<MediaDrag>() {
                            match &*dropped {
                                MediaDrag::AlbumEntry {
                                    album_id: source,
                                    media_id,
                                } if album_id.as_deref() == Some(source.as_str())
                                    && media_id != &media.id =>
                                {
                                    event = Some(MediaEvent::MoveInAlbum {
                                        album_id: source.clone(),
                                        media_id: media_id.clone(),
                                        before_id: Some(media.id.clone()),
                                    });
                                }
                                MediaDrag::AlbumEntry { media_id, .. } if album_id.is_some() => {
                                    event = Some(MediaEvent::AddToAlbum {
                                        album_id: album_id.clone().expect("album"),
                                        media_id: media_id.clone(),
                                    });
                                }
                                MediaDrag::Media(media_id) if album_id.is_some() => {
                                    event = Some(MediaEvent::AddToAlbum {
                                        album_id: album_id.clone().expect("album"),
                                        media_id: media_id.clone(),
                                    });
                                }
                                _ => {}
                            }
                        }
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
        if ui.button("Browse\u{2026}").clicked() {
            event = Some(MediaEvent::Browse);
        }
        let missing_path = !state.draft.file_path.trim().is_empty()
            && !crate::media_path::resolve(db_path, &state.draft.file_path).is_file();
        if missing_path && ui.button("Locate\u{2026}").clicked() {
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
            && !crate::media_path::resolve(db_path, &state.draft.file_path).is_file()
        {
            ui.label(
                RichText::new("File not found; the path is stored as entered.")
                    .color(Color32::from_rgb(239, 68, 68))
                    .small(),
            );
        }
        if !state.draft.file_path.trim().is_empty()
            && Path::new(state.draft.file_path.trim()).is_relative()
        {
            ui.label(
                RichText::new("Stored relative to the tree folder.")
                    .weak()
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

        ui.separator();
        ui.label(RichText::new("Albums").strong());
        if state.media_albums.is_empty() {
            ui.label(RichText::new("(not in an album)").weak().small());
        }
        for album in &state.media_albums {
            ui.horizontal(|ui| {
                ui.label(&album.name);
                if ui.small_button("Remove").clicked()
                    && let Some(media_id) = &state.selected
                {
                    event = Some(MediaEvent::RemoveFromAlbum {
                        album_id: album.id.clone(),
                        media_id: media_id.clone(),
                    });
                }
            });
        }
        if let Some(media_id) = state.selected.clone() {
            let available: Vec<AlbumSummary> = state
                .albums
                .iter()
                .filter(|album| {
                    !state
                        .media_albums
                        .iter()
                        .any(|member| member.id == album.id)
                })
                .cloned()
                .collect();
            if !available.is_empty() {
                egui::ComboBox::from_id_salt("media_add_album")
                    .selected_text("Add to album\u{2026}")
                    .show_ui(ui, |ui| {
                        for album in available {
                            if ui.selectable_label(false, &album.name).clicked() {
                                event = Some(MediaEvent::AddToAlbum {
                                    album_id: album.id,
                                    media_id: media_id.clone(),
                                });
                            }
                        }
                    });
            }
        }
    });
    event
}

fn albums_section(
    ui: &mut Ui,
    state: &mut MediaState,
    people: &[Person],
    surname_first: bool,
    event: &mut Option<MediaEvent>,
) {
    ui.label(RichText::new("Albums").strong());
    if ui.button("+ New album").clicked() {
        *event = Some(MediaEvent::NewAlbum);
    }
    if ui
        .selectable_label(state.selected_album.is_none(), "All media")
        .clicked()
        && state.selected_album.is_some()
    {
        state.selected_album = None;
        *event = Some(MediaEvent::SelectAlbum(None));
    }
    let albums = state.albums.clone();
    egui::ScrollArea::vertical()
        .id_salt("media_album_list")
        .max_height(180.0)
        .show(ui, |ui| {
            for album in &albums {
                let selected = state.selected_album.as_deref() == Some(album.id.as_str());
                let label = match &album.person_name {
                    Some(person) => format!("{} \u{2014} {}", album.name, person),
                    None => album.name.clone(),
                };
                let mut clicked = false;
                let (_, dropped) = ui.dnd_drop_zone::<MediaDrag, _>(
                    egui::Frame::default().inner_margin(2.0),
                    |ui| {
                        ui.horizontal(|ui| {
                            clicked = ui.selectable_label(selected, label).clicked();
                            ui.label(RichText::new(album.media_count.to_string()).weak().small());
                        });
                    },
                );
                if clicked {
                    state.selected_album = Some(album.id.clone());
                    *event = Some(MediaEvent::SelectAlbum(Some(album.id.clone())));
                }
                if let Some(payload) = dropped {
                    match &*payload {
                        MediaDrag::Media(media_id) => {
                            *event = Some(MediaEvent::AddToAlbum {
                                album_id: album.id.clone(),
                                media_id: media_id.clone(),
                            });
                        }
                        MediaDrag::AlbumEntry { album_id, media_id } => {
                            if album_id == &album.id {
                                *event = Some(MediaEvent::MoveInAlbum {
                                    album_id: album.id.clone(),
                                    media_id: media_id.clone(),
                                    before_id: None,
                                });
                            } else {
                                *event = Some(MediaEvent::AddToAlbum {
                                    album_id: album.id.clone(),
                                    media_id: media_id.clone(),
                                });
                            }
                        }
                    }
                }
            }
        });
    if state.album_form {
        album_form(ui, state, people, surname_first, event);
    } else if let Some(error) = &state.album_error {
        ui.label(
            RichText::new(error)
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
}

fn album_form(
    ui: &mut Ui,
    state: &mut MediaState,
    people: &[Person],
    surname_first: bool,
    event: &mut Option<MediaEvent>,
) {
    ui.separator();
    ui.label(
        RichText::new(if state.album_edit.id.is_some() {
            "Edit album"
        } else {
            "New album"
        })
        .strong(),
    );
    egui::Grid::new("album_fields")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Name *").strong());
            ui.text_edit_singleline(&mut state.album_edit.name);
            ui.end_row();

            ui.label("Description:");
            ui.add(
                egui::TextEdit::multiline(&mut state.album_edit.description)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.end_row();
        });

    let selected_name = state
        .album_edit
        .person_id
        .as_deref()
        .and_then(|id| people.iter().find(|person| person.id == id))
        .map(|person| person_label(person, surname_first));
    ui.horizontal(|ui| {
        ui.label("Person:");
        if let Some(name) = &selected_name {
            ui.label(name);
            if ui.small_button("Clear").clicked() {
                state.album_edit.person_id = None;
            }
        } else {
            ui.label(RichText::new("General album").weak().small());
        }
    });

    ui.add(
        egui::TextEdit::singleline(&mut state.album_person_search)
            .hint_text("Search people\u{2026}"),
    );
    let needle = state.album_person_search.trim().to_lowercase();
    if !needle.is_empty() {
        egui::ScrollArea::vertical()
            .id_salt("album_person_choices")
            .max_height(120.0)
            .show(ui, |ui| {
                for person in people
                    .iter()
                    .filter(|person| {
                        person_label(person, surname_first)
                            .to_lowercase()
                            .contains(&needle)
                    })
                    .take(8)
                {
                    if ui
                        .selectable_label(false, person_label(person, surname_first))
                        .clicked()
                    {
                        state.album_edit.person_id = Some(person.id.clone());
                        state.album_person_search.clear();
                    }
                }
            });
    }

    if let Some(error) = &state.album_error {
        ui.label(
            RichText::new(error)
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !state.album_edit.name.trim().is_empty(),
                egui::Button::new(RichText::new("Save").strong()),
            )
            .clicked()
        {
            *event = Some(MediaEvent::SaveAlbum(Box::new(state.album_edit.to_draft())));
        }
        if state.album_edit.id.is_some() {
            if state.album_confirm_delete {
                if ui.button("Delete album").clicked()
                    && let Some(id) = &state.album_edit.id
                {
                    *event = Some(MediaEvent::DeleteAlbum(id.clone()));
                }
                if ui.button("Cancel delete").clicked() {
                    state.album_confirm_delete = false;
                }
            } else if ui.button("Delete").clicked() {
                state.album_confirm_delete = true;
            }
        }
        if ui.button("Cancel").clicked() {
            state.album_form = false;
            state.album_confirm_delete = false;
            state.album_error = None;
        }
    });
    if state.album_confirm_delete {
        ui.label(
            RichText::new("Delete this album? Media records stay in the library.")
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
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
        .id(egui::Id::unique("media_attach_window"))
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
    if let Some(texture) = cache.thumbnail(ui.ctx(), &media.resolved_path.to_string_lossy()) {
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
                resolved_path: std::path::PathBuf::from("/definitely/missing/photo.jpg"),
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
            show(ui, &mut state, &[], false, None);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "the media tab paints");
    }

    #[test]
    fn media_albums_render_headless() {
        let ctx = Context::default();
        let mut state = state();
        state.albums = vec![AlbumSummary {
            id: "a1".to_string(),
            name: "Trip".to_string(),
            description: None,
            person_id: None,
            person_name: None,
            media_count: 1,
        }];
        state.selected_album = Some("a1".to_string());
        state.album_media = state.media.clone();
        state.album_form = true;
        state.album_edit = AlbumEdit {
            id: Some("a1".to_string()),
            name: "Trip".to_string(),
            ..AlbumEdit::default()
        };
        state.media_albums = state.albums.clone();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(ui, &mut state, &[], false, None);
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "albums and their form paint");
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
