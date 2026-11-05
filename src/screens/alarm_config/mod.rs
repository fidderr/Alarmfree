//! Making and changing an alarm. The time and the days are right there; everything
//! else is one summary row that opens a sheet.

mod challenge_sheets;
mod simple_sheets;
mod sound_sheets;

use chrono::TimeZone;
use dioxus::prelude::*;

use challenge_sheets::{AddChallengeSheet, ChallengeDetailSheet, ChallengesSheet};
use simple_sheets::{
    DateSheet, DeleteSheet, DiscardSheet, LabelSheet, LimitSheet, NoticeSheet, SnoozeSheet,
    TimeSheet, WakeCheckSheet,
};
use sound_sheets::{SoundSheet, VibrationSheet, VIBRATION_KEYS};

use crate::alarm_ops;
use crate::app::{t, AlarmData, Translations};
use crate::back::use_back_handler;
use crate::components::{Icon, IconName, Row, ScreenHeader, SwitchRow, ToastMessage};
use crate::models::{AppSettings, ChallengeSetup};
use crate::screens::practice::ChallengeRunner;

/// How long trying a sound or a vibration lasts.
const PREVIEW_MS: u64 = 15_000;

/// The panel open over the editor.
#[derive(Clone, PartialEq)]
enum EditorSheet {
    Time,
    Date,
    Label,
    Sound,
    Vibration,
    Snooze,
    Challenges,
    AddChallenge,
    Detail(usize),
    Limit,
    WakeCheck,
    Notice,
    Delete,
    Discard,
}

/// A sound or a vibration being tried. The token makes an older timer leave a newer preview
/// alone.
#[derive(Clone, Copy)]
struct Preview {
    handle: Signal<u64>,
    token: Signal<u64>,
    sounding: Signal<bool>,
    vibrating: Signal<bool>,
}

impl Preview {
    fn stop(mut self) {
        stop_preview(*self.handle.peek());
        self.handle.set(0);
        if *self.vibrating.peek() {
            mobile_sentinel::haptics::cancel();
        }
        let token = *self.token.peek() + 1;
        self.token.set(token);
        self.sounding.set(false);
        self.vibrating.set(false);
    }

    /// Whichever panel started it: the timer goes with that panel, and so does what it stopped.
    fn stop_after_a_while(self) {
        let token = *self.token.peek();
        spawn(async move {
            crate::timer::sleep_ms(PREVIEW_MS).await;
            if *self.token.peek() == token {
                self.stop();
            }
        });
    }

    fn play(mut self, sound: &str) {
        self.stop();
        let handle = play_preview(sound);
        self.handle.set(handle);
        self.sounding.set(handle != 0);
        self.stop_after_a_while();
    }

    /// Not through the touch feedback: turning that off should not turn this off.
    fn vibrate(mut self, pattern: u8) {
        self.stop();
        let pattern = crate::platform::VIBRATION_PATTERNS
            [usize::from(pattern).min(crate::platform::VIBRATION_PATTERNS.len() - 1)];
        mobile_sentinel::haptics::vibrate_pattern(&crate::platform::repeated_pattern(
            pattern, PREVIEW_MS,
        ));
        self.vibrating.set(true);
        self.stop_after_a_while();
    }
}

/// What the editor and its sheets share.
#[derive(Clone, Copy)]
struct EditorState {
    draft: Signal<AlarmData>,
    /// An alarm with no weekday selected rings once, on a date.
    one_time: Signal<bool>,
    date_value: Signal<String>,
    setups: Signal<Vec<ChallengeSetup>>,
    sheet: Signal<Option<EditorSheet>>,
    error: Signal<String>,
    typed_time: Signal<String>,
    /// The user's own sound being renamed, and its name so far.
    renaming: Signal<Option<(String, String)>>,
    /// Challenges being tried; shown instead of the editor, which keeps its state.
    trying: Signal<Option<Vec<ChallengeSetup>>>,
    preview: Preview,
}

