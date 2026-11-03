//! The panels over the settings.

use dioxus::prelude::*;

use super::{format_name, notify_name, theme_name, volume_name, week_name, SettingsState};
use crate::alarm_ops;
use crate::app::{t, AlarmData, Translations};
use crate::components::{Chips, GroupTitle, Sheet, SheetDone, SheetItem, ToastMessage};
use crate::i18n;
use crate::models::{Accent, Locale, ThemeMode, TimeFormat, WeekStart};

/// What "Before an alarm" offers, in minutes.
const NOTIFY_MINUTES: [u8; 4] = [0, 5, 10, 30];

/// The percentages for the minimum volume.
const VOLUMES: [u8; 5] = [0, 25, 50, 75, 100];

#[component]
pub(super) fn ThemeSheet() -> Element {
    let state = use_context::<SettingsState>();
    let SettingsState {
        settings,
        mut sheet,
    } = state;
    let translations = use_context::<Translations>();
    let now = settings.read().clone();
    rsx! {
        Sheet {
            title: t(&translations, "settings.theme"),
            on_close: move |_| sheet.set(None),
            for mode in [ThemeMode::System, ThemeMode::Dark, ThemeMode::Light, ThemeMode::Black, ThemeMode::Colorblind] {
                SheetItem {
                    label: theme_name(mode, &translations),
                    selected: now.theme == mode,
                    on_click: move |_| {
                        state.update(&mut |s| s.theme = mode);
                        sheet.set(None);
                    },
                }
            }
            if now.theme != ThemeMode::Colorblind {
                GroupTitle { text: t(&translations, "settings.accent") }
                div { class: "swatches",
                    for accent in Accent::ALL {
                        button {
                            class: if now.prefs.accent == accent { "swatch selected {accent.css_class()}" } else { "swatch {accent.css_class()}" },
                            "aria-label": t(&translations, accent.name_key()),
                            "aria-pressed": if now.prefs.accent == accent { "true" } else { "false" },
                            onclick: move |_| state.update(&mut |s| s.prefs.accent = accent),
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn LanguageSheet() -> Element {
    let state = use_context::<SettingsState>();
    let SettingsState {
        settings,
        mut sheet,
    } = state;
    let mut translations = use_context::<Translations>();
    let now = settings.read().clone();
    rsx! {
        Sheet {
            title: t(&translations, "settings.language"),
            on_close: move |_| sheet.set(None),
            SheetItem {
                label: t(&translations, "settings.follow_phone"),
                selected: now.prefs.language_follows_phone,
                on_click: move |_| {
                    let locale = Locale::from_language_code(&mobile_sentinel::device_language()).unwrap_or_default();
                    state.update(&mut |s| {
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
                    selected: !now.prefs.language_follows_phone && now.locale == locale,
                    on_click: move |_| {
                        state.update(&mut |s| {
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
    }
}

#[component]
pub(super) fn TimeFormatSheet() -> Element {
    let state = use_context::<SettingsState>();
    let SettingsState {
        settings,
        mut sheet,
    } = state;
    let translations = use_context::<Translations>();
    let selected = settings.read().prefs.time_format;
    rsx! {
        Sheet {
            title: t(&translations, "settings.time_format"),
            on_close: move |_| sheet.set(None),
            for format in [TimeFormat::System, TimeFormat::H24, TimeFormat::H12] {
                SheetItem {
                    label: format_name(format, &translations),
                    selected: selected == format,
                    on_click: move |_| {
                        state.update(&mut |s| s.prefs.time_format = format);
                        sheet.set(None);
                    },
                }
            }
        }
    }
}

#[component]
pub(super) fn WeekStartSheet() -> Element {
    let state = use_context::<SettingsState>();
    let SettingsState {
        settings,
        mut sheet,
    } = state;
    let translations = use_context::<Translations>();
    let selected = settings.read().prefs.week_start;
    rsx! {
        Sheet {
            title: t(&translations, "settings.week_start"),
            on_close: move |_| sheet.set(None),
            for start in [WeekStart::Monday, WeekStart::Sunday, WeekStart::Saturday] {
                SheetItem {
                    label: week_name(start, &translations),
                    selected: selected == start,
                    on_click: move |_| {
                        state.update(&mut |s| s.prefs.week_start = start);
                        sheet.set(None);
                    },
                }
            }
        }
    }
}

/// How long before an alarm a notification comes.
#[component]
pub(super) fn NotifySheet() -> Element {
    let state = use_context::<SettingsState>();
    let SettingsState {
        settings,
        mut sheet,
    } = state;
    let translations = use_context::<Translations>();
    let selected = settings.read().prefs.notify_before_minutes;
    rsx! {
        Sheet {
            title: t(&translations, "settings.notify_before"),
            note: t(&translations, "settings.notify_before_note"),
            on_close: move |_| sheet.set(None),
            Chips::<u8> {
                grow: true,
                options: NOTIFY_MINUTES.iter().map(|m| (*m, notify_name(*m, &translations))).collect::<Vec<_>>(),
                selected: Some(selected),
                on_pick: move |minutes: u8| state.update(&mut |s| s.prefs.notify_before_minutes = minutes),
            }
            SheetDone { label: t(&translations, "action.done"), on_click: move |_| sheet.set(None) }
        }
    }
}

#[component]
pub(super) fn VolumeSheet() -> Element {
    let state = use_context::<SettingsState>();
    let SettingsState {
        settings,
        mut sheet,
    } = state;
    let translations = use_context::<Translations>();
    let selected = settings.read().min_volume_percent;
    rsx! {
        Sheet {
            title: t(&translations, "settings.min_volume"),
            note: t(&translations, "settings.min_volume_desc"),
            on_close: move |_| sheet.set(None),
            Chips::<u8> {
                grow: true,
                options: VOLUMES.iter().map(|p| (*p, volume_name(*p, &translations))).collect::<Vec<_>>(),
                selected: Some(selected),
                on_pick: move |percent: u8| {
                    state.update(&mut |s| s.min_volume_percent = percent);
                    crate::platform::set_min_volume(percent);
                },
            }
            SheetDone { label: t(&translations, "action.done"), on_click: move |_| sheet.set(None) }
        }
    }
}

/// Saving the alarms to a file, and taking them back from one.
#[component]
pub(super) fn BackupSheet() -> Element {
    let SettingsState { mut sheet, .. } = use_context();
    let translations = use_context::<Translations>();
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    rsx! {
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
                                let message = crate::text::counted(&translations, "settings.backup_restored", added as u32);
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
    }
}

#[component]
pub(super) fn AboutSheet() -> Element {
    let SettingsState { mut sheet, .. } = use_context();
    let translations = use_context::<Translations>();
    rsx! {
        Sheet {
            title: format!("AlarmFree {}", env!("CARGO_PKG_VERSION")),
            note: t(&translations, "app.description"),
            on_close: move |_| sheet.set(None),
            p { class: "about-line", {t(&translations, "app.tagline")} }
            SheetDone { label: t(&translations, "action.done"), on_click: move |_| sheet.set(None) }
        }
    }
}

/// Write the alarms to a file in Downloads. Returns where it went.
fn save_backup(alarms: &[AlarmData]) -> Result<String, String> {
    let name = format!("alarmfree-{}.json", chrono::Local::now().format("%Y-%m-%d"));
    crate::platform::save_to_downloads(&alarm_ops::export_json(alarms), &name, "application/json")
}

/// Let the user pick a backup and read the alarms from it. Blocks, so off the UI
/// thread. An empty error is the user changing their mind.
fn read_backup() -> Result<Vec<AlarmData>, String> {
    let picked = mobile_sentinel::media_picker::pick_files(
        &["application/json", "text/plain", "application/octet-stream"],
        false,
    )
    .map_err(|_| String::new())?;
    let Some(picked) = picked.into_iter().next() else {
        return Err(String::new());
    };
    let json = std::fs::read_to_string(&picked.path).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&picked.path);
    alarm_ops::import_json(
        &json?,
        crate::custom_sounds::sound_exists,
        chrono::Local::now().naive_local(),
    )
    .ok_or_else(|| "not a backup".to_owned())
}
