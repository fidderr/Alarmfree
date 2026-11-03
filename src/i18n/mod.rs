//! Internationalization module - loads translations from embedded JSON files.

use std::collections::HashMap;

use crate::models::Locale;

/// Type alias for translation maps (key -> translated string).
pub type TranslationMap = HashMap<String, String>;

/// Load translations for a given locale.
pub fn load_translations(locale: Locale) -> TranslationMap {
    let json = match locale {
        Locale::English => include_str!("en.json"),
        Locale::Dutch => include_str!("nl.json"),
        Locale::Spanish => include_str!("es.json"),
        Locale::French => include_str!("fr.json"),
        Locale::German => include_str!("de.json"),
    };
    serde_json::from_str(json).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCALES: [Locale; 5] = [
        Locale::English,
        Locale::Dutch,
        Locale::Spanish,
        Locale::French,
        Locale::German,
    ];

    /// `load_translations` falls back to an empty map on bad JSON, which would
    /// silently show raw keys in the UI — make that a test failure instead.
    #[test]
    fn every_locale_parses_and_matches_english_keys() {
        let en = load_translations(Locale::English);
        assert!(!en.is_empty(), "en.json failed to parse");
        for locale in LOCALES {
            let map = load_translations(locale);
            let mut missing: Vec<_> = en.keys().filter(|k| !map.contains_key(*k)).collect();
            let mut extra: Vec<_> = map.keys().filter(|k| !en.contains_key(*k)).collect();
            missing.sort();
            extra.sort();
            assert!(missing.is_empty(), "{locale:?} missing keys: {missing:?}");
            assert!(extra.is_empty(), "{locale:?} has keys not in en: {extra:?}");
        }
    }

    fn placeholders(text: &str) -> Vec<&str> {
        let mut found: Vec<&str> = text
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split('}').next())
            .collect();
        found.sort_unstable();
        found
    }

    /// A placeholder a translation lacks would never be filled in, and one with another
    /// name would show as `{name}`.
    #[test]
    fn every_translation_has_the_placeholders_of_the_english() {
        let en = load_translations(Locale::English);
        for locale in LOCALES {
            for (key, text) in load_translations(locale) {
                assert_eq!(
                    placeholders(&text),
                    placeholders(&en[&key]),
                    "{locale:?} {key}: {text}"
                );
            }
        }
    }

    fn rust_files(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                found.push(path);
            }
        }
    }

    /// What goes through `text::counted` has its form for one.
    #[test]
    fn every_counted_text_has_its_form_for_one() {
        let mut files = Vec::new();
        rust_files(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let en = load_translations(Locale::English);
        let mut seen = 0;
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for call in text.split("counted(").skip(1) {
                if let Some(key) = call.split('"').nth(1).filter(|key| en.contains_key(*key)) {
                    seen += 1;
                    assert!(en.contains_key(&format!("{key}_one")), "{key}");
                }
            }
        }
        assert!(seen >= 5, "found {seen}");
    }

    /// A key that does not exist shows on screen as the key.
    #[test]
    fn every_key_the_code_names_exists() {
        let en = load_translations(Locale::English);
        let groups: std::collections::HashSet<&str> =
            en.keys().filter_map(|key| key.split('.').next()).collect();
        let mut files = Vec::new();
        rust_files(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let mut missing = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            // Between quotes: every second piece.
            for quoted in code.split('"').skip(1).step_by(2) {
                let looks_like_key = quoted.contains('.')
                    && !quoted.ends_with('.')
                    && quoted.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_'
                    })
                    && quoted.split('.').next().is_some_and(|g| groups.contains(g));
                if looks_like_key && !en.contains_key(quoted) {
                    missing.push(format!("{}: {quoted}", file.display()));
                }
            }
        }
        assert!(missing.is_empty(), "not in en.json: {missing:#?}");
    }
}
