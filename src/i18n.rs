//! Embedded TOML interface translations with live language switching.

use std::cell::Cell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{OnceLock, RwLock};

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
}

#[derive(Debug)]
struct Catalog {
    code: String,
    name: String,
    strings: HashMap<String, String>,
}

#[expect(
    clippy::panic,
    reason = "malformed embedded locale TOML is a build defect caught by the catalog tests"
)]
fn load_catalog(code: &str, text: &str) -> Catalog {
    let value: toml::Value =
        toml::from_str(text).unwrap_or_else(|error| panic!("invalid locale {code}: {error}"));
    let mut strings = HashMap::new();
    flatten("", &value, &mut strings);
    let name = strings
        .remove("language.name")
        .unwrap_or_else(|| code.to_string());
    let _ = strings.remove("language.code");
    Catalog {
        code: code.to_string(),
        name,
        strings,
    }
}

fn flatten(prefix: &str, value: &toml::Value, out: &mut HashMap<String, String>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(_) => flatten(&path, value, out),
            toml::Value::String(text) => {
                out.insert(path, text.clone());
            }
            other => {
                out.insert(path, other.to_string());
            }
        }
    }
}

fn catalogs() -> &'static HashMap<&'static str, Catalog> {
    static CATALOGS: OnceLock<HashMap<&'static str, Catalog>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        EMBEDDED_LOCALES
            .iter()
            .map(|(code, text)| (*code, load_catalog(code, text)))
            .collect()
    })
}

/// The embedded English catalog, guaranteed by the build script and the
/// catalog tests.
#[expect(
    clippy::expect_used,
    reason = "en.toml is embedded at build time and verified by the catalog tests"
)]
fn english() -> &'static Catalog {
    catalogs().get("en").expect("English locale is embedded")
}

/// The catalog for `code`, falling back to English when it is unsupported.
fn catalog_for(code: &str) -> &'static Catalog {
    catalogs().get(code).unwrap_or_else(english)
}

/// The process-wide active catalog, swapped by [`set_language`].
fn active() -> &'static RwLock<&'static Catalog> {
    static ACTIVE: OnceLock<RwLock<&'static Catalog>> = OnceLock::new();
    ACTIVE.get_or_init(|| RwLock::new(english()))
}

thread_local! {
    /// Catalog used by [`with_language`] on this thread (background report jobs).
    static LANGUAGE_OVERRIDE: Cell<Option<&'static Catalog>> = const { Cell::new(None) };
}

fn current_catalog() -> &'static Catalog {
    if let Some(catalog) = LANGUAGE_OVERRIDE.with(|current| current.get()) {
        return catalog;
    }
    *active().read().unwrap_or_else(|poison| poison.into_inner())
}

fn lookup(key: &str) -> Option<&'static str> {
    current_catalog()
        .strings
        .get(key)
        .or_else(|| english().strings.get(key))
        .map(String::as_str)
}

