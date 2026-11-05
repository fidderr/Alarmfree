//! The editor's small sheets: one or two settings, or a question.

use chrono::Duration;
use dioxus::prelude::*;

use super::EditorState;
use crate::app::{t, AlarmData, Translations};
use crate::components::time_picker::TimePicker;
use crate::components::{Chips, GroupTitle, Sheet, SheetDone, SheetItem, Stepper, ToastMessage};
use crate::models::AppSettings;

const LIMIT_MINUTES: [u32; 4] = [30, 60, 90, 120];
const CHECK_MINUTES: [u8; 4] = [0, 3, 5, 10];
const NOTICE_MINUTES: [u8; 5] = [0, 5, 10, 30, 60];

/// A time typed in by hand: "7", "730", "07:30", "7.30".
fn parse_typed_time(text: &str) -> Option<(u8, u8)> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let separated = text.contains([':', '.', ' ', 'h']);
    let (hour, minute) = match digits.len() {
        1 | 2 if !separated => (digits.parse().ok()?, 0),
        3 => (digits[..1].parse().ok()?, digits[1..].parse().ok()?),
        4 => (digits[..2].parse().ok()?, digits[2..].parse().ok()?),
        _ => return None,
    };
    (hour < 24 && minute < 60).then_some((hour, minute))
}

/// The next length of a snooze: by the minute up to ten, by five after that, so an hour
/// is not sixty taps away.
fn longer_or_shorter(minutes: u8, longer: bool) -> u8 {
    let next = match (longer, minutes) {
        (true, 0..=9) => minutes + 1,
        (true, _) => minutes.saturating_add(5),
        (false, 0..=10) => minutes.saturating_sub(1),
        (false, _) => minutes - 5,
    };
    next.clamp(1, 60)
}

