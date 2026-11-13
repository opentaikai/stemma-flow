//! Persistent user preferences stored in the OS configuration directory.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::people_columns::{PeopleColumnConfig, default_columns, sanitize_columns};
use crate::model::person_search::SavedPersonSearch;

const MAX_SAVED_SEARCHES: usize = 50;
const MAX_SAVED_SEARCH_NAME: usize = 80;

/// How the UI picks its light or dark visuals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

/// User preferences persisted to `stemma-flow/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub theme: ThemeMode,
    /// Rendered font size in points; the baseline of 14.0 maps to zoom 1.0.
    pub font_size: f32,
    /// Show comma-separated place names from largest to smallest in
    /// read-only displays. Stored places and GEDCOM keep the original order.
    pub reverse_place_names: bool,
    /// Allow place-name lookups against OpenStreetMap Nominatim.
    pub allow_online_geocoding: bool,
    /// BCP 47-style locale code for the interface language.
    pub language: String,
    /// Visible columns, order and widths for the People view.
    #[serde(default = "default_columns")]
    pub people_columns: Vec<PeopleColumnConfig>,
    /// Named advanced-search rules shared by every tree.
    #[serde(default)]
    pub saved_searches: Vec<SavedPersonSearch>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Light,
            font_size: 14.0,
            reverse_place_names: false,
            allow_online_geocoding: false,
            language: "en".to_string(),
            people_columns: default_columns(),
            saved_searches: Vec::new(),
        }
    }
}

impl AppConfig {
    /// Reads `config.toml` from the OS config directory, falling back to
    /// defaults (and a best-effort rewrite) when it is missing or corrupt.
    pub fn load() -> AppConfig {
        match config_path() {
            Some(path) => Self::load_from(&path),
            None => AppConfig::default(),
        }
    }

    /// Writes the config to the OS config directory; the caller surfaces
    /// failures (read-only filesystems, missing config dir) as status text.
    pub fn save(&self) -> Result<(), String> {
        match config_path() {
            Some(path) => self.save_to(&path),
            None => Err("no configuration directory available".to_string()),
        }
    }

    fn load_from(path: &Path) -> AppConfig {
        let mut config = match fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<AppConfig>(&text) {
                Ok(config) => config,
                Err(_) => rewrite_default(path),
            },
            Err(_) => rewrite_default(path),
        };
        config.sanitize();
        config
    }

    fn save_to(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string_pretty(self).map_err(|error| error.to_string())?;
        let parent = path
            .parent()
            .ok_or_else(|| "config path has no parent directory".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        let temp_path = PathBuf::from(temp);
        fs::write(&temp_path, text).map_err(|error| error.to_string())?;
        fs::rename(&temp_path, path).map_err(|error| error.to_string())
    }

    /// Adds a saved search or replaces the rule with the same name.
    pub fn upsert_saved_search(&mut self, saved: SavedPersonSearch) {
        SavedPersonSearch::upsert(&mut self.saved_searches, saved);
        self.saved_searches = sanitize_saved_searches(std::mem::take(&mut self.saved_searches));
    }

    /// Removes a saved search by name, returning whether one was removed.
    pub fn remove_saved_search(&mut self, name: &str) -> bool {
        let removed = SavedPersonSearch::remove(&mut self.saved_searches, name);
        if removed {
            self.saved_searches = sanitize_saved_searches(std::mem::take(&mut self.saved_searches));
        }
        removed
    }

    fn sanitize(&mut self) {
        if !self.font_size.is_finite() || !(10.0..=24.0).contains(&self.font_size) {
            self.font_size = 14.0;
        }
        if !crate::i18n::is_supported(&self.language) {
            self.language = "en".to_string();
        }
        self.people_columns = sanitize_columns(&self.people_columns);
        self.saved_searches = sanitize_saved_searches(std::mem::take(&mut self.saved_searches));
    }
}

fn sanitize_saved_searches(items: Vec<SavedPersonSearch>) -> Vec<SavedPersonSearch> {
    let mut sanitized: Vec<SavedPersonSearch> = Vec::new();
    for mut saved in items {
        saved.name = saved
            .name
            .trim()
            .chars()
            .take(MAX_SAVED_SEARCH_NAME)
            .collect();
        saved.criteria.sanitize();
        if saved.name.is_empty() || saved.criteria.is_empty() {
            continue;
        }
        if sanitized
            .iter()
            .any(|item| item.name.to_lowercase() == saved.name.to_lowercase())
        {
            continue;
        }
        sanitized.push(saved);
        if sanitized.len() >= MAX_SAVED_SEARCHES {
            break;
        }
    }
    sanitized
}

/// `<config_dir>/stemma-flow/config.toml`, or `None` without a config dir.
fn config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("stemma-flow").join("config.toml"))
}

