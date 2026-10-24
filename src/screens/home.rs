use crate::app::{t, AlarmData, Route, Translations};
use crate::components::{Icon, IconName};
use crate::schedule;
use std::collections::HashMap;

use dioxus::prelude::*;

/// Translation keys for day abbreviations.
const DAY_KEYS: &[&str] = &[
    "day.mon", "day.tue", "day.wed", "day.thu", "day.fri", "day.sat", "day.sun",
];

/// Home screen displaying all alarms with toggle controls.
/// Reads alarm data from shared global context.
#[component]
pub fn Home() -> Element {
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let translations = use_context::<Translations>();
    let nav = use_navigator();
    let has_alarms = !alarms.read().is_empty();

    let app_name = t(&translations, "app.name");
    let no_alarms_text = t(&translations, "alarm.no_alarms");
    let tap_to_create_text = t(&translations, "alarm.tap_to_create");
    let time_until_template = t(&translations, "alarm.time_until");

    // The countdown and the "Today / Tomorrow" labels follow the clock, so
    // re-render on a timer. Reading `tick` below subscribes this component.
    let mut tick = use_signal(|| 0u32);
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(15_000).await;
            tick += 1;
        }
    });
    let _ = tick();

    // Hold an alarm and drag it to a new place in the list; the order is saved.
    crate::reorder::use_reorder(
        "alarm-list",
        "alarm-card",
        None,
        EventHandler::new(move |(from, to)| {
            if crate::ordering::apply_reorder(&mut alarms.write(), from, to) {
                crate::persistence::save_alarms(&alarms.read());
            }
        }),
    );

    // The "more actions" sheet: which alarm it is open for, and whether it is
    // asking to confirm a delete.
    let mut menu_for: Signal<Option<String>> = use_signal(|| None);
    let mut confirm_delete = use_signal(|| false);
    let menu_aria = t(&translations, "home.menu_aria");
    let duplicate_text = t(&translations, "action.duplicate");
    let delete_text = t(&translations, "action.delete");
    let cancel_text = t(&translations, "action.cancel");
    let delete_confirm_text = t(&translations, "alarm.delete_confirm");
    let delete_warning_text = t(&translations, "alarm.delete_warning");

    // When each enabled alarm really rings next, from AlarmKit (it knows
    // the weekdays, a snooze in progress and one-time dates).
    let now = chrono::Local::now();
    let now_ms = now.timestamp_millis();
    let next_fire: HashMap<String, i64> = {
        let kit = crate::platform::alarm_kit_optional();
        alarms
            .read()
            .iter()
            .filter(|a| a.enabled)
            .filter_map(|a| {
                let at = kit?
                    .next_fire(&mobile_sentinel::InstanceId::new(a.id.clone()))
                    .ok()
                    .flatten()?;
                Some((a.id.clone(), at))
            })
            .collect()
    };

    let next_alarm_text = match schedule::earliest_future(now_ms, next_fire.values().copied()) {
        Some(at) => {
            let time_str = schedule::format_countdown(
                schedule::countdown(now_ms, at),
                &t(&translations, "home.next_in_dhm"),
                &t(&translations, "home.next_in_hm"),
                &t(&translations, "home.next_in_m"),
            );
            time_until_template.replace("{time}", &time_str)
        }
        None => String::new(),
    };

    // Alarms in clock order.
    let sorted: Vec<AlarmData> = crate::ordering::sorted_for_display(&alarms.read());

    // What the actions sheet can do for the alarm it is open for.
    let menu_alarm = menu_for().and_then(|id| alarms.read().iter().find(|a| a.id == id).cloned());
    let skip_active = menu_alarm
        .as_ref()
        .is_some_and(|a| a.skip_until_ms.is_some_and(|s| s > now_ms));
    let menu_next_fire = menu_alarm
        .as_ref()
        .and_then(|a| next_fire.get(&a.id).copied());
    let can_skip = menu_alarm
        .as_ref()
        .is_some_and(|a| a.enabled && !schedule::is_one_time(a))
        && (skip_active || menu_next_fire.is_some());
    let skip_text = if skip_active {
        t(&translations, "action.undo_skip")
    } else {
        t(&translations, "action.skip_next")
    };
    let skipped_text = t(&translations, "home.skipped");
    let practice_text = t(&translations, "action.practice");

    let nav_alarms = t(&translations, "nav.alarms");
    let nav_history = t(&translations, "nav.history");
    let nav_settings = t(&translations, "nav.settings");
    let create_aria = t(&translations, "home.create_aria");

    rsx! {
           div { class: "screen home-screen",
    // Header
               div { class: "header",
                   h1 { "{app_name}" }
                   if has_alarms {
                       p { class: "next-alarm", "{next_alarm_text}" }
                   }
               }

    // Alarm list or empty state
               if has_alarms {
                   div { id: "alarm-list", class: "alarm-list",
                       for alarm in sorted.iter() {
                           {
                               let alarm_id = alarm.id.clone();
                               let alarm_id2 = alarm.id.clone();
                               let alarm_id3 = alarm.id.clone();
                               let time = format!("{:02}:{:02}", alarm.time_hour, alarm.time_minute);
                               let label = alarm.label.clone();
                               let schedule_text = format_schedule(&alarm.days, &translations);
                               let skipped = alarm.skip_until_ms.is_some_and(|s| s > now_ms);
                               let next_ring = next_fire
                                   .get(&alarm.id)
                                   .and_then(|&at| ring_day_label(at, !schedule::is_one_time(alarm), &translations));
                               let enabled = alarm.enabled;
                               rsx! {
                                   div {
                                       class: if enabled { "alarm-card" } else { "alarm-card disabled" },
                                       onclick: move |_| {
                                           nav.push(Route::AlarmConfig { id: alarm_id.clone() });
                                       },
                                       div { class: "alarm-card-left",
                                           div { class: "alarm-time", "{time}" }
                                           div { class: "alarm-details",
                                               span { class: "alarm-label", "{label}" }
                                               span { class: "alarm-schedule", "{schedule_text}" }
                                               if let Some(next_ring) = next_ring {
                                                   span { class: "alarm-next-ring", "{next_ring}" }
                                               }
                                               if skipped {
                                                   span { class: "alarm-skipped", "{skipped_text}" }
                                               }
                                           }
                                       }
                                       div { class: "alarm-card-right",
                                           button {
                                               class: "card-menu-btn",
                                               "aria-label": "{menu_aria}",
                                               onclick: move |evt| {
                                                   evt.stop_propagation();
                                                   confirm_delete.set(false);
                                                   menu_for.set(Some(alarm_id3.clone()));
                                               },
                                               Icon { name: IconName::EllipsisVertical, size: 18 }
                                           }
                                           label {
                                               class: "toggle-switch",
                                               onclick: move |evt| { evt.stop_propagation(); },
                                               input {
                                                   r#type: "checkbox",
                                                   checked: enabled,
                                                   onchange: move |_| {
                                                       let mut list = alarms.write();
                                                       if let Some(a) = list.iter_mut().find(|a| a.id == alarm_id2) {
                                                           a.enabled = !a.enabled;
                                                           let now_enabled = a.enabled;
                                                           let alarm_copy = a.clone();
                                                           drop(list);

    // Persist to disk
                                                           crate::persistence::save_alarms(&alarms.read());

                                                           {
                                                               if now_enabled {
                                                                   crate::platform::schedule_alarm_from_ui(&alarm_copy);
                                                               } else {
                                                                   crate::platform::cancel_alarm_from_ui(&alarm_copy.id);
                                                               }
                                                           }
                                                       }
                                                   },
                                               }
                                               span { class: "toggle-slider" }
                                           }
                                       }
                                   }
                               }
                           }
                       }
                   }
               } else {
                   div { class: "empty-state",
                       div { class: "empty-icon", Icon { name: IconName::Clock } }
                       p { class: "empty-title", "{no_alarms_text}" }
                       p { class: "empty-subtitle", "{tap_to_create_text}" }
                   }
               }

    // More-actions sheet for one alarm
               if let Some(menu_id) = menu_for() {
                   div {
                       class: "sheet-overlay",
                       onclick: move |_| { menu_for.set(None); confirm_delete.set(false); },
                       div {
                           class: "sheet",
                           onclick: move |evt| { evt.stop_propagation(); },
                           if confirm_delete() {
                               h3 { "{delete_confirm_text}" }
                               p { "{delete_warning_text}" }
                               div { class: "confirm-actions",
                                   button {
                                       class: "btn-cancel",
                                       onclick: move |_| { menu_for.set(None); confirm_delete.set(false); },
                                       "{cancel_text}"
                                   }
                                   button {
                                       class: "btn-delete",
                                       onclick: {
                                           let menu_id = menu_id.clone();
                                           move |_| {
                                               crate::platform::cancel_alarm_from_ui(&menu_id);
                                               let unused_sound = crate::app::remove_alarm(&mut alarms.write(), &menu_id);
                                               if let Some(sound) = unused_sound {
                                                   crate::custom_sounds::delete_sound(&sound);
                                               }
                                               crate::persistence::save_alarms(&alarms.read());
                                               menu_for.set(None);
                                               confirm_delete.set(false);
                                           }
                                       },
                                       "{delete_text}"
                                   }
                               }
                           } else {
                               if menu_alarm.as_ref().is_some_and(|a| !a.challenges.is_empty()) {
                                   button {
                                       class: "sheet-item",
                                       onclick: {
                                           let menu_id = menu_id.clone();
                                           move |_| {
                                               menu_for.set(None);
                                               nav.push(Route::Practice { id: menu_id.clone() });
                                           }
                                       },
                                       "{practice_text}"
                                   }
                               }
                               if can_skip {
                                   button {
                                       class: "sheet-item",
                                       onclick: {
                                           let menu_id = menu_id.clone();
                                           move |_| {
                                               // Skipping remembers the occurrence to skip (the
                                               // next ring); undoing clears it.
                                               if let Some(alarm) = alarms.write().iter_mut().find(|a| a.id == menu_id) {
                                                   alarm.skip_until_ms = if skip_active { None } else { menu_next_fire };
                                               }
                                               crate::persistence::save_alarms(&alarms.read());
                                               menu_for.set(None);
                                           }
                                       },
                                       "{skip_text}"
                                   }
                               }
                               button {
                                   class: "sheet-item",
                                   onclick: {
                                       let menu_id = menu_id.clone();
                                       move |_| {
                                           menu_for.set(None);
                                           nav.push(Route::AlarmConfig { id: format!("copy-{menu_id}") });
                                       }
                                   },
                                   "{duplicate_text}"
                               }
                               button {
                                   class: "sheet-item danger",
                                   onclick: move |_| { confirm_delete.set(true); },
                                   "{delete_text}"
                               }
                           }
                       }
                   }
               }

    // Add alarm FAB
               button {
                   class: "fab",
                   "aria-label": "{create_aria}",
                   onclick: move |_| {
                       nav.push(Route::AlarmConfig { id: "new".to_string() });
                   },
                   Icon { name: IconName::Plus }
               }

    // Bottom navigation
               nav { class: "bottom-nav",
                   Link { to: Route::Home {}, class: "nav-item active",
                       Icon { name: IconName::Clock }
                       " {nav_alarms}"
                   }
                   Link { to: Route::History {}, class: "nav-item",
                       Icon { name: IconName::ChartBar }
                       " {nav_history}"
                   }
                   Link { to: Route::Settings {}, class: "nav-item",
                       Icon { name: IconName::Gear }
                       " {nav_settings}"
                   }
               }
           }
       }
}

