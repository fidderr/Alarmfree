//! The alarms and what can be done to them in one tap.

mod card;
mod sheets;

use std::collections::HashMap;

use chrono::Duration;
use dioxus::prelude::*;

use card::AlarmCard;
use sheets::{AlarmMenuSheet, PauseAllSheet, PauseDateSheet, SortSheet};

use crate::alarm_ops;
use crate::app::{t, AlarmData, Route, Translations};
use crate::components::{BottomNav, Icon, IconName, ToastMessage};
use crate::models::{AppSettings, SortMode};
use crate::schedule;

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

/// What the screen and its parts share.
#[derive(Clone, Copy)]
struct HomeState {
    alarms: Signal<Vec<AlarmData>>,
    toast: Signal<Option<ToastMessage>>,
    translations: Translations,
    sheet: Signal<Option<HomeSheet>>,
    pause_date: Signal<String>,
}

impl HomeState {
    /// Change the list, tell the user, and offer to take it back.
    fn change(mut self, message: String, apply: &mut dyn FnMut(&mut Vec<AlarmData>)) {
        let before = self.alarms.peek().clone();
        apply(&mut self.alarms.write());
        crate::persistence::save_alarms(&self.alarms.peek());
        self.toast
            .set(Some(ToastMessage::with_undo(message, before)));
    }

    fn toggle_skip(self, id: String, next: Option<i64>, skip_active: bool) {
        let message = if skip_active {
            t(&self.translations, "home.skip_undone")
        } else {
            t(&self.translations, "home.skipped")
        };
        self.change(message, &mut |list| {
            // Skipping remembers the occurrence to skip (the next ring); undoing clears it.
            if let Some(alarm) = list.iter_mut().find(|a| a.id == id) {
                alarm.skip_until_ms = if skip_active { None } else { next };
            }
        });
    }

    fn delete_alarm(self, id: String, name: &str) {
        let message = if name.is_empty() {
            t(&self.translations, "home.deleted_plain")
        } else {
            t(&self.translations, "home.deleted").replace("{alarm}", name)
        };
        self.change(message, &mut |list| list.retain(|a| a.id != id));
    }
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
    let first_problem =
        move || crate::screens::reliability::first_problem(&settings.peek().prefs.hidden_warnings);
    let mut problem = use_signal(first_problem);
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(15_000).await;
            tick += 1;
            problem.set(first_problem());
        }
    });
    let _ = tick();

    let sort = settings.read().prefs.sort;
    let twenty_four = crate::text::uses_24_hours(&settings.read());
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
    let pause_date = use_signal(|| {
        (chrono::Local::now().date_naive() + Duration::days(7))
            .format("%Y-%m-%d")
            .to_string()
    });
    let state = HomeState {
        alarms,
        toast,
        translations,
        sheet,
        pause_date,
    };
    use_context_provider(|| state);

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

    let next_label = t(&translations, "home.next_label");
    let pause_aria = t(&translations, "home.pause_aria");
    let sort_aria = t(&translations, "home.sort_aria");
    let create_aria = t(&translations, "home.create_aria");
    let quick_label = t(&translations, "home.quick");

    let sheet_view = match sheet() {
        None => rsx! {},
        Some(HomeSheet::Sort) => rsx! { SortSheet {} },
        Some(HomeSheet::PauseAll) => rsx! { PauseAllSheet {} },
        Some(HomeSheet::PauseDate(only)) => rsx! { PauseDateSheet { only } },
        Some(HomeSheet::Menu(id)) => {
            rsx! { AlarmMenuSheet { next: next_fire.get(&id).copied(), id } }
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
                    let hide = t(&translations, "reliability.hide");
                    rsx! {
                        div { class: "banner-row",
                            button {
                                class: "banner",
                                onclick: move |_| { nav.push(Route::Reliability {}); },
                                Icon { name: IconName::Warning, size: 18 }
                                span { class: "banner-text", "{title}" }
                                span { class: "banner-action", "{fix}" }
                            }
                            if crate::screens::reliability::can_hide(check) {
                                button {
                                    class: "icon-btn",
                                    "aria-label": "{hide}",
                                    onclick: move |_| {
                                        settings
                                            .write()
                                            .prefs
                                            .hidden_warnings
                                            .push(crate::screens::reliability::check_name(check).to_owned());
                                        crate::persistence::save_settings(&settings.peek());
                                        problem.set(first_problem());
                                    },
                                    Icon { name: IconName::Xmark, size: 16 }
                                }
                            }
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
                                state.change(message, &mut |list| list.push(alarm.clone()));
                            }
                        },
                        "+{minutes}"
                    }
                }
            }

            div { class: "scroll scroll-fab",
                if has_alarms {
                    // Dragging only means something in the user's own order.
                    ul { id: if sort == SortMode::Own { "alarm-list" } else { "alarm-list-sorted" }, class: "list alarm-list",
                        for alarm in list.iter() {
                            AlarmCard {
                                key: "{alarm.id}",
                                alarm: alarm.clone(),
                                next: next_fire.get(&alarm.id).copied(),
                                now_ms,
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
