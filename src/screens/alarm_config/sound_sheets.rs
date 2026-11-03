//! What it sounds and feels like.

use dioxus::prelude::*;

use super::{seconds_label, EditorState};
use crate::alarm_ops;
use crate::app::{t, AlarmData, Translations};
use crate::components::{Chips, GroupTitle, Icon, IconName, Sheet, SheetDone, ToastMessage};
use crate::models::AppSettings;

/// How long tapping a sound plays it.
const SOUND_PREVIEW_MS: u64 = 5_000;

const FADE_SECONDS: [u16; 6] = [0, 15, 30, 60, 120, 300];
const SOUND_DELAY_SECONDS: [u16; 4] = [0, 15, 30, 60];

/// Translation keys of the vibration patterns, in the order of
/// `platform::VIBRATION_PATTERNS`.
pub(super) const VIBRATION_KEYS: [&str; 3] =
    ["vibration.pulse", "vibration.steady", "vibration.heartbeat"];

/// Which sound, how loud, and how it starts.
#[component]
pub(super) fn SoundSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        mut renaming,
        preview,
        ..
    } = use_context();
    let mut settings = use_context::<Signal<AppSettings>>();
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    let translations = use_context::<Translations>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
    let defaults = crate::custom_sounds::list_default_sounds();
    let custom = crate::custom_sounds::list_custom_sounds();
    let names = settings.read().prefs.sound_names.clone();
    let general_volume = settings.read().min_volume_percent;
    let volume = current.volume_percent.unwrap_or(general_volume);
    let selected_sound = current.sound.clone();
    let vibrate_only = current.sound_mode == 2;
    rsx! {
        Sheet {
            title: t(&translations, "config.sound"),
            note: t(&translations, "config.sound_note"),
            on_close: move |_| {
                preview.stop();
                sheet.set(None);
            },
            button {
                class: if vibrate_only { "radio selected" } else { "radio" },
                onclick: move |_| {
                    preview.stop();
                    draft.write().sound_mode = 2;
                },
                span { class: "radio-mark" }
                span { class: "radio-text", {t(&translations, "config.no_sound")} }
            }
            for sound in defaults.into_iter().chain(custom.clone()) {
                {
                    let is_selected = !vibrate_only && sound == selected_sound;
                    let name = crate::custom_sounds::display_name(&sound, &names);
                    let to_pick = sound.clone();
                    let to_delete = sound.clone();
                    let to_rename = sound.clone();
                    let is_custom = crate::custom_sounds::is_custom_sound(&sound);
                    let delete_aria = t(&translations, "sound.delete");
                    let rename_aria = t(&translations, "sound.rename");
                    let done_aria = done_text.clone();
                    let new_name = renaming
                        .read()
                        .as_ref()
                        .filter(|(file, _)| *file == sound)
                        .map(|(_, text)| text.clone());
                    let shown_name = name.clone();
                    rsx! {
                        if let Some(new_name) = new_name {
                            div { class: "radio-row",
                                input {
                                    class: "field",
                                    r#type: "text",
                                    maxlength: "60",
                                    "aria-label": "{rename_aria}",
                                    value: "{new_name}",
                                    oninput: move |evt| {
                                        if let Some(editing) = renaming.write().as_mut() {
                                            editing.1 = evt.value();
                                        }
                                    },
                                }
                                button {
                                    class: "icon-btn",
                                    "aria-label": "{done_aria}",
                                    onclick: move |_| {
                                        let Some((file, name)) = renaming.write().take() else {
                                            return;
                                        };
                                        {
                                            let mut now = settings.write();
                                            let names = &mut now.prefs.sound_names;
                                            match name.trim() {
                                                "" => names.remove(&file),
                                                name => names.insert(file, name.to_owned()),
                                            };
                                        }
                                        crate::persistence::save_settings(&settings.peek());
                                    },
                                    Icon { name: IconName::Check, size: 18 }
                                }
                            }
                        } else {
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
                                        preview.play_for(to_pick.clone(), SOUND_PREVIEW_MS);
                                    },
                                    span { class: "radio-mark" }
                                    span { class: "radio-text", "{name}" }
                                }
                                if is_custom {
                                    button {
                                        class: "icon-btn",
                                        "aria-label": "{rename_aria}",
                                        onclick: move |_| {
                                            preview.stop();
                                            renaming.set(Some((to_rename.clone(), shown_name.clone())));
                                        },
                                        Icon { name: IconName::Pencil, size: 16 }
                                    }
                                    button {
                                        class: "icon-btn",
                                        "aria-label": "{delete_aria}",
                                        onclick: move |_| {
                                            preview.stop();
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
                                            if settings.write().prefs.sound_names.remove(&to_delete).is_some() {
                                                crate::persistence::save_settings(&settings.peek());
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
            }
            button {
                class: "btn btn-quiet",
                onclick: move |_| {
                    // Stop it first, or the tone keeps looping behind the picker.
                    preview.stop();
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
                    preview.stop();
                    sheet.set(None);
                },
            }
        }
    }
}

/// How it vibrates, and whether the sound waits for that.
#[component]
pub(super) fn VibrationSheet() -> Element {
    let EditorState {
        mut sheet,
        mut draft,
        ..
    } = use_context();
    let translations = use_context::<Translations>();
    let current = draft.read().clone();
    let done_text = t(&translations, "action.done");
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
            if current.sound_mode == 0 {
                GroupTitle { text: t(&translations, "config.sound_delay") }
                Chips::<u16> {
                    grow: true,
                    options: SOUND_DELAY_SECONDS
                        .iter()
                        .map(|secs| {
                            let text = match secs {
                                0 => t(&translations, "config.sound_together"),
                                secs => seconds_label(&translations, *secs),
                            };
                            (*secs, text)
                        })
                        .collect::<Vec<_>>(),
                    selected: Some(current.sound_delay_secs),
                    on_pick: move |secs: u16| draft.write().sound_delay_secs = secs,
                }
            }
            SheetDone { label: done_text.clone(), on_click: move |_| sheet.set(None) }
        }
    }
}