/// Renders `f` using `code` without changing the process-wide UI language.
pub fn with_language<T>(code: &str, f: impl FnOnce() -> T) -> T {
    let previous = LANGUAGE_OVERRIDE.with(|current| current.replace(Some(catalog_for(code))));
    let result = catch_unwind(AssertUnwindSafe(f));
    LANGUAGE_OVERRIDE.with(|current| current.set(previous));
    match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

/// Returns the embedded, sorted language list.
pub fn available_languages() -> Vec<LanguageInfo> {
    let mut languages: Vec<LanguageInfo> = catalogs()
        .values()
        .map(|catalog| LanguageInfo {
            code: catalog.code.clone(),
            name: catalog.name.clone(),
        })
        .collect();
    languages.sort_by(|left, right| left.name.cmp(&right.name));
    languages
}

pub fn is_supported(code: &str) -> bool {
    catalogs().contains_key(code)
}

pub fn current_language() -> String {
    current_catalog().code.clone()
}

/// Switches the process-wide UI language; unknown codes fall back to English.
pub fn set_language(code: &str) {
    *active()
        .write()
        .unwrap_or_else(|poison| poison.into_inner()) = catalog_for(code);
}

/// Looks up a compile-time key, borrowing the text from the embedded catalogs.
///
/// Falls back to English and then the key itself; the fallback is a static
/// string, so the lookup never allocates.
pub fn tr(key: &'static str) -> &'static str {
    lookup(key).unwrap_or_else(|| {
        debug_assert!(false, "unknown translation key {key}");
        key
    })
}

/// Looks up a key built at runtime, returning owned text.
pub fn tr_dynamic(key: &str) -> String {
    lookup(key).map(str::to_string).unwrap_or_else(|| {
        debug_assert!(false, "unknown translation key {key}");
        key.to_string()
    })
}

/// Localized built-in fact label with the stored DB label as fallback.
pub fn fact_label(key: &str, fallback: &str) -> String {
    lookup(&format!("facts.{key}"))
        .unwrap_or(fallback)
        .to_string()
}

/// Localized built-in source-template name with the stored DB name as fallback.
pub fn template_label(key: &str, fallback: &str) -> String {
    lookup(&format!("templates.{key}"))
        .unwrap_or(fallback)
        .to_string()
}

/// Looks up a string and replaces `{name}` placeholders.
pub fn tr_args(key: &str, args: &[(&str, String)]) -> String {
    let mut text = tr_dynamic(key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// Resolves a plural key and replaces additional placeholders.
pub fn trn_args(key: &str, count: i64, args: &[(&str, String)]) -> String {
    let suffix = if count == 1 { "one" } else { "other" };
    let mut values = args.to_vec();
    values.push(("count", count.to_string()));
    tr_args(&format!("{key}_{suffix}"), &values)
}

/// Resolves a plural key (`key_one` or `key_other`).
pub fn trn(key: &str, count: i64) -> String {
    trn_args(key, count, &[])
}

/// Parses the locale from a TOML file; used by catalog tests.
#[cfg(test)]
fn parse_for_test(code: &str, text: &str) -> Catalog {
    load_catalog(code, text)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn catalogs_have_matching_keys_and_placeholders() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let english = catalogs().get("en").expect("English");
        let german = catalogs().get("de").expect("German");
        let mut en_keys: Vec<_> = english.strings.keys().collect();
        let mut de_keys: Vec<_> = german.strings.keys().collect();
        en_keys.sort();
        de_keys.sort();
        assert_eq!(en_keys, de_keys, "locale key sets differ");
        for key in english.strings.keys() {
            assert!(
                !german
                    .strings
                    .get(key)
                    .expect("German key")
                    .trim()
                    .is_empty(),
                "empty German translation for {key}"
            );
            let mut en_placeholders = placeholders(english.strings.get(key).expect("English"));
            let mut de_placeholders = placeholders(german.strings.get(key).expect("German"));
            en_placeholders.sort();
            de_placeholders.sort();
            assert_eq!(
                en_placeholders, de_placeholders,
                "placeholder mismatch for {key}"
            );
        }
    }

    #[test]
    fn switching_languages_changes_lookup_and_rejects_unknown_codes() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_language("de");
        assert_eq!(tr("tabs.graph"), "Grafikansicht");
        set_language("xx");
        assert_eq!(current_language(), "en");
        assert_eq!(tr("tabs.graph"), "Graph View");
    }

    #[test]
    fn translations_borrow_from_the_static_catalogs() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_language("en");
        // The type annotation is the regression test: `tr` must borrow, not allocate.
        let value: &'static str = tr("tabs.graph");
        assert_eq!(value, "Graph View");
        assert_eq!(tr_dynamic("tabs.graph"), "Graph View");
    }

    #[test]
    fn placeholders_and_plurals_substitute() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_language("en");
        assert_eq!(
            tr_args("menu.active_tree", &[("name", "Tree".to_string())]),
            "Active Tree: Tree"
        );
        assert_eq!(trn("media.links", 1), "1 link");
        assert_eq!(trn("media.links", 2), "2 links");
        assert_eq!(
            trn_args("status.coordinates_updated", 2, &[]),
            "Updated coordinates for 2 places"
        );
    }

    #[test]
    fn built_in_labels_localize_and_custom_labels_fall_back() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_language("de");
        assert_eq!(fact_label("BIRTH", "Birth"), "Geburt");
        assert_eq!(template_label("book", "Book"), "Buch");
        assert_eq!(fact_label("CUSTOM", "My custom fact"), "My custom fact");
        set_language("en");
    }

    #[test]
    fn scoped_language_restores_global_state() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_language("en");
        assert_eq!(with_language("de", || tr("tabs.graph")), "Grafikansicht");
        assert_eq!(tr("tabs.graph"), "Graph View");
        assert_eq!(current_language(), "en");
    }

    #[test]
    fn scoped_language_does_not_leak_across_threads() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_language("en");
        let other = with_language("de", || {
            std::thread::spawn(|| tr("tabs.graph"))
                .join()
                .expect("thread")
        });
        assert_eq!(other, "Graph View");
        assert_eq!(with_language("de", || tr("tabs.graph")), "Grafikansicht");
    }

    #[test]
    fn scoped_language_restores_after_panic() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_language("en");
        let result = std::panic::catch_unwind(|| {
            with_language("de", || panic!("report panic"));
        });
        assert!(result.is_err());
        assert_eq!(tr("tabs.graph"), "Graph View");
    }

    #[test]
    fn language_metadata_is_available() {
        let languages = available_languages();
        assert!(languages.iter().any(|language| language.code == "en"));
        assert!(languages.iter().any(|language| language.code == "de"));
    }

    #[test]
    fn parser_flattens_tables_and_reads_metadata() {
        let catalog = parse_for_test(
            "xx",
            "[language]\ncode = \"xx\"\nname = \"Test\"\n[a]\nb = \"B\"\n",
        );
        assert_eq!(catalog.name, "Test");
        assert_eq!(catalog.strings.get("a.b").map(String::as_str), Some("B"));
    }

    fn placeholders(text: &str) -> Vec<String> {
        let mut values = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                break;
            };
            values.push(after[..end].to_string());
            rest = &after[end + 1..];
        }
        values
    }
}
