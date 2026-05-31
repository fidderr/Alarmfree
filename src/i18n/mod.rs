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
