//! App-wide preferences.

mod sheets;

use dioxus::prelude::*;

use sheets::{
    AboutSheet, BackupSheet, LanguageSheet, TimeFormatSheet, VolumeSheet, WeekStartSheet,
};

use crate::app::{t, Route, Translations};
use crate::components::{BottomNav, Icon, IconName, Row, SwitchRow};
use crate::models::{AppSettings, TimeFormat, WeekStart};
use crate::screens::history::DeleteHistorySheet;

/// The panel open over the settings.
#[derive(Clone, Copy, PartialEq)]
enum SettingsSheet {
    Language,
    TimeFormat,
    WeekStart,
    Volume,
    Backup,
    DeleteHistory,
    About,
}

/// What the screen and its sheets share.
#[derive(Clone, Copy)]
struct SettingsState {
    settings: Signal<AppSettings>,
    sheet: Signal<Option<SettingsSheet>>,
}

impl SettingsState {
    fn update(mut self, change: &mut dyn FnMut(&mut AppSettings)) {
        change(&mut self.settings.write());
        crate::persistence::save_settings(&self.settings.peek());
        // The widget speaks the language and writes the time as set here.
        crate::platform::refresh_outside_app();
    }
}

fn format_name(format: TimeFormat, translations: &Translations) -> String {
    t(
        translations,
        match format {
            TimeFormat::System => "settings.follow_phone",
            TimeFormat::H24 => "settings.h24",
            TimeFormat::H12 => "settings.h12",
        },
    )
}

fn week_name(start: WeekStart, translations: &Translations) -> String {
    t(
        translations,
        match start {
            WeekStart::Monday => "day.monday",
            WeekStart::Sunday => "day.sunday",
            WeekStart::Saturday => "day.saturday",
        },
    )
}

fn volume_name(percent: u8, translations: &Translations) -> String {
    if percent == 0 {
        t(translations, "settings.volume_off")
    } else {
        format!("{percent}%")
    }
}

/// Settings screen for app-wide preferences.
#[component]
pub fn Settings() -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let translations = use_context::<Translations>();
    let nav = use_navigator();
    let mut sheet: Signal<Option<SettingsSheet>> = use_signal(|| None);
    let state = SettingsState { settings, sheet };
    use_context_provider(|| state);

    let problems = use_hook(|| {
        mobile_sentinel::health::report(crate::platform::FOREGROUND_CHANNEL_ID)
            .problems()
            .len()
    });
    let can_add_widget = use_hook(mobile_sentinel::home_widget::can_add);

    let now = settings.read().clone();
    let prefs = now.prefs.clone();
    let language_name = if prefs.language_follows_phone {
        t(&translations, "settings.follow_phone")
    } else {
        now.locale.display_name().to_owned()
    };

    let sheet_view = match sheet() {
        None => rsx! {},
        Some(SettingsSheet::Language) => rsx! { LanguageSheet {} },
        Some(SettingsSheet::TimeFormat) => rsx! { TimeFormatSheet {} },
        Some(SettingsSheet::WeekStart) => rsx! { WeekStartSheet {} },
        Some(SettingsSheet::Volume) => rsx! { VolumeSheet {} },
        Some(SettingsSheet::Backup) => rsx! { BackupSheet {} },
        Some(SettingsSheet::DeleteHistory) => {
            rsx! { DeleteHistorySheet { on_close: move |_| sheet.set(None) } }
        }
        Some(SettingsSheet::About) => rsx! { AboutSheet {} },
    };

    let reliability_sub = if problems == 0 {
        t(&translations, "settings.reliability_fine")
    } else if problems == 1 {
        t(&translations, "settings.reliability_one")
    } else {
        t(&translations, "settings.reliability_many").replace("{count}", &problems.to_string())
    };

    rsx! {
        div { class: "screen",
            header { class: "main-header",
                h1 { class: "screen-title-large", {t(&translations, "settings.title")} }
            }
            div { class: "scroll",
                button {
                    class: if problems > 0 { "banner banner-tall" } else { "banner banner-tall fine" },
                    onclick: move |_| { nav.push(Route::Reliability {}); },
                    span { class: "banner-text",
                        span { class: "banner-title", {t(&translations, "reliability.title")} }
                        span { class: "banner-sub", "{reliability_sub}" }
                    }
                    span { class: "banner-action",
                        if problems > 0 {
                            {t(&translations, "settings.check")}
                        } else {
                            Icon { name: IconName::ChevronRight, size: 14 }
                        }
                    }
                }

                h2 { class: "list-title", {t(&translations, "settings.general")} }
                ul { class: "list",
                    Row { title: t(&translations, "settings.theme"), value: t(&translations, now.theme.name_key()), on_click: move |_| { nav.push(Route::ThemeScreen {}); } }
                    Row { title: t(&translations, "settings.language"), value: language_name, on_click: move |_| sheet.set(Some(SettingsSheet::Language)) }
                    Row { title: t(&translations, "settings.time_format"), value: format_name(prefs.time_format, &translations), on_click: move |_| sheet.set(Some(SettingsSheet::TimeFormat)) }
                    Row { title: t(&translations, "settings.week_start"), value: week_name(prefs.week_start, &translations), on_click: move |_| sheet.set(Some(SettingsSheet::WeekStart)) }
                    if can_add_widget {
                        Row {
                            title: t(&translations, "settings.add_widget"),
                            value: String::new(),
                            on_click: move |_| {
                                mobile_sentinel::home_widget::add();
                            },
                        }
                    }
                }

                h2 { class: "list-title", {t(&translations, "settings.alarms")} }
                ul { class: "list",
                    Row {
                        title: t(&translations, "settings.new_alarms"),
                        on_click: move |_| { nav.push(Route::AlarmConfig { id: "defaults".to_string() }); },
                    }
                    Row { title: t(&translations, "settings.min_volume"), value: volume_name(now.min_volume_percent, &translations), on_click: move |_| sheet.set(Some(SettingsSheet::Volume)) }
                }

                h2 { class: "list-title", {t(&translations, "settings.feel")} }
                ul { class: "list",
                    SwitchRow {
                        title: t(&translations, "settings.touch_feedback"),
                        on: prefs.touch_feedback,
                        on_toggle: move |on: bool| {
                            state.update(&mut |s| s.prefs.touch_feedback = on);
                            crate::feedback::set_enabled(on);
                        },
                    }
                    SwitchRow {
                        title: t(&translations, "settings.less_motion"),
                        on: prefs.less_motion,
                        on_toggle: move |on: bool| state.update(&mut |s| s.prefs.less_motion = on),
                    }
                    SwitchRow {
                        title: t(&translations, "settings.confetti"),
                        on: prefs.confetti,
                        on_toggle: move |on: bool| state.update(&mut |s| s.prefs.confetti = on),
                    }
                }

                h2 { class: "list-title", {t(&translations, "settings.data")} }
                ul { class: "list",
                    Row { title: t(&translations, "settings.backup"), on_click: move |_| sheet.set(Some(SettingsSheet::Backup)) }
                    Row { title: t(&translations, "history.delete_history"), on_click: move |_| sheet.set(Some(SettingsSheet::DeleteHistory)) }
                    Row { title: t(&translations, "settings.about"), value: env!("CARGO_PKG_VERSION").to_string(), on_click: move |_| sheet.set(Some(SettingsSheet::About)) }
                }
            }
            BottomNav { current: Route::Settings {} }
            {sheet_view}
        }
    }
}
