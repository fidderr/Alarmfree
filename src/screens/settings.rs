use crate::alarm_ops;
use crate::app::{t, AlarmData, Route, Translations};
use crate::components::{
    BottomNav, Chips, GroupTitle, Icon, IconName, Row, Sheet, SheetDone, SheetItem, SwitchRow,
    ToastMessage,
};
use crate::i18n;
use crate::models::{Accent, AppSettings, Locale, ThemeMode, TimeFormat, WeekStart};
use dioxus::prelude::*;

/// What "Before an alarm" offers, in minutes.
const NOTIFY_MINUTES: [u8; 4] = [0, 5, 10, 30];

/// The percentages for the minimum volume.
const VOLUMES: [u8; 5] = [0, 25, 50, 75, 100];

/// The panel open over the settings.
#[derive(Clone, Copy, PartialEq)]
enum SettingsSheet {
    Theme,
    Language,
    TimeFormat,
    WeekStart,
    NotifyBefore,
    Volume,
    Backup,
    DeleteHistory,
    About,
}

/// Settings screen for app-wide preferences.
#[component]
pub fn Settings() -> Element {
    let mut settings = use_context::<Signal<AppSettings>>();
    let mut translations = use_context::<Translations>();
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    let nav = use_navigator();
    let mut sheet: Signal<Option<SettingsSheet>> = use_signal(|| None);

    let problems = use_hook(|| {
        mobile_sentinel::health::report(crate::platform::FOREGROUND_CHANNEL_ID)
            .problems()
            .len()
    });

    let mut update = move |change: &mut dyn FnMut(&mut AppSettings)| {
        change(&mut settings.write());
        crate::persistence::save_settings(&settings.peek());
        // The widget speaks the language and writes the time as set here.
        crate::platform::refresh_reminders();
    };
    let can_add_widget = use_hook(mobile_sentinel::home_widget::can_add);

    let now = settings.read().clone();
    let prefs = now.prefs.clone();
    let theme_name = |mode: ThemeMode, translations: &Translations| {
        t(
            translations,
            match mode {
                ThemeMode::System => "theme.system",
                ThemeMode::Dark => "theme.dark",
                ThemeMode::Light => "theme.light",
                ThemeMode::Black => "theme.black",
                ThemeMode::Colorblind => "theme.colorblind",
            },
        )
    };
    let format_name = |format: TimeFormat, translations: &Translations| {
        t(
            translations,
            match format {
                TimeFormat::System => "settings.follow_phone",
                TimeFormat::H24 => "settings.h24",
                TimeFormat::H12 => "settings.h12",
            },
        )
    };
    let week_name = |start: WeekStart, translations: &Translations| {
        t(
            translations,
            match start {
                WeekStart::Monday => "day.monday",
                WeekStart::Sunday => "day.sunday",
                WeekStart::Saturday => "day.saturday",
            },
        )
    };
    let language_name = if prefs.language_follows_phone {
        t(&translations, "settings.follow_phone")
    } else {
        now.locale.display_name().to_owned()
    };
    let notify_name = |minutes: u8, translations: &Translations| {
        if minutes == 0 {
            t(translations, "config.fade_off")
        } else {
            t(translations, "text.minutes").replace("{minutes}", &minutes.to_string())
        }
    };
    let volume_name = |percent: u8, translations: &Translations| {
        if percent == 0 {
            t(translations, "settings.volume_off")
        } else {
            format!("{percent}%")
        }
    };
    let done_text = t(&translations, "action.done");
    let version_text = format!("AlarmFree {}", env!("CARGO_PKG_VERSION"));

    let sheet_view = match sheet() {
        None => rsx! {},
        Some(SettingsSheet::Theme) => rsx! {
            Sheet {
                title: t(&translations, "settings.theme"),
                on_close: move |_| sheet.set(None),
                for mode in [ThemeMode::System, ThemeMode::Dark, ThemeMode::Light, ThemeMode::Black, ThemeMode::Colorblind] {
                    SheetItem {
                        label: theme_name(mode, &translations),
                        selected: now.theme == mode,
                        on_click: move |_| {
                            update(&mut |s| s.theme = mode);
                            sheet.set(None);
                        },
                    }
                }
                if now.theme != ThemeMode::Colorblind {
                    GroupTitle { text: t(&translations, "settings.accent") }
                    div { class: "swatches",
                        for accent in Accent::ALL {
                            button {
                                class: if prefs.accent == accent { "swatch selected {accent.css_class()}" } else { "swatch {accent.css_class()}" },
                                "aria-label": t(&translations, accent.name_key()),
                                "aria-pressed": if prefs.accent == accent { "true" } else { "false" },
                                onclick: move |_| update(&mut |s| s.prefs.accent = accent),
                            }
                        }
                    }
                }
            }
        },
        Some(SettingsSheet::Language) => rsx! {
            Sheet {
                title: t(&translations, "settings.language"),
                on_close: move |_| sheet.set(None),
                SheetItem {
                    label: t(&translations, "settings.follow_phone"),
                    selected: prefs.language_follows_phone,
                    on_click: move |_| {
                        let locale = Locale::from_language_code(&mobile_sentinel::device_language()).unwrap_or_default();
                        update(&mut |s| {
                            s.prefs.language_follows_phone = true;
                            s.locale = locale;
                        });
                        translations.set(i18n::load_translations(locale));
                        crate::platform::set_ring_language(locale);
                        sheet.set(None);
                    },
                }
                for locale in Locale::ALL {
                    SheetItem {
                        label: locale.display_name().to_owned(),
                        selected: !prefs.language_follows_phone && now.locale == locale,
                        on_click: move |_| {
                            update(&mut |s| {
                                s.prefs.language_follows_phone = false;
                                s.locale = locale;
                            });
                            translations.set(i18n::load_translations(locale));
                            crate::platform::set_ring_language(locale);
                            sheet.set(None);
                        },
                    }
                }
            }
        },
        Some(SettingsSheet::TimeFormat) => rsx! {
            Sheet {
                title: t(&translations, "settings.time_format"),
                on_close: move |_| sheet.set(None),
                for format in [TimeFormat::System, TimeFormat::H24, TimeFormat::H12] {
                    SheetItem {
                        label: format_name(format, &translations),
                        selected: prefs.time_format == format,
                        on_click: move |_| {
                            update(&mut |s| s.prefs.time_format = format);
                            sheet.set(None);
                        },
                    }
                }
            }
        },
        Some(SettingsSheet::WeekStart) => rsx! {
            Sheet {
                title: t(&translations, "settings.week_start"),
                on_close: move |_| sheet.set(None),
                for start in [WeekStart::Monday, WeekStart::Sunday, WeekStart::Saturday] {
                    SheetItem {
                        label: week_name(start, &translations),
                        selected: prefs.week_start == start,
                        on_click: move |_| {
                            update(&mut |s| s.prefs.week_start = start);
                            sheet.set(None);
                        },
                    }
                }
            }
        },
        Some(SettingsSheet::NotifyBefore) => rsx! {
            Sheet {
                title: t(&translations, "settings.notify_before"),
                note: t(&translations, "settings.notify_before_note"),
                on_close: move |_| sheet.set(None),
                Chips::<u8> {
                    grow: true,
                    options: NOTIFY_MINUTES.iter().map(|m| (*m, notify_name(*m, &translations))).collect::<Vec<_>>(),
                    selected: Some(prefs.notify_before_minutes),
                    on_pick: move |minutes: u8| {
                        update(&mut |s| s.prefs.notify_before_minutes = minutes);
                        crate::platform::refresh_reminders();
                    },
                }
                SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
            }
        },
        Some(SettingsSheet::Volume) => rsx! {
            Sheet {
                title: t(&translations, "settings.min_volume"),
                note: t(&translations, "settings.min_volume_desc"),
                on_close: move |_| sheet.set(None),
                Chips::<u8> {
                    grow: true,
                    options: VOLUMES.iter().map(|p| (*p, volume_name(*p, &translations))).collect::<Vec<_>>(),
                    selected: Some(now.min_volume_percent),
                    on_pick: move |percent: u8| {
                        update(&mut |s| s.min_volume_percent = percent);
                        crate::platform::set_min_volume(percent);
                    },
                }
                SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
            }
        },
        Some(SettingsSheet::Backup) => rsx! {
            Sheet {
                title: t(&translations, "settings.backup"),
                note: t(&translations, "settings.backup_note"),
                on_close: move |_| sheet.set(None),
                SheetItem {
                    label: t(&translations, "settings.backup_save"),
                    on_click: move |_| {
                        let message = match save_backup(&alarms.peek()) {
                            Ok(place) => t(&translations, "settings.backup_saved").replace("{place}", &place),
                            Err(e) => {
                                log::warn!("[AlarmFree] Backup failed: {e}");
                                t(&translations, "settings.backup_failed")
                            }
                        };
                        toast.set(Some(ToastMessage::info(message)));
                        sheet.set(None);
                    },
                }
                SheetItem {
                    label: t(&translations, "settings.backup_restore"),
                    on_click: move |_| {
                        sheet.set(None);
                        spawn(async move {
                            let (tx, rx) = futures_channel::oneshot::channel();
                            std::thread::spawn(move || {
                                let _ = tx.send(read_backup());
                            });
                            match rx.await {
                                Ok(Ok(found)) => {
                                    let before = alarms.peek().clone();
                                    let room = alarm_ops::MAX_ALARMS.saturating_sub(before.len());
                                    let added = found.len().min(room);
                                    {
                                        let mut list = alarms.write();
                                        let first = crate::ordering::order_for_new(&list);
                                        for (position, mut alarm) in found.into_iter().take(room).enumerate() {
                                            alarm.order = first + position as i64;
                                            list.push(alarm);
                                        }
                                    }
                                    crate::persistence::save_alarms(&alarms.peek());
                                    let message = t(&translations, "settings.backup_restored").replace("{count}", &added.to_string());
                                    toast.set(Some(ToastMessage::with_undo(message, before)));
                                }
                                Ok(Err(e)) if e.is_empty() => {}
                                Ok(Err(e)) => {
                                    log::warn!("[AlarmFree] Restoring failed: {e}");
                                    toast.set(Some(ToastMessage::error(t(&translations, "settings.backup_bad"))));
                                }
                                Err(_) => {}
                            }
                        });
                    },
                }
            }
        },
        Some(SettingsSheet::DeleteHistory) => rsx! {
            Sheet {
                title: t(&translations, "history.delete_history"),
                on_close: move |_| sheet.set(None),
                SheetItem {
                    label: t(&translations, "history.delete_30"),
                    on_click: move |_| {
                        crate::history_persistence::delete_history(Some(30));
                        sheet.set(None);
                    },
                }
                SheetItem {
                    label: t(&translations, "history.delete_90"),
                    on_click: move |_| {
                        crate::history_persistence::delete_history(Some(90));
                        sheet.set(None);
                    },
                }
                SheetItem {
                    label: t(&translations, "history.delete_all"),
                    danger: true,
                    on_click: move |_| {
                        crate::history_persistence::delete_history(None);
                        sheet.set(None);
                    },
                }
            }
        },
        Some(SettingsSheet::About) => rsx! {
            Sheet {
                title: version_text.clone(),
                note: t(&translations, "app.description"),
                on_close: move |_| sheet.set(None),
                p { class: "about-line", {t(&translations, "app.tagline")} }
                SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
            }
        },
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
            div { class: "scroll scroll-nav",
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
                    Row { title: t(&translations, "settings.theme"), value: theme_name(now.theme, &translations), on_click: move |_| sheet.set(Some(SettingsSheet::Theme)) }
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

                h2 { class: "list-title", {t(&translations, "settings.alarm_defaults")} }
                ul { class: "list",
                    Row {
                        title: t(&translations, "settings.new_alarms"),
                        value: t(&translations, "settings.edit"),
                        on_click: move |_| { nav.push(Route::AlarmConfig { id: "defaults".to_string() }); },
                    }
                    Row { title: t(&translations, "settings.notify_before"), value: notify_name(prefs.notify_before_minutes, &translations), on_click: move |_| sheet.set(Some(SettingsSheet::NotifyBefore)) }
                }

                h2 { class: "list-title", {t(&translations, "settings.ringing")} }
                ul { class: "list",
                    Row { title: t(&translations, "settings.min_volume"), value: volume_name(now.min_volume_percent, &translations), on_click: move |_| sheet.set(Some(SettingsSheet::Volume)) }
                    SwitchRow {
                        title: t(&translations, "settings.touch_feedback"),
                        on: prefs.touch_feedback,
                        on_toggle: move |on: bool| {
                            update(&mut |s| s.prefs.touch_feedback = on);
                            crate::feedback::set_enabled(on);
                        },
                    }
                    SwitchRow {
                        title: t(&translations, "settings.less_motion"),
                        on: prefs.less_motion,
                        on_toggle: move |on: bool| update(&mut |s| s.prefs.less_motion = on),
                    }
                    SwitchRow {
                        title: t(&translations, "settings.confetti"),
                        on: prefs.confetti,
                        on_toggle: move |on: bool| update(&mut |s| s.prefs.confetti = on),
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

/// Write the alarms to a file in Downloads. Returns where it went.
fn save_backup(alarms: &[AlarmData]) -> Result<String, String> {
    let json = alarm_ops::export_json(alarms);
    let path = mobile_sentinel::app_files_dir().join("alarmfree-backup.json");
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    let name = format!("alarmfree-{}.json", chrono::Local::now().format("%Y-%m-%d"));
    let result = mobile_sentinel::share::save_to_downloads(
        &path.to_string_lossy(),
        &name,
        "application/json",
    )
    .map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&path);
    result
}

/// Let the user pick a backup and read the alarms from it. Blocks, so off the UI
/// thread. An empty error is the user changing their mind.
fn read_backup() -> Result<Vec<AlarmData>, String> {
    let picked =
        mobile_sentinel::media_picker::pick_file(&["application/json", "text/plain", "*/*"])
            .map_err(|_| String::new())?;
    let json = std::fs::read_to_string(&picked).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&picked);
    alarm_ops::import_json(&json?, crate::custom_sounds::sound_exists)
        .ok_or_else(|| "not a backup".to_owned())
}