/// Stops whatever a sound preview started, also when the editor goes away.
fn stop_preview(handle_id: u64) {
    if handle_id != 0 {
        let handle = mobile_sentinel::PlaybackHandle::from_id(handle_id);
        let _ = mobile_sentinel::audio::stop(&handle);
    }
}

/// Play a sound via the gated audio capability. Returns the handle (0 on failure).
fn play_preview(sound: &str) -> u64 {
    let Some(path) = crate::custom_sounds::get_playback_path(sound) else {
        log::warn!("[AlarmFree] No playback path found for: {sound}");
        return 0;
    };
    // As the alarm will sound, and also with the media volume at zero. Round and round, as it
    // would ring: a short sound still lasts as long as the preview.
    match mobile_sentinel::audio::play_as_alarm(&path.to_string_lossy(), true) {
        Ok(handle) => handle.id(),
        Err(e) => {
            log::warn!("[AlarmFree] Preview play failed: {e}");
            0
        }
    }
}

fn seconds_label(translations: &Translations, secs: u16) -> String {
    match secs {
        0 => t(translations, "config.fade_off"),
        s if s < 60 => t(translations, "text.seconds").replace("{seconds}", &s.to_string()),
        s => t(translations, "text.minutes").replace("{minutes}", &(s / 60).to_string()),
    }
}

/// "Rings tomorrow, in 7 h 12 min" for the settings as they are right now.
fn rings_text(
    translations: &Translations,
    current: &AlarmData,
    one_time: bool,
    date_value: &str,
) -> String {
    let now = chrono::Local::now();
    let days = if one_time { [false; 7] } else { current.days };
    let date = if one_time {
        chrono::NaiveDate::parse_from_str(date_value, "%Y-%m-%d").ok()
    } else {
        None
    };
    crate::schedule::next_ring_moment(
        current.time_hour,
        current.time_minute,
        days,
        date,
        now.naive_local(),
    )
    .map(|moment| {
        // Measured in real local time, so a clock change in between counts.
        let left_ms = chrono::Local
            .from_local_datetime(&moment)
            .earliest()
            .map(|at| (at - now).num_milliseconds())
            .unwrap_or_else(|| (moment - now.naive_local()).num_milliseconds());
        let time = crate::text::time_left(translations, crate::schedule::countdown(0, left_ms));
        let day = match crate::schedule::day_label(now.date_naive(), moment.date()) {
            crate::schedule::DayLabel::Today => t(translations, "home.today_lower"),
            crate::schedule::DayLabel::Tomorrow => t(translations, "home.tomorrow_lower"),
            crate::schedule::DayLabel::Weekday(_) => {
                crate::text::short_date(translations, moment.date())
            }
        };
        t(translations, "config.rings_on")
            .replace("{day}", &day)
            .replace("{time}", &time)
    })
    .unwrap_or_default()
}

/// Alarm configuration screen for creating/editing alarms, and for what new alarms
/// start from ("defaults"). Next to the list on a wide screen another alarm can be picked
/// while this is open, and the editor must start over for it.
#[component]
pub fn AlarmConfig(id: String) -> Element {
    rsx! {
        Editor { key: "{id}", id }
    }
}

