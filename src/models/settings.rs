//! Application settings types: AppSettings, ThemeMode, Locale, Prefs.

use serde::{Deserialize, Serialize};

use crate::models::theme::{
    BaseColours, CustomTheme, Palette, Rgb, BLACK_THEME, DARK, FOREST, LIGHT, MIDNIGHT, MIST, PLUM,
    SAND,
};
use crate::models::AlarmData;

/// Visual theme for the app. Persisted via serde directly. A theme that no longer exists
/// reads as the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Follows the phone's dark theme setting.
    #[default]
    System,
    Dark,
    Light,
    /// Dark, on a pure black background.
    Black,
    Midnight,
    Forest,
    Plum,
    Sand,
    Mist,
    /// The colours the user chose.
    Custom,
}

impl ThemeMode {
    /// The themes to pick from, apart from the user's own.
    pub const PRESETS: [ThemeMode; 9] = [
        ThemeMode::System,
        ThemeMode::Dark,
        ThemeMode::Light,
        ThemeMode::Black,
        ThemeMode::Midnight,
        ThemeMode::Forest,
        ThemeMode::Plum,
        ThemeMode::Sand,
        ThemeMode::Mist,
    ];

    /// This theme's colours. The user's own theme has none of its own.
    pub fn base(self, system_dark: bool) -> BaseColours {
        match self {
            ThemeMode::System if system_dark => DARK,
            ThemeMode::System | ThemeMode::Light => LIGHT,
            ThemeMode::Dark | ThemeMode::Custom => DARK,
            ThemeMode::Black => BLACK_THEME,
            ThemeMode::Midnight => MIDNIGHT,
            ThemeMode::Forest => FOREST,
            ThemeMode::Plum => PLUM,
            ThemeMode::Sand => SAND,
            ThemeMode::Mist => MIST,
        }
    }

    pub fn name_key(self) -> &'static str {
        match self {
            ThemeMode::System => "theme.system",
            ThemeMode::Dark => "theme.dark",
            ThemeMode::Light => "theme.light",
            ThemeMode::Black => "theme.black",
            ThemeMode::Midnight => "theme.midnight",
            ThemeMode::Forest => "theme.forest",
            ThemeMode::Plum => "theme.plum",
            ThemeMode::Sand => "theme.sand",
            ThemeMode::Mist => "theme.mist",
            ThemeMode::Custom => "theme.custom",
        }
    }
}

/// The colour of what stands out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accent {
    #[default]
    Blue,
    Teal,
    Purple,
    Pink,
    Amber,
    Coral,
    Sage,
    Sky,
    Lime,
    /// The colour the user picked on the wheel.
    Custom,
}

impl Accent {
    /// The soft colours that come with the app.
    pub const PRESETS: [Accent; 9] = [
        Accent::Blue,
        Accent::Teal,
        Accent::Purple,
        Accent::Pink,
        Accent::Amber,
        Accent::Coral,
        Accent::Sage,
        Accent::Sky,
        Accent::Lime,
    ];

    /// What it looks like on a dark and on a light background: one shade does not read on both.
    pub fn colours(self) -> (Rgb, Rgb) {
        let pair = |dark: Rgb, light: Rgb| (dark, light);
        match self {
            Accent::Blue | Accent::Custom => {
                pair(Rgb::new(0x7c, 0xb0, 0xff), Rgb::new(0x1f, 0x5f, 0xd6))
            }
            Accent::Teal => pair(Rgb::new(0x5f, 0xd4, 0xc4), Rgb::new(0x00, 0x73, 0x6a)),
            Accent::Purple => pair(Rgb::new(0xb9, 0xa0, 0xff), Rgb::new(0x6a, 0x3f, 0xd1)),
            Accent::Pink => pair(Rgb::new(0xff, 0x94, 0xc8), Rgb::new(0xb8, 0x24, 0x6f)),
            Accent::Amber => pair(Rgb::new(0xff, 0xc4, 0x5c), Rgb::new(0x8a, 0x5a, 0x00)),
            Accent::Coral => pair(Rgb::new(0xff, 0x9d, 0x8a), Rgb::new(0xc2, 0x45, 0x2d)),
            Accent::Sage => pair(Rgb::new(0x9a, 0xd4, 0x9a), Rgb::new(0x3c, 0x7d, 0x4a)),
            Accent::Sky => pair(Rgb::new(0x86, 0xd4, 0xf0), Rgb::new(0x0b, 0x72, 0x85)),
            Accent::Lime => pair(Rgb::new(0xcf, 0xe3, 0x7a), Rgb::new(0x5f, 0x7a, 0x00)),
        }
    }

