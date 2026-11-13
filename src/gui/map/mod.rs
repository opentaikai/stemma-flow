//! Map tab: geocoded places, group filters and optional OSM tiles.

mod tiles;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use egui::{
    Align2, Color32, FontId, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    TextureHandle, Ui, pos2, vec2,
};

use crate::db::groups::GroupSummary;
use crate::db::places::PlaceSummary;
use crate::gui::labels::color32;
use crate::gui::symbols::color_swatch;
use crate::i18n::{tr, tr_args};
use crate::model::map::{self, MapBounds, TILE_SIZE, TileKey};

const MAX_PENDING_TILES: usize = 16;
const MAX_CACHED_TILES: usize = 256;
const DEFAULT_MARKER: Color32 = Color32::from_rgb(37, 99, 235);

/// What the map tab asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapEvent {
    SelectPlace(String),
    SelectGroup(Option<String>),
    OpenSettings,
    OpenGeocode,
}

enum TileState {
    Ready(TextureHandle),
    Failed(Instant),
}

/// State owned by the map tab.
pub struct MapState {
    pub loaded: bool,
    pub error: Option<String>,
    pub places: Vec<PlaceSummary>,
    pub total_places: usize,
    pub groups: Vec<GroupSummary>,
    pub active_group: Option<String>,
    pub visible_ids: Option<HashSet<String>>,
    pub center_x: f64,
    pub center_y: f64,
    pub zoom: u8,
    pub needs_fit: bool,
    pub selected: Option<String>,
    pub pending_zoom: Option<i8>,
    tiles: HashMap<TileKey, TileState>,
    pending: HashSet<TileKey>,
    loader: Option<tiles::TileLoader>,
    tile_error: Option<String>,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            loaded: false,
            error: None,
            places: Vec::new(),
            total_places: 0,
            groups: Vec::new(),
            active_group: None,
            visible_ids: None,
            center_x: 0.5,
            center_y: 0.5,
            zoom: map::MIN_ZOOM,
            needs_fit: true,
            selected: None,
            pending_zoom: None,
            tiles: HashMap::new(),
            pending: HashSet::new(),
            loader: None,
            tile_error: None,
        }
    }
}

impl MapState {
    pub fn set_data(&mut self, places: Vec<PlaceSummary>, groups: Vec<GroupSummary>) {
        self.total_places = places.len();
        self.places = places
            .into_iter()
            .filter(|place| place.latitude.is_some() && place.longitude.is_some())
            .collect();
        self.groups = groups;
        if self
            .active_group
            .as_ref()
            .is_some_and(|id| !self.groups.iter().any(|group| &group.id == id))
        {
            self.active_group = None;
            self.visible_ids = None;
        }
        self.tiles
            .retain(|_, state| !matches!(state, TileState::Failed(_)));
        self.tile_error = None;
        self.needs_fit = true;
        self.loaded = true;
    }

