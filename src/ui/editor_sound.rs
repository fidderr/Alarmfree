//! The editor's sheets for how it sounds and feels: the sound, the vibration, and the small
//! ones with one row of choices (ring limit, wake-up check, notice). Also the label sheet's
//! chips of labels used before, and trying a sound or a vibration.

use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, SharedString};

use super::editor::{
    edit, model, seconds_label, EditorState, CHECK_MINUTES, FADE_SECONDS, LIMIT_MINUTES,
    NOTICE_MINUTES, SOUND_DELAY_SECONDS, VIBRATION_KEYS,
};
use super::{Ctx, Ed, EditorSheet, MainWindow, SoundRow, UsedChip};
use crate::i18n::{t, Translations};
use crate::models::{AlarmData, AppSettings};
use crate::{alarm_ops, custom_sounds};

/// How long trying a sound or a vibration lasts.
const PREVIEW_MS: u64 = 15_000;

pub(super) fn sound_ids() -> Vec<String> {
    custom_sounds::list_default_sounds()
        .into_iter()
        .chain(custom_sounds::list_custom_sounds())
        .collect()
}

/// The labels of the alarms there are, each once, for picking again.
pub(super) fn used_labels(ctx: &Ctx) -> Vec<String> {
    used_in(&ctx.alarms.borrow())
}

fn used_in(alarms: &[AlarmData]) -> Vec<String> {
    let mut used: Vec<String> = alarms
        .iter()
        .filter(|a| !a.hidden && !a.label.is_empty())
        .map(|a| a.label.clone())
        .collect();
    used.sort();
    used.dedup();
    used
}

/// Which line each chip lands on when they flow left to right in `budget` px, with 8 px
/// between them. A chip wider than a line has one to itself.
pub(super) fn flow(widths: &[f32], budget: f32) -> Vec<i32> {
    let mut line = 0;
    let mut used = 0.0;
    widths
        .iter()
        .map(|width| {
            if used > 0.0 && used + 8.0 + width > budget {
                line += 1;
                used = 0.0;
            }
            used += if used > 0.0 { 8.0 } else { 0.0 } + width;
            line
        })
        .collect()
}

/// About how wide a chip with this text is.
fn chip_width(text: &str) -> f32 {
    (text.chars().count() as f32 * 8.0 + 28.0).max(52.0)
}

