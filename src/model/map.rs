//! Web Mercator projection and slippy-map helpers.

use std::f64::consts::PI;

/// Latitude clamp used by Web Mercator.
pub const MAX_LATITUDE: f64 = 85.051_128_78;
/// Pixel size of one map tile at integer zoom levels.
pub const TILE_SIZE: f64 = 256.0;
/// Lowest map zoom.
pub const MIN_ZOOM: u8 = 2;
/// Highest map zoom.
pub const MAX_ZOOM: u8 = 18;

/// One slippy-map tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

/// A visible tile and its unwrapped tile position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleTile {
    pub key: TileKey,
    pub x: i64,
    pub y: i64,
}

/// Normalized Mercator bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl MapBounds {
    pub fn from_points(points: impl IntoIterator<Item = (f64, f64)>) -> Option<Self> {
        let mut points = points.into_iter();
        let (x, y) = points.next()?;
        let mut bounds = Self {
            min_x: x,
            min_y: y,
            max_x: x,
            max_y: y,
        };
        for (x, y) in points {
            bounds.min_x = bounds.min_x.min(x);
            bounds.min_y = bounds.min_y.min(y);
            bounds.max_x = bounds.max_x.max(x);
            bounds.max_y = bounds.max_y.max(y);
        }
        Some(bounds)
    }

    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    pub fn center_x(&self) -> f64 {
        (self.min_x + self.max_x) / 2.0
    }

    pub fn center_y(&self) -> f64 {
        (self.min_y + self.max_y) / 2.0
    }
}

/// Pixel width of the whole world at `zoom`.
pub fn world_size(zoom: u8) -> f64 {
    TILE_SIZE * 2f64.powi(zoom as i32)
}

/// Converts longitude in degrees to a normalized Mercator x coordinate.
pub fn lon_to_x(lon: f64) -> f64 {
    ((lon + 180.0) / 360.0).clamp(0.0, 1.0)
}

/// Converts latitude in degrees to a normalized Mercator y coordinate.
pub fn lat_to_y(lat: f64) -> f64 {
    let lat = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE).to_radians();
    0.5 - (lat.tan() + 1.0 / lat.cos()).ln() / (2.0 * PI)
}

/// Converts a normalized Mercator x coordinate back to longitude.
pub fn x_to_lon(x: f64) -> f64 {
    x * 360.0 - 180.0
}

/// Converts a normalized Mercator y coordinate back to latitude.
pub fn y_to_lat(y: f64) -> f64 {
    let value = PI * (1.0 - 2.0 * y);
    value.sinh().atan().to_degrees()
}

/// Circular mean longitude, used to centre maps spanning the antimeridian.
pub fn circular_mean_lon(lons: &[f64]) -> f64 {
    if lons.is_empty() {
        return 0.0;
    }
    let (sin, cos) = lons.iter().fold((0.0, 0.0), |(sin, cos), lon| {
        let radians = lon.to_radians();
        (sin + radians.sin(), cos + radians.cos())
    });
    sin.atan2(cos).to_degrees()
}

/// Returns the tile containing `lat`/`lon` at `zoom`.
pub fn tile_key(lat: f64, lon: f64, zoom: u8) -> TileKey {
    let count = 1u32 << zoom;
    let max = (count - 1) as f64;
    let x = (lon_to_x(lon) * count as f64).floor().clamp(0.0, max) as u32;
    let y = (lat_to_y(lat) * count as f64).floor().clamp(0.0, max) as u32;
    TileKey { z: zoom, x, y }
}

