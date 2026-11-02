//! Making and changing an alarm. The time and the days are right there; everything
//! else is one summary row that opens a sheet.

use chrono::{Duration, TimeZone};
use dioxus::prelude::*;

use crate::alarm_ops;
use crate::app::{t, AlarmData, Translations};
use crate::back::use_back_handler;
use crate::components::time_picker::TimePicker;
use crate::components::{
    Chips, GroupTitle, Icon, IconName, Row, ScreenHeader, Sheet, SheetDone, SheetItem, Stepper,
    ToastMessage,
};
use crate::models::setup::{difficulty_name, preset, CHALLENGE_TYPES};
use crate::models::{AppSettings, ChallengeSetup, Difficulty};
use crate::screens::practice::ChallengeRunner;

/// How long tapping a sound plays it.
const SOUND_PREVIEW_MS: u64 = 5_000;
/// How long "Try" rings.
const TRY_MS: u64 = 10_000;
/// The most challenges an alarm takes.
const MAX_CHALLENGES: usize = 10;

/// Translation keys of the vibration patterns, in the order of
/// `platform::VIBRATION_PATTERNS`.
const VIBRATION_KEYS: [&str; 3] = ["vibration.pulse", "vibration.steady", "vibration.heartbeat"];

const DIFFICULTIES: [Difficulty; 4] = [
    Difficulty::Easy,
    Difficulty::Medium,
    Difficulty::Hard,
    Difficulty::Extreme,
];

const FADE_SECONDS: [u16; 6] = [0, 15, 30, 60, 120, 300];
const LIMIT_MINUTES: [u32; 4] = [30, 60, 90, 120];
const CHECK_MINUTES: [u8; 4] = [0, 3, 5, 10];

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
    Delete,
    Discard,
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
    match mobile_sentinel::audio::play(&path.to_string_lossy(), false) {
        Ok(handle) => handle.id(),
        Err(e) => {
            log::warn!("[AlarmFree] Preview play failed: {e}");
            0
        }
    }
}

