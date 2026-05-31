use crate::app::{t, AlarmData, Route, Translations};
use crate::components::time_picker::TimePicker;
use crate::components::{Icon, IconName};
use dioxus::prelude::*;

/// Stop any currently playing preview sound via the gated audio capability.
fn stop_preview(handle_id: u64) {
    if handle_id == 0 {
        return;
    }
    let handle = mobile_sentinel::PlaybackHandle::from_id(handle_id);
    let _ = mobile_sentinel::audio::stop(&handle);
}

/// Play a sound preview via the gated audio capability. Returns the new
/// handle ID (0 on failure).
fn play_preview(sound_name: &str) -> u64 {
    // Get the file path for any sound (default or custom)
    let path = match crate::custom_sounds::get_playback_path(sound_name) {
        Some(p) => p,
        None => {
            eprintln!("[AlarmFree] No playback path found for: {}", sound_name);
            return 0;
        }
    };
    match mobile_sentinel::audio::play(&path.to_string_lossy(), false) {
        Ok(handle) => handle.id(),
        Err(e) => {
            eprintln!("[AlarmFree] Preview play failed: {}", e);
            0
        }
    }
}

/// All bundled alarm sounds — loaded dynamically from the sounds/default directory.
fn get_default_sounds() -> Vec<String> {
    crate::custom_sounds::list_default_sounds()
}

/// Translation keys for challenge types.
const CHALLENGE_KEYS: &[&str] = &[
    "challenge.math",
    "challenge.scan",
    "challenge.shake",
    "challenge.steps",
    "challenge.memory",
    "challenge.typing",
    "challenge.hold",
    "challenge.reaction",
];

/// Difficulty level values (value, translation key) for math/memory/typing challenges.
const DIFFICULTY_VALUES: &[(&str, &str)] = &[
    ("easy", "difficulty.easy"),
    ("medium", "difficulty.medium"),
    ("hard", "difficulty.hard"),
    ("extreme", "difficulty.extreme"),
];

/// Challenge description translation keys (matched to CHALLENGE_KEYS order).
const CHALLENGE_DESC_KEYS: &[&str] = &[
    "challenge.desc.math",
    "challenge.desc.scan",
    "challenge.desc.shake",
    "challenge.desc.steps",
    "challenge.desc.memory",
    "challenge.desc.typing",
    "challenge.desc.hold",
    "challenge.desc.reaction",
];

/// Returns a human-readable summary of a challenge's JSON config.
fn challenge_config_summary(
    translations: &Translations,
    type_key: &str,
    config_json: &str,
) -> String {
    if let Ok(config) = serde_json::from_str::<serde_json::Value>(config_json) {
        if let Some(diff) = config.get("difficulty").and_then(|v| v.as_str()) {
            let key = match diff {
                "easy" => "difficulty.easy",
                "medium" => "difficulty.medium",
                "hard" => "difficulty.hard",
                "extreme" => "difficulty.extreme",
                _ => return diff.to_string(),
            };
            return t(translations, key);
        }
        if let Some(count) = config.get("count").and_then(|v| v.as_u64()) {
            return match type_key {
                "challenge.shake" => t(translations, "challenge.summary.shakes")
                    .replace("{count}", &count.to_string()),
                "challenge.steps" => t(translations, "challenge.summary.steps")
                    .replace("{count}", &count.to_string()),
                "challenge.hold" => {
                    let secs = config
                        .get("hold_ms")
                        .and_then(|v| v.as_u64())
                        .map(|ms| ms / 1000)
                        .unwrap_or(3);
                    t(translations, "challenge.summary.hold")
                        .replace("{count}", &count.to_string())
                        .replace("{secs}", &secs.to_string())
                }
                "challenge.reaction" => {
                    let secs = config
                        .get("time_ms")
                        .and_then(|v| v.as_u64())
                        .map(|ms| ms / 1000)
                        .unwrap_or(3);
                    let rounds = config.get("rounds").and_then(|v| v.as_u64()).unwrap_or(2);
                    t(translations, "challenge.summary.reaction")
                        .replace("{count}", &count.to_string())
                        .replace("{secs}", &secs.to_string())
                        .replace("{rounds}", &rounds.to_string())
                }
                _ => format!("{} count", count),
            };
        }
        if let Some(value) = config.get("value").and_then(|v| v.as_str()) {
            if value.is_empty() {
                return t(translations, "challenge.summary.any");
            }
            return if value.len() > 15 {
                format!("{}...", &value[..12])
            } else {
                value.to_string()
            };
        }
        if config.as_object().is_some_and(|o| o.is_empty()) {
            return t(translations, "challenge.summary.default");
        }
    }
    config_json.to_string()
}

/// Returns true if the challenge type uses a difficulty selector.
fn challenge_uses_difficulty(type_key: &str) -> bool {
    matches!(
        type_key,
        "challenge.math" | "challenge.memory" | "challenge.typing"
    )
}

/// Returns an example string showing what a difficulty level looks like for a given challenge type.
fn difficulty_example(translations: &Translations, type_key: &str, difficulty: &str) -> String {
    let key = match (type_key, difficulty) {
        ("challenge.math", "easy") => "challenge.example.math_easy",
        ("challenge.math", "medium") => "challenge.example.math_medium",
        ("challenge.math", "hard") => "challenge.example.math_hard",
        ("challenge.math", "extreme") => "challenge.example.math_extreme",
        ("challenge.memory", "easy") => "challenge.example.memory_easy",
        ("challenge.memory", "medium") => "challenge.example.memory_medium",
        ("challenge.memory", "hard") => "challenge.example.memory_hard",
        ("challenge.memory", "extreme") => "challenge.example.memory_extreme",
        ("challenge.typing", "easy") => "challenge.example.typing_easy",
        ("challenge.typing", "medium") => "challenge.example.typing_medium",
        ("challenge.typing", "hard") => "challenge.example.typing_hard",
        ("challenge.typing", "extreme") => "challenge.example.typing_extreme",
        _ => return String::new(),
    };
    t(translations, key)
}

