use crate::alarm_ops;
use crate::app::{t, AlarmData, Route, Translations};
use crate::components::{
    BottomNav, Icon, IconName, Sheet, SheetDone, SheetItem, Switch, ToastMessage,
};
use crate::models::{AppSettings, SortMode};
use crate::schedule;
use chrono::{Duration, TimeZone};
use std::collections::HashMap;

use dioxus::prelude::*;

/// What the quick row offers, in minutes.
const QUICK_MINUTES: [i64; 5] = [5, 10, 15, 30, 60];

/// The panel open over the list.
#[derive(Clone, PartialEq)]
enum HomeSheet {
    Sort,
    PauseAll,
    /// A date for pausing: everything, or the alarm with this id.
    PauseDate(Option<String>),
    Menu(String),
}

/// "in 7 h 12 min".
fn countdown_text(translations: &Translations, now_ms: i64, at_ms: i64) -> String {
    let time = schedule::format_countdown(
        schedule::countdown(now_ms, at_ms),
        &t(translations, "home.next_in_dhm"),
        &t(translations, "home.next_in_hm"),
        &t(translations, "home.next_in_m"),
    );
    t(translations, "home.in").replace("{time}", &time)
}

fn local_date(at_ms: i64) -> Option<chrono::NaiveDate> {
    Some(
        chrono::DateTime::from_timestamp_millis(at_ms)?
            .with_timezone(&chrono::Local)
            .date_naive(),
    )
}

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

/// The alarms as the list shows them.
fn sorted_for_list(
    alarms: &[AlarmData],
    sort: SortMode,
    next: &HashMap<String, i64>,
) -> Vec<AlarmData> {
    let mut list = crate::ordering::sorted_for_display(alarms);
    list.retain(|a| !a.hidden);
    match sort {
        SortMode::Own => {}
        SortMode::Time => list.sort_by_key(|a| (a.time_hour, a.time_minute)),
        SortMode::NextRing => list.sort_by_key(|a| next.get(&a.id).copied().unwrap_or(i64::MAX)),
    }
    list
}

/// Start of this day, `days` from today, in unix ms.
fn start_of_day_ms(days: i64) -> Option<i64> {
    let day = chrono::Local::now().date_naive() + Duration::days(days);
    chrono::Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|at| at.timestamp_millis())
}

