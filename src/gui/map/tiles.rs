//! Background loading and disk caching of OpenStreetMap raster tiles.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use egui::ColorImage;

use crate::model::map::TileKey;

const TILE_URL: &str = "https://tile.openstreetmap.org";
const USER_AGENT: &str =
    "stemma-flow/0.1 (genealogy desktop app; https://github.com/opentaikai/stemma-flow)";
const MIN_REQUEST_GAP: Duration = Duration::from_millis(100);

/// One loaded or failed tile.
pub struct TileResult {
    pub key: TileKey,
    pub image: Result<ColorImage, String>,
}

/// Background tile fetcher with a disk-backed cache.
pub struct TileLoader {
    requests: Sender<TileKey>,
    results: Receiver<TileResult>,
}

impl TileLoader {
    pub fn spawn() -> Self {
        let (request_tx, request_rx) = channel::<TileKey>();
        let (result_tx, result_rx) = channel::<TileResult>();
        thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(15)))
                .user_agent(USER_AGENT)
                .build()
                .into();
            let root = cache_root();
            let mut last_fetch = None;
            for key in request_rx {
                let image = load_bytes(&agent, root.as_deref(), key, &mut last_fetch)
                    .and_then(|bytes| decode(&bytes));
                if image.is_err()
                    && let Some(root) = root.as_deref()
                {
                    let _ = std::fs::remove_file(cache_path(root, key));
                }
                if result_tx.send(TileResult { key, image }).is_err() {
                    break;
                }
            }
        });
        Self {
            requests: request_tx,
            results: result_rx,
        }
    }

    pub fn request(&self, key: TileKey) {
        let _ = self.requests.send(key);
    }

    pub fn poll(&self) -> Vec<TileResult> {
        self.results.try_iter().collect()
    }
}

/// Default tile cache directory under the OS cache directory.
pub fn cache_root() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("stemma-flow").join("tiles"))
}

/// Path for one tile inside `root`.
pub fn cache_path(root: &Path, key: TileKey) -> PathBuf {
    root.join(key.z.to_string())
        .join(key.x.to_string())
        .join(format!("{}.png", key.y))
}

/// OpenStreetMap raster tile URL.
pub fn tile_url(key: TileKey) -> String {
    format!("{TILE_URL}/{}/{}/{}.png", key.z, key.x, key.y)
}

fn load_bytes(
    agent: &ureq::Agent,
    root: Option<&Path>,
    key: TileKey,
    last_fetch: &mut Option<Instant>,
) -> Result<Vec<u8>, String> {
    if let Some(root) = root {
        let path = cache_path(root, key);
        if let Ok(bytes) = std::fs::read(&path)
            && !bytes.is_empty()
        {
            return Ok(bytes);
        }
    }
    if let Some(last) = *last_fetch {
        let elapsed = last.elapsed();
        if elapsed < MIN_REQUEST_GAP {
            thread::sleep(MIN_REQUEST_GAP - elapsed);
        }
    }
    *last_fetch = Some(Instant::now());
    let mut response = agent
        .get(&tile_url(key))
        .call()
        .map_err(|error| error.to_string())?;
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        return Err("empty map tile".to_string());
    }
    if let Some(root) = root {
        let path = cache_path(root, key);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, &bytes);
    }
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    let image = image::load_from_memory(bytes)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.into_raw();
    Ok(ColorImage::from_rgba_unmultiplied(size, &pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("png encodes");
        bytes
    }

    #[test]
    fn cache_paths_and_urls_are_stable() {
        let root = Path::new("/tmp/tiles");
        let key = TileKey { z: 7, x: 42, y: 33 };
        assert_eq!(
            cache_path(root, key),
            PathBuf::from("/tmp/tiles/7/42/33.png")
        );
        assert_eq!(tile_url(key), "https://tile.openstreetmap.org/7/42/33.png");
    }

    #[test]
    fn cached_tiles_skip_the_network() {
        let root = std::env::temp_dir().join(format!(
            "stemma-tile-cache-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let key = TileKey { z: 3, x: 1, y: 2 };
        let path = cache_path(&root, key);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("cache dir");
        std::fs::write(&path, png()).expect("cache tile");

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(50)))
            .build()
            .into();
        let mut last = None;
        let bytes = load_bytes(&agent, Some(&root), key, &mut last).expect("cache hit");
        assert!(!bytes.is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn decodes_png_tiles() {
        let image = decode(&png()).expect("png decodes");
        assert_eq!(image.size, [2, 2]);
        assert!(decode(b"not a png").is_err());
    }
}