/// Translation keys for day abbreviations.
const DAY_KEYS: &[&str] = &[
    "day.mon", "day.tue", "day.wed", "day.thu", "day.fri", "day.sat", "day.sun",
];

/// Translation keys for sound mode options.
const SOUND_MODE_KEYS: &[&str] = &[
    "config.sound_vibration",
    "config.sound_only",
    "config.vibration_only",
];

/// Picker field values parsed from a stored challenge config JSON. Used to
/// pre-fill the challenge overlay when editing an existing challenge.
struct ParsedPickerConfig {
    difficulty: String,
    shake_count: u32,
    step_count: u32,
    hold_count: u32,
    hold_secs: u32,
    scan_value: String,
    reaction_targets: u32,
    reaction_secs: u32,
    reaction_rounds: u32,
}

/// Parse a stored `(type_key, config_json)` pair back into picker field
/// values so the overlay can open in "edit" mode pre-filled with the
/// challenge's current settings. Unset fields fall back to the same
/// defaults used when adding a fresh challenge.
fn parse_picker_config(type_key: &str, config_json: &str) -> ParsedPickerConfig {
    let config: serde_json::Value =
        serde_json::from_str(config_json).unwrap_or_else(|_| serde_json::json!({}));

    let difficulty = config
        .get("difficulty")
        .and_then(|v| v.as_str())
        .unwrap_or("medium")
        .to_string();
    let count = config.get("count").and_then(|v| v.as_u64());
    let hold_secs = config
        .get("hold_ms")
        .and_then(|v| v.as_u64())
        .map(|ms| (ms / 1000) as u32)
        .unwrap_or(3);
    let value = config
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    ParsedPickerConfig {
        difficulty,
        shake_count: if type_key == "challenge.shake" {
            count.unwrap_or(20) as u32
        } else {
            20
        },
        step_count: if type_key == "challenge.steps" {
            count.unwrap_or(20) as u32
        } else {
            20
        },
        hold_count: if type_key == "challenge.hold" {
            count.unwrap_or(10) as u32
        } else {
            10
        },
        hold_secs,
        scan_value: if type_key == "challenge.scan" {
            value
        } else {
            String::new()
        },
        reaction_targets: if type_key == "challenge.reaction" {
            count.unwrap_or(10) as u32
        } else {
            10
        },
        reaction_secs: config
            .get("time_ms")
            .and_then(|v| v.as_u64())
            .map(|ms| (ms / 1000) as u32)
            .unwrap_or(3),
        reaction_rounds: config
            .get("rounds")
            .and_then(|v| v.as_u64())
            .map(|r| r as u32)
            .unwrap_or(2),
    }
}