/// Home screen: the alarms and what can be done to them in one tap.
#[component]
pub fn Home() -> Element {
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let translations = use_context::<Translations>();
    let mut settings = use_context::<Signal<AppSettings>>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    let nav = use_navigator();

    // The countdown and the "Today / Tomorrow" labels follow the clock, so
    // re-render on a timer. What the phone allows can change meanwhile too.
    let mut tick = use_signal(|| 0u32);
    let mut problem = use_signal(crate::screens::reliability::first_problem);
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(15_000).await;
            tick += 1;
            problem.set(crate::screens::reliability::first_problem());
        }
    });
    let _ = tick();

    let sort = settings.read().prefs.sort;
    let twenty_four = crate::text::uses_24_hours(&settings.read());
    let week_start = settings.read().prefs.week_start;
    let defaults = settings.read().prefs.alarm_defaults.clone();

    // Hold an alarm and drag it to a new place in the list; the order is saved.
    crate::reorder::use_reorder(
        "alarm-list",
        "alarm-card",
        None,
        EventHandler::new(move |(from, to)| {
            if settings.peek().prefs.sort != SortMode::Own {
                return;
            }
            // Positions in the list, which leaves out the alarms it does not show.
            let mut list = alarms.write();
            let shown: Vec<String> = crate::ordering::sorted_for_display(&list)
                .into_iter()
                .filter(|a| !a.hidden)
                .map(|a| a.id)
                .collect();
            let position = |id: Option<&String>| {
                id.and_then(|id| {
                    crate::ordering::sorted_for_display(&list)
                        .iter()
                        .position(|a| a.id == *id)
                })
            };
            if let (Some(from), Some(to)) = (position(shown.get(from)), position(shown.get(to))) {
                if crate::ordering::apply_reorder(&mut list, from, to) {
                    drop(list);
                    crate::persistence::save_alarms(&alarms.read());
                }
            }
        }),
    );

    let mut sheet: Signal<Option<HomeSheet>> = use_signal(|| None);
    let mut pause_date = use_signal(|| {
        (chrono::Local::now().date_naive() + Duration::days(7))
            .format("%Y-%m-%d")
            .to_string()
    });

    // When each enabled alarm really rings next, from AlarmKit (it knows
    // the weekdays, a snooze in progress and one-time dates).
    let now_ms = chrono::Local::now().timestamp_millis();
    let next_fire: HashMap<String, i64> = {
        let kit = crate::platform::alarm_kit_optional();
        alarms
            .read()
            .iter()
            .filter(|a| a.enabled && !a.hidden)
            .filter_map(|a| {
                let at = kit?
                    .next_fire(&mobile_sentinel::InstanceId::new(a.id.clone()))
                    .ok()
                    .flatten()?;
                Some((a.id.clone(), at))
            })
            .collect()
    };
    let list = sorted_for_list(&alarms.read(), sort, &next_fire);
    let has_alarms = !list.is_empty();
    let paused_until = alarm_ops::all_paused_until(&alarms.read(), now_ms);

    let next_ms = schedule::earliest_future(now_ms, next_fire.values().copied());
    let (next_title, next_sub) = match next_ms {
        Some(at) => {
            let local = chrono::DateTime::from_timestamp_millis(at)
                .map(|at| at.with_timezone(&chrono::Local));
            let title = local
                .map(|at| {
                    use chrono::Timelike;
                    format!(
                        "{} {}",
                        crate::text::day_name(&translations, at.date_naive()),
                        crate::text::clock(at.hour() as u8, at.minute() as u8, twenty_four)
                    )
                })
                .unwrap_or_default();
            (title, countdown_text(&translations, now_ms, at))
        }
        None => (t(&translations, "home.none_on"), String::new()),
    };

    // Change the list, tell the user, and offer to take it back.
    let mut change = move |message: String, apply: &mut dyn FnMut(&mut Vec<AlarmData>)| {
        let before = alarms.peek().clone();
        apply(&mut alarms.write());
        crate::persistence::save_alarms(&alarms.peek());
        toast.set(Some(ToastMessage::with_undo(message, before)));
    };

    let next_label = t(&translations, "home.next_label");
    let pause_aria = t(&translations, "home.pause_aria");
    let sort_aria = t(&translations, "home.sort_aria");
    let menu_aria = t(&translations, "home.menu_aria");
    let create_aria = t(&translations, "home.create_aria");
    let quick_label = t(&translations, "home.quick");

    let sheet_view = match sheet() {
        None => rsx! {},
        Some(HomeSheet::Sort) => {
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
        Some(HomeSheet::PauseAll) => rsx! {
            Sheet {
                title: t(&translations, "home.pause_title"),
                note: t(&translations, "home.pause_note"),
                on_close: move |_| sheet.set(None),
                if paused_until.is_some() {
                    SheetItem {
                        label: t(&translations, "home.pause_end"),
                        on_click: move |_| {
                            let message = t(&translations, "home.pause_ended");
                            change(message, &mut |list| alarm_ops::resume_all(list));
                            sheet.set(None);
                        },
                    }
                }
                SheetItem {
                    label: t(&translations, "home.pause_tomorrow"),
                    on_click: move |_| {
                        if let Some(until) = start_of_day_ms(1) {
                            let message = paused_message(&translations, until);
                            change(message, &mut |list| alarm_ops::pause_all(list, until));
                        }
                        sheet.set(None);
                    },
                }
                SheetItem {
                    label: t(&translations, "home.pause_week"),
                    on_click: move |_| {
                        if let Some(until) = start_of_day_ms(7) {
                            let message = paused_message(&translations, until);
                            change(message, &mut |list| alarm_ops::pause_all(list, until));
                        }
                        sheet.set(None);
                    },
                }
                SheetItem {
                    label: t(&translations, "home.pause_date"),
                    on_click: move |_| sheet.set(Some(HomeSheet::PauseDate(None))),
                }
            }
        },
        Some(HomeSheet::PauseDate(only)) => {
            let min = chrono::Local::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string();
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
                                change(message, &mut |list| match &only {
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
        Some(HomeSheet::Menu(id)) => {
            let found = alarms.read().iter().find(|a| a.id == id).cloned();
            match found {
                None => rsx! {},
                Some(alarm) => {
                    let skip_active = alarm.skip_until_ms.is_some_and(|skip| skip > now_ms);
                    let pause_active = alarm.pause_until_ms.is_some_and(|until| until > now_ms);
                    let next = next_fire.get(&id).copied();
                    let can_skip = alarm.enabled
                        && !schedule::is_one_time(&alarm)
                        && (skip_active || next.is_some());
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
                                        let message = if skip_active {
                                            t(&translations, "home.skip_undone")
                                        } else {
                                            t(&translations, "home.skipped")
                                        };
                                        let id = id_skip.clone();
                                        change(message, &mut |list| {
                                            // Skipping remembers the occurrence to skip (the
                                            // next ring); undoing clears it.
                                            if let Some(alarm) = list.iter_mut().find(|a| a.id == id) {
                                                alarm.skip_until_ms = if skip_active { None } else { next };
                                            }
                                        });
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
                                        change(message, &mut |list| {
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
                                    let message = if name.is_empty() {
                                        t(&translations, "home.deleted_plain")
                                    } else {
                                        t(&translations, "home.deleted").replace("{alarm}", &name)
                                    };
                                    let id = id_delete.clone();
                                    change(message, &mut |list| list.retain(|a| a.id != id));
                                    sheet.set(None);
                                },
                            }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "screen",
            header { class: "home-header",
                div {
                    p { class: "home-next-label", "{next_label}" }
                    h1 { class: "home-next", "{next_title}" }
                    if !next_sub.is_empty() {
                        p { class: "home-next-sub", "{next_sub}" }
                    }
                }
                div { class: "home-actions",
                    button {
                        class: if paused_until.is_some() { "round-icon-btn on" } else { "round-icon-btn" },
                        "aria-label": "{pause_aria}",
                        onclick: move |_| sheet.set(Some(HomeSheet::PauseAll)),
                        Icon { name: IconName::Pause, size: 20 }
                    }
                    button {
                        class: "round-icon-btn",
                        "aria-label": "{sort_aria}",
                        onclick: move |_| sheet.set(Some(HomeSheet::Sort)),
                        Icon { name: IconName::Sort, size: 20 }
                    }
                }
            }

            if let Some(check) = problem() {
                {
                    let (name, _) = crate::screens::reliability::check_text(&translations, check);
                    let title = t(&translations, "reliability.banner").replace("{check}", &name);
                    let fix = t(&translations, "reliability.fix");
                    rsx! {
                        button {
                            class: "banner",
                            onclick: move |_| { nav.push(Route::Reliability {}); },
                            Icon { name: IconName::Warning, size: 18 }
                            span { class: "banner-text", "{title}" }
                            span { class: "banner-action", "{fix}" }
                        }
                    }
                }
            }

            div { class: "quick-row",
                span { class: "quick-label", "{quick_label}" }
                for minutes in QUICK_MINUTES {
                    button {
                        class: "chip-slim",
                        onclick: {
                            let defaults = defaults.clone();
                            move |_| {
                                if alarms.peek().len() >= alarm_ops::MAX_ALARMS {
                                    toast.set(Some(ToastMessage::error(t(&translations, "alarm.limit_reached"))));
                                    return;
                                }
                                let label = t(&translations, "home.quick_alarm");
                                let message = t(&translations, "home.quick_set").replace("{minutes}", &minutes.to_string());
                                let now = chrono::Local::now().naive_local();
                                let mut alarm = alarm_ops::quick_alarm(now, minutes, label, defaults.as_ref());
                                alarm.order = crate::ordering::order_for_new(&alarms.peek());
                                change(message, &mut |list| list.push(alarm.clone()));
                            }
                        },
                        "+{minutes}"
                    }
                }
            }

            div { class: "scroll scroll-fab",
                if has_alarms {
                    ul { id: "alarm-list", class: "list alarm-list",
                        for alarm in list.iter() {
                            {
                                let open_id = alarm.id.clone();
                                let menu_id = alarm.id.clone();
                                let toggle_id = alarm.id.clone();
                                let time = crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four);
                                let label = alarm.label.clone();
                                let when = when_text(&translations, alarm, next_fire.get(&alarm.id).copied(), now_ms);
                                let on = alarm.enabled;
                                let days = alarm.days;
                                let one_time = schedule::is_one_time(alarm);
                                let switch_aria = t(&translations, "home.switch_aria");
                                let name = alarm.label.clone();
                                rsx! {
                                    li { key: "{alarm.id}", class: if on { "alarm-card" } else { "alarm-card off" },
                                        // 12 hours take more room, and all the times stay in line.
                                        button {
                                            class: "alarm-main",
                                            onclick: move |_| { nav.push(Route::AlarmConfig { id: open_id.clone() }); },
                                            span { class: if twenty_four { "alarm-time" } else { "alarm-time h12" }, "{time}" }
                                            span { class: "alarm-info",
                                                span { class: "alarm-label", "{label}" }
                                                span { class: "alarm-when",
                                                    span { "{when}" }
                                                    if !one_time {
                                                        span { class: "alarm-days",
                                                            for day in week_start.day_order() {
                                                                span {
                                                                    class: if days[day] && on { "alarm-day on" } else { "alarm-day" },
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
                                            "aria-label": "{switch_aria} {name}",
                                            onclick: move |_| {
                                                let now = chrono::Local::now().naive_local();
                                                let id = toggle_id.clone();
                                                let mut message = String::new();
                                                change(String::new(), &mut |list| {
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
                                }
                            }
                        }
                    }
                } else {
                    div { class: "empty",
                        div { class: "empty-icon", Icon { name: IconName::Clock, size: 40 } }
                        p { class: "empty-title", {t(&translations, "alarm.no_alarms")} }
                        p { class: "empty-sub", {t(&translations, "alarm.tap_to_create")} }
                        for (key, hour, days) in [
                            ("home.template_weekdays", 7u8, [true, true, true, true, true, false, false]),
                            ("home.template_weekend", 9u8, [false, false, false, false, false, true, true]),
                        ] {
                            button {
                                class: "btn empty-template",
                                onclick: {
                                    let defaults = defaults.clone();
                                    move |_| {
                                        let mut alarm = alarm_ops::new_alarm(defaults.as_ref());
                                        alarm.id = uuid::Uuid::new_v4().to_string();
                                        alarm.time_hour = hour;
                                        alarm.time_minute = 0;
                                        alarm.days = days;
                                        alarm.label = t(&translations, key);
                                        alarm.order = crate::ordering::order_for_new(&alarms.peek());
                                        alarms.write().push(alarm);
                                        crate::persistence::save_alarms(&alarms.peek());
                                    }
                                },
                                {t(&translations, key)}
                            }
                        }
                    }
                }
            }

            button {
                class: "fab",
                "aria-label": "{create_aria}",
                onclick: move |_| {
                    if alarms.peek().len() >= alarm_ops::MAX_ALARMS {
                        toast.set(Some(ToastMessage::error(t(&translations, "alarm.limit_reached"))));
                        return;
                    }
                    nav.push(Route::AlarmConfig { id: "new".to_string() });
                },
                Icon { name: IconName::Plus, size: 24 }
            }

            BottomNav { current: Route::Home {} }

            {sheet_view}
        }
    }
}

fn paused_message(translations: &Translations, until_ms: i64) -> String {
    let date = local_date(until_ms).unwrap_or_default();
    t(translations, "home.paused_message")
        .replace("{date}", &crate::text::day_month(translations, date))
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