    fn visible_places(&self) -> impl Iterator<Item = &PlaceSummary> {
        self.places.iter().filter(|place| {
            place.latitude.is_some()
                && place.longitude.is_some()
                && self
                    .visible_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&place.id))
        })
    }

    fn active_color(&self) -> Color32 {
        self.active_group
            .as_ref()
            .and_then(|id| self.groups.iter().find(|group| &group.id == id))
            .and_then(|group| group.color)
            .map(color32)
            .unwrap_or(DEFAULT_MARKER)
    }

    fn fit(&mut self, rect: Rect) {
        let lons: Vec<f64> = self
            .visible_places()
            .filter_map(|place| place.longitude)
            .collect();
        if lons.is_empty() {
            self.center_x = 0.5;
            self.center_y = 0.5;
            self.zoom = map::MIN_ZOOM;
            self.needs_fit = false;
            return;
        }
        let mean_x = map::lon_to_x(map::circular_mean_lon(&lons));
        let points: Vec<(f64, f64)> = self
            .visible_places()
            .filter_map(|place| {
                let x = wrap_x(map::lon_to_x(place.longitude?), mean_x);
                let y = map::lat_to_y(place.latitude?);
                Some((x, y))
            })
            .collect();
        let Some(bounds) = MapBounds::from_points(points) else {
            self.needs_fit = false;
            return;
        };
        self.zoom = map::fit_zoom(bounds, rect.width() as f64, rect.height() as f64);
        self.center_x = bounds.center_x().clamp(0.0, 1.0);
        self.center_y = bounds.center_y().clamp(0.0, 1.0);
        self.needs_fit = false;
    }

    fn zoom_step(&mut self, direction: i8, anchor: Pos2, rect: Rect) {
        let new_zoom = if direction > 0 {
            (self.zoom + 1).min(map::MAX_ZOOM)
        } else {
            self.zoom.saturating_sub(1).max(map::MIN_ZOOM)
        };
        if new_zoom == self.zoom {
            return;
        }
        let world = map::world_size(self.zoom);
        let x = self.center_x + (anchor.x - rect.center().x) as f64 / world;
        let y = self.center_y + (anchor.y - rect.center().y) as f64 / world;
        self.zoom = new_zoom;
        let new_world = map::world_size(new_zoom);
        self.center_x = x - (anchor.x - rect.center().x) as f64 / new_world;
        self.center_y = y - (anchor.y - rect.center().y) as f64 / new_world;
        self.clamp_center();
    }

    fn clamp_center(&mut self) {
        self.center_x = self.center_x.clamp(0.0, 1.0);
        self.center_y = self.center_y.clamp(0.0, 1.0);
    }

    fn world_to_screen(&self, x: f64, y: f64, rect: Rect) -> Pos2 {
        let world = map::world_size(self.zoom);
        let x = wrap_x(x, self.center_x);
        let dx = (x - self.center_x) * world;
        let dy = (y - self.center_y) * world;
        pos2(rect.center().x + dx as f32, rect.center().y + dy as f32)
    }

    fn screen_to_world(&self, pos: Pos2, rect: Rect) -> (f64, f64) {
        let world = map::world_size(self.zoom);
        let x = self.center_x + (pos.x - rect.center().x) as f64 / world;
        let y = self.center_y + (pos.y - rect.center().y) as f64 / world;
        (x, y.clamp(0.0, 1.0))
    }

    fn poll_tiles(&mut self, ctx: &egui::Context) {
        let results = self
            .loader
            .as_ref()
            .map(tiles::TileLoader::poll)
            .unwrap_or_default();
        for result in results {
            self.pending.remove(&result.key);
            match result.image {
                Ok(image) => {
                    let name = format!(
                        "map-tile:{}/{}/{}",
                        result.key.z, result.key.x, result.key.y
                    );
                    let texture = ctx.load_texture(name, image, egui::TextureOptions::LINEAR);
                    self.tiles.insert(result.key, TileState::Ready(texture));
                }
                Err(message) => {
                    self.tiles
                        .insert(result.key, TileState::Failed(Instant::now()));
                    self.tile_error = Some(message);
                }
            }
        }
    }

    fn request_tiles(&mut self, rect: Rect, allow_online: bool) {
        if !allow_online || rect.width() < 1.0 || rect.height() < 1.0 {
            return;
        }
        if self.loader.is_none() {
            self.loader = Some(tiles::TileLoader::spawn());
        }
        let visible = map::visible_tiles(
            self.center_x,
            self.center_y,
            self.zoom,
            rect.width() as f64,
            rect.height() as f64,
        );
        let wanted: HashSet<TileKey> = visible.iter().map(|tile| tile.key).collect();
        for tile in &visible {
            if self.tiles.contains_key(&tile.key) || self.pending.len() >= MAX_PENDING_TILES {
                continue;
            }
            if self.pending.insert(tile.key)
                && let Some(loader) = &self.loader
            {
                loader.request(tile.key);
            }
        }

        let retry: Vec<TileKey> = self
            .tiles
            .iter()
            .filter_map(|(key, state)| match state {
                TileState::Failed(at) if at.elapsed() > Duration::from_secs(30) => Some(*key),
                _ => None,
            })
            .collect();
        for key in retry {
            if self.pending.len() >= MAX_PENDING_TILES {
                break;
            }
            self.tiles.remove(&key);
            if self.pending.insert(key)
                && let Some(loader) = &self.loader
            {
                loader.request(key);
            }
        }

        if self.tiles.len() > MAX_CACHED_TILES {
            self.tiles.retain(|key, _| wanted.contains(key));
        }
    }

    fn draw_graticule(&self, painter: &Painter, rect: Rect, ui: &Ui) {
        let stroke = Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.18));
        for lon in (-180..=180).step_by(30) {
            let screen = self
                .world_to_screen(map::lon_to_x(lon as f64), self.center_y, rect)
                .x;
            if screen >= rect.left() && screen <= rect.right() {
                painter.line_segment(
                    [pos2(screen, rect.top()), pos2(screen, rect.bottom())],
                    stroke,
                );
            }
        }
        for lat in (-75..=75).step_by(15) {
            let screen = self
                .world_to_screen(self.center_x, map::lat_to_y(lat as f64), rect)
                .y;
            if screen >= rect.top() && screen <= rect.bottom() {
                painter.line_segment(
                    [pos2(rect.left(), screen), pos2(rect.right(), screen)],
                    stroke,
                );
            }
        }
    }

    fn draw_tiles(&self, painter: &Painter, rect: Rect) {
        let world = map::world_size(self.zoom);
        let left = self.center_x * world - rect.width() as f64 / 2.0;
        let top = self.center_y * world - rect.height() as f64 / 2.0;
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        for tile in map::visible_tiles(
            self.center_x,
            self.center_y,
            self.zoom,
            rect.width() as f64,
            rect.height() as f64,
        ) {
            let screen = Rect::from_min_size(
                pos2(
                    rect.left() + (tile.x as f64 * TILE_SIZE - left) as f32,
                    rect.top() + (tile.y as f64 * TILE_SIZE - top) as f32,
                ),
                vec2(TILE_SIZE as f32, TILE_SIZE as f32),
            );
            if !screen.intersects(rect) {
                continue;
            }
            if let Some(TileState::Ready(texture)) = self.tiles.get(&tile.key) {
                painter.image(texture.id(), screen, uv, Color32::WHITE);
            }
        }
    }
}