/// Ask for the Android permission a challenge needs (steps, camera) before it is
/// added or tried. A no-op elsewhere.
fn request_challenge_permission(setup: &ChallengeSetup) {
    #[cfg(target_os = "android")]
    {
        let permission = match setup {
            ChallengeSetup::Steps { .. } => Some("android.permission.ACTIVITY_RECOGNITION"),
            ChallengeSetup::Scan { .. } => Some("android.permission.CAMERA"),
            _ => None,
        };
        if let Some(permission) = permission {
            let _ = mobile_sentinel::permissions::request(permission);
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = setup;
}

/// A time typed in by hand: "7", "730", "07:30", "7.30".
fn parse_typed_time(text: &str) -> Option<(u8, u8)> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let separated = text.contains([':', '.', ' ', 'h']);
    let (hour, minute) = match digits.len() {
        1 | 2 if !separated => (digits.parse().ok()?, 0),
        3 if !separated => (digits[..1].parse().ok()?, digits[1..].parse().ok()?),
        3 => (digits[..1].parse().ok()?, digits[1..].parse().ok()?),
        4 => (digits[..2].parse().ok()?, digits[2..].parse().ok()?),
        _ => return None,
    };
    (hour < 24 && minute < 60).then_some((hour, minute))
}

/// What an alarm's challenge is set to, in a few words.
fn challenge_summary(translations: &Translations, setup: &ChallengeSetup) -> String {
    let difficulty = |d: Difficulty| t(translations, &format!("difficulty.{}", difficulty_name(d)));
    match setup {
        ChallengeSetup::Math { difficulty: d }
        | ChallengeSetup::Memory { difficulty: d }
        | ChallengeSetup::Typing { difficulty: d } => difficulty(*d),
        ChallengeSetup::Shake { count } => {
            t(translations, "challenge.summary.shakes").replace("{count}", &count.to_string())
        }
        ChallengeSetup::Steps { count } => {
            t(translations, "challenge.summary.steps").replace("{count}", &count.to_string())
        }
        ChallengeSetup::Hold { count, secs } => t(translations, "challenge.summary.hold")
            .replace("{count}", &count.to_string())
            .replace("{secs}", &secs.to_string()),
        ChallengeSetup::Reaction {
            targets,
            secs,
            rounds,
        } => t(translations, "challenge.summary.reaction")
            .replace("{count}", &targets.to_string())
            .replace("{secs}", &secs.to_string())
            .replace("{rounds}", &rounds.to_string()),
        ChallengeSetup::Scan { code } if code.is_empty() => {
            t(translations, "challenge.summary.any")
        }
        ChallengeSetup::Scan { code } => {
            t(translations, "challenge.summary.code").replace("{code}", code)
        }
    }
}

/// An example of what a difficulty means for this challenge.
fn difficulty_example(translations: &Translations, setup: &ChallengeSetup) -> String {
    let (name, difficulty) = match setup {
        ChallengeSetup::Math { difficulty } => ("math", difficulty),
        ChallengeSetup::Memory { difficulty } => ("memory", difficulty),
        ChallengeSetup::Typing { difficulty } => ("typing", difficulty),
        _ => return String::new(),
    };
    t(
        translations,
        &format!("challenge.example.{name}_{}", difficulty_name(*difficulty)),
    )
}

fn seconds_label(translations: &Translations, secs: u16) -> String {
    match secs {
        0 => t(translations, "config.fade_off"),
        s if s < 60 => t(translations, "text.seconds").replace("{seconds}", &s.to_string()),
        s => t(translations, "text.minutes").replace("{minutes}", &(s / 60).to_string()),
    }
}

/// Alarm configuration screen for creating/editing alarms, and for what new alarms
/// start from ("defaults").
#[component]
pub fn AlarmConfig(id: String) -> Element {
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
    // An alarm with no weekday selected rings once, on a date.
    let mut one_time = use_signal(|| crate::schedule::is_one_time(&original));
    let mut date_value = use_signal(|| {
        original.date.clone().unwrap_or_else(|| {
            chrono::Local::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string()
        })
    });
    let mut setups = use_signal(|| ChallengeSetup::list_from_stored(&original.challenges));
    let mut sheet: Signal<Option<EditorSheet>> = use_signal(|| None);
    let mut error = use_signal(String::new);
    let mut typed_time = use_signal(String::new);
    // Challenges being tried; shown instead of the editor, which keeps its state.
    let mut trying: Signal<Option<Vec<ChallengeSetup>>> = use_signal(|| None);

    // A sound playing for a moment. The token makes an older timer leave a newer
    // preview alone.
    let mut preview_handle = use_signal(|| 0u64);
    let mut preview_token = use_signal(|| 0u64);
    let mut try_running = use_signal(|| false);
    let mut stop_sound = move || {
        stop_preview(*preview_handle.peek());
        preview_handle.set(0);
        let token = *preview_token.peek() + 1;
        preview_token.set(token);
        try_running.set(false);
    };
    let mut play_for = move |sound: String, ms: u64| {
        stop_sound();
        preview_handle.set(play_preview(&sound));
        let token = *preview_token.peek();
        spawn(async move {
            crate::timer::sleep_ms(ms).await;
            if *preview_token.peek() == token {
                stop_sound();
            }
        });
    };
    use_drop(move || stop_preview(*preview_handle.peek()));

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
        let id = id.clone();
        move || -> bool {
            let mut now = build();
            let mut then = original.clone();
            now.id = String::new();
            then.id = String::new();
            now.order = then.order;
            now.enabled = then.enabled;
            now.skip_until_ms = then.skip_until_ms;
            if then.days == [false; 7] && then.date.is_none() {
                then.date = now.date.clone();
            }
            let _ = &id;
            now != then
        }
    };

    let mut leave = {
        let has_changes = has_changes.clone();
        move || {
            if has_changes() {
                sheet.set(Some(EditorSheet::Discard));
            } else {
                stop_sound();
                nav.go_back();
            }
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
    let twenty_four = crate::text::uses_24_hours(&settings.read());
    let week_start = settings.read().prefs.week_start;
    let is_one_time = one_time();

    // "Rings tomorrow, in 7 h 12 min" for the settings as they are right now.
    let rings_text = {
        let now = chrono::Local::now();
        let days = if is_one_time {
            [false; 7]
        } else {
            current.days
        };
        let date = if is_one_time {
            chrono::NaiveDate::parse_from_str(&date_value.read(), "%Y-%m-%d").ok()
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
            let time = crate::schedule::format_countdown(
                crate::schedule::countdown(0, left_ms),
                &t(&translations, "home.next_in_dhm"),
                &t(&translations, "home.next_in_hm"),
                &t(&translations, "home.next_in_m"),
            );
            let day = match crate::schedule::day_label(now.date_naive(), moment.date()) {
                crate::schedule::DayLabel::Today => t(&translations, "home.today_lower"),
                crate::schedule::DayLabel::Tomorrow => t(&translations, "home.tomorrow_lower"),
                crate::schedule::DayLabel::Weekday(_) => {
                    crate::text::short_date(&translations, moment.date())
                }
            };
            t(&translations, "config.rings_on")
                .replace("{day}", &day)
                .replace("{time}", &time)
        })
        .unwrap_or_default()
    };

    let done_text = t(&translations, "action.done");
    let title = if for_defaults {
        t(&translations, "config.defaults_title")
    } else if is_new {
        t(&translations, "alarm.create")
    } else {
        t(&translations, "alarm.edit")
    };

    // Summary rows.
    let sound_summary = if current.sound_mode == 2 {
        t(&translations, "config.vibration_only")
    } else {
        format!(
            "{}, {} {}",
            crate::custom_sounds::display_name(&current.sound),
            t(&translations, "config.fade_in_lower"),
            seconds_label(&translations, current.fade_in_secs),
        )
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
        t(&translations, "config.snooze_summary")
            .replace("{minutes}", &current.snooze_interval.to_string())
            .replace("{count}", &current.snooze_count.to_string())
    };
    let challenge_summary_text = if setups.read().is_empty() {
        t(&translations, "config.none")
    } else {
        setups
            .read()
            .iter()
            .map(|s| t(&translations, s.type_key()))
            .collect::<Vec<_>>()
            .join(", ")
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
        t(&translations, "text.minutes")
            .replace("{minutes}", &current.wake_check_minutes.to_string())
    };
    let label_summary = if current.label.is_empty() {
        t(&translations, "config.none")
    } else {
        current.label.clone()
    };

    let sheet_view = match sheet() {
        None => rsx! {},
        Some(EditorSheet::Time) => rsx! {
            Sheet {
                title: t(&translations, "config.time"),
                note: t(&translations, "config.time_note"),
                on_close: move |_| sheet.set(None),
                TimePicker {
                    hour: current.time_hour,
                    minute: current.time_minute,
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
        },
        Some(EditorSheet::Date) => {
            let today = chrono::Local::now().date_naive();
            let quick = [
                ("home.today", today),
                ("home.tomorrow", today + Duration::days(1)),
                ("config.next_week", today + Duration::days(7)),
            ];
            rsx! {
                Sheet {
                    title: t(&translations, "config.date_title"),
                    note: t(&translations, "config.date_note"),
                    on_close: move |_| sheet.set(None),
                    label { class: "field-label",
                        {t(&translations, "config.date")}
                        input {
                            class: "field",
                            r#type: "date",
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
        Some(EditorSheet::Label) => {
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
                    note: t(&translations, "config.label_note"),
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
        Some(EditorSheet::Sound) => {
            let defaults = crate::custom_sounds::list_default_sounds();
            let custom = crate::custom_sounds::list_custom_sounds();
            let general_volume = settings.read().min_volume_percent;
            let volume = current.volume_percent.unwrap_or(general_volume);
            let selected_sound = current.sound.clone();
            let vibrate_only = current.sound_mode == 2;
            rsx! {
                Sheet {
                    title: t(&translations, "config.sound"),
                    note: t(&translations, "config.sound_note"),
                    on_close: move |_| {
                        stop_sound();
                        sheet.set(None);
                    },
                    button {
                        class: if vibrate_only { "radio selected" } else { "radio" },
                        onclick: move |_| {
                            stop_sound();
                            draft.write().sound_mode = 2;
                        },
                        span { class: "radio-mark" }
                        span { class: "radio-text", {t(&translations, "config.no_sound")} }
                    }
                    for sound in defaults.into_iter().chain(custom.clone()) {
                        {
                            let is_selected = !vibrate_only && sound == selected_sound;
                            let name = crate::custom_sounds::display_name(&sound);
                            let to_pick = sound.clone();
                            let to_delete = sound.clone();
                            let is_custom = crate::custom_sounds::is_custom_sound(&sound);
                            let delete_aria = t(&translations, "sound.delete");
                            rsx! {
                                div { class: "radio-row",
                                    button {
                                        class: if is_selected { "radio selected" } else { "radio" },
                                        onclick: move |_| {
                                            {
                                                let mut alarm = draft.write();
                                                alarm.sound = to_pick.clone();
                                                if alarm.sound_mode == 2 {
                                                    alarm.sound_mode = 0;
                                                }
                                            }
                                            play_for(to_pick.clone(), SOUND_PREVIEW_MS);
                                        },
                                        span { class: "radio-mark" }
                                        span { class: "radio-text", "{name}" }
                                    }
                                    if is_custom {
                                        button {
                                            class: "icon-btn",
                                            "aria-label": "{delete_aria}",
                                            onclick: move |_| {
                                                stop_sound();
                                                // An alarm that uses it falls back to the first sound
                                                // when it is saved or edited, and always rings: the
                                                // ring has its own fallbacks.
                                                let replacement = alarm_ops::factory_alarm().sound;
                                                {
                                                    let mut alarm = draft.write();
                                                    if alarm.sound == to_delete {
                                                        alarm.sound = replacement.clone();
                                                    }
                                                }
                                                let mut list = alarms.write();
                                                let mut changed = false;
                                                for alarm in list.iter_mut().filter(|a| a.sound == to_delete) {
                                                    alarm.sound = replacement.clone();
                                                    changed = true;
                                                }
                                                drop(list);
                                                if changed {
                                                    crate::persistence::save_alarms(&alarms.peek());
                                                }
                                                crate::custom_sounds::delete_sound(&to_delete);
                                            },
                                            Icon { name: IconName::Xmark, size: 16 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "btn btn-quiet",
                        onclick: move |_| {
                            // Stop it first, or the tone keeps looping behind the picker.
                            stop_sound();
                            spawn(async move {
                                let (tx, rx) = futures_channel::oneshot::channel();
                                std::thread::spawn(move || {
                                    let _ = tx.send(crate::custom_sounds::pick_audio_file_native());
                                });
                                match rx.await {
                                    Ok(Ok(filename)) => {
                                        let mut alarm = draft.write();
                                        alarm.sound = filename;
                                        if alarm.sound_mode == 2 {
                                            alarm.sound_mode = 0;
                                        }
                                    }
                                    Ok(Err(reason)) if reason == crate::custom_sounds::NOT_A_SOUND => {
                                        toast.set(Some(ToastMessage::error(t(&translations, "sound.not_a_sound"))));
                                    }
                                    // Cancelled, mostly.
                                    Ok(Err(reason)) => log::info!("[AlarmFree] No sound imported: {reason}"),
                                    Err(_) => {}
                                }
                            });
                        },
                        {t(&translations, "sound.import")}
                    }
                    label { class: "slider-row",
                        span { {t(&translations, "config.volume")} }
                        input {
                            class: "slider",
                            r#type: "range",
                            min: "0",
                            max: "100",
                            step: "5",
                            value: "{volume}",
                            oninput: move |evt| {
                                if let Ok(value) = evt.value().parse::<u8>() {
                                    draft.write().volume_percent = Some(value);
                                }
                            },
                        }
                        span { class: "slider-value", "{volume}%" }
                    }
                    GroupTitle { text: t(&translations, "config.fade_in") }
                    Chips::<u16> {
                        grow: true,
                        options: FADE_SECONDS.iter().map(|s| (*s, seconds_label(&translations, *s))).collect::<Vec<_>>(),
                        selected: Some(current.fade_in_secs),
                        on_pick: move |secs: u16| draft.write().fade_in_secs = secs,
                    }
                    SheetDone {
                        label: done_text.clone(),
                        on_click: move |_| {
                            stop_sound();
                            sheet.set(None);
                        },
                    }
                }
            }
        }
        Some(EditorSheet::Vibration) => {
            let selected = if current.sound_mode == 1 {
                None
            } else {
                Some(current.vibration)
            };
            let mut options = vec![(u8::MAX, t(&translations, "vibration.off"))];
            options.extend(
                VIBRATION_KEYS
                    .iter()
                    .enumerate()
                    .map(|(i, key)| (i as u8, t(&translations, key))),
            );
            rsx! {
                Sheet {
                    title: t(&translations, "config.vibration"),
                    note: t(&translations, "config.vibration_note"),
                    on_close: move |_| sheet.set(None),
                    Chips::<u8> {
                        grow: true,
                        options,
                        selected: Some(selected.unwrap_or(u8::MAX)),
                        on_pick: move |pattern: u8| {
                            let mut alarm = draft.write();
                            if pattern == u8::MAX {
                                // Nothing at all is no alarm: without sound it keeps vibrating.
                                if alarm.sound_mode != 2 {
                                    alarm.sound_mode = 1;
                                }
                            } else {
                                alarm.vibration = pattern;
                                if alarm.sound_mode == 1 {
                                    alarm.sound_mode = 0;
                                }
                                crate::feedback::pattern(crate::platform::VIBRATION_PATTERNS[usize::from(pattern)]);
                            }
                        },
                    }
                    SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
                }
            }
        }
        Some(EditorSheet::Snooze) => rsx! {
            Sheet {
                title: t(&translations, "config.snooze"),
                note: t(&translations, "config.snooze_note"),
                on_close: move |_| sheet.set(None),
                Stepper {
                    title: t(&translations, "config.snooze_interval"),
                    value: t(&translations, "text.minutes").replace("{minutes}", &current.snooze_interval.to_string()),
                    on_less: move |_| {
                        let mut alarm = draft.write();
                        alarm.snooze_interval = alarm.snooze_interval.saturating_sub(1).max(1);
                    },
                    on_more: move |_| {
                        let mut alarm = draft.write();
                        alarm.snooze_interval = alarm.snooze_interval.saturating_add(1).min(60);
                    },
                }
                Stepper {
                    title: t(&translations, "config.snooze_count"),
                    value: t(&translations, "text.times").replace("{count}", &current.snooze_count.to_string()),
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
        },
        Some(EditorSheet::Challenges) => {
            let list = setups.read().clone();
            let count = list.len();
            let try_text = t(&translations, "challenge.try");
            let remove_text = t(&translations, "config.remove_challenge");
            let drag_text = t(&translations, "config.drag_challenge");
            let order_options = vec![
                (false, t(&translations, "config.order_fixed")),
                (true, t(&translations, "config.order_random")),
            ];
            let preset_options = vec![
                ("gentle", t(&translations, "preset.gentle")),
                ("normal", t(&translations, "preset.normal")),
                ("hard", t(&translations, "preset.hard")),
            ];
            rsx! {
                Sheet {
                    title: t(&translations, "config.challenges"),
                    note: t(&translations, "config.challenges_note"),
                    on_close: move |_| sheet.set(None),
                    div { id: "challenge-list", class: "challenge-list",
                        for (index, setup) in list.iter().cloned().enumerate() {
                            {
                                let name = t(&translations, setup.type_key());
                                let summary = challenge_summary(&translations, &setup);
                                let to_try = setup.clone();
                                rsx! {
                                    div { key: "{index}-{setup.type_key()}", class: "challenge-row",
                                        span { class: "drag-handle", "aria-label": "{drag_text}",
                                            Icon { name: IconName::Grip, size: 18 }
                                        }
                                        span { class: "challenge-number", "aria-hidden": "true", "{index + 1}" }
                                        button {
                                            class: "challenge-open",
                                            onclick: move |_| sheet.set(Some(EditorSheet::Detail(index))),
                                            span { class: "challenge-name", "{name}" }
                                            span { class: "challenge-summary", "{summary}" }
                                        }
                                        button {
                                            class: "link-btn",
                                            onclick: move |_| {
                                                request_challenge_permission(&to_try);
                                                trying.set(Some(vec![to_try.clone()]));
                                            },
                                            "{try_text}"
                                        }
                                        button {
                                            class: "icon-btn",
                                            "aria-label": "{remove_text} {name}",
                                            onclick: move |_| {
                                                setups.write().remove(index);
                                            },
                                            Icon { name: IconName::Xmark, size: 18 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if count < MAX_CHALLENGES {
                        button {
                            class: "btn btn-quiet",
                            onclick: move |_| sheet.set(Some(EditorSheet::AddChallenge)),
                            {t(&translations, "config.add_challenge")}
                        }
                    }
                    if count > 0 {
                        button {
                            class: "btn btn-quiet",
                            onclick: move |_| {
                                let list = setups.peek().clone();
                                for setup in &list {
                                    request_challenge_permission(setup);
                                }
                                trying.set(Some(list));
                            },
                            {t(&translations, "config.try_all")}
                        }
                    }
                    GroupTitle { text: t(&translations, "config.order") }
                    Chips::<bool> {
                        grow: true,
                        options: order_options,
                        selected: Some(current.random_order),
                        on_pick: move |random: bool| draft.write().random_order = random,
                    }
                    GroupTitle { text: t(&translations, "config.presets") }
                    Chips::<&'static str> {
                        grow: true,
                        options: preset_options,
                        selected: None,
                        on_pick: move |name: &'static str| {
                            let list = preset(name);
                            for setup in &list {
                                request_challenge_permission(setup);
                            }
                            setups.set(list);
                        },
                    }
                    SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
                }
            }
        }
        Some(EditorSheet::AddChallenge) => rsx! {
            Sheet {
                title: t(&translations, "config.select_challenge"),
                note: t(&translations, "config.add_note"),
                on_close: move |_| sheet.set(Some(EditorSheet::Challenges)),
                for key in CHALLENGE_TYPES {
                    button {
                        class: "add-option",
                        onclick: move |_| {
                            if let Some(setup) = ChallengeSetup::new(key) {
                                request_challenge_permission(&setup);
                                let mut list = setups.write();
                                list.push(setup);
                                let index = list.len() - 1;
                                drop(list);
                                sheet.set(Some(EditorSheet::Detail(index)));
                            }
                        },
                        span { class: "add-name", {t(&translations, key)} }
                        span { class: "add-desc", {t(&translations, &key.replace("challenge.", "challenge.desc."))} }
                    }
                }
                SheetDone {
                    label: t(&translations, "challenge.config.back"),
                    on_click: move |_| sheet.set(Some(EditorSheet::Challenges)),
                }
            }
        },
        Some(EditorSheet::Detail(index)) => {
            let Some(setup) = setups.read().get(index).cloned() else {
                return rsx! {};
            };
            let name = t(&translations, setup.type_key());
            let description = t(
                &translations,
                &setup.type_key().replace("challenge.", "challenge.desc."),
            );
            let example = difficulty_example(&translations, &setup);
            let mut update = move |change: &dyn Fn(&mut ChallengeSetup)| {
                if let Some(setup) = setups.write().get_mut(index) {
                    change(setup);
                }
            };
            let to_try = setup.clone();
            let controls = match setup {
                ChallengeSetup::Math { difficulty }
                | ChallengeSetup::Memory { difficulty }
                | ChallengeSetup::Typing { difficulty } => rsx! {
                    GroupTitle { text: t(&translations, "challenge.config.difficulty") }
                    Chips::<Difficulty> {
                        grow: true,
                        options: DIFFICULTIES
                            .iter()
                            .map(|d| (*d, t(&translations, &format!("difficulty.{}", difficulty_name(*d)))))
                            .collect::<Vec<_>>(),
                        selected: Some(difficulty),
                        on_pick: move |picked: Difficulty| update(&|setup| match setup {
                            ChallengeSetup::Math { difficulty }
                            | ChallengeSetup::Memory { difficulty }
                            | ChallengeSetup::Typing { difficulty } => *difficulty = picked,
                            _ => {}
                        }),
                    }
                    p { class: "example", "{example}" }
                },
                ChallengeSetup::Shake { count } => rsx! {
                    Stepper {
                        title: t(&translations, "challenge.config.how_many_shakes"),
                        value: "{count}",
                        on_less: move |_| update(&|s| if let ChallengeSetup::Shake { count } = s { *count = count.saturating_sub(5).max(5) }),
                        on_more: move |_| update(&|s| if let ChallengeSetup::Shake { count } = s { *count = (*count + 5).min(200) }),
                    }
                },
                ChallengeSetup::Steps { count } => rsx! {
                    Stepper {
                        title: t(&translations, "challenge.config.how_many_steps"),
                        value: "{count}",
                        on_less: move |_| update(&|s| if let ChallengeSetup::Steps { count } = s { *count = count.saturating_sub(5).max(5) }),
                        on_more: move |_| update(&|s| if let ChallengeSetup::Steps { count } = s { *count = (*count + 5).min(200) }),
                    }
                },
                ChallengeSetup::Hold { count, secs } => rsx! {
                    Stepper {
                        title: t(&translations, "challenge.config.how_many_holds"),
                        value: "{count}",
                        on_less: move |_| update(&|s| if let ChallengeSetup::Hold { count, .. } = s { *count = count.saturating_sub(1).max(1) }),
                        on_more: move |_| update(&|s| if let ChallengeSetup::Hold { count, .. } = s { *count = (*count + 1).min(30) }),
                    }
                    Stepper {
                        title: t(&translations, "challenge.config.hold_duration"),
                        value: t(&translations, "text.seconds").replace("{seconds}", &secs.to_string()),
                        on_less: move |_| update(&|s| if let ChallengeSetup::Hold { secs, .. } = s { *secs = secs.saturating_sub(1).max(1) }),
                        on_more: move |_| update(&|s| if let ChallengeSetup::Hold { secs, .. } = s { *secs = (*secs + 1).min(15) }),
                    }
                },
                ChallengeSetup::Reaction {
                    targets,
                    secs,
                    rounds,
                } => rsx! {
                    Stepper {
                        title: t(&translations, "challenge.config.how_many_targets"),
                        value: "{targets}",
                        on_less: move |_| update(&|s| if let ChallengeSetup::Reaction { targets, .. } = s { *targets = targets.saturating_sub(1).max(3) }),
                        on_more: move |_| update(&|s| if let ChallengeSetup::Reaction { targets, .. } = s { *targets = (*targets + 1).min(30) }),
                    }
                    Stepper {
                        title: t(&translations, "challenge.config.time_limit"),
                        value: t(&translations, "text.seconds").replace("{seconds}", &secs.to_string()),
                        on_less: move |_| update(&|s| if let ChallengeSetup::Reaction { secs, .. } = s { *secs = secs.saturating_sub(1).max(2) }),
                        on_more: move |_| update(&|s| if let ChallengeSetup::Reaction { secs, .. } = s { *secs = (*secs + 1).min(10) }),
                    }
                    Stepper {
                        title: t(&translations, "challenge.config.how_many_rounds"),
                        value: "{rounds}",
                        on_less: move |_| update(&|s| if let ChallengeSetup::Reaction { rounds, .. } = s { *rounds = rounds.saturating_sub(1).max(1) }),
                        on_more: move |_| update(&|s| if let ChallengeSetup::Reaction { rounds, .. } = s { *rounds = (*rounds + 1).min(5) }),
                    }
                },
                ChallengeSetup::Scan { code } => rsx! {
                    label { class: "field-label",
                        {t(&translations, "challenge.config.scan_value")}
                        input {
                            class: "field",
                            r#type: "text",
                            placeholder: t(&translations, "challenge.config.scan_placeholder"),
                            value: "{code}",
                            oninput: move |evt| {
                                let text = evt.value();
                                update(&|s| if let ChallengeSetup::Scan { code } = s { code.clone_from(&text) });
                            },
                        }
                    }
                    button {
                        class: "btn btn-quiet",
                        onclick: move |_| {
                            spawn(async move {
                                let (tx, rx) = futures_channel::oneshot::channel::<String>();
                                std::thread::spawn(move || {
                                    let _ = tx.send(mobile_sentinel::scanner::scan().unwrap_or_default());
                                });
                                if let Ok(value) = rx.await {
                                    if !value.is_empty() {
                                        if let Some(ChallengeSetup::Scan { code }) = setups.write().get_mut(index) {
                                            *code = value;
                                        }
                                    }
                                }
                            });
                        },
                        {t(&translations, "challenge.config.scan_now")}
                    }
                },
            };
            rsx! {
                Sheet {
                    title: name,
                    note: description,
                    on_close: move |_| sheet.set(Some(EditorSheet::Challenges)),
                    {controls}
                    button {
                        class: "btn btn-quiet",
                        onclick: move |_| {
                            request_challenge_permission(&to_try);
                            trying.set(Some(vec![to_try.clone()]));
                        },
                        {t(&translations, "challenge.config.test")}
                    }
                    button {
                        class: "btn btn-quiet danger",
                        onclick: move |_| {
                            setups.write().remove(index);
                            sheet.set(Some(EditorSheet::Challenges));
                        },
                        {t(&translations, "config.remove_from_alarm")}
                    }
                    SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(Some(EditorSheet::Challenges)) }
                }
            }
        }
        Some(EditorSheet::Limit) => {
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
                    note: t(&translations, "config.limit_note"),
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
        Some(EditorSheet::WakeCheck) => rsx! {
            Sheet {
                title: t(&translations, "config.wake_check"),
                note: t(&translations, "config.wake_check_note"),
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
        },
        Some(EditorSheet::Delete) => {
            let delete_id = id.clone();
            rsx! {
                Sheet {
                    title: t(&translations, "alarm.delete_confirm"),
                    note: t(&translations, "alarm.delete_warning"),
                    on_close: move |_| sheet.set(None),
                    SheetItem {
                        label: t(&translations, "action.delete"),
                        danger: true,
                        on_click: move |_| {
                            stop_sound();
                            alarms.write().retain(|a| a.id != delete_id);
                            crate::persistence::save_alarms(&alarms.peek());
                            nav.go_back();
                        },
                    }
                    SheetDone { label: t(&translations, "alarm.keep"), on_click: move |_| sheet.set(None) }
                }
            }
        }
        Some(EditorSheet::Discard) => rsx! {
            Sheet {
                title: t(&translations, "config.discard_title"),
                note: t(&translations, "config.discard_note"),
                on_close: move |_| sheet.set(None),
                SheetItem {
                    label: t(&translations, "config.discard"),
                    danger: true,
                    on_click: move |_| {
                        stop_sound();
                        nav.go_back();
                    },
                }
                SheetDone { label: t(&translations, "config.keep_editing"), on_click: move |_| sheet.set(None) }
            }
        },
    };

    let shift_time = move |minutes: i32| {
        let mut alarm = draft.write();
        let total = i32::from(alarm.time_hour) * 60 + i32::from(alarm.time_minute) + minutes;
        let total = total.rem_euclid(24 * 60);
        alarm.time_hour = (total / 60) as u8;
        alarm.time_minute = (total % 60) as u8;
    };
    let mut shift_earlier = shift_time;
    let mut shift_later = shift_time;

    let time_text = crate::text::clock(current.time_hour, current.time_minute, twenty_four);
    let date_text = chrono::NaiveDate::parse_from_str(&date_value.read(), "%Y-%m-%d")
        .map(|date| crate::text::short_date(&translations, date))
        .unwrap_or_default();
    let once_text = t(&translations, "config.once_on").replace("{date}", &date_text);
    let error_text = error();

    let save = {
        let build = build.clone();
        let id = id.clone();
        move |_| {
            stop_sound();
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
            {
                let mut list = alarms.write();
                if let Some(existing) = list.iter_mut().find(|a| a.id == saved_id) {
                    *existing = alarm;
                } else {
                    list.push(alarm);
                }
            }
            crate::persistence::save_alarms(&alarms.peek());
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

            div { class: "scroll scroll-save",
                if !for_defaults {
                    section { class: "editor-time",
                        div { class: "time-row",
                            button {
                                class: "time-shift",
                                "aria-label": t(&translations, "config.earlier"),
                                onclick: move |_| shift_earlier(-5),
                                "\u{2212}5"
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
                            button {
                                class: "time-shift",
                                "aria-label": t(&translations, "config.later"),
                                onclick: move |_| shift_later(5),
                                "+5"
                            }
                        }
                        p { class: "rings-in", "{rings_text}" }
                    }

                    section { class: "editor-repeat",
                        if is_one_time {
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
                                    class: "chip-small",
                                    onclick: move |_| {
                                        one_time.set(false);
                                        draft.write().days = days;
                                        error.set(String::new());
                                    },
                                    {t(&translations, key)}
                                }
                            }
                            button {
                                class: "chip-small",
                                onclick: move |_| {
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

                ul { class: "list",
                    Row { title: t(&translations, "config.label"), value: label_summary, on_click: move |_| sheet.set(Some(EditorSheet::Label)) }
                    Row { title: t(&translations, "config.sound"), value: sound_summary, on_click: move |_| sheet.set(Some(EditorSheet::Sound)) }
                    Row { title: t(&translations, "config.vibration"), value: vibration_summary, on_click: move |_| sheet.set(Some(EditorSheet::Vibration)) }
                    Row { title: t(&translations, "config.snooze"), value: snooze_summary, on_click: move |_| sheet.set(Some(EditorSheet::Snooze)) }
                    Row { title: t(&translations, "config.challenges"), value: challenge_summary_text, on_click: move |_| sheet.set(Some(EditorSheet::Challenges)) }
                    Row { title: t(&translations, "config.ring_limit"), value: limit_summary, on_click: move |_| sheet.set(Some(EditorSheet::Limit)) }
                    Row { title: t(&translations, "config.wake_check"), value: check_summary, on_click: move |_| sheet.set(Some(EditorSheet::WakeCheck)) }
                }
            }

            div { class: "save-bar",
                if !for_defaults {
                    button {
                        class: "btn btn-quiet try-btn",
                        onclick: {
                            let build = build.clone();
                            move |_| {
                                if try_running() {
                                    stop_sound();
                                    return;
                                }
                                let alarm = build();
                                if alarm.sound_mode != 1 {
                                    crate::feedback::pattern(crate::platform::VIBRATION_PATTERNS[usize::from(alarm.vibration).min(2)]);
                                }
                                if alarm.sound_mode == 2 {
                                    return;
                                }
                                play_for(alarm.sound, TRY_MS);
                                try_running.set(true);
                            }
                        },
                        if try_running() {
                            {t(&translations, "config.try_stop")}
                        } else {
                            {t(&translations, "config.try_ring")}
                        }
                    }
                }
                button { class: "btn btn-primary save-btn", onclick: save, {t(&translations, "action.save")} }
            }

            {sheet_view}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_typed_time;

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