pub(super) fn sync(
    ui: &MainWindow,
    ed: &EditorState,
    settings: &AppSettings,
    tr: &Translations,
    alarms: &[AlarmData],
) {
    let e = ui.global::<Ed>();
    let current = &ed.draft;
    let vibrate_only = current.sound_mode == 2;
    e.set_sound_summary(
        if vibrate_only {
            t(tr, "config.vibration_only")
        } else {
            custom_sounds::display_name(&current.sound, &settings.prefs.sound_names)
        }
        .into(),
    );
    e.set_sound_none(vibrate_only);
    e.set_sounding(ed.preview != 0);
    e.set_sound_rows(model(
        sound_ids()
            .into_iter()
            .map(|sound| {
                let state = if ed.renaming.as_ref().is_some_and(|(file, _)| *file == sound) {
                    1
                } else if ed.deleting.as_deref() == Some(sound.as_str()) {
                    2
                } else {
                    0
                };
                SoundRow {
                    text: custom_sounds::display_name(&sound, &settings.prefs.sound_names).into(),
                    selected: !vibrate_only && sound == current.sound,
                    custom: custom_sounds::is_custom_sound(&sound),
                    state,
                }
            })
            .collect(),
    ));
    e.set_rename_text(
        ed.renaming
            .as_ref()
            .map(|(_, name)| name.clone())
            .unwrap_or_default()
            .into(),
    );
    e.set_volume(i32::from(
        current
            .volume_percent
            .unwrap_or(settings.min_volume_percent),
    ));
    e.set_fade_labels(model(
        FADE_SECONDS
            .iter()
            .map(|secs| SharedString::from(seconds_label(tr, *secs)))
            .collect(),
    ));
    e.set_fade_selected(
        FADE_SECONDS
            .iter()
            .position(|secs| *secs == current.fade_in_secs)
            .map_or(-1, |i| i as i32),
    );

    // Vibration.
    e.set_vib_labels(model(
        std::iter::once(t(tr, "vibration.off"))
            .chain(VIBRATION_KEYS.iter().map(|key| t(tr, key)))
            .map(SharedString::from)
            .collect(),
    ));
    e.set_vib_selected(if current.sound_mode == 1 {
        0
    } else {
        1 + i32::from(current.vibration)
    });
    e.set_vibrating(ed.vibrating);
    e.set_vib_can_try(current.sound_mode != 1);
    e.set_delay_visible(current.sound_mode == 0);
    e.set_delay_labels(model(
        SOUND_DELAY_SECONDS
            .iter()
            .map(|secs| {
                SharedString::from(match secs {
                    0 => t(tr, "config.sound_together"),
                    secs => seconds_label(tr, *secs),
                })
            })
            .collect(),
    ));
    e.set_delay_selected(
        SOUND_DELAY_SECONDS
            .iter()
            .position(|secs| *secs == current.sound_delay_secs)
            .map_or(-1, |i| i as i32),
    );

    // The labels used before: as many to a line as the sheet has room for.
    let used = used_in(alarms);
    let scale = ui.window().scale_factor();
    let width = ui.window().size().width as f32 / scale;
    let lines = flow(
        &used.iter().map(|text| chip_width(text)).collect::<Vec<_>>(),
        (width - 32.0).max(120.0),
    );
    e.set_used_lines(lines.iter().copied().max().map_or(0, |last| last + 1));
    e.set_used(model(
        used.into_iter()
            .zip(lines)
            .map(|(text, line)| UsedChip {
                selected: text == current.label,
                text: text.into(),
                line,
            })
            .collect(),
    ));
}

/// The choices of the limit, wake-up check and notice sheets.
pub(super) fn sync_options(ctx: &Ctx, sheet: EditorSheet) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let editor = ctx.editor.borrow();
    let Some(ed) = editor.as_ref() else {
        return;
    };
    let tr = ctx.tr.borrow();
    let minutes = |m: u32| t(&tr, "text.minutes").replace("{minutes}", &m.to_string());
    let off_or = |m: u32| {
        if m == 0 {
            t(&tr, "config.fade_off")
        } else {
            minutes(m)
        }
    };
    let (labels, selected): (Vec<String>, i32) = match sheet {
        EditorSheet::Limit => {
            let mut labels: Vec<String> = LIMIT_MINUTES.iter().map(|m| minutes(*m)).collect();
            labels.push(t(&tr, "config.limit_forever"));
            let selected = match ed.draft.ring_limit_secs {
                None => Some(LIMIT_MINUTES.len()),
                Some(secs) => LIMIT_MINUTES.iter().position(|m| m * 60 == secs),
            };
            (labels, selected.map_or(-1, |i| i as i32))
        }
        EditorSheet::WakeCheck => (
            CHECK_MINUTES
                .iter()
                .map(|m| off_or(u32::from(*m)))
                .collect(),
            CHECK_MINUTES
                .iter()
                .position(|m| *m == ed.draft.wake_check_minutes)
                .map_or(-1, |i| i as i32),
        ),
        _ => (
            NOTICE_MINUTES
                .iter()
                .map(|m| off_or(u32::from(*m)))
                .collect(),
            NOTICE_MINUTES
                .iter()
                .position(|m| *m == ed.draft.notify_before_minutes)
                .map_or(-1, |i| i as i32),
        ),
    };
    let e = ui.global::<Ed>();
    e.set_opt_labels(model(labels.into_iter().map(SharedString::from).collect()));
    e.set_opt_selected(selected);
}

