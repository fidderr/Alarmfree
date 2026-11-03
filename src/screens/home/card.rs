//! One alarm on the list.

use chrono::TimeZone;
use dioxus::prelude::*;

use super::{local_date, HomeSheet, HomeState};
use crate::app::{t, AlarmData, Route, Translations};
use crate::components::{Icon, IconName, Switch};
use crate::models::AppSettings;
use crate::schedule;

/// What an alarm's card says about when it rings.
fn when_text(
    translations: &Translations,
    alarm: &AlarmData,
    next_fire: Option<i64>,
    now_ms: i64,
) -> String {
    if !alarm.enabled {
        return t(translations, "home.off");
    }
    if let Some(until) = alarm.pause_until_ms.filter(|until| *until > now_ms) {
        return local_date(until)
            .map(|date| {
                t(translations, "home.paused_until")
                    .replace("{date}", &crate::text::day_month(translations, date))
            })
            .unwrap_or_default();
    }
    if alarm.skip_until_ms.is_some_and(|skip| skip > now_ms) {
        if let Some(date) = alarm.skip_until_ms.and_then(local_date) {
            return t(translations, "home.skips")
                .replace("{day}", &crate::text::day_name(translations, date));
        }
    }
    let Some(date) = next_fire.and_then(local_date) else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    match schedule::day_label(today, date) {
        schedule::DayLabel::Today => t(translations, "home.today"),
        schedule::DayLabel::Tomorrow => t(translations, "home.tomorrow"),
        schedule::DayLabel::Weekday(_) if schedule::is_one_time(alarm) => {
            crate::text::short_date(translations, date)
        }
        schedule::DayLabel::Weekday(_) => crate::text::day_name(translations, date),
    }
}

/// "Rings in 7 h 12 min", for an alarm switched on just now.
fn rings_in(translations: &Translations, alarm: &AlarmData) -> String {
    let now = chrono::Local::now();
    let date = alarm
        .date
        .as_deref()
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    schedule::next_ring_moment(
        alarm.time_hour,
        alarm.time_minute,
        alarm.days,
        date,
        now.naive_local(),
    )
    .and_then(|moment| chrono::Local.from_local_datetime(&moment).earliest())
    .map(|at| {
        let time = schedule::format_countdown(
            schedule::countdown(now.timestamp_millis(), at.timestamp_millis()),
            &t(translations, "home.next_in_dhm"),
            &t(translations, "home.next_in_hm"),
            &t(translations, "home.next_in_m"),
        );
        t(translations, "config.rings_in").replace("{time}", &time)
    })
    .unwrap_or_default()
}

/// An alarm: its time, when it rings, its menu and switch, and behind it what swiping it
/// aside shows.
#[component]
pub(super) fn AlarmCard(alarm: AlarmData, next: Option<i64>, now_ms: i64) -> Element {
    let state = use_context::<HomeState>();
    let HomeState {
        mut toast,
        translations,
        mut sheet,
        ..
    } = state;
    let settings = use_context::<Signal<AppSettings>>();
    let nav = use_navigator();

    let twenty_four = crate::text::uses_24_hours(&settings.read());
    let week_start = settings.read().prefs.week_start;
    let open_id = alarm.id.clone();
    let menu_id = alarm.id.clone();
    let toggle_id = alarm.id.clone();
    let skip_id = alarm.id.clone();
    let delete_id = alarm.id.clone();
    let delete_name = alarm.label.clone();
    let time = crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four);
    let when = when_text(&translations, &alarm, next, now_ms);
    let on = alarm.enabled;
    let one_time = schedule::is_one_time(&alarm);
    let skip_active = alarm.skip_until_ms.is_some_and(|skip| skip > now_ms);
    let can_skip = on && !one_time && (skip_active || next.is_some());
    let menu_aria = t(&translations, "home.menu_aria");
    let switch_aria = t(&translations, "home.switch_aria");
    let skip_aria = if skip_active {
        t(&translations, "action.undo_skip")
    } else {
        t(&translations, "action.skip_next")
    };
    let delete_aria = t(&translations, "action.delete");

    rsx! {
        li { class: if on { "alarm-card" } else { "alarm-card off" },
            div { class: "alarm-face",
                // 12 hours take more room, and all the times stay in line.
                button {
                    class: "alarm-main hold-to-drag",
                    onclick: move |_| { nav.push(Route::AlarmConfig { id: open_id.clone() }); },
                    span { class: if twenty_four { "alarm-time" } else { "alarm-time h12" }, "{time}" }
                    span { class: "alarm-info",
                        span { class: "alarm-label", "{alarm.label}" }
                        span { class: "alarm-when",
                            span { "{when}" }
                            if !one_time {
                                span { class: "alarm-days",
                                    for day in week_start.day_order() {
                                        span {
                                            class: if alarm.days[day] && on { "alarm-day on" } else { "alarm-day" },
                                            "{crate::text::day_letter(&translations, day)}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                button {
                    class: "alarm-menu",
                    "aria-label": "{menu_aria}",
                    onclick: move |_| sheet.set(Some(HomeSheet::Menu(menu_id.clone()))),
                    Icon { name: IconName::EllipsisVertical, size: 18 }
                }
                button {
                    class: "alarm-switch",
                    role: "switch",
                    "aria-checked": if on { "true" } else { "false" },
                    "aria-label": "{switch_aria} {alarm.label}",
                    onclick: move |_| {
                        let now = chrono::Local::now().naive_local();
                        let id = toggle_id.clone();
                        let mut message = String::new();
                        state.change(String::new(), &mut |list| {
                            if let Some(alarm) = list.iter_mut().find(|a| a.id == id) {
                                alarm.enabled = !alarm.enabled;
                                if alarm.enabled {
                                    schedule::refresh_one_time_date(alarm, now);
                                    message = rings_in(&translations, alarm);
                                } else {
                                    message = t(&translations, "home.switched_off");
                                }
                            }
                        });
                        if let Some(toast_now) = toast.write().as_mut() {
                            toast_now.message = message;
                        }
                    },
                    Switch { on }
                }
            }
            // Behind the card: swiping it aside shows these.
            div { class: "alarm-swipe",
                if can_skip {
                    button {
                        class: "swipe-action",
                        "aria-label": "{skip_aria}",
                        onclick: move |_| {
                            state.toggle_skip(skip_id.clone(), next, skip_active);
                            // The card stays: back to showing the alarm.
                            let _ = document::eval("document.querySelectorAll('.alarm-card').forEach(function (c) { c.scrollTo({ left: 0, behavior: 'smooth' }); });");
                        },
                        Icon { name: IconName::Skip, size: 20 }
                    }
                }
                button {
                    class: "swipe-action danger",
                    "aria-label": "{delete_aria}",
                    onclick: move |_| state.delete_alarm(delete_id.clone(), &delete_name),
                    Icon { name: IconName::Trash, size: 20 }
                }
            }
        }
    }
}
