//! Application settings types: AppSettings, ThemeMode, Locale.

use serde::{Deserialize, Serialize};

/// Visual theme for the app. Persisted via serde directly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    Colorblind,
}

impl ThemeMode {
    pub fn css_class(self) -> &'static str {
        match self {
            ThemeMode::Dark => "theme-dark",
            ThemeMode::Light => "theme-light",
            ThemeMode::Colorblind => "theme-colorblind",
        }
    }
}

/// UI language. Persisted via serde directly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Locale {
    #[default]
    English,
    Dutch,
    Spanish,
    French,
    German,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub locale: Locale,
    pub default_snooze_count: u8,
    pub default_snooze_interval_minutes: u8,
    /// While an alarm rings, the alarm volume is raised to at least this
    /// percentage (and restored afterwards), so a muted phone still wakes
    /// the user. 0 leaves the volume alone.
    pub min_volume_percent: u8,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::default(),
            locale: Locale::default(),
            default_snooze_count: 3,
            default_snooze_interval_minutes: 1,
            min_volume_percent: 50,
        }
    }
}