#[component]
fn Editor(id: String) -> Element {
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let translations = use_context::<Translations>();
    let mut settings = use_context::<Signal<AppSettings>>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    let nav = use_navigator();

    // "copy-<id>" opens a new alarm prefilled from the alarm <id>.
    let copy_of = id.strip_prefix("copy-").map(str::to_owned);
    let for_defaults = id == "defaults";
    let is_new = id == "new" || copy_of.is_some();

    // What the editor starts from, once; comparing with it tells if anything changed.
    let original = use_hook(|| {
        let defaults = settings.peek().prefs.alarm_defaults.clone();
        if for_defaults || id == "new" {
            return alarm_ops::new_alarm(defaults.as_ref());
        }
        let source = copy_of.as_deref().unwrap_or(&id);
        let found = alarms.peek().iter().find(|a| a.id == source).cloned();
        let mut alarm = found.unwrap_or_else(|| alarm_ops::new_alarm(defaults.as_ref()));
        if copy_of.is_some() && !alarm.label.is_empty() {
            alarm.label = t(&translations, "alarm.copy_label").replace("{label}", &alarm.label);
        }
        alarm
    });

    let mut draft = use_signal(|| original.clone());
    let one_time = use_signal(|| crate::schedule::is_one_time(&original));
    // Its date if it rings once. Only today while its time is still ahead.
    let start_date = use_hook(|| {
        let now = chrono::Local::now().naive_local();
        crate::schedule::resolve_one_time_date(&original, now).unwrap_or_else(|| {
            crate::schedule::next_occurrence(original.time_hour, original.time_minute, now)
        })
    });
    let date_value = use_signal(|| start_date.clone());
    let mut setups = use_signal(|| ChallengeSetup::list_from_stored(&original.challenges));
    let mut sheet: Signal<Option<EditorSheet>> = use_signal(|| None);
    let mut error = use_signal(String::new);
    let typed_time = use_signal(String::new);
    let renaming = use_signal(|| None);
    let mut trying: Signal<Option<Vec<ChallengeSetup>>> = use_signal(|| None);
    let handle = use_signal(|| 0u64);
    let token = use_signal(|| 0u64);
    let sounding = use_signal(|| false);
    let vibrating = use_signal(|| false);
    let preview = Preview {
        handle,
        token,
        sounding,
        vibrating,
    };
    use_context_provider(|| EditorState {
        draft,
        one_time,
        date_value,
        setups,
        sheet,
        error,
        typed_time,
        renaming,
        trying,
        preview,
    });
    use_drop(move || {
        stop_preview(*handle.peek());
        if *vibrating.peek() {
            mobile_sentinel::haptics::cancel();
        }
    });

    // Drag a challenge by its handle to put it in a different step.
    crate::reorder::use_reorder(
        "challenge-list",
        "challenge-row",
        Some("drag-handle"),
        EventHandler::new(move |(from, to)| {
            crate::list_edit::move_item(&mut setups.write(), from, to);
        }),
    );

    // The alarm as it would be saved right now.
    let build = {
        let id = id.clone();
        let original = original.clone();
        move || -> AlarmData {
            let mut alarm = draft.peek().clone();
            alarm.id = if is_new {
                uuid::Uuid::new_v4().to_string()
            } else {
                id.clone()
            };
            alarm.label = crate::sanitize::sanitize_label(&alarm.label);
            alarm.note = crate::sanitize::sanitize_label(&alarm.note);
            if *one_time.peek() {
                alarm.days = [false; 7];
                alarm.date = Some(date_value.peek().clone());
            } else {
                alarm.date = None;
            }
            alarm.challenges = setups
                .peek()
                .iter()
                .map(ChallengeSetup::to_stored)
                .collect();
            alarm.order = if is_new {
                crate::ordering::order_for_new(&alarms.peek())
            } else {
                original.order
            };
            // Saving an alarm starts it fresh, apart from a holiday the user asked for.
            alarm.enabled = true;
            alarm.skip_until_ms = None;
            alarm
        }
    };

    let has_changes = {
        let build = build.clone();
        let original = original.clone();
        move || -> bool {
            let mut now = build();
            let mut then = original.clone();
            now.id = String::new();
            then.id = String::new();
            now.order = then.order;
            now.enabled = then.enabled;
            now.skip_until_ms = then.skip_until_ms;
            // The date it starts out with is not a change.
            if crate::schedule::is_one_time(&then) {
                then.date = Some(start_date.clone());
            }
            now != then
        }
    };

    let mut leave = move || {
        if has_changes() {
            sheet.set(Some(EditorSheet::Discard));
        } else {
            preview.stop();
            nav.go_back();
        }
    };
    use_back_handler(EventHandler::new({
        let mut leave = leave.clone();
        move |_| leave()
    }));

    if let Some(list) = trying() {
        return rsx! {
            ChallengeRunner {
                challenges: list,
                on_exit: move |_| trying.set(None),
            }
        };
    }

    let current = draft.read().clone();
    let title = if for_defaults {
        t(&translations, "config.defaults_title")
    } else if is_new {
        t(&translations, "alarm.create")
    } else {
        t(&translations, "alarm.edit")
    };

    // Summary rows: what it is, and the details are one tap away.
    let sound_summary = if current.sound_mode == 2 {
        t(&translations, "config.vibration_only")
    } else {
        crate::custom_sounds::display_name(&current.sound, &settings.read().prefs.sound_names)
    };
    let vibration_summary = if current.sound_mode == 1 {
        t(&translations, "vibration.off")
    } else {
        t(
            &translations,
            VIBRATION_KEYS[usize::from(current.vibration).min(VIBRATION_KEYS.len() - 1)],
        )
    };
    let snooze_summary = if current.snooze_count == 0 {
        t(&translations, "config.no_snooze")
    } else {
        crate::text::counted(
            &translations,
            "config.snooze_summary",
            u32::from(current.snooze_count),
        )
        .replace("{minutes}", &current.snooze_interval.to_string())
    };
    let challenge_summary_text = {
        let names: Vec<String> = setups
            .read()
            .iter()
            .map(|s| t(&translations, s.type_key()))
            .collect();
        match names.len() {
            0 => t(&translations, "config.none"),
            1 | 2 => names.join(", "),
            n => format!("{} +{}", names[0], n - 1),
        }
    };
    let limit_summary = match current.ring_limit_secs {
        None => t(&translations, "config.limit_forever"),
        Some(secs) => {
            t(&translations, "text.minutes").replace("{minutes}", &(secs / 60).to_string())
        }
    };
    let check_summary = if current.wake_check_minutes == 0 {
        t(&translations, "config.fade_off")
    } else {
        t(&translations, "config.check_after")
            .replace("{minutes}", &current.wake_check_minutes.to_string())
    };
    let notice_summary = if current.notify_before_minutes == 0 {
        t(&translations, "config.fade_off")
    } else {
        t(&translations, "text.minutes")
            .replace("{minutes}", &current.notify_before_minutes.to_string())
    };
    let label_summary = if current.label.is_empty() {
        t(&translations, "config.none")
    } else {
        current.label.clone()
    };

    let sheet_view = match sheet() {
        None => rsx! {},
        Some(EditorSheet::Time) => rsx! { TimeSheet {} },
        Some(EditorSheet::Date) => rsx! { DateSheet {} },
        Some(EditorSheet::Label) => rsx! { LabelSheet {} },
        Some(EditorSheet::Sound) => rsx! { SoundSheet {} },
        Some(EditorSheet::Vibration) => rsx! { VibrationSheet {} },
        Some(EditorSheet::Snooze) => rsx! { SnoozeSheet {} },
        Some(EditorSheet::Challenges) => rsx! { ChallengesSheet {} },
        Some(EditorSheet::AddChallenge) => rsx! { AddChallengeSheet {} },
        Some(EditorSheet::Detail(index)) => rsx! { ChallengeDetailSheet { key: "{index}", index } },
        Some(EditorSheet::Limit) => rsx! { LimitSheet {} },
        Some(EditorSheet::WakeCheck) => rsx! { WakeCheckSheet {} },
        Some(EditorSheet::Notice) => rsx! { NoticeSheet {} },
        Some(EditorSheet::Delete) => rsx! { DeleteSheet { id: id.clone() } },
        Some(EditorSheet::Discard) => rsx! { DiscardSheet {} },
    };

    let save = {
        let id = id.clone();
        move |_| {
            preview.stop();
            let alarm = build();
            if for_defaults {
                let mut defaults = alarm;
                defaults.id = String::new();
                settings.write().prefs.alarm_defaults = Some(defaults);
                crate::persistence::save_settings(&settings.peek());
                nav.go_back();
                return;
            }
            if *one_time.peek() {
                let ahead = chrono::NaiveDate::parse_from_str(&date_value.peek(), "%Y-%m-%d")
                    .ok()
                    .and_then(|d| {
                        d.and_hms_opt(u32::from(alarm.time_hour), u32::from(alarm.time_minute), 0)
                    })
                    .is_some_and(|at| at > chrono::Local::now().naive_local());
                if !ahead {
                    error.set(t(&translations, "validation.date_past"));
                    return;
                }
            } else if !alarm.days.iter().any(|d| *d) {
                error.set(t(&translations, "validation.select_day"));
                return;
            }
            error.set(String::new());
            let saved_id = if is_new { alarm.id.clone() } else { id.clone() };
            let rings = crate::text::rings_in(&translations, &alarm);
            {
                let mut list = alarms.write();
                if let Some(existing) = list.iter_mut().find(|a| a.id == saved_id) {
                    *existing = alarm;
                } else {
                    list.push(alarm);
                }
            }
            crate::persistence::save_alarms(&alarms.peek());
            if !rings.is_empty() {
                toast.set(Some(ToastMessage::info(rings)));
            }
            nav.go_back();
        }
    };

    rsx! {
        div { class: "screen",
            ScreenHeader {
                title,
                on_back: move |_| leave(),
                if !is_new && !for_defaults {
                    button {
                        class: "icon-btn",
                        "aria-label": t(&translations, "alarm.delete"),
                        onclick: move |_| sheet.set(Some(EditorSheet::Delete)),
                        Icon { name: IconName::Trash, size: 20 }
                    }
                }
            }

            div { class: "scroll",
                if !for_defaults {
                    TimeSection {}
                    RepeatSection {}
                }

                ul { class: "list",
                    Row { title: t(&translations, "config.label"), value: label_summary, on_click: move |_| sheet.set(Some(EditorSheet::Label)) }
                    Row { title: t(&translations, "config.sound"), value: sound_summary, on_click: move |_| sheet.set(Some(EditorSheet::Sound)) }
                    Row { title: t(&translations, "config.vibration"), value: vibration_summary, on_click: move |_| sheet.set(Some(EditorSheet::Vibration)) }
                    Row { title: t(&translations, "config.snooze"), value: snooze_summary, on_click: move |_| sheet.set(Some(EditorSheet::Snooze)) }
                    Row { title: t(&translations, "config.challenges"), value: challenge_summary_text, on_click: move |_| sheet.set(Some(EditorSheet::Challenges)) }
                }
                ul { class: "list",
                    Row { title: t(&translations, "config.ring_limit"), value: limit_summary, on_click: move |_| sheet.set(Some(EditorSheet::Limit)) }
                    Row { title: t(&translations, "config.wake_check"), value: check_summary, on_click: move |_| sheet.set(Some(EditorSheet::WakeCheck)) }
                    Row { title: t(&translations, "config.notice"), value: notice_summary, on_click: move |_| sheet.set(Some(EditorSheet::Notice)) }
                    SwitchRow {
                        title: t(&translations, "config.max_brightness"),
                        on: current.max_brightness,
                        on_toggle: move |on: bool| draft.write().max_brightness = on,
                    }
                }
            }

            div { class: "save-bar",
                button { class: "btn btn-primary save-btn", onclick: save, {t(&translations, "action.save")} }
            }

            {sheet_view}
        }
    }
}

