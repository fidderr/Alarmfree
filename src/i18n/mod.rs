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
}
