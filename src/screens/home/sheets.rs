//! The panels over the list.

use chrono::{Duration, TimeZone};
use dioxus::prelude::*;

use super::{local_date, HomeSheet, HomeState};
use crate::alarm_ops;
use crate::app::{t, AlarmData, Route, Translations};
use crate::components::{Sheet, SheetDone, SheetItem};
use crate::models::{AppSettings, SortMode};
use crate::schedule;

/// Start of this day, `days` from today, in unix ms.
fn start_of_day_ms(days: i64) -> Option<i64> {
    let day = chrono::Local::now().date_naive() + Duration::days(days);
    chrono::Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|at| at.timestamp_millis())
}

fn paused_message(translations: &Translations, until_ms: i64) -> String {
    let date = local_date(until_ms).unwrap_or_default();
    t(translations, "home.paused_message")
        .replace("{date}", &crate::text::day_month(translations, date))
}

/// The order of the list.
#[component]
pub(super) fn SortSheet() -> Element {
    let HomeState {
        mut sheet,
        translations,
        ..
    } = use_context();
    let mut settings = use_context::<Signal<AppSettings>>();
    let sort = settings.read().prefs.sort;
    let options = [
        (SortMode::Own, "home.sort_own"),
        (SortMode::Time, "home.sort_time"),
        (SortMode::NextRing, "home.sort_next"),
    ];
    rsx! {
        Sheet {
            title: t(&translations, "home.sort_title"),
            note: t(&translations, "home.sort_note"),
            on_close: move |_| sheet.set(None),
            for (mode, key) in options {
                SheetItem {
                    label: t(&translations, key),
                    selected: sort == mode,
                    on_click: move |_| {
                        settings.write().prefs.sort = mode;
                        crate::persistence::save_settings(&settings.peek());
                        sheet.set(None);
                    },
                }
            }
        }
    }
}

/// Pausing all alarms, for a holiday.
#[component]
pub(super) fn PauseAllSheet() -> Element {
    let state = use_context::<HomeState>();
    let HomeState {
        alarms,
        mut sheet,
        translations,
        ..
    } = state;
    let now_ms = chrono::Local::now().timestamp_millis();
    let paused_until = alarm_ops::all_paused_until(&alarms.read(), now_ms);
    rsx! {
        Sheet {
            title: t(&translations, "home.pause_title"),
            note: t(&translations, "home.pause_note"),
            on_close: move |_| sheet.set(None),
            if paused_until.is_some() {
                SheetItem {
                    label: t(&translations, "home.pause_end"),
                    on_click: move |_| {
                        let message = t(&translations, "home.pause_ended");
                        state.change(message, &mut |list| alarm_ops::resume_all(list));
                        sheet.set(None);
                    },
                }
            }
            SheetItem {
                label: t(&translations, "home.pause_tomorrow"),
                on_click: move |_| {
                    if let Some(until) = start_of_day_ms(1) {
                        let message = paused_message(&translations, until);
                        state.change(message, &mut |list| alarm_ops::pause_all(list, until));
                    }
                    sheet.set(None);
                },
            }
            SheetItem {
                label: t(&translations, "home.pause_week"),
                on_click: move |_| {
                    if let Some(until) = start_of_day_ms(7) {
                        let message = paused_message(&translations, until);
                        state.change(message, &mut |list| alarm_ops::pause_all(list, until));
                    }
                    sheet.set(None);
                },
            }
            SheetItem {
                label: t(&translations, "home.pause_date"),
                on_click: move |_| sheet.set(Some(HomeSheet::PauseDate(None))),
            }
        }
    }
}