/// Draws the map tab.
pub fn show(ui: &mut Ui, state: &mut MapState, allow_online: bool) -> Option<MapEvent> {
    state.poll_tiles(ui.ctx());
    let mut event = None;
    egui::Panel::left("map_sidebar")
        .resizable(true)
        .default_size(250.0)
        .show(ui, |ui| sidebar(ui, state, allow_online, &mut event));

    let (rect, response) =
        ui.allocate_exact_size(ui.available_size_before_wrap(), Sense::click_and_drag());
    if rect.width() < 4.0 || rect.height() < 4.0 {
        return event;
    }
    if state.needs_fit {
        state.fit(rect);
    }
    if let Some(direction) = state.pending_zoom.take() {
        state.zoom_step(direction, rect.center(), rect);
    }
    handle_navigation(ui, &response, state, rect);
    state.request_tiles(rect, allow_online);

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    state.draw_graticule(&painter, rect, ui);
    if allow_online {
        state.draw_tiles(&painter, rect);
    }
    draw_markers(ui, &painter, state, rect, &response, &mut event);
    draw_pointer_coordinates(ui, &painter, state, rect);
    event
}

fn sidebar(ui: &mut Ui, state: &mut MapState, allow_online: bool, event: &mut Option<MapEvent>) {
    ui.heading(tr("map.title"));
    ui.label(
        RichText::new(tr_args(
            "map.coverage",
            &[
                ("geocoded", state.places.len().to_string()),
                ("total", state.total_places.to_string()),
            ],
        ))
        .weak()
        .small(),
    );
    let missing = state.total_places.saturating_sub(state.places.len());
    if missing > 0 {
        ui.label(
            RichText::new(tr_args("map.missing", &[("count", missing.to_string())]))
                .weak()
                .small(),
        );
        if ui.button(tr("map.geocode")).clicked() {
            *event = Some(MapEvent::OpenGeocode);
        }
    }

    ui.separator();
    ui.label(RichText::new(tr("map.groups")).strong());
    let mut choice = state.active_group.clone();
    let mut changed = false;
    if ui
        .selectable_label(choice.is_none(), tr("map.all_places"))
        .clicked()
        && choice.is_some()
    {
        choice = None;
        changed = true;
    }
    egui::ScrollArea::vertical()
        .id_salt("map_group_list")
        .max_height(240.0)
        .show(ui, |ui| {
            for group in &state.groups {
                ui.horizontal(|ui| {
                    if let Some(color) = group.color {
                        color_swatch(ui, color32(color));
                    } else {
                        ui.add_space(10.0);
                    }
                    let selected = choice.as_deref() == Some(group.id.as_str());
                    if ui.selectable_label(selected, &group.name).clicked() && !selected {
                        choice = Some(group.id.clone());
                        changed = true;
                    }
                    ui.label(RichText::new(group.member_count.to_string()).weak().small());
                });
            }
        });
    if state.groups.is_empty() {
        ui.label(RichText::new(tr("map.no_groups")).weak().small());
    }
    if changed {
        state.active_group = choice.clone();
        state.visible_ids = None;
        state.needs_fit = true;
        *event = Some(MapEvent::SelectGroup(choice));
    }

    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("+").on_hover_text(tr("map.zoom_in")).clicked() {
            state.pending_zoom = Some(1);
        }
        if ui.button("-").on_hover_text(tr("map.zoom_out")).clicked() {
            state.pending_zoom = Some(-1);
        }
        if ui.button(tr("map.fit")).clicked() {
            state.needs_fit = true;
        }
    });

    ui.separator();
    if allow_online {
        ui.hyperlink_to(
            tr("map.attribution"),
            "https://www.openstreetmap.org/copyright",
        );
    } else {
        ui.label(
            RichText::new(tr("map.online_disabled"))
                .color(ui.visuals().warn_fg_color)
                .strong(),
        );
        ui.label(RichText::new(tr("map.enable_hint")).weak().small());
        if ui.button(tr("map.open_settings")).clicked() {
            *event = Some(MapEvent::OpenSettings);
        }
    }
    if let Some(error) = &state.error {
        ui.label(
            RichText::new(error)
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
    if let Some(error) = &state.tile_error {
        ui.label(
            RichText::new(tr_args("map.tiles_error", &[("error", error.clone())]))
                .color(ui.visuals().error_fg_color)
                .small(),
        );
    }
}

fn handle_navigation(ui: &Ui, response: &egui::Response, state: &mut MapState, rect: Rect) {
    if response.dragged() {
        let delta = response.drag_delta();
        let world = map::world_size(state.zoom);
        state.center_x -= delta.x as f64 / world;
        state.center_y -= delta.y as f64 / world;
        state.clamp_center();
    }
    if response.hovered() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll.abs() > 4.0 {
            let anchor = ui
                .input(|input| input.pointer.hover_pos())
                .unwrap_or(rect.center());
            state.zoom_step(if scroll > 0.0 { 1 } else { -1 }, anchor, rect);
        }
    }
}