/// The time, large, with a step either way, and when that rings.
#[component]
fn TimeSection() -> Element {
    let EditorState {
        mut draft,
        one_time,
        date_value,
        mut sheet,
        mut typed_time,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let settings = use_context::<Signal<AppSettings>>();
    let current = draft.read().clone();
    let twenty_four = crate::text::uses_24_hours(&settings.read());
    let time_text = crate::text::clock(current.time_hour, current.time_minute, twenty_four);
    let rings = rings_text(&translations, &current, one_time(), &date_value.read());
    let mut shift = move |minutes: i32| {
        let mut alarm = draft.write();
        let total = i32::from(alarm.time_hour) * 60 + i32::from(alarm.time_minute) + minutes;
        let total = total.rem_euclid(24 * 60);
        alarm.time_hour = (total / 60) as u8;
        alarm.time_minute = (total % 60) as u8;
    };
    rsx! {
        section { class: "editor-time",
            div { class: "time-row",
                div { class: "time-shifts",
                    button {
                        class: "time-shift",
                        "aria-label": t(&translations, "config.earlier_one"),
                        onclick: move |_| shift(-1),
                        "\u{2212}1"
                    }
                    button {
                        class: "time-shift",
                        "aria-label": t(&translations, "config.earlier"),
                        onclick: move |_| shift(-5),
                        "\u{2212}5"
                    }
                }
                button {
                    class: if twenty_four { "time-big" } else { "time-big h12" },
                    "aria-label": t(&translations, "config.change_time"),
                    onclick: move |_| {
                        typed_time.set(String::new());
                        sheet.set(Some(EditorSheet::Time));
                    },
                    "{time_text}"
                }
                div { class: "time-shifts",
                    button {
                        class: "time-shift",
                        "aria-label": t(&translations, "config.later_one"),
                        onclick: move |_| shift(1),
                        "+1"
                    }
                    button {
                        class: "time-shift",
                        "aria-label": t(&translations, "config.later"),
                        onclick: move |_| shift(5),
                        "+5"
                    }
                }
            }
            p { class: "rings-in", "{rings}" }
        }
    }
}

/// Which days, or the one date.
#[component]
fn RepeatSection() -> Element {
    let EditorState {
        mut draft,
        mut one_time,
        mut date_value,
        mut sheet,
        mut error,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let settings = use_context::<Signal<AppSettings>>();
    let current = draft.read().clone();
    let week_start = settings.read().prefs.week_start;
    let date_text = chrono::NaiveDate::parse_from_str(&date_value.read(), "%Y-%m-%d")
        .map(|date| crate::text::short_date(&translations, date))
        .unwrap_or_default();
    let once_text = t(&translations, "config.once_on").replace("{date}", &date_text);
    let error_text = error();
    rsx! {
        section { class: "editor-repeat",
            if one_time() {
                button {
                    class: "btn btn-primary once-btn",
                    onclick: move |_| sheet.set(Some(EditorSheet::Date)),
                    "{once_text}"
                }
            } else {
                div { class: "day-chips",
                    for day in week_start.day_order() {
                        {
                            let on = current.days[day];
                            let name = t(&translations, crate::text::FULL_DAY_KEYS[day]);
                            rsx! {
                                button {
                                    class: if on { "day-chip on" } else { "day-chip" },
                                    "aria-label": "{name}",
                                    "aria-pressed": if on { "true" } else { "false" },
                                    onclick: move |_| {
                                        draft.write().days[day] = !on;
                                        error.set(String::new());
                                    },
                                    {crate::text::day_letter(&translations, day)}
                                }
                            }
                        }
                    }
                }
            }
            div { class: "repeat-presets",
                for (key, days) in [
                    ("config.weekdays", [true, true, true, true, true, false, false]),
                    ("config.weekend", [false, false, false, false, false, true, true]),
                    ("config.every_day", [true; 7]),
                ] {
                    button {
                        class: if !one_time() && current.days == days { "chip-small on" } else { "chip-small" },
                        onclick: move |_| {
                            one_time.set(false);
                            draft.write().days = days;
                            error.set(String::new());
                        },
                        {t(&translations, key)}
                    }
                }
                button {
                    class: if one_time() { "chip-small on" } else { "chip-small" },
                    onclick: move |_| {
                        let now = chrono::Local::now().naive_local();
                        let (hour, minute) = {
                            let alarm = draft.peek();
                            (alarm.time_hour, alarm.time_minute)
                        };
                        let ahead = chrono::NaiveDate::parse_from_str(&date_value.peek(), "%Y-%m-%d")
                            .ok()
                            .and_then(|d| d.and_hms_opt(u32::from(hour), u32::from(minute), 0))
                            .is_some_and(|at| at > now);
                        if !ahead {
                            date_value.set(crate::schedule::next_occurrence(hour, minute, now));
                        }
                        one_time.set(true);
                        error.set(String::new());
                        sheet.set(Some(EditorSheet::Date));
                    },
                    {t(&translations, "config.one_date")}
                }
            }
            if !error_text.is_empty() {
                p { class: "field-error", "{error_text}" }
            }
        }
    }
}