/// The time, on wheels or typed in.
#[component]
pub(super) fn TimeSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        mut typed_time,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let settings = use_context::<Signal<AppSettings>>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
    rsx! {
        Sheet {
            title: t(&translations, "config.time"),
            on_close: move |_| sheet.set(None),
            TimePicker {
                hour: current.time_hour,
                minute: current.time_minute,
                twenty_four: crate::text::uses_24_hours(&settings.read()),
                on_change: move |(h, m)| {
                    let mut alarm = draft.write();
                    alarm.time_hour = h;
                    alarm.time_minute = m;
                },
            }
            label { class: "field-label",
                {t(&translations, "config.time_type")}
                input {
                    class: "field",
                    r#type: "text",
                    inputmode: "numeric",
                    placeholder: "07:00",
                    value: "{typed_time}",
                    oninput: move |evt| {
                        let text = evt.value();
                        if let Some((h, m)) = parse_typed_time(&text) {
                            let mut alarm = draft.write();
                            alarm.time_hour = h;
                            alarm.time_minute = m;
                        }
                        typed_time.set(text);
                    },
                }
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}

/// The date of an alarm that rings once.
#[component]
pub(super) fn DateSheet() -> Element {
    let EditorState {
        mut sheet,
        mut date_value,
        mut error,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let done_text = t(&translations, "action.done");
    let today = chrono::Local::now().date_naive();
    let quick = [
        ("home.today", today),
        ("home.tomorrow", today + Duration::days(1)),
        ("config.next_week", today + Duration::days(7)),
    ];
    rsx! {
        Sheet {
            title: t(&translations, "config.date_title"),
            help: t(&translations, "config.date_note"),
            on_close: move |_| sheet.set(None),
            label { class: "field-label",
                {t(&translations, "config.date")}
                input {
                    class: "field",
                    r#type: "date",
                    min: "{today}",
                    value: "{date_value}",
                    onchange: move |evt| {
                        date_value.set(evt.value());
                        error.set(String::new());
                    },
                }
            }
            GroupTitle { text: t(&translations, "config.quick") }
            Chips::<chrono::NaiveDate> {
                grow: true,
                options: quick.iter().map(|(key, day)| (*day, t(&translations, key))).collect::<Vec<_>>(),
                selected: chrono::NaiveDate::parse_from_str(&date_value.read(), "%Y-%m-%d").ok(),
                on_pick: move |day: chrono::NaiveDate| {
                    date_value.set(day.format("%Y-%m-%d").to_string());
                    error.set(String::new());
                },
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}

/// Its name, and the note shown while it rings.
#[component]
pub(super) fn LabelSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        ..
    } = use_context();
    let alarms = use_context::<Signal<Vec<AlarmData>>>();
    let translations = use_context::<Translations>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
    let mut used: Vec<String> = alarms
        .read()
        .iter()
        .filter(|a| !a.hidden && !a.label.is_empty())
        .map(|a| a.label.clone())
        .collect();
    used.sort();
    used.dedup();
    rsx! {
        Sheet {
            title: t(&translations, "config.label"),
            help: t(&translations, "config.label_note"),
            on_close: move |_| sheet.set(None),
            label { class: "field-label",
                {t(&translations, "config.label_name")}
                input {
                    class: "field",
                    r#type: "text",
                    maxlength: "100",
                    placeholder: t(&translations, "config.label_placeholder"),
                    value: "{current.label}",
                    oninput: move |evt| draft.write().label = evt.value(),
                }
            }
            label { class: "field-label",
                {t(&translations, "config.note")}
                input {
                    class: "field",
                    r#type: "text",
                    maxlength: "100",
                    placeholder: t(&translations, "config.note_placeholder"),
                    value: "{current.note}",
                    oninput: move |evt| draft.write().note = evt.value(),
                }
            }
            if !used.is_empty() {
                GroupTitle { text: t(&translations, "config.used_before") }
                Chips::<String> {
                    options: used.into_iter().map(|l| (l.clone(), l)).collect::<Vec<_>>(),
                    selected: Some(current.label.clone()),
                    on_pick: move |label: String| draft.write().label = label,
                }
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}

/// How long and how often it can be snoozed.
#[component]
pub(super) fn SnoozeSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
    rsx! {
        Sheet {
            title: t(&translations, "config.snooze"),
            help: t(&translations, "config.snooze_note"),
            on_close: move |_| sheet.set(None),
            Stepper {
                title: t(&translations, "config.snooze_interval"),
                value: t(&translations, "text.minutes").replace("{minutes}", &current.snooze_interval.to_string()),
                on_less: move |_| {
                    let mut alarm = draft.write();
                    alarm.snooze_interval = longer_or_shorter(alarm.snooze_interval, false);
                },
                on_more: move |_| {
                    let mut alarm = draft.write();
                    alarm.snooze_interval = longer_or_shorter(alarm.snooze_interval, true);
                },
            }
            Stepper {
                title: t(&translations, "config.snooze_count"),
                value: crate::text::counted(&translations, "text.times", u32::from(current.snooze_count)),
                on_less: move |_| {
                    let mut alarm = draft.write();
                    alarm.snooze_count = alarm.snooze_count.saturating_sub(1);
                },
                on_more: move |_| {
                    let mut alarm = draft.write();
                    alarm.snooze_count = alarm.snooze_count.saturating_add(1).min(10);
                },
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}

/// How long it rings with nobody doing anything.
#[component]
pub(super) fn LimitSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
    let mut options: Vec<(Option<u32>, String)> = LIMIT_MINUTES
        .iter()
        .map(|m| {
            (
                Some(m * 60),
                t(&translations, "text.minutes").replace("{minutes}", &m.to_string()),
            )
        })
        .collect();
    options.push((None, t(&translations, "config.limit_forever")));
    rsx! {
        Sheet {
            title: t(&translations, "config.ring_limit"),
            help: t(&translations, "config.limit_note"),
            on_close: move |_| sheet.set(None),
            Chips::<Option<u32>> {
                grow: true,
                options,
                selected: Some(current.ring_limit_secs),
                on_pick: move |limit: Option<u32>| draft.write().ring_limit_secs = limit,
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}

/// How long after it is over to check the user is up.
#[component]
pub(super) fn WakeCheckSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
    rsx! {
        Sheet {
            title: t(&translations, "config.wake_check"),
            help: t(&translations, "config.wake_check_note"),
            on_close: move |_| sheet.set(None),
            Chips::<u8> {
                grow: true,
                options: CHECK_MINUTES
                    .iter()
                    .map(|m| (*m, if *m == 0 {
                        t(&translations, "config.fade_off")
                    } else {
                        t(&translations, "text.minutes").replace("{minutes}", &m.to_string())
                    }))
                    .collect::<Vec<_>>(),
                selected: Some(current.wake_check_minutes),
                on_pick: move |minutes: u8| draft.write().wake_check_minutes = minutes,
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}

/// How long before it rings a silent notice comes, from which the ring can be skipped.
#[component]
pub(super) fn NoticeSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
    rsx! {
        Sheet {
            title: t(&translations, "config.notice"),
            help: t(&translations, "config.notice_note"),
            on_close: move |_| sheet.set(None),
            Chips::<u8> {
                grow: true,
                options: NOTICE_MINUTES
                    .iter()
                    .map(|m| (*m, if *m == 0 {
                        t(&translations, "config.fade_off")
                    } else {
                        t(&translations, "text.minutes").replace("{minutes}", &m.to_string())
                    }))
                    .collect::<Vec<_>>(),
                selected: Some(current.notify_before_minutes),
                on_pick: move |minutes: u8| draft.write().notify_before_minutes = minutes,
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}

/// Deleting the alarm, after asking.
#[component]
pub(super) fn DeleteSheet(id: String) -> Element {
    let EditorState {
        mut sheet, preview, ..
    } = use_context();
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    let translations = use_context::<Translations>();
    let nav = use_navigator();
    let delete_id = id.clone();
    rsx! {
        Sheet {
            title: t(&translations, "alarm.delete_confirm"),
            on_close: move |_| sheet.set(None),
            SheetItem {
                label: t(&translations, "action.delete"),
                danger: true,
                on_click: move |_| {
                    preview.stop();
                    let before = alarms.peek().clone();
                    let name = before
                        .iter()
                        .find(|a| a.id == delete_id)
                        .map(|a| a.label.clone())
                        .unwrap_or_default();
                    alarms.write().retain(|a| a.id != delete_id);
                    crate::persistence::save_alarms(&alarms.peek());
                    let message = if name.is_empty() {
                        t(&translations, "home.deleted_plain")
                    } else {
                        t(&translations, "home.deleted").replace("{alarm}", &name)
                    };
                    toast.set(Some(ToastMessage::with_undo(message, before)));
                    nav.go_back();
                },
            }
            SheetDone { label: t(&translations, "alarm.keep"), on_click: move |_| sheet.set(None) }
        }
    }
}

/// Leaving with changes, after asking.
#[component]
pub(super) fn DiscardSheet() -> Element {
    let EditorState {
        mut sheet, preview, ..
    } = use_context();
    let translations = use_context::<Translations>();
    let nav = use_navigator();
    rsx! {
        Sheet {
            title: t(&translations, "config.discard_title"),
            on_close: move |_| sheet.set(None),
            SheetItem {
                label: t(&translations, "config.discard"),
                danger: true,
                on_click: move |_| {
                    preview.stop();
                    nav.go_back();
                },
            }
            SheetDone { label: t(&translations, "config.keep_editing"), on_click: move |_| sheet.set(None) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{longer_or_shorter, parse_typed_time};

    #[test]
    fn a_snooze_grows_by_the_minute_then_by_five() {
        assert_eq!(longer_or_shorter(1, false), 1);
        assert_eq!(longer_or_shorter(1, true), 2);
        assert_eq!(longer_or_shorter(9, true), 10);
        assert_eq!(longer_or_shorter(10, true), 15);
        assert_eq!(longer_or_shorter(15, false), 10);
        assert_eq!(longer_or_shorter(10, false), 9);
        assert_eq!(longer_or_shorter(60, true), 60);
        assert_eq!(longer_or_shorter(12, true), 17);
    }

    #[test]
    fn a_time_can_be_typed_in_the_ways_people_type_it() {
        assert_eq!(parse_typed_time("7"), Some((7, 0)));
        assert_eq!(parse_typed_time("07"), Some((7, 0)));
        assert_eq!(parse_typed_time("730"), Some((7, 30)));
        assert_eq!(parse_typed_time("0730"), Some((7, 30)));
        assert_eq!(parse_typed_time("07:30"), Some((7, 30)));
        assert_eq!(parse_typed_time("7:30"), Some((7, 30)));
        assert_eq!(parse_typed_time("7.30"), Some((7, 30)));
        assert_eq!(parse_typed_time("23:59"), Some((23, 59)));
        assert_eq!(parse_typed_time("24:00"), None);
        assert_eq!(parse_typed_time("7:75"), None);
        assert_eq!(parse_typed_time(""), None);
        assert_eq!(parse_typed_time("12345"), None);
    }
}
