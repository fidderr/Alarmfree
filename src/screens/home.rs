use crate::app::{t, AlarmData, Route, Translations};
use crate::components::{Icon, IconName};
use chrono::{Local, Timelike};
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

    // Calculate actual time until next enabled alarm
    let next_alarm_text = {
        let alarm_list = alarms.read();
        let now = Local::now();
        let now_minutes = now.hour() * 60 + now.minute();
        let next = alarm_list
            .iter()
            .filter(|a| a.enabled)
            .map(|a| {
                let alarm_minutes = (a.time_hour as u32) * 60 + (a.time_minute as u32);
                if alarm_minutes > now_minutes {
                    alarm_minutes - now_minutes
                } else {
                    // Alarm is tomorrow (or later this week, simplified)
                    (24 * 60) - now_minutes + alarm_minutes
                }
            })
            .min();

        match next {
            Some(minutes) => {
                let h = minutes / 60;
                let m = minutes % 60;
                let time_str = if h > 0 {
                    t(&translations, "home.next_in_hm")
                        .replace("{hours}", &h.to_string())
                        .replace("{minutes}", &format!("{:02}", m))
                } else {
                    t(&translations, "home.next_in_m").replace("{minutes}", &m.to_string())
                };
                time_until_template.replace("{time}", &time_str)
            }
            None => String::new(),
        }
    };

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
                   div { class: "alarm-list",
                       for alarm in alarms.read().iter() {
                           {
                               let alarm_id = alarm.id.clone();
                               let alarm_id2 = alarm.id.clone();
                               let time = format!("{:02}:{:02}", alarm.time_hour, alarm.time_minute);
                               let label = alarm.label.clone();
                               let schedule = format_schedule(&alarm.days, &translations);
                               let enabled = alarm.enabled;
                               rsx! {
                                   div {
                                       class: "alarm-card",
                                       onclick: move |_| {
                                           nav.push(Route::AlarmConfig { id: alarm_id.clone() });
                                       },
                                       div { class: "alarm-card-left",
                                           div { class: "alarm-time", "{time}" }
                                           div { class: "alarm-details",
                                               span { class: "alarm-label", "{label}" }
                                               span { class: "alarm-schedule", "{schedule}" }
                                           }
                                       }
                                       div { class: "alarm-card-right",
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