/// Alarm configuration screen for creating/editing alarms.
/// Reads and writes alarm data to/from shared global context.
#[component]
pub fn AlarmConfig(id: String) -> Element {
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let translations = use_context::<Translations>();
    let nav = use_navigator();
    let is_new = id == "new";
    let title = if is_new {
        t(&translations, "alarm.create")
    } else {
        t(&translations, "alarm.edit")
    };

    // Load existing alarm data if editing
    let existing = if !is_new {
        alarms.read().iter().find(|a| a.id == id).cloned()
    } else {
        None
    };

    // Form state - initialized from existing alarm or defaults
    let mut hour = use_signal(|| existing.as_ref().map_or(7u8, |a| a.time_hour));
    let mut minute = use_signal(|| existing.as_ref().map_or(0u8, |a| a.time_minute));
    let mut label_value =
        use_signal(|| existing.as_ref().map_or(String::new(), |a| a.label.clone()));
    let mut schedule_mode = use_signal(|| "weekdays".to_string());
    let mut selected_days = use_signal(|| existing.as_ref().map_or([false; 7], |a| a.days));
    // Prefill the date field with today (YYYY-MM-DD — the format the native
    // <input type="date"> expects) so switching to "specific date" mode never
    // shows an empty picker.
    let mut date_value = use_signal(|| {
        chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()
    });
    let mut sound_value = use_signal(|| {
        existing.as_ref().map_or_else(
            || {
                get_default_sounds()
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "alarm".to_string())
            },
            |a| a.sound.clone(),
        )
    });
    let mut show_sound_picker = use_signal(|| false);
    let mut show_file_picker = use_signal(|| false);
    let mut playing_sound = use_signal(String::new);
    let mut preview_handle_id = use_signal(|| 0u64);
    let mut pick_audio_requested = use_signal(|| false);
    let mut sound_mode_index = use_signal(|| existing.as_ref().map_or(0usize, |a| a.sound_mode));
    let default_sounds_list = use_signal(get_default_sounds);

    let mut challenges: Signal<Vec<(String, String)>> = use_signal(|| {
        existing
            .as_ref()
            .map_or(Vec::new(), |a| a.challenges.clone())
    });
    let mut show_challenge_picker = use_signal(|| false);
    let mut picker_selected_type = use_signal(String::new);
    let mut editing_index: Signal<Option<usize>> = use_signal(|| None);
    let mut picker_difficulty = use_signal(|| "medium".to_string());
    let mut picker_shake_count = use_signal(|| 20u32);
    let mut picker_step_count = use_signal(|| 20u32);
    let mut picker_hold_count = use_signal(|| 10u32);
    let mut picker_hold_secs = use_signal(|| 3u32);
    let mut picker_reaction_targets = use_signal(|| 10u32);
    let mut picker_reaction_secs = use_signal(|| 3u32);
    let mut picker_reaction_rounds = use_signal(|| 2u32);
    let mut picker_scan_value = use_signal(String::new);
    let mut scan_requested = use_signal(|| false);
    let mut snooze_count = use_signal(|| existing.as_ref().map_or(3u8, |a| a.snooze_count));
    let mut snooze_interval = use_signal(|| existing.as_ref().map_or(1u8, |a| a.snooze_interval));
    let mut show_delete_confirm = use_signal(|| false);

    // Background task for native file picker
    let _pick_task = use_future(move || async move {
        loop {
            // Poll for pick request
            loop {
                let (tx, rx) = futures_channel::oneshot::channel::<()>();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let _ = tx.send(());
                });
                let _ = rx.await;
                if *pick_audio_requested.read() {
                    break;
                }
            }
            pick_audio_requested.set(false);

            let (tx, rx) = futures_channel::oneshot::channel::<String>();
            std::thread::spawn(move || {
                let result = crate::custom_sounds::pick_audio_file_native();
                match result {
                    Ok(filename) => {
                        let _ = tx.send(filename);
                    }
                    Err(_) => {
                        let _ = tx.send(String::new());
                    }
                }
            });

            if let Ok(filename) = rx.await {
                if !filename.is_empty() {
                    sound_value.set(filename);
                    show_sound_picker.set(false);
                    show_file_picker.set(false);
                    playing_sound.set(String::new());
                    preview_handle_id.set(0);
                }
            }
        }
    });

    // Background task for code scanning (barcode / QR)
    let _scan_task = use_future(move || async move {
        loop {
            // Poll for scan request
            loop {
                let (tx, rx) = futures_channel::oneshot::channel::<()>();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let _ = tx.send(());
                });
                let _ = rx.await;
                if *scan_requested.read() {
                    break;
                }
            }

            scan_requested.set(false);

            let (tx, rx) = futures_channel::oneshot::channel::<String>();
            std::thread::spawn(move || {
                // Scanning goes through the feature-gated `camera` module —
                // the only sanctioned entry point. The raw JNI facade is
                // crate-private, so there is no bypass.
                let result = mobile_sentinel::scanner::scan().unwrap_or_default();
                let _ = tx.send(result);
            });

            if let Ok(value) = rx.await {
                if !value.is_empty() {
                    picker_scan_value.set(value);
                }
            }
        }
    });

    let mut validation_errors: Signal<std::collections::HashMap<String, String>> =
        use_signal(std::collections::HashMap::new);

    // Translate day labels
    let day_labels: Vec<String> = DAY_KEYS.iter().map(|k| t(&translations, k)).collect();

    // Delete confirmation dialog
    if *show_delete_confirm.read() {
        let delete_id = id.clone();
        let cancel_text = t(&translations, "action.cancel");
        let delete_text = t(&translations, "action.delete");
        let delete_confirm_text = t(&translations, "alarm.delete_confirm");
        let delete_warning_text = t(&translations, "alarm.delete_warning");
        return rsx! {
                   div { class: "screen alarm-config-screen",
                       div { class: "confirm-dialog-overlay",
                           div { class: "confirm-dialog",
                               h3 { "{delete_confirm_text}" }
                               p { "{delete_warning_text}" }
                               div { class: "confirm-actions",
                                   button {
                                       class: "btn-cancel",
                                       onclick: move |_| { show_delete_confirm.set(false); },
                                       "{cancel_text}"
                                   }
                                   button {
                                       class: "btn-delete",
                                       onclick: move |_| {
        // Cancel platform alarm first
                                           {
                                               crate::platform::cancel_alarm_from_ui(&delete_id);
                                           }
        // Delete custom sound file if this alarm uses one
                                           if let Some(alarm) = alarms.read().iter().find(|a| a.id == delete_id) {
                                               if crate::custom_sounds::is_custom_sound(&alarm.sound) {
                                                   crate::custom_sounds::delete_sound(&alarm.sound);
                                               }
                                           }
        // Remove alarm from shared state
                                           alarms.write().retain(|a| a.id != delete_id);
        // Persist to disk
                                           crate::persistence::save_alarms(&alarms.read());
                                           nav.push(Route::Home {});
                                       },
                                       "{delete_text}"
                                   }
                               }
                           }
                       }
                   }
               };
    }

    let time_label = t(&translations, "config.time");
    let schedule_label = t(&translations, "config.repeat");
    let label_label = t(&translations, "config.label");
    let label_placeholder = t(&translations, "config.label_placeholder");
    let sound_label = t(&translations, "config.sound");
    let sound_mode_label = t(&translations, "config.sound_mode");
    let sound_picker_title = t(&translations, "sound.select");
    let close_btn_text = t(&translations, "action.close");
    let import_sound_text = t(&translations, "sound.import");
    let preview_sound_text = t(&translations, "sound.preview");
    let challenges_label = t(&translations, "config.challenges");
    let snooze_label = t(&translations, "config.snooze");
    let snooze_count_label = t(&translations, "config.snooze_count");
    let snooze_interval_label = t(&translations, "config.snooze_interval");
    let decrease_text = t(&translations, "action.decrease");
    let increase_text = t(&translations, "action.increase");

    let _back_text = t(&translations, "action.back");
    let weekdays_text = t(&translations, "config.weekdays");
    let specific_date_text = t(&translations, "config.specific_date");
    let select_challenge_text = t(&translations, "config.select_challenge");
    let cancel_text = t(&translations, "action.cancel");
    let add_challenge_text = t(&translations, "config.add_challenge");
    let edit_challenge_text = t(&translations, "config.edit_challenge");
    let remove_challenge_text = t(&translations, "config.remove_challenge");

    // Translate sound mode labels
    let sound_mode_labels: Vec<String> = SOUND_MODE_KEYS
        .iter()
        .map(|k| t(&translations, k))
        .collect();

    // Translate challenge type options
    let challenge_labels: Vec<String> =
        CHALLENGE_KEYS.iter().map(|k| t(&translations, k)).collect();

    rsx! {
    // Challenge picker overlay
           if *show_challenge_picker.read() {
               div {
                   class: "challenge-overlay",
                   onclick: move |_| {
                       show_challenge_picker.set(false);
                       picker_selected_type.set(String::new());
                       editing_index.set(None);
                   },
                   div {
                       class: "challenge-overlay-content",
                       onclick: move |evt| { evt.stop_propagation(); },

                       if picker_selected_type.read().is_empty() {
                           h3 { "{select_challenge_text}" }
                           div { class: "challenge-type-list",
                               for (i, ct_label) in challenge_labels.iter().enumerate() {
                                   {
                                       let ct_key = CHALLENGE_KEYS[i].to_string();
                                       let ctl = ct_label.clone();
                                       let desc = t(&translations, CHALLENGE_DESC_KEYS[i]);
                                       rsx! {
                                           button {
                                               class: "challenge-type-option",
                                               onclick: move |_| {
                                                   picker_selected_type.set(ct_key.clone());
                                                   picker_difficulty.set("medium".to_string());
                                                   picker_shake_count.set(20);
                                                   picker_step_count.set(20);
                                                   picker_hold_count.set(10);
                                                   picker_hold_secs.set(3);
                                                   picker_reaction_targets.set(10);
                                                   picker_reaction_secs.set(3);
                                                   picker_reaction_rounds.set(2);
                                                   picker_scan_value.set(String::new());
                                               },
                                               span { class: "type-name", "{ctl}" }
                                               span { class: "type-desc", "{desc}" }
                                           }
                                       }
                                   }
                               }
                           }
                           div { class: "overlay-actions",
                               button {
                                   class: "overlay-btn-cancel",
                                   onclick: move |_| {
                                       show_challenge_picker.set(false);
                                       picker_selected_type.set(String::new());
                                       editing_index.set(None);
                                   },
                                   "{cancel_text}"
                               }
                           }
                       } else {
    // Config mode for selected type
                           {
                               let selected_type = picker_selected_type.read().clone();
                               let type_display = t(&translations, &selected_type);
                               let type_idx = CHALLENGE_KEYS.iter().position(|k| *k == selected_type.as_str()).unwrap_or(0);
                               let type_desc = t(&translations, CHALLENGE_DESC_KEYS[type_idx]);
                               let difficulty_label = t(&translations, "challenge.config.difficulty");
                               let shakes_label = t(&translations, "challenge.config.how_many_shakes");
                               let steps_label = t(&translations, "challenge.config.how_many_steps");
                               let hold_count_label = t(&translations, "challenge.config.how_many_holds");
                               let hold_secs_label = t(&translations, "challenge.config.hold_duration");
                               let reaction_targets_label = t(&translations, "challenge.config.how_many_targets");
                               let reaction_time_label = t(&translations, "challenge.config.time_limit");
                               let reaction_rounds_label = t(&translations, "challenge.config.how_many_rounds");
                               let scan_label = t(&translations, "challenge.config.scan_value");
                               let scan_placeholder = t(&translations, "challenge.config.scan_placeholder");
                               let scan_tip = t(&translations, "challenge.config.scan_tip");
                               let back_btn_text = t(&translations, "challenge.config.back");
                               let add_btn_text = if editing_index.read().is_some() {
                                   t(&translations, "action.save")
                               } else {
                                   t(&translations, "challenge.config.add")
                               };
                               let scan_btn_text = t(&translations, "challenge.config.scan");
                               rsx! {
                                   h3 { "{type_display}" }
                                   p { class: "overlay-desc", "{type_desc}" }

                                   if challenge_uses_difficulty(&selected_type) {
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{difficulty_label}" }
                                           div { class: "difficulty-segments",
                                               for (val, label_key) in DIFFICULTY_VALUES.iter() {
                                                   {
                                                       let v = val.to_string();
                                                       let l = t(&translations, label_key);
                                                       let is_active = *picker_difficulty.read() == v;
                                                       let btn_class = if is_active { "active" } else { "" };
                                                       rsx! {
                                                           button {
                                                               class: "{btn_class}",
                                                               onclick: move |_| { picker_difficulty.set(v.clone()); },
                                                               "{l}"
                                                           }
                                                       }
                                                   }
                                               }
                                           }
                                           {
                                               let diff = picker_difficulty.read().clone();
                                               let example = difficulty_example(&translations, &selected_type, &diff);
                                               rsx! {
                                                   p { class: "overlay-desc",
                                                       style: "margin-top: 12px; font-family: monospace; text-align: center; font-size: 16px;",
                                                       "{example}"
                                                   }
                                               }
                                           }
                                       }
                                   }

                                   if selected_type == "challenge.shake" {
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{shakes_label}" }
                                           div { class: "count-display", "{picker_shake_count}" }
                                           div { class: "count-controls",
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_shake_count.read();
                                                       if current > 5 { picker_shake_count.set(current - 5); }
                                                   },
                                                   "\u{2212}"
                                               }
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_shake_count.read();
                                                       if current < 100 { picker_shake_count.set(current + 5); }
                                                   },
                                                   "+"
                                               }
                                           }
                                       }
                                   }

                                   if selected_type == "challenge.steps" {
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{steps_label}" }
                                           div { class: "count-display", "{picker_step_count}" }
                                           div { class: "count-controls",
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_step_count.read();
                                                       if current > 5 { picker_step_count.set(current - 5); }
                                                   },
                                                   "\u{2212}"
                                               }
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_step_count.read();
                                                       if current < 200 { picker_step_count.set(current + 5); }
                                                   },
                                                   "+"
                                               }
                                           }
                                       }
                                   }

                                   if selected_type == "challenge.hold" {
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{hold_count_label}" }
                                           div { class: "count-display", "{picker_hold_count}" }
                                           div { class: "count-controls",
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_hold_count.read();
                                                       if current > 1 { picker_hold_count.set(current - 1); }
                                                   },
                                                   "\u{2212}"
                                               }
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_hold_count.read();
                                                       if current < 20 { picker_hold_count.set(current + 1); }
                                                   },
                                                   "+"
                                               }
                                           }
                                       }
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{hold_secs_label}" }
                                           div { class: "count-display", "{picker_hold_secs}s" }
                                           div { class: "count-controls",
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_hold_secs.read();
                                                       if current > 1 { picker_hold_secs.set(current - 1); }
                                                   },
                                                   "\u{2212}"
                                               }
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_hold_secs.read();
                                                       if current < 15 { picker_hold_secs.set(current + 1); }
                                                   },
                                                   "+"
                                               }
                                           }
                                       }
                                   }

                                   if selected_type == "challenge.reaction" {
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{reaction_targets_label}" }
                                           div { class: "count-display", "{picker_reaction_targets}" }
                                           div { class: "count-controls",
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_reaction_targets.read();
                                                       if current > 3 { picker_reaction_targets.set(current - 1); }
                                                   },
                                                   "\u{2212}"
                                               }
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_reaction_targets.read();
                                                       if current < 30 { picker_reaction_targets.set(current + 1); }
                                                   },
                                                   "+"
                                               }
                                           }
                                       }
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{reaction_time_label}" }
                                           div { class: "count-display", "{picker_reaction_secs}s" }
                                           div { class: "count-controls",
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_reaction_secs.read();
                                                       if current > 2 { picker_reaction_secs.set(current - 1); }
                                                   },
                                                   "\u{2212}"
                                               }
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_reaction_secs.read();
                                                       if current < 10 { picker_reaction_secs.set(current + 1); }
                                                   },
                                                   "+"
                                               }
                                           }
                                       }
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{reaction_rounds_label}" }
                                           div { class: "count-display", "{picker_reaction_rounds}" }
                                           div { class: "count-controls",
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_reaction_rounds.read();
                                                       if current > 1 { picker_reaction_rounds.set(current - 1); }
                                                   },
                                                   "\u{2212}"
                                               }
                                               button {
                                                   onclick: move |_| {
                                                       let current = *picker_reaction_rounds.read();
                                                       if current < 5 { picker_reaction_rounds.set(current + 1); }
                                                   },
                                                   "+"
                                               }
                                           }
                                       }
                                   }

                                   if selected_type == "challenge.scan" {
                                       div { class: "overlay-section",
                                           div { class: "overlay-label", "{scan_label}" }
                                           div { style: "display: flex; gap: 8px; align-items: center;",
                                               input {
                                                   r#type: "text",
                                                   placeholder: "{scan_placeholder}",
                                                   value: "{picker_scan_value}",
                                                   style: "flex: 1;",
                                                   oninput: move |evt| { picker_scan_value.set(evt.value()); },
                                               }
                                               button {
                                                   class: "overlay-btn-add",
                                                   style: "flex: 0; padding: 12px 16px; white-space: nowrap;",
                                                   onclick: move |_| {
    #[cfg(not(target_os = "android"))]
                                                       picker_scan_value.set("simulated-code".to_string());
    #[cfg(target_os = "android")]
                                                       scan_requested.set(true);
                                                   },
                                                   "{scan_btn_text}"
                                               }
                                           }
                                           p { class: "overlay-desc", "{scan_tip}" }
                                       }
                                   }

    // Action buttons: Back + Add
                                   div { class: "overlay-actions",
                                       button {
                                           class: "overlay-btn-cancel",
                                           onclick: move |_| {
                                               if editing_index.read().is_some() {
                                                   // Editing an existing challenge — there is no type
                                                   // list behind us, so Back closes the overlay.
                                                   show_challenge_picker.set(false);
                                                   editing_index.set(None);
                                               }
                                               picker_selected_type.set(String::new());
                                           },
                                           "{back_btn_text}"
                                       }
                                       button {
                                           class: "overlay-btn-add",
                                           onclick: move |_| {
                                               let st = picker_selected_type.read().clone();

                                               // Check permissions for challenges that need them
                                               #[cfg(target_os = "android")]
                                               {
                                                   let permission_needed = match st.as_str() {
                                                       "challenge.steps" => Some("android.permission.ACTIVITY_RECOGNITION"),
                                                       "challenge.scan" => Some("android.permission.CAMERA"),
                                                       _ => None,
                                                   };
                                                   if let Some(perm) = permission_needed {
                                                       let _ = mobile_sentinel::permissions::request(perm);
                                                   }
                                               }

                                               let config_json = match st.as_str() {
                                                   "challenge.math" | "challenge.memory" | "challenge.typing" => {
                                                       format!("{{\"difficulty\":\"{}\"}}", picker_difficulty.read())
                                                   }
                                                   "challenge.shake" => {
                                                       format!("{{\"count\":{}}}", picker_shake_count.read())
                                                   }
                                                   "challenge.steps" => {
                                                       format!("{{\"count\":{}}}", picker_step_count.read())
                                                   }
                                                   "challenge.hold" => {
                                                       format!(
                                                           "{{\"count\":{},\"hold_ms\":{}}}",
                                                           picker_hold_count.read(),
                                                           *picker_hold_secs.read() * 1000
                                                       )
                                                   }
                                                   "challenge.reaction" => {
                                                       format!(
                                                           "{{\"count\":{},\"time_ms\":{},\"rounds\":{}}}",
                                                           picker_reaction_targets.read(),
                                                           *picker_reaction_secs.read() * 1000,
                                                           picker_reaction_rounds.read()
                                                       )
                                                   }
                                                   "challenge.scan" => {
                                                       let sanitized = crate::sanitize::sanitize_reference_value(&picker_scan_value.read());
                                                       format!("{{\"value\":\"{}\"}}", sanitized)
                                                   }
                                                   _ => "{}".to_string(),
                                               };
                                               match *editing_index.read() {
                                                   Some(idx) => {
                                                       let mut list = challenges.write();
                                                       if idx < list.len() {
                                                           list[idx] = (st, config_json);
                                                       } else {
                                                           list.push((st, config_json));
                                                       }
                                                   }
                                                   None => {
                                                       challenges.write().push((st, config_json));
                                                   }
                                               }
                                               show_challenge_picker.set(false);
                                               picker_selected_type.set(String::new());
                                               editing_index.set(None);
                                           },
                                           "{add_btn_text}"
                                       }
                                   }
                               }
                           }
                       }
                   }
               }
           }

    // Main config screen content
           div { class: "screen alarm-config-screen",
    // Header with back, title, save, delete
               div { class: "config-header",
                   button {
                       class: "back-btn",
                       onclick: move |_| {
    // Stop any playing preview
                           {
                               let hid = *preview_handle_id.read();
                               if hid != 0 {
                                   stop_preview(hid);
                               }
                           }
                           nav.push(Route::Home {});
                       },
                       Icon { name: IconName::ArrowLeft, size: 16 }
                   }
                   span { class: "config-title", "{title}" }
                   div { class: "header-actions",
                       if !is_new {
                           button {
                               class: "header-icon-btn delete-icon",
                               onclick: move |_| { show_delete_confirm.set(true); },
                               Icon { name: IconName::Trash }
                           }
                       }
                       button {
                           class: "header-icon-btn save-icon",
                           onclick: move |_| {
    // Stop any playing preview
                               {
                                   let hid = *preview_handle_id.read();
                                   if hid != 0 {
                                       stop_preview(hid);
                                   }
                               }

    // Validate
                               let mut errors = std::collections::HashMap::new();
                               let h = *hour.read();
                               let m = *minute.read();
                               if h > 23 {
                                   errors.insert("time".to_string(), t(&translations, "validation.hour"));
                               }
                               if m > 59 {
                                   errors.insert("time".to_string(), t(&translations, "validation.minute"));
                               }
                               if *schedule_mode.read() == "weekdays"
                                   && !selected_days.read().iter().any(|&d| d)
                               {
                                   errors.insert("schedule".to_string(), t(&translations, "validation.select_day"));
                               }
                               if label_value.read().len() > 100 {
                                   errors.insert("label".to_string(), t(&translations, "validation.label_too_long"));
                               }
                               let sc = *snooze_count.read();
                               if sc > 10 {
                                   errors.insert("snooze_count".to_string(), t(&translations, "validation.snooze_count"));
                               }
                               let si = *snooze_interval.read();
                               if si == 0 || si > 30 {
                                   errors.insert("snooze_interval".to_string(), t(&translations, "validation.snooze_interval"));
                               }
                               if challenges.read().len() > 10 {
                                   errors.insert("challenges".to_string(), t(&translations, "validation.max_challenges"));
                               }

                               if !errors.is_empty() {
                                   validation_errors.set(errors);
                                   return;
                               }
                               validation_errors.set(std::collections::HashMap::new());

    // Build alarm data
                               let alarm_data = AlarmData {
                                   id: if is_new {
                                       uuid::Uuid::new_v4().to_string()
                                   } else {
                                       id.clone()
                                   },
                                   time_hour: *hour.read(),
                                   time_minute: *minute.read(),
                                   label: crate::sanitize::sanitize_label(&label_value.read()),
                                   days: *selected_days.read(),
                                   enabled: true,
                                   sound: sound_value.read().clone(),
                                   sound_mode: *sound_mode_index.read(),
                                   challenges: challenges.read().clone(),
                                   snooze_count: *snooze_count.read(),
                                   snooze_interval: *snooze_interval.read(),
                               };

    // Save to state
                               {
                                   let mut list = alarms.write();
                                   if let Some(existing) = list.iter_mut().find(|a| a.id == alarm_data.id) {
    *existing = alarm_data.clone();
                                   } else {
                                       list.push(alarm_data.clone());
                                   }
                               }

    // Persist to disk
                               crate::persistence::save_alarms(&alarms.read());

    // Schedule with the platform AlarmManager
                               {
                                   eprintln!("[AlarmFree] SAVE: Scheduling alarm {} via platform", alarm_data.id);
                                   crate::platform::schedule_alarm_from_ui(&alarm_data);
                               }

                               nav.push(Route::Home {});
                           },
                           Icon { name: IconName::Check }
                       }
                   }
               }

    // Time section
               div { class: "config-section",
                   label { class: "section-label", "{time_label}" }
                   TimePicker {
                       hour: *hour.read(),
                       minute: *minute.read(),
                       on_change: move |(h, m)| {
                           hour.set(h);
                           minute.set(m);
                       },
                   }
               }

    // Label section
               div { class: "config-section",
                   label { class: "section-label", "{label_label}" }
                   input {
                       r#type: "text",
                       placeholder: "{label_placeholder}",
                       value: "{label_value}",
                       oninput: move |evt| {
                           label_value.set(evt.value());
                           validation_errors.write().remove("label");
                       },
                   }
                   if let Some(err) = validation_errors.read().get("label") {
                       p { style: "color: var(--error); font-size: 12px; margin-top: 4px;", "{err}" }
                   }
               }

    // Schedule section
               div { class: "config-section",
                   label { class: "section-label", "{schedule_label}" }
                   div { class: "schedule-toggle",
                       button {
                           class: if *schedule_mode.read() == "weekdays" { "toggle-btn active" } else { "toggle-btn" },
                           onclick: move |_| { schedule_mode.set("weekdays".to_string()); },
                           "{weekdays_text}"
                       }
                       button {
                           class: if *schedule_mode.read() == "date" { "toggle-btn active" } else { "toggle-btn" },
                           onclick: move |_| { schedule_mode.set("date".to_string()); },
                           "{specific_date_text}"
                       }
                   }
                   if *schedule_mode.read() == "weekdays" {
                       div { class: "weekday-selector",
                           for (i, day_label) in day_labels.iter().enumerate() {
                               {
                                   let dl = day_label.clone();
                                   let is_active = selected_days.read()[i];
                                   rsx! {
                                       button {
                                           class: if is_active { "day-btn active" } else { "day-btn" },
                                           onclick: move |_| {
                                               let mut days = *selected_days.read();
                                               days[i] = !days[i];
                                               selected_days.set(days);
                                           },
                                           "{dl}"
                                       }
                                   }
                               }
                           }
                       }
                   } else {
                       input {
                           r#type: "date",
                           class: "date-input",
                           value: "{date_value}",
                           onchange: move |evt| { date_value.set(evt.value()); },
                       }
                   }
                   if let Some(err) = validation_errors.read().get("schedule") {
                       p { style: "color: var(--error); font-size: 12px; margin-top: 4px;", "{err}" }
                   }
               }

    // Sound section
               div { class: "config-section",
                   label { class: "section-label", "{sound_label}" }
                   button {
                       class: "dropdown-trigger",
                       onclick: move |_| {
                           {
                               let hid = *preview_handle_id.read();
                               if hid != 0 {
                                   stop_preview(hid);
                               }
                           }
                           let current = *show_sound_picker.read();
                           show_sound_picker.set(!current);
                       },
                       span { class: "dropdown-value", "{crate::custom_sounds::display_name_translated(&sound_value.read(), &translations)}" }
                       span { class: "dropdown-arrow", Icon { name: IconName::ChevronDown, size: 12 } }
                   }

                   if *show_sound_picker.read() {
                       div {
                           class: "challenge-overlay",
                           onclick: move |_| {
    // Stop preview and close
                               let hid = *preview_handle_id.read();
                               if hid != 0 {
                                   stop_preview(hid);
                               }
                               playing_sound.set(String::new());
                               preview_handle_id.set(0);
                               show_sound_picker.set(false);
                           },
                           div {
                               class: "challenge-overlay-content",
                               onclick: move |evt| { evt.stop_propagation(); },
                               h3 { "{sound_picker_title}" }

    // Default sounds
                               div { class: "challenge-type-list",
                                   for sound_name in default_sounds_list.read().clone().into_iter() {
                                       {
                                           let sn = sound_name.clone();
                                           let sn2 = sound_name.clone();
                                           let sn3 = sound_name.clone();
                                           let is_selected = *sound_value.read() == sn;
                                           let is_playing = *playing_sound.read() == sn;
                                           let option_class = if is_selected { "sound-option selected" } else { "sound-option" };
                                           let preview_class = if is_playing { "sound-preview-btn playing" } else { "sound-preview-btn" };
                                           rsx! {
                                               div {
                                                   class: "{option_class}",
                                                   onclick: move |_| {
                                                       let hid = *preview_handle_id.read();
                                                       if hid != 0 {
                                                           stop_preview(hid);
                                                       }
                                                       sound_value.set(sn2.clone());
                                                       playing_sound.set(String::new());
                                                       preview_handle_id.set(0);
                                                       show_sound_picker.set(false);
                                                   },
                                                   span { class: "sound-name", "{sn}" }
                                                   if is_selected {
                                                       span { class: "sound-selected-check",
                                                           Icon { name: IconName::Check, size: 14 }
                                                       }
                                                   }
                                                   button {
                                                       class: "{preview_class}",
                                                       "aria-label": "{preview_sound_text}",
                                                       onclick: move |evt| {
                                                           evt.stop_propagation();
                                                           if is_playing {
                                                               let hid = *preview_handle_id.read();
                                                               if hid != 0 {
                                                                   stop_preview(hid);
                                                               }
                                                               playing_sound.set(String::new());
                                                               preview_handle_id.set(0);
                                                           } else {
                                                               let hid = *preview_handle_id.read();
                                                               if hid != 0 {
                                                                   stop_preview(hid);
                                                               }
                                                               let new_hid = play_preview(&sn3);
                                                               preview_handle_id.set(new_hid);
                                                               playing_sound.set(sn3.clone());
                                                           }
                                                       },
                                                       if is_playing {
                                                           Icon { name: IconName::Pause, size: 13 }
                                                       } else {
                                                           Icon { name: IconName::Play, size: 13 }
                                                       }
                                                   }
                                               }
                                           }
                                       }
                                   }

    // Custom sound (if selected)
                                   {
                                       let custom_sound = sound_value.read().clone();
                                       if crate::custom_sounds::is_custom_sound(&custom_sound) {
                                           let display = crate::custom_sounds::display_name_translated(&custom_sound, &translations);
                                           let cs = custom_sound.clone();
                                           let is_custom_playing = *playing_sound.read() == custom_sound;
                                           let preview_class = if is_custom_playing { "sound-preview-btn playing" } else { "sound-preview-btn" };
                                           rsx! {
                                               div {
                                                   class: "sound-option selected",
                                                   span { class: "sound-name", "{display}" }
                                                   span { class: "sound-selected-check",
                                                       Icon { name: IconName::Check, size: 14 }
                                                   }
                                                   button {
                                                       class: "{preview_class}",
                                                       "aria-label": "{preview_sound_text}",
                                                       onclick: move |evt| {
                                                           evt.stop_propagation();
                                                           if is_custom_playing {
                                                               let hid = *preview_handle_id.read();
                                                               if hid != 0 {
                                                                   stop_preview(hid);
                                                               }
                                                               playing_sound.set(String::new());
                                                               preview_handle_id.set(0);
                                                           } else {
                                                               let hid = *preview_handle_id.read();
                                                               if hid != 0 {
                                                                   stop_preview(hid);
                                                               }
                                                               let new_hid = play_preview(&cs);
                                                               preview_handle_id.set(new_hid);
                                                               playing_sound.set(cs.clone());
                                                           }
                                                       },
                                                       if is_custom_playing {
                                                           Icon { name: IconName::Pause, size: 13 }
                                                       } else {
                                                           Icon { name: IconName::Play, size: 13 }
                                                       }
                                                   }
                                               }
                                           }
                                       } else {
                                           rsx! {}
                                       }
                                   }
                               }

    // Actions
                               div { class: "overlay-actions",
                                   button {
                                       class: "overlay-btn-cancel",
                                       onclick: move |_| {
                                           let hid = *preview_handle_id.read();
                                           if hid != 0 {
                                               stop_preview(hid);
                                           }
                                           playing_sound.set(String::new());
                                           preview_handle_id.set(0);
                                           show_sound_picker.set(false);
                                       },
                                       "{close_btn_text}"
                                   }
                                   button {
                                       class: "overlay-btn-add",
                                       onclick: move |_| {
                                           // Stop any running preview before opening the native
                                           // picker — otherwise the previewed tone keeps looping
                                           // behind the import dialog.
                                           let hid = *preview_handle_id.read();
                                           if hid != 0 {
                                               stop_preview(hid);
                                           }
                                           playing_sound.set(String::new());
                                           preview_handle_id.set(0);
                                           pick_audio_requested.set(true);
                                       },
                                       "{import_sound_text}"
                                   }
                               }
                           }
                       }
                   }
               }

    // Sound mode section
               div { class: "config-section",
                   label { class: "section-label", "{sound_mode_label}" }
                   div { class: "radio-group",
                       for (i, mode_label) in sound_mode_labels.iter().enumerate() {
                           {
                               let ml = mode_label.clone();
                               let is_selected = *sound_mode_index.read() == i;
                               rsx! {
                                   div {
                                       class: "radio-item",
                                       onclick: move |_| { sound_mode_index.set(i); },
                                       input {
                                           r#type: "radio",
                                           checked: is_selected,
                                       }
                                       span { class: "radio-label", "{ml}" }
                                   }
                               }
                           }
                       }
                   }
               }

    // Challenges section
               div { class: "config-section",
                   label { class: "section-label", "{challenges_label}" }
                   if !challenges.read().is_empty() {
                       div { class: "challenge-list",
                           for (idx, (type_key, config_json)) in challenges.read().iter().enumerate() {
                               {
                                   let type_label = t(&translations, type_key);
                                   let summary = challenge_config_summary(&translations, type_key, config_json);
                                   let tk = type_key.clone();
                                   let cj = config_json.clone();
                                   rsx! {
                                       div { class: "challenge-item",
                                           div { class: "challenge-info",
                                               span { class: "challenge-name", "{type_label}" }
                                               span { class: "challenge-difficulty", "{summary}" }
                                           }
                                           div { class: "challenge-actions",
                                               button {
                                                   class: "edit-btn",
                                                   "aria-label": "{edit_challenge_text}",
                                                   onclick: move |_| {
                                                       // Pre-fill the picker from the stored config and
                                                       // open straight into config mode for this index.
                                                       let parsed = parse_picker_config(&tk, &cj);
                                                       picker_difficulty.set(parsed.difficulty.clone());
                                                       picker_shake_count.set(parsed.shake_count);
                                                       picker_step_count.set(parsed.step_count);
                                                       picker_hold_count.set(parsed.hold_count);
                                                       picker_hold_secs.set(parsed.hold_secs);
                                                       picker_reaction_targets.set(parsed.reaction_targets);
                                                       picker_reaction_secs.set(parsed.reaction_secs);
                                                       picker_reaction_rounds.set(parsed.reaction_rounds);
                                                       picker_scan_value.set(parsed.scan_value.clone());
                                                       editing_index.set(Some(idx));
                                                       picker_selected_type.set(tk.clone());
                                                       show_challenge_picker.set(true);
                                                   },
                                                   Icon { name: IconName::Pencil, size: 13 }
                                               }
                                               button {
                                                   class: "remove-btn",
                                                   "aria-label": "{remove_challenge_text}",
                                                   onclick: move |_| {
                                                       challenges.write().remove(idx);
                                                   },
                                                   Icon { name: IconName::Xmark, size: 10 }
                                               }
                                           }
                                       }
                                   }
                               }
                           }
                       }
                   }
                   button {
                       class: "add-challenge-btn",
                       onclick: move |_| {
                           #[cfg(target_os = "android")]
                           log::info!("[AlarmFree] Add Challenge button clicked");
                           editing_index.set(None);
                           picker_selected_type.set(String::new());
                           show_challenge_picker.set(true);
                       },
                       "{add_challenge_text}"
                   }
                   if let Some(err) = validation_errors.read().get("challenges") {
                       p { style: "color: var(--error); font-size: 12px; margin-top: 4px;", "{err}" }
                   }
               }

    // Snooze section
               div { class: "config-section",
                   label { class: "section-label", "{snooze_label}" }
                   div { class: "snooze-config",
                       div { class: "snooze-field",
                           span { class: "field-label", "{snooze_count_label}" }
                           div { class: "stepper",
                               button {
                                   class: "stepper-btn",
                                   disabled: *snooze_count.read() == 0,
                                   "aria-label": "{decrease_text}",
                                   onclick: move |_| {
                                       let current = *snooze_count.read();
                                       if current > 0 {
                                           snooze_count.set(current - 1);
                                       }
                                   },
                                   "\u{2212}"
                               }
                               span { class: "stepper-value", "{snooze_count}" }
                               button {
                                   class: "stepper-btn",
                                   disabled: *snooze_count.read() >= 10,
                                   "aria-label": "{increase_text}",
                                   onclick: move |_| {
                                       let current = *snooze_count.read();
                                       if current < 10 {
                                           snooze_count.set(current + 1);
                                       }
                                   },
                                   "+"
                               }
                           }
                       }
                       div { class: "snooze-field",
                           span { class: "field-label", "{snooze_interval_label}" }
                           div { class: "stepper",
                               button {
                                   class: "stepper-btn",
                                   disabled: *snooze_interval.read() <= 1,
                                   "aria-label": "{decrease_text}",
                                   onclick: move |_| {
                                       let current = *snooze_interval.read();
                                       if current > 1 {
                                           snooze_interval.set(current - 1);
                                       }
                                       validation_errors.write().remove("snooze_interval");
                                   },
                                   "\u{2212}"
                               }
                               span { class: "stepper-value", "{snooze_interval}" }
                               button {
                                   class: "stepper-btn",
                                   disabled: *snooze_interval.read() >= 30,
                                   "aria-label": "{increase_text}",
                                   onclick: move |_| {
                                       let current = *snooze_interval.read();
                                       if current < 30 {
                                           snooze_interval.set(current + 1);
                                       }
                                       validation_errors.write().remove("snooze_interval");
                                   },
                                   "+"
                               }
                           }
                       }
                   }
                   if let Some(err) = validation_errors.read().get("snooze_count") {
                       p { style: "color: var(--error); font-size: 12px; margin-top: 4px;", "{err}" }
                   }
                   if let Some(err) = validation_errors.read().get("snooze_interval") {
                       p { style: "color: var(--error); font-size: 12px; margin-top: 4px;", "{err}" }
                   }
               }
           }
       }
}