fn draw_markers(
    ui: &Ui,
    painter: &Painter,
    state: &mut MapState,
    rect: Rect,
    response: &egui::Response,
    event: &mut Option<MapEvent>,
) {
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let selected = state.selected.clone();
    let color = state.active_color();
    let mut markers: Vec<(String, String, i64, Pos2, f32)> = state
        .visible_places()
        .filter_map(|place| {
            let x = map::lon_to_x(place.longitude?);
            let y = map::lat_to_y(place.latitude?);
            let screen = state.world_to_screen(x, y, rect);
            if !rect.expand(16.0).contains(screen) {
                return None;
            }
            Some((
                place.id.clone(),
                place.name.clone(),
                place.event_count,
                screen,
                marker_radius(place.event_count),
            ))
        })
        .collect();
    markers.sort_by(|left, right| left.2.cmp(&right.2).then_with(|| left.1.cmp(&right.1)));

    let hovered = pointer.and_then(|pointer| {
        markers
            .iter()
            .rev()
            .find(|(_, _, _, pos, radius)| pointer.distance(*pos) <= radius + 3.0)
            .map(|(id, _, _, _, _)| id.clone())
    });

    for (id, _, _, pos, radius) in &markers {
        let is_selected = selected.as_deref() == Some(id.as_str());
        let is_hovered = hovered.as_deref() == Some(id.as_str());
        painter.circle_filled(*pos, *radius, color);
        let stroke = if is_selected {
            Stroke::new(3.0, Color32::WHITE)
        } else if is_hovered {
            Stroke::new(2.0, Color32::WHITE)
        } else {
            Stroke::new(1.2, Color32::from_black_alpha(110))
        };
        painter.circle_stroke(*pos, *radius, stroke);
    }

    if let Some(id) = &hovered
        && let Some((_, name, count, pos, _)) = markers
            .iter()
            .find(|(marker_id, _, _, _, _)| marker_id == id)
    {
        draw_hover_label(ui, painter, rect, *pos, name, *count);
    }

    if response.clicked()
        && let Some(pointer) = pointer
        && let Some((id, _, _, _, _)) = markers
            .iter()
            .rev()
            .find(|(_, _, _, pos, radius)| pointer.distance(*pos) <= radius + 3.0)
    {
        state.selected = Some(id.clone());
        *event = Some(MapEvent::SelectPlace(id.clone()));
    }

    if markers.is_empty() {
        let message = if state.active_group.is_some() {
            tr("map.no_group_places")
        } else {
            tr("map.empty")
        };
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            message,
            FontId::proportional(14.0),
            ui.visuals().weak_text_color(),
        );
    }
}

