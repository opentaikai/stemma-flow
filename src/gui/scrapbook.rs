//! Scrapbook export options.

use crate::i18n::{tr, tr_args};

use std::collections::HashSet;

use egui::{Context, RichText, Window};

use crate::db::albums::AlbumSummary;

/// State owned by the scrapbook dialog.
#[derive(Debug, Default)]
pub struct ScrapbookState {
    pub open: bool,
    pub albums: Vec<AlbumSummary>,
    pub selected: HashSet<String>,
}

impl ScrapbookState {
    pub fn open_with(&mut self, albums: Vec<AlbumSummary>) {
        self.open = true;
        self.selected = albums.iter().map(|album| album.id.clone()).collect();
        self.albums = albums;
    }
}

/// Draws the scrapbook options window, returning selected album ids.
pub fn show(ctx: &Context, state: &mut ScrapbookState) -> Option<Vec<String>> {
    if !state.open {
        return None;
    }
    let mut open = true;
    let mut event = None;
    Window::new(tr("scrapbook.title"))
        .id(egui::Id::unique("scrapbook_window"))
        .collapsible(false)
        .resizable(true)
        .default_width(460.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(RichText::new(tr("scrapbook.description")).weak().small());
            if state.albums.is_empty() {
                ui.label(RichText::new(tr("common.no_albums")).weak());
                if ui.button(tr("common.close")).clicked() {
                    state.open = false;
                }
                return;
            }
            ui.horizontal(|ui| {
                if ui.button(tr("common.select_all")).clicked() {
                    state.selected = state.albums.iter().map(|album| album.id.clone()).collect();
                }
                if ui.button(tr("common.select_none")).clicked() {
                    state.selected.clear();
                }
                ui.label(
                    RichText::new(tr_args(
                        "scrapbook.selected",
                        &[("count", state.selected.len().to_string())],
                    ))
                    .weak()
                    .small(),
                );
            });
            egui::ScrollArea::vertical()
                .id_salt("scrapbook_albums")
                .max_height(280.0)
                .show(ui, |ui| {
                    for album in &state.albums {
                        let mut checked = state.selected.contains(&album.id);
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    state.selected.insert(album.id.clone());
                                } else {
                                    state.selected.remove(&album.id);
                                }
                            }
                            ui.label(RichText::new(&album.name).strong());
                            if let Some(person) = &album.person_name {
                                ui.label(RichText::new(person).weak().small());
                            }
                            ui.label(
                                RichText::new(tr_args(
                                    "scrapbook.photos",
                                    &[("count", album.media_count.to_string())],
                                ))
                                .weak()
                                .small(),
                            );
                        });
                    }
                });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !state.selected.is_empty(),
                        egui::Button::new(RichText::new(tr("common.export")).strong()),
                    )
                    .clicked()
                {
                    state.open = false;
                    event = Some(state.selected.iter().cloned().collect());
                }
                if ui.button(tr("common.cancel")).clicked() {
                    state.open = false;
                }
            });
        });
    if !open {
        state.open = false;
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, RawInput};

    fn album() -> AlbumSummary {
        AlbumSummary {
            id: "a1".to_string(),
            name: "Trip".to_string(),
            description: None,
            person_id: None,
            person_name: None,
            media_count: 2,
        }
    }

    #[test]
    fn selects_all_albums_and_renders() {
        let ctx = Context::default();
        let mut state = ScrapbookState::default();
        state.open_with(vec![album()]);
        assert_eq!(state.selected.len(), 1);
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(show(ui.ctx(), &mut state).is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