/// Until which date to pause: everything, or `only` this alarm.
#[component]
pub(super) fn PauseDateSheet(only: Option<String>) -> Element {
    let state = use_context::<HomeState>();
    let HomeState {
        mut sheet,
        mut pause_date,
        translations,
        ..
    } = state;
    let now = chrono::Local::now();
    let now_ms = now.timestamp_millis();
    let min = now.date_naive().format("%Y-%m-%d").to_string();
    rsx! {
        Sheet {
            title: t(&translations, "home.pause_date"),
            note: t(&translations, "home.pause_date_note"),
            on_close: move |_| sheet.set(None),
            input {
                class: "field",
                r#type: "date",
                min: "{min}",
                value: "{pause_date}",
                onchange: move |evt| pause_date.set(evt.value()),
            }
            SheetDone {
                label: t(&translations, "home.pause_apply"),
                on_click: move |_| {
                    let picked = chrono::NaiveDate::parse_from_str(&pause_date.read(), "%Y-%m-%d")
                        .ok()
                        .and_then(|day| day.and_hms_opt(0, 0, 0))
                        .and_then(|at| chrono::Local.from_local_datetime(&at).earliest())
                        .map(|at| at.timestamp_millis());
                    if let Some(until) = picked.filter(|until| *until > now_ms) {
                        let message = paused_message(&translations, until);
                        let only = only.clone();
                        state.change(message, &mut |list| match &only {
                            Some(id) => {
                                if let Some(alarm) = list.iter_mut().find(|a| a.id == *id) {
                                    alarm.pause_until_ms = Some(until);
                                }
                            }
                            None => alarm_ops::pause_all(list, until),
                        });
                    }
                    sheet.set(None);
                },
            }
        }
    }
}

/// What else can be done to one alarm.
#[component]
pub(super) fn AlarmMenuSheet(id: String, next: Option<i64>) -> Element {
    let state = use_context::<HomeState>();
    let HomeState {
        alarms,
        mut sheet,
        translations,
        ..
    } = state;
    let settings = use_context::<Signal<AppSettings>>();
    let nav = use_navigator();

    let found: Option<AlarmData> = alarms.read().iter().find(|a| a.id == id).cloned();
    let Some(alarm) = found else {
        return rsx! {};
    };
    let now_ms = chrono::Local::now().timestamp_millis();
    let twenty_four = crate::text::uses_24_hours(&settings.read());
    let skip_active = alarm.skip_until_ms.is_some_and(|skip| skip > now_ms);
    let pause_active = alarm.pause_until_ms.is_some_and(|until| until > now_ms);
    let can_skip =
        alarm.enabled && !schedule::is_one_time(&alarm) && (skip_active || next.is_some());
    let title = format!(
        "{} {}",
        crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four),
        alarm.label
    );
    let name = alarm.label.clone();
    let (id_try, id_skip, id_copy, id_pause, id_delete) =
        (id.clone(), id.clone(), id.clone(), id.clone(), id.clone());
    rsx! {
        Sheet {
            title,
            note: t(&translations, "home.menu_note"),
            on_close: move |_| sheet.set(None),
            if !alarm.challenges.is_empty() {
                SheetItem {
                    label: t(&translations, "home.practice"),
                    on_click: move |_| {
                        sheet.set(None);
                        nav.push(Route::Practice { id: id_try.clone() });
                    },
                }
            }
            if can_skip {
                SheetItem {
                    label: if skip_active {
                        t(&translations, "action.undo_skip")
                    } else {
                        t(&translations, "action.skip_next")
                    },
                    on_click: move |_| {
                        state.toggle_skip(id_skip.clone(), next, skip_active);
                        sheet.set(None);
                    },
                }
            }
            SheetItem {
                label: t(&translations, "action.duplicate"),
                on_click: move |_| {
                    sheet.set(None);
                    nav.push(Route::AlarmConfig { id: format!("copy-{id_copy}") });
                },
            }
            SheetItem {
                label: if pause_active {
                    t(&translations, "home.pause_end")
                } else {
                    t(&translations, "home.pause_one")
                },
                on_click: move |_| {
                    if pause_active {
                        let message = t(&translations, "home.pause_ended");
                        let id = id_pause.clone();
                        state.change(message, &mut |list| {
                            if let Some(alarm) = list.iter_mut().find(|a| a.id == id) {
                                alarm.pause_until_ms = None;
                            }
                        });
                        sheet.set(None);
                    } else {
                        sheet.set(Some(HomeSheet::PauseDate(Some(id_pause.clone()))));
                    }
                },
            }
            SheetItem {
                label: t(&translations, "action.delete"),
                danger: true,
                on_click: move |_| {
                    state.delete_alarm(id_delete.clone(), &name);
                    sheet.set(None);
                },
            }
        }
    }
}