fn draw_hover_label(ui: &Ui, painter: &Painter, rect: Rect, pos: Pos2, name: &str, count: i64) {
    let text = format!(
        "{name}  ·  {}",
        tr_args("map.events", &[("count", count.to_string())])
    );
    let galley = painter.layout_no_wrap(
        text,
        FontId::proportional(12.0),
        ui.visuals().strong_text_color(),
    );
    let size = galley.size();
    let mut origin = pos + vec2(10.0, -size.y - 8.0);
    if origin.x + size.x + 10.0 > rect.right() {
        origin.x = pos.x - size.x - 12.0;
    }
    if origin.y < rect.top() + 4.0 {
        origin.y = pos.y + 10.0;
    }
    let background = Rect::from_min_size(origin - vec2(5.0, 3.0), size + vec2(10.0, 6.0));
    painter.rect_filled(background, 4.0, ui.visuals().window_fill);
    painter.rect_stroke(
        background,
        4.0,
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        StrokeKind::Inside,
    );
    painter.galley(origin, galley, ui.visuals().strong_text_color());
}

fn draw_pointer_coordinates(ui: &Ui, painter: &Painter, state: &MapState, rect: Rect) {
    let Some(pointer) = ui.input(|input| input.pointer.hover_pos()) else {
        return;
    };
    if !rect.contains(pointer) {
        return;
    }
    let (x, y) = state.screen_to_world(pointer, rect);
    let lon = map::x_to_lon(x.rem_euclid(1.0));
    let lat = map::y_to_lat(y.clamp(0.0, 1.0));
    painter.text(
        rect.left_bottom() + vec2(8.0, -8.0),
        Align2::LEFT_BOTTOM,
        format!("{lat:.4}, {lon:.4}"),
        FontId::monospace(10.0),
        ui.visuals().weak_text_color(),
    );
}

fn marker_radius(event_count: i64) -> f32 {
    (4.0 + (event_count.max(0) as f32).sqrt() * 0.9).clamp(4.0, 12.0)
}

fn wrap_x(mut x: f64, center: f64) -> f64 {
    while x - center > 0.5 {
        x -= 1.0;
    }
    while center - x > 0.5 {
        x += 1.0;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::places::PlaceSummary;
    use egui::{Context, RawInput};

    fn place(id: &str, name: &str, latitude: f64, longitude: f64) -> PlaceSummary {
        PlaceSummary {
            id: id.to_string(),
            name: name.to_string(),
            notes: None,
            latitude: Some(latitude),
            longitude: Some(longitude),
            event_count: 1,
        }
    }

    fn group(id: &str, name: &str) -> GroupSummary {
        GroupSummary {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            rule: None,
            color: Some(0x336699),
            member_count: 2,
        }
    }

    #[test]
    fn keeps_only_geocoded_places() {
        let mut state = MapState::default();
        let mut missing = place("p2", "Nowhere", 0.0, 0.0);
        missing.latitude = None;
        state.set_data(
            vec![place("p1", "Uppsala", 59.8586, 17.6389), missing],
            Vec::new(),
        );
        assert!(state.loaded);
        assert_eq!(state.total_places, 2);
        assert_eq!(state.places.len(), 1);
        assert!(state.needs_fit);
    }

    #[test]
    fn visible_places_respect_the_group_filter() {
        let mut state = MapState::default();
        state.set_data(
            vec![
                place("p1", "Uppsala", 59.8586, 17.6389),
                place("p2", "Lund", 55.7047, 13.1910),
            ],
            Vec::new(),
        );
        assert_eq!(state.visible_places().count(), 2);
        state.visible_ids = Some(HashSet::from(["p2".to_string()]));
        assert_eq!(state.visible_places().count(), 1);
    }

    #[test]
    fn fit_centres_markers() {
        let mut state = MapState::default();
        state.set_data(
            vec![
                place("p1", "Uppsala", 59.8586, 17.6389),
                place("p2", "Lund", 55.7047, 13.1910),
            ],
            Vec::new(),
        );
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        state.fit(rect);
        assert!(!state.needs_fit);
        assert!(state.zoom >= map::MIN_ZOOM);
        assert!((state.center_x - 0.5).abs() < 0.3);
    }

    #[test]
    fn map_tab_renders_offline() {
        let ctx = Context::default();
        let mut state = MapState::default();
        state.set_data(
            vec![
                place("p1", "Uppsala", 59.8586, 17.6389),
                place("p2", "Lund", 55.7047, 13.1910),
            ],
            vec![group("g1", "Swedes")],
        );
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let event = show(ui, &mut state, false);
            assert!(event.is_none());
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    }
}