    pub fn name_key(self) -> &'static str {
        match self {
            Accent::Blue => "accent.blue",
            Accent::Teal => "accent.teal",
            Accent::Purple => "accent.purple",
            Accent::Pink => "accent.pink",
            Accent::Amber => "accent.amber",
            Accent::Coral => "accent.coral",
            Accent::Sage => "accent.sage",
            Accent::Sky => "accent.sky",
            Accent::Lime => "accent.lime",
            Accent::Custom => "accent.custom",
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
    /// What the wheel was set to, for [`Accent::Custom`].
    pub custom_accent: Option<Rgb>,
    /// The colours of [`ThemeMode::Custom`].
    pub custom_theme: Option<CustomTheme>,
    /// What the user called their own sounds, by file name.
    pub sound_names: std::collections::BTreeMap<String, String>,
    /// From when this was one setting for all alarms. Each alarm has its own now, and this is
    /// only read once to hand it to them.
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
            custom_accent: None,
            custom_theme: None,
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

impl AppSettings {
    /// The colours on screen right now.
    pub fn palette(&self, system_dark: bool) -> Palette {
        let wheel = self
            .prefs
            .custom_accent
            .filter(|_| self.prefs.accent == Accent::Custom);
        // A colour off the wheel is what it is, on any background.
        let accent = wheel.map_or_else(|| self.prefs.accent.colours(), |colour| (colour, colour));
        match (self.theme, self.prefs.custom_theme) {
            (ThemeMode::Custom, Some(colours)) => Palette::new(colours, accent),
            (theme, _) => Palette::from_base(theme.base(system_dark), accent),
        }
    }
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

    fn palette_of(theme: ThemeMode, system_dark: bool) -> Palette {
        AppSettings {
            theme,
            ..AppSettings::default()
        }
        .palette(system_dark)
    }

    #[test]
    fn the_system_theme_follows_the_phone_and_the_others_do_not() {
        assert_eq!(
            palette_of(ThemeMode::System, true).background,
            DARK.background
        );
        assert_eq!(
            palette_of(ThemeMode::System, false).background,
            LIGHT.background
        );
        assert_eq!(
            palette_of(ThemeMode::Black, false).background,
            BLACK_THEME.background
        );
        assert!(palette_of(ThemeMode::Light, true).light);
        assert!(palette_of(ThemeMode::Sand, true).light);
        assert!(!palette_of(ThemeMode::Plum, false).light);
    }

    #[test]
    fn the_colorblind_theme_someone_had_saved_reads_as_the_default() {
        let theme: ThemeMode = serde_json::from_str("\"Colorblind\"").unwrap_or_default();
        assert_eq!(theme, ThemeMode::System);
    }

    #[test]
    fn every_theme_that_comes_with_the_app_reads_with_every_accent() {
        for theme in ThemeMode::PRESETS {
            for accent in Accent::PRESETS {
                let settings = AppSettings {
                    theme,
                    prefs: Prefs {
                        accent,
                        ..Prefs::default()
                    },
                    ..AppSettings::default()
                };
                let palette = settings.palette(true);
                let wanted = if palette.light {
                    accent.colours().1
                } else {
                    accent.colours().0
                };
                assert_eq!(
                    palette.accent, wanted,
                    "{accent:?} on {theme:?} was changed"
                );
            }
        }
    }

    #[test]
    fn a_colour_off_the_wheel_is_used_on_any_theme() {
        let colour = Rgb::new(0xb0, 0x40, 0x10);
        let mut settings = AppSettings {
            theme: ThemeMode::Light,
            prefs: Prefs {
                accent: Accent::Custom,
                custom_accent: Some(colour),
                ..Prefs::default()
            },
            ..AppSettings::default()
        };
        assert_eq!(settings.palette(false).accent, colour);
        // Only what would not read gets changed, and only as far as it takes.
        settings.prefs.custom_accent = Some(Rgb::new(0xff, 0xff, 0xf0));
        let on_light = settings.palette(false);
        assert!(on_light.accent.contrast(on_light.card) >= crate::models::theme::MIN_CONTRAST);
    }

    #[test]
    fn the_users_own_theme_uses_their_colours() {
        let colours = CustomTheme {
            background: Rgb::new(0x20, 0x10, 0x30),
            card: None,
            text: None,
        };
        let mut settings = AppSettings {
            theme: ThemeMode::Custom,
            ..AppSettings::default()
        };
        // Nothing chosen yet: it looks like Dark.
        assert_eq!(settings.palette(false).background, DARK.background);
        settings.prefs.custom_theme = Some(colours);
        assert_eq!(settings.palette(false).background, colours.background);
    }

    #[test]
    fn colours_survive_saving() {
        let prefs = Prefs {
            accent: Accent::Custom,
            custom_accent: Some(Rgb::new(1, 2, 3)),
            custom_theme: Some(CustomTheme {
                background: Rgb::new(4, 5, 6),
                card: Some(Rgb::new(7, 8, 9)),
                text: None,
            }),
            ..Prefs::default()
        };
        let again: Prefs = serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(again, prefs);
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