/// Best-effort creation/repair of the config file; I/O failures are not
/// fatal, the app just runs on in-memory defaults.
fn rewrite_default(path: &Path) -> AppConfig {
    let config = AppConfig::default();
    let _ = config.save_to(path);
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "stemma-config-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    #[test]
    fn toml_roundtrip_preserves_every_field() {
        let config = AppConfig {
            theme: ThemeMode::Dark,
            font_size: 18.5,
            reverse_place_names: true,
            allow_online_geocoding: true,
            language: "de".to_string(),
            saved_searches: vec![SavedPersonSearch::new(
                "London born",
                crate::model::person_search::PersonSearchCriteria {
                    place: "London".to_string(),
                    ..Default::default()
                },
            )],
            ..AppConfig::default()
        };
        let text = toml::to_string_pretty(&config).expect("serialize");
        let back: AppConfig = toml::from_str(&text).expect("parse");
        assert_eq!(back, config);
    }

    #[test]
    fn missing_file_creates_the_default_config() {
        let dir = temp_dir("missing");
        let path = dir.join("nested").join("config.toml");

        let loaded = AppConfig::load_from(&path);

        assert_eq!(loaded, AppConfig::default());
        assert!(path.is_file(), "the file was written");
        let saved: AppConfig =
            toml::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(saved, AppConfig::default());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = temp_dir("corrupt");
        let path = dir.join("config.toml");
        fs::write(&path, "theme = [not valid toml").expect("write");

        let loaded = AppConfig::load_from(&path);

        assert_eq!(loaded, AppConfig::default());
        let rewritten: AppConfig =
            toml::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(rewritten, AppConfig::default(), "repaired in place");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn insane_font_sizes_reset_to_the_baseline() {
        let dir = temp_dir("sanitize");
        let path = dir.join("config.toml");
        fs::write(&path, "font_size = -3.0\n").expect("write");
        assert_eq!(AppConfig::load_from(&path).font_size, 14.0);

        fs::write(&path, "font_size = nan\n").expect("write");
        assert_eq!(AppConfig::load_from(&path).font_size, 14.0);

        fs::write(&path, "font_size = 900.0\n").expect("write");
        assert_eq!(AppConfig::load_from(&path).font_size, 14.0);

        fs::write(&path, "font_size = 20.5\n").expect("write");
        assert_eq!(AppConfig::load_from(&path).font_size, 20.5);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_to_creates_parent_directories() {
        let dir = temp_dir("save");
        let path = dir.join("a").join("b").join("config.toml");
        let config = AppConfig {
            theme: ThemeMode::System,
            font_size: 16.0,
            reverse_place_names: false,
            language: "en".to_string(),
            ..AppConfig::default()
        };

        config.save_to(&path).expect("save");

        let back: AppConfig =
            toml::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(back, config);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_configs_default_reverse_place_names_to_false() {
        let dir = temp_dir("reverse-default");
        let path = dir.join("config.toml");
        fs::write(&path, "theme = \"Dark\"\nfont_size = 14.0\n").expect("write");

        let loaded = AppConfig::load_from(&path);

        assert!(!loaded.reverse_place_names);
        assert!(!loaded.allow_online_geocoding);
        assert_eq!(loaded.theme, ThemeMode::Dark);
        assert!(loaded.saved_searches.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_searches_are_trimmed_deduplicated_and_upserted() {
        use crate::model::person_search::{PersonSearchCriteria, YearRange};

        let dir = temp_dir("saved-searches");
        let path = dir.join("config.toml");
        let config = AppConfig {
            saved_searches: vec![
                SavedPersonSearch::new(
                    "  London  ",
                    PersonSearchCriteria {
                        name: " Ada ".to_string(),
                        birth: YearRange {
                            from: Some(1900),
                            to: Some(1800),
                        },
                        ..PersonSearchCriteria::default()
                    },
                ),
                SavedPersonSearch::new(
                    "london",
                    PersonSearchCriteria {
                        place: "Paris".to_string(),
                        ..PersonSearchCriteria::default()
                    },
                ),
                SavedPersonSearch::new(
                    "Paris",
                    PersonSearchCriteria {
                        place: "Paris".to_string(),
                        ..PersonSearchCriteria::default()
                    },
                ),
                SavedPersonSearch::new(
                    "   ",
                    PersonSearchCriteria {
                        place: "London".to_string(),
                        ..PersonSearchCriteria::default()
                    },
                ),
                SavedPersonSearch::new("Empty", PersonSearchCriteria::default()),
            ],
            ..AppConfig::default()
        };
        config.save_to(&path).expect("save");

        let mut loaded = AppConfig::load_from(&path);
        assert_eq!(loaded.saved_searches.len(), 2);
        assert_eq!(loaded.saved_searches[0].name, "London");
        assert_eq!(loaded.saved_searches[0].criteria.name, "Ada");
        assert_eq!(loaded.saved_searches[0].criteria.birth.from, Some(1800));
        assert_eq!(loaded.saved_searches[0].criteria.birth.to, Some(1900));

        loaded.upsert_saved_search(SavedPersonSearch::new(
            "LONDON",
            PersonSearchCriteria {
                place: "Cambridge".to_string(),
                ..PersonSearchCriteria::default()
            },
        ));
        assert_eq!(
            loaded.saved_searches.len(),
            2,
            "the rule stays at its index"
        );
        assert_eq!(loaded.saved_searches[0].name, "LONDON");
        assert_eq!(loaded.saved_searches[0].criteria.place, "Cambridge");
        assert_eq!(loaded.saved_searches[1].name, "Paris");

        assert!(loaded.remove_saved_search("london"));
        assert_eq!(loaded.saved_searches.len(), 1);
        assert!(loaded.remove_saved_search("paris"));
        assert!(loaded.saved_searches.is_empty());
        assert!(!loaded.remove_saved_search("london"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn language_defaults_to_english_and_rejects_unknown_codes() {
        let dir = temp_dir("language");
        let path = dir.join("config.toml");
        fs::write(&path, "language = \"de\"\n").expect("write");
        assert_eq!(AppConfig::load_from(&path).language, "de");
        fs::write(&path, "language = \"xx\"\n").expect("write");
        assert_eq!(AppConfig::load_from(&path).language, "en");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_path_targets_the_app_folder() {
        let path = config_path().expect("config dir exists on this host");
        assert_eq!(
            path.parent().and_then(Path::file_name),
            Some(std::ffi::OsStr::new("stemma-flow"))
        );
        assert_eq!(path.file_name(), Some(std::ffi::OsStr::new("config.toml")));
    }
}