pub fn opt_picked(ctx: &Rc<Ctx>, index: usize) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let sheet = ui.get_ed_sheet();
    edit(ctx, |ed| match sheet {
        EditorSheet::Limit => {
            ed.draft.ring_limit_secs = LIMIT_MINUTES.get(index).map(|m| m * 60);
        }
        EditorSheet::WakeCheck => {
            if let Some(m) = CHECK_MINUTES.get(index) {
                ed.draft.wake_check_minutes = *m;
            }
        }
        EditorSheet::Notice => {
            if let Some(m) = NOTICE_MINUTES.get(index) {
                ed.draft.notify_before_minutes = *m;
            }
        }
        _ => {}
    });
    sync_options(ctx, sheet);
}

// ---- Trying a sound or a vibration -----------------------------------------------------

fn stop_handle(handle: u64) {
    if handle != 0 {
        let handle = mobile_sentinel::PlaybackHandle::from_id(handle);
        let _ = mobile_sentinel::audio::stop(&handle);
    }
}

/// Play a sound as the alarm will sound, and also with the media volume at zero. Round and
/// round, as it would ring: a short sound still lasts as long as the preview.
fn play_handle(sound: &str) -> u64 {
    let Some(path) = custom_sounds::get_playback_path(sound) else {
        log::warn!("[AlarmFree] No playback path found for: {sound}");
        return 0;
    };
    match mobile_sentinel::audio::play_as_alarm(&path.to_string_lossy(), true) {
        Ok(handle) => handle.id(),
        Err(e) => {
            log::warn!("[AlarmFree] Preview play failed: {e}");
            0
        }
    }
}

/// Stop whatever is being tried, also when the editor goes away.
pub fn stop_preview(ctx: &Ctx) {
    ctx.preview_timer.stop();
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        stop_handle(ed.preview);
        ed.preview = 0;
        // Not through the touch feedback: turning that off should not turn this off.
        if ed.vibrating {
            mobile_sentinel::haptics::cancel();
            ed.vibrating = false;
        }
        ed.preview_token += 1;
    }
    super::editor::sync(ctx);
}

/// Whatever started it, it stops after a while; a newer preview is left alone.
fn stop_after_a_while(ctx: &Rc<Ctx>, token: u64) {
    let weak = Rc::downgrade(ctx);
    ctx.preview_timer.start(
        slint::TimerMode::SingleShot,
        Duration::from_millis(PREVIEW_MS),
        move || {
            if let Some(ctx) = weak.upgrade() {
                let same = ctx
                    .editor
                    .borrow()
                    .as_ref()
                    .is_some_and(|ed| ed.preview_token == token);
                if same {
                    stop_preview(&ctx);
                }
            }
        },
    );
}

fn start_preview(ctx: &Rc<Ctx>) {
    stop_preview(ctx);
    let sound = ctx
        .editor
        .borrow()
        .as_ref()
        .map(|ed| ed.draft.sound.clone());
    let Some(sound) = sound else {
        return;
    };
    let handle = play_handle(&sound);
    let token = {
        let mut editor = ctx.editor.borrow_mut();
        let Some(ed) = editor.as_mut() else {
            return;
        };
        ed.preview = handle;
        ed.preview_token
    };
    stop_after_a_while(ctx, token);
    super::editor::sync(ctx);
}

fn start_vibration(ctx: &Rc<Ctx>, pattern: u8) {
    stop_preview(ctx);
    let pattern = crate::platform::VIBRATION_PATTERNS
        [usize::from(pattern).min(crate::platform::VIBRATION_PATTERNS.len() - 1)];
    mobile_sentinel::haptics::vibrate_pattern(&crate::platform::repeated_pattern(
        pattern, PREVIEW_MS,
    ));
    let token = {
        let mut editor = ctx.editor.borrow_mut();
        let Some(ed) = editor.as_mut() else {
            return;
        };
        ed.vibrating = true;
        ed.preview_token
    };
    stop_after_a_while(ctx, token);
    super::editor::sync(ctx);
}

// ---- The sound sheet -------------------------------------------------------------------

pub fn pick_sound(ctx: &Rc<Ctx>, index: usize) {
    let Some(sound) = sound_ids().get(index).cloned() else {
        return;
    };
    let playing = ctx
        .editor
        .borrow()
        .as_ref()
        .is_some_and(|ed| ed.preview != 0);
    edit(ctx, |ed| {
        ed.draft.sound = sound;
        if ed.draft.sound_mode == 2 {
            ed.draft.sound_mode = 0;
        }
    });
    // Only a tap on Start makes a sound. While one plays, the sound picked takes over.
    if playing {
        start_preview(ctx);
    }
}

