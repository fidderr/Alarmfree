//! Application settings types: AppSettings, ThemeMode, Locale, Prefs.

use serde::{Deserialize, Serialize};

use crate::app::AlarmData;

/// Visual theme for the app. Persisted via serde directly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follows the phone's dark theme setting.
    #[default]
    System,
    Dark,
    Light,
    /// Dark, on a pure black background.
    Black,
    Colorblind,
}

impl ThemeMode {
    pub fn css_class(self, system_dark: bool) -> &'static str {
        match self {
            ThemeMode::System if system_dark => "theme-dark",
            ThemeMode::System => "theme-light",
            ThemeMode::Dark => "theme-dark",
            ThemeMode::Light => "theme-light",
            ThemeMode::Black => "theme-black",
            ThemeMode::Colorblind => "theme-colorblind",
        }
    }

    /// Whether this theme has a light background, for the system bar icons.
    pub fn is_light(self, system_dark: bool) -> bool {
        self.css_class(system_dark) == "theme-light"
    }
}

/// The colour of what stands out. The colorblind theme keeps its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    Blue,
    Teal,
    Purple,
    Pink,
    Amber,
}

impl Accent {
    pub const ALL: [Accent; 5] = [
        Accent::Blue,
        Accent::Teal,
        Accent::Purple,
        Accent::Pink,
        Accent::Amber,
    ];

    pub fn css_class(self) -> &'static str {
        match self {
            Accent::Blue => "accent-blue",
            Accent::Teal => "accent-teal",
            Accent::Purple => "accent-purple",
            Accent::Pink => "accent-pink",
            Accent::Amber => "accent-amber",
        }
    }

    pub fn name_key(self) -> &'static str {
        match self {
            Accent::Blue => "accent.blue",
            Accent::Teal => "accent.teal",
            Accent::Purple => "accent.purple",
            Accent::Pink => "accent.pink",
            Accent::Amber => "accent.amber",
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

/// How times are written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeFormat {
    #[default]
    System,
    H24,
    H12,
}

/// The day the week starts on, in the day chips and the weekly chart.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeekStart {
    #[default]
    Monday,
    Sunday,
    Saturday,
}

impl WeekStart {
    /// The days in display order, as indices with Monday as 0.
    pub fn day_order(self) -> [usize; 7] {
        match self {
            WeekStart::Monday => [0, 1, 2, 3, 4, 5, 6],
            WeekStart::Sunday => [6, 0, 1, 2, 3, 4, 5],
            WeekStart::Saturday => [5, 6, 0, 1, 2, 3, 4],
        }
    }

    /// How many days of this week have gone by on `day`.
    pub fn days_into_week(self, day: chrono::Weekday) -> u32 {
        (day.num_days_from_monday() + 7 - self.day_order()[0] as u32) % 7
    }
}

/// The order of the home list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortMode {
    /// The order the user dragged the alarms in.
    #[default]
    Own,
    Time,
    NextRing,
}

/// Everything added after the settings table's first columns. Stored as one JSON
/// column, so a new preference needs no migration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// The language follows the phone's, instead of the one in `AppSettings`.
    pub language_follows_phone: bool,
    pub time_format: TimeFormat,
    pub week_start: WeekStart,
    pub sort: SortMode,
    pub touch_feedback: bool,
    pub less_motion: bool,
    pub confetti: bool,
    pub accent: Accent,
    /// What the user called their own sounds, by file name.
    pub sound_names: std::collections::BTreeMap<String, String>,
    /// Show a notification this long before an alarm rings (0 = off).
    pub notify_before_minutes: u8,
    /// The first-run setup was gone through.
    pub setup_done: bool,
    /// What a new alarm starts from, when the user set that up.
    pub alarm_defaults: Option<AlarmData>,
    /// Warnings the user does not want on the home screen. The reliability screen still
    /// has them.
    pub hidden_warnings: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            language_follows_phone: true,
            time_format: TimeFormat::default(),
            week_start: WeekStart::default(),
            sort: SortMode::default(),
            touch_feedback: true,
            less_motion: false,
            confetti: true,
            accent: Accent::default(),
            sound_names: std::collections::BTreeMap::new(),
            notify_before_minutes: 0,
            setup_done: false,
            alarm_defaults: None,
            hidden_warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub locale: Locale,
    /// While an alarm rings, the alarm volume is raised to at least this
    /// percentage (and restored afterwards), so a muted phone still wakes
    /// the user. 0 leaves the volume alone.
    pub min_volume_percent: u8,
    pub prefs: Prefs,
}

impl Locale {
    /// The language for a two-letter code ("nl"), if the app has it.
    pub fn from_language_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::English),
            "nl" => Some(Self::Dutch),
            "es" => Some(Self::Spanish),
            "fr" => Some(Self::French),
            "de" => Some(Self::German),
            _ => None,
        }
    }

    pub const ALL: [Locale; 5] = [
        Locale::English,
        Locale::Dutch,
        Locale::Spanish,
        Locale::French,
        Locale::German,
    ];

    /// The language's name, in that language.
    pub fn display_name(self) -> &'static str {
        match self {
            Locale::English => "English",
            Locale::Dutch => "Nederlands",
            Locale::Spanish => "Español",
            Locale::French => "Français",
            Locale::German => "Deutsch",
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::default(),
            locale: Locale::default(),
            min_volume_percent: 50,
            prefs: Prefs::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_theme_follows_the_phone_and_the_others_do_not() {
        assert_eq!(ThemeMode::System.css_class(true), "theme-dark");
        assert_eq!(ThemeMode::System.css_class(false), "theme-light");
        assert_eq!(ThemeMode::Black.css_class(false), "theme-black");
        assert!(ThemeMode::Light.is_light(true));
        assert!(!ThemeMode::Colorblind.is_light(false));
    }

    #[test]
    fn prefs_saved_by_an_older_version_get_defaults_for_the_rest() {
        let prefs: Prefs = serde_json::from_str(r#"{"less_motion":true}"#).unwrap();
        assert!(prefs.less_motion);
        assert!(prefs.touch_feedback, "on unless turned off");
        assert!(prefs.confetti);
        assert_eq!(prefs.accent, Accent::Blue);
        assert!(!prefs.setup_done);
    }

    #[test]
    fn every_week_start_lists_each_day_once() {
        for start in [WeekStart::Monday, WeekStart::Sunday, WeekStart::Saturday] {
            let mut days = start.day_order();
            days.sort_unstable();
            assert_eq!(days, [0, 1, 2, 3, 4, 5, 6]);
        }
        assert_eq!(WeekStart::Sunday.day_order()[0], 6);
    }

    #[test]
    fn a_week_starts_on_the_day_the_user_says() {
        use chrono::Weekday::{Mon, Sat, Sun};
        assert_eq!(WeekStart::Monday.days_into_week(Mon), 0);
        assert_eq!(WeekStart::Monday.days_into_week(Sun), 6);
        assert_eq!(WeekStart::Sunday.days_into_week(Sun), 0);
        assert_eq!(WeekStart::Sunday.days_into_week(Mon), 1);
        assert_eq!(WeekStart::Saturday.days_into_week(Sat), 0);
        assert_eq!(WeekStart::Saturday.days_into_week(Mon), 2);
    }
}