/// Format the days array into a human-readable schedule string.
fn format_schedule(days: &[bool; 7], translations: &Translations) -> String {
    let day_labels: Vec<String> = DAY_KEYS.iter().map(|k| t(translations, k)).collect();
    let selected: Vec<&String> = days
        .iter()
        .enumerate()
        .filter(|(_, &d)| d)
        .map(|(i, _)| &day_labels[i])
        .collect();

    if selected.is_empty() {
        t(translations, "config.one_time")
    } else if selected.len() == 7 {
        t(translations, "config.every_day")
    } else if selected.len() == 5
        && days[0]
        && days[1]
        && days[2]
        && days[3]
        && days[4]
        && !days[5]
        && !days[6]
    {
        t(translations, "config.weekdays")
    } else {
        selected
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>()
            .join(", ")
    }
}

/// "Today", "Tomorrow" or the weekday an alarm next rings on (the card
/// already shows its clock time).
fn ring_day_label(at_ms: i64, repeating: bool, translations: &Translations) -> Option<String> {
    use chrono::Local;
    let ring_day = chrono::DateTime::from_timestamp_millis(at_ms)?
        .with_timezone(&Local)
        .date_naive();
    let today = Local::now().date_naive();
    let label = schedule::day_label(today, ring_day);
    // A repeating alarm already lists its weekdays on the card, so a bare
    // weekday adds nothing; Today and Tomorrow still do.
    if repeating && matches!(label, schedule::DayLabel::Weekday(_)) {
        return None;
    }
    Some(match label {
        schedule::DayLabel::Today => t(translations, "home.today"),
        schedule::DayLabel::Tomorrow => t(translations, "home.tomorrow"),
        schedule::DayLabel::Weekday(day) => {
            t(translations, DAY_KEYS[day.num_days_from_monday() as usize])
        }
    })
}