pub fn no_sound(ctx: &Rc<Ctx>) {
    stop_preview(ctx);
    edit(ctx, |ed| ed.draft.sound_mode = 2);
}

pub fn toggle_preview(ctx: &Rc<Ctx>) {
    let playing = ctx
        .editor
        .borrow()
        .as_ref()
        .is_some_and(|ed| ed.preview != 0);
    if playing {
        stop_preview(ctx);
    } else {
        start_preview(ctx);
    }
}

/// The system's file picker, off the UI thread because it waits for the user.
pub fn add_sound(ctx: &Rc<Ctx>) {
    // Stop it first, or the tone keeps looping behind the picker.
    stop_preview(ctx);
    std::thread::spawn(|| {
        let picked = custom_sounds::pick_audio_file_native();
        let _ = slint::invoke_from_event_loop(move || {
            super::with_ctx(|ctx| match picked {
                Ok(Some(imported)) => {
                    ctx.settings
                        .borrow_mut()
                        .prefs
                        .sound_names
                        .insert(imported.file.clone(), imported.name);
                    crate::persistence::save_settings(&ctx.settings.borrow());
                    edit(ctx, |ed| {
                        ed.draft.sound = imported.file;
                        if ed.draft.sound_mode == 2 {
                            ed.draft.sound_mode = 0;
                        }
                    });
                }
                // The user changed their mind.
                Ok(None) => {}
                Err(reason) if reason == custom_sounds::NOT_A_SOUND => {
                    let message = t(&ctx.tr.borrow(), "sound.not_a_sound");
                    super::toast::error(ctx, message);
                }
                Err(reason) => {
                    log::warn!("[AlarmFree] No sound imported: {reason}");
                    let message = t(&ctx.tr.borrow(), "db.save_failed");
                    super::toast::error(ctx, message);
                }
            });
        });
    });
}

pub fn rename_start(ctx: &Rc<Ctx>, index: usize) {
    let Some(file) = sound_ids().get(index).cloned() else {
        return;
    };
    stop_preview(ctx);
    let shown = custom_sounds::display_name(&file, &ctx.settings.borrow().prefs.sound_names);
    edit(ctx, |ed| {
        ed.deleting = None;
        ed.renaming = Some((file, shown));
    });
}

pub fn rename_edited(ctx: &Ctx, text: &str) {
    // No `sync`: it would replace what is being typed.
    if let Some(editing) = ctx
        .editor
        .borrow_mut()
        .as_mut()
        .and_then(|ed| ed.renaming.as_mut())
    {
        editing.1 = text.to_owned();
    }
}

pub fn rename_done(ctx: &Ctx) {
    let renamed = ctx
        .editor
        .borrow_mut()
        .as_mut()
        .and_then(|ed| ed.renaming.take());
    if let Some((file, name)) = renamed {
        {
            let mut settings = ctx.settings.borrow_mut();
            let names = &mut settings.prefs.sound_names;
            match name.trim() {
                "" => names.remove(&file),
                name => names.insert(file, name.to_owned()),
            };
        }
        crate::persistence::save_settings(&ctx.settings.borrow());
    }
    super::editor::sync(ctx);
}

pub fn delete_ask(ctx: &Rc<Ctx>, index: usize) {
    let Some(file) = sound_ids().get(index).cloned() else {
        return;
    };
    stop_preview(ctx);
    edit(ctx, |ed| {
        ed.renaming = None;
        ed.deleting = Some(file);
    });
}

pub fn delete_cancel(ctx: &Ctx) {
    edit(ctx, |ed| ed.deleting = None);
}