/// Lists tiles intersecting a viewport, repeating the world horizontally.
pub fn visible_tiles(
    center_x: f64,
    center_y: f64,
    zoom: u8,
    width: f64,
    height: f64,
) -> Vec<VisibleTile> {
    let world = world_size(zoom);
    let left = center_x * world - width / 2.0;
    let top = center_y * world - height / 2.0;
    let count = 1u32 << zoom;
    let max = (count - 1) as f64;
    let min_x = (left / TILE_SIZE).floor() as i64;
    let max_x = ((left + width) / TILE_SIZE).floor() as i64;
    let min_y = (top / TILE_SIZE).floor().clamp(0.0, max) as i64;
    let max_y = ((top + height) / TILE_SIZE).floor().clamp(0.0, max) as i64;
    let mut tiles = Vec::new();
    for raw_x in min_x..=max_x {
        let x = raw_x.rem_euclid(count as i64) as u32;
        for raw_y in min_y..=max_y {
            tiles.push(VisibleTile {
                key: TileKey {
                    z: zoom,
                    x,
                    y: raw_y as u32,
                },
                x: raw_x,
                y: raw_y,
            });
        }
    }
    tiles
}

/// Chooses the highest zoom that fits `bounds` inside the viewport.
pub fn fit_zoom(bounds: MapBounds, viewport_width: f64, viewport_height: f64) -> u8 {
    if bounds.width() < 1e-9 && bounds.height() < 1e-9 {
        return 12;
    }
    for zoom in (MIN_ZOOM..=MAX_ZOOM).rev() {
        let world = world_size(zoom);
        if bounds.width() * world <= viewport_width * 0.88
            && bounds.height() * world <= viewport_height * 0.88
        {
            return zoom;
        }
    }
    MIN_ZOOM
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-6, "{left} != {right}");
    }

    #[test]
    fn projects_known_coordinates_and_round_trips() {
        approx(lon_to_x(0.0), 0.5);
        approx(lat_to_y(0.0), 0.5);
        approx(lon_to_x(180.0), 1.0);
        approx(lon_to_x(-180.0), 0.0);
        approx(lat_to_y(MAX_LATITUDE), 0.0);
        approx(lat_to_y(-MAX_LATITUDE), 1.0);

        for lon in [-170.0, -45.0, 0.0, 37.5, 170.0] {
            approx(x_to_lon(lon_to_x(lon)), lon);
        }
        for lat in [-70.0, -23.5, 0.0, 51.5, 80.0] {
            approx(y_to_lat(lat_to_y(lat)), lat);
        }
    }

    #[test]
    fn finds_tiles_at_known_locations() {
        let key = tile_key(0.0, 0.0, 1);
        assert_eq!(key, TileKey { z: 1, x: 1, y: 1 });
        let key = tile_key(51.5, -0.12, 10);
        assert_eq!(key.z, 10);
        assert!(key.x < (1 << 10));
        assert!(key.y < (1 << 10));
    }

    #[test]
    fn visible_tiles_cover_the_viewport() {
        let tiles = visible_tiles(0.5, 0.5, 4, 800.0, 600.0);
        assert!(!tiles.is_empty());
        assert!(tiles.iter().all(|tile| tile.key.z == 4));
        let min_x = tiles.iter().map(|tile| tile.x).min().expect("min");
        let max_x = tiles.iter().map(|tile| tile.x).max().expect("max");
        assert!(max_x - min_x >= 3, "the viewport spans several tiles");
    }

    #[test]
    fn fits_bounds_and_single_markers() {
        let bounds = MapBounds {
            min_x: 0.48,
            min_y: 0.48,
            max_x: 0.52,
            max_y: 0.52,
        };
        let zoom = fit_zoom(bounds, 800.0, 600.0);
        assert!((MIN_ZOOM..=MAX_ZOOM).contains(&zoom));

        let point = MapBounds {
            min_x: 0.5,
            min_y: 0.5,
            max_x: 0.5,
            max_y: 0.5,
        };
        assert_eq!(fit_zoom(point, 800.0, 600.0), 12);
    }

    #[test]
    fn circular_mean_handles_the_antimeridian() {
        let mean = circular_mean_lon(&[179.0, -179.0]);
        assert!(
            (mean - 180.0).abs() < 1e-6 || (mean + 180.0).abs() < 1e-6,
            "mean {mean}"
        );
        approx(circular_mean_lon(&[10.0, 20.0]), 15.0);
    }
}
