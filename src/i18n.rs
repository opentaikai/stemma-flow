//! Embedded TOML interface translations with live language switching.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageInfo {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone)]
struct Catalog {
    code: String,
    name: String,
    strings: HashMap<String, String>,
}

#[derive(Debug)]
struct Translator {
    catalog: Catalog,
    fallback: HashMap<String, String>,
}

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

fn active() -> &'static RwLock<Arc<Translator>> {
    static ACTIVE: OnceLock<RwLock<Arc<Translator>>> = OnceLock::new();
    ACTIVE.get_or_init(|| {
        let catalog = catalogs()
            .get("en")
            .expect("English locale is required")
            .clone();
        let fallback = catalog.strings.clone();
        RwLock::new(Arc::new(Translator { catalog, fallback }))
    })
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
    active()
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .catalog
        .code
        .clone()
}

/// Switches the process-wide UI language; unknown codes fall back to English.
pub fn set_language(code: &str) {
    let code = if is_supported(code) { code } else { "en" };
    let catalog = catalogs().get(code).expect("supported locale").clone();
    let fallback = catalogs()
        .get("en")
        .expect("English locale is required")
        .strings
        .clone();
    let mut language = active()
        .write()
        .unwrap_or_else(|poison| poison.into_inner());
    *language = Arc::new(Translator { catalog, fallback });
}

/// Looks up a translated string, falling back to English and then the key.
pub fn tr(key: &str) -> String {
    let language = active()
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    if let Some(value) = language.catalog.strings.get(key) {
        return value.clone();
    }
    if let Some(value) = language.fallback.get(key) {
        debug_assert!(false, "missing translation for {key}");
        return value.clone();
    }
    debug_assert!(false, "unknown translation key {key}");
    key.to_string()
}

fn maybe_tr(key: &str) -> Option<String> {
    let language = active()
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    language
        .catalog
        .strings
        .get(key)
        .or_else(|| language.fallback.get(key))
        .cloned()
}

/// Localized built-in fact label with the stored DB label as fallback.
pub fn fact_label(key: &str, fallback: &str) -> String {
    maybe_tr(&format!("facts.{key}")).unwrap_or_else(|| fallback.to_string())
}

/// Localized built-in source-template name with the stored DB name as fallback.
pub fn template_label(key: &str, fallback: &str) -> String {
    maybe_tr(&format!("templates.{key}")).unwrap_or_else(|| fallback.to_string())
}

/// Looks up a string and replaces `{name}` placeholders.
pub fn tr_args(key: &str, args: &[(&str, String)]) -> String {
    let mut text = tr(key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// Resolves a plural key (`key_one` or `key_other`).
pub fn trn(key: &str, count: i64) -> String {
    let suffix = if count == 1 { "one" } else { "other" };
    tr_args(&format!("{key}_{suffix}"), &[("count", count.to_string())])
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