/// The user's own sound goes for good. An alarm that uses it falls back to the first sound
/// when it is saved or edited, and always rings: the ring has its own fallbacks.
pub fn delete_confirm(ctx: &Rc<Ctx>, index: usize) {
    let Some(file) = sound_ids().get(index).cloned() else {
        return;
    };
    stop_preview(ctx);
    let replacement = alarm_ops::factory_alarm().sound;
    edit(ctx, |ed| {
        ed.deleting = None;
        if ed.draft.sound == file {
            ed.draft.sound = replacement.clone();
        }
    });
    let mut changed = false;
    for alarm in ctx
        .alarms
        .borrow_mut()
        .iter_mut()
        .filter(|a| a.sound == file)
    {
        alarm.sound = replacement.clone();
        changed = true;
    }
    if changed {
        crate::persistence::save_alarms(&ctx.alarms.borrow());
    }
    {
        let mut settings = ctx.settings.borrow_mut();
        settings.prefs.sound_names.remove(&file);
        if let Some(defaults) = settings
            .prefs
            .alarm_defaults
            .as_mut()
            .filter(|defaults| defaults.sound == file)
        {
            defaults.sound = replacement.clone();
        }
    }
    crate::persistence::save_settings(&ctx.settings.borrow());
    custom_sounds::delete_sound(&file);
    super::editor::sync(ctx);
}

pub fn set_volume(ctx: &Ctx, percent: i32) {
    let percent = percent.clamp(0, 100) as u8;
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        ed.draft.volume_percent = Some(percent);
    }
    // Only the slider: the rest of the editor does not change with it.
    if let Some(ui) = ctx.ui.upgrade() {
        ui.global::<Ed>().set_volume(i32::from(percent));
    }
}

pub fn pick_fade(ctx: &Ctx, index: usize) {
    if let Some(secs) = FADE_SECONDS.get(index) {
        edit(ctx, |ed| ed.draft.fade_in_secs = *secs);
    }
}

// ---- The vibration sheet ---------------------------------------------------------------

pub fn vib_picked(ctx: &Rc<Ctx>, index: usize) {
    if index == 0 {
        // Nothing at all is no alarm: without sound it keeps vibrating.
        edit(ctx, |ed| {
            if ed.draft.sound_mode != 2 {
                ed.draft.sound_mode = 1;
            }
        });
        stop_preview(ctx);
        return;
    }
    let pattern = (index - 1) as u8;
    edit(ctx, |ed| {
        ed.draft.vibration = pattern;
        if ed.draft.sound_mode == 1 {
            ed.draft.sound_mode = 0;
        }
    });
    start_vibration(ctx, pattern);
}

pub fn vib_preview(ctx: &Rc<Ctx>) {
    let state = ctx
        .editor
        .borrow()
        .as_ref()
        .map(|ed| (ed.vibrating, ed.draft.vibration));
    match state {
        Some((true, _)) => stop_preview(ctx),
        Some((false, pattern)) => start_vibration(ctx, pattern),
        None => {}
    }
}

pub fn delay_picked(ctx: &Ctx, index: usize) {
    if let Some(secs) = SOUND_DELAY_SECONDS.get(index) {
        edit(ctx, |ed| ed.draft.sound_delay_secs = *secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chips_flow_to_a_new_line_when_the_line_is_full() {
        assert_eq!(flow(&[100.0, 100.0, 100.0], 316.0), vec![0, 0, 0]);
        assert_eq!(flow(&[100.0, 100.0, 100.0], 315.0), vec![0, 0, 1]);
        assert_eq!(flow(&[400.0, 50.0], 300.0), vec![0, 1]);
        assert!(flow(&[], 300.0).is_empty());
    }

    #[test]
    fn labels_used_before_are_each_listed_once_and_hidden_alarms_are_left_out() {
        let mut one = alarm_ops::factory_alarm();
        one.label = "Work".into();
        let mut two = one.clone();
        two.label = "Gym".into();
        let mut hidden = one.clone();
        hidden.label = "Test".into();
        hidden.hidden = true;
        let mut blank = one.clone();
        blank.label = String::new();
        assert_eq!(
            used_in(&[one.clone(), two, one, hidden, blank]),
            vec!["Gym".to_owned(), "Work".to_owned()]
        );
    }
}
