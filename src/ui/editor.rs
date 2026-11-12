//! Making and changing an alarm. The time and the days are right there; everything else is
//! one summary row that opens a sheet.

use std::rc::Rc;
use std::time::Duration;

use chrono::TimeZone;
use slint::{ModelRc, SharedString, VecModel};

use super::{ChoiceRow, Ctx, DayCell, EditorSheet};
use crate::i18n::{t, Translations};
use crate::models::AlarmData;
use crate::{alarm_ops, schedule};

/// How long trying a sound lasts.
const PREVIEW_MS: u64 = 15_000;

/// The alarm being edited, and what it started from.
pub struct EditorState {
    pub id: String,
    pub is_new: bool,
    /// What the editor starts from, once; comparing with it tells if anything changed.
    pub original: AlarmData,
    pub draft: AlarmData,
    /// An alarm with no weekday selected rings once, on a date.
    pub one_time: bool,
    pub date_value: String,
    start_date: String,
    error: String,
    /// The handle of the sound being tried, 0 for none.
    preview: u64,
    preview_token: u64,
}

/// "copy-<id>" opens a new alarm prefilled from the alarm <id>.
pub fn open(ctx: &Rc<Ctx>, id: &str) {
    let copy_of = id.strip_prefix("copy-").map(str::to_owned);
    let is_new = id == "new" || copy_of.is_some();
    let defaults = ctx.settings.borrow().prefs.alarm_defaults.clone();
    let original = if id == "new" {
        alarm_ops::new_alarm(defaults.as_ref())
    } else {
        let source = copy_of.as_deref().unwrap_or(id);
        let found = ctx.alarms.borrow().iter().find(|a| a.id == source).cloned();
        let mut alarm = found.unwrap_or_else(|| alarm_ops::new_alarm(defaults.as_ref()));
        if copy_of.is_some() && !alarm.label.is_empty() {
            alarm.label = t(&ctx.tr.borrow(), "alarm.copy_label").replace("{label}", &alarm.label);
        }
        alarm
    };
    // Its date if it rings once. Only today while its time is still ahead.
    let now = chrono::Local::now().naive_local();
    let start_date = schedule::resolve_one_time_date(&original, now).unwrap_or_else(|| {
        schedule::next_occurrence(original.time_hour, original.time_minute, now)
    });
    *ctx.editor.borrow_mut() = Some(EditorState {
        id: id.to_owned(),
        is_new,
        one_time: schedule::is_one_time(&original),
        date_value: start_date.clone(),
        start_date,
        draft: original.clone(),
        original,
        error: String::new(),
        preview: 0,
        preview_token: 0,
    });
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    ui.set_ed_sheet(EditorSheet::None);
    ui.set_screen(1);
    sync(ctx);
}

/// The alarm as it would be saved right now.
fn build(ctx: &Ctx, ed: &EditorState) -> AlarmData {
    let mut alarm = ed.draft.clone();
    alarm.id = if ed.is_new {
        uuid::Uuid::new_v4().to_string()
    } else {
        ed.id.clone()
    };
    alarm.label = crate::sanitize::sanitize_label(&alarm.label);
    alarm.note = crate::sanitize::sanitize_label(&alarm.note);
    if ed.one_time {
        alarm.days = [false; 7];
        alarm.date = Some(ed.date_value.clone());
    } else {
        alarm.date = None;
    }
    alarm.order = if ed.is_new {
        crate::ordering::order_for_new(&ctx.alarms.borrow())
    } else {
        ed.original.order
    };
    // Saving an alarm starts it fresh, apart from a holiday the user asked for.
    alarm.enabled = true;
    alarm.skip_until_ms = None;
    alarm
}

fn has_changes(ctx: &Ctx, ed: &EditorState) -> bool {
    let mut now = build(ctx, ed);
    let mut then = ed.original.clone();
    now.id = String::new();
    then.id = String::new();
    now.order = then.order;
    now.enabled = then.enabled;
    now.skip_until_ms = then.skip_until_ms;
    // The date it starts out with is not a change.
    if schedule::is_one_time(&then) {
        then.date = Some(ed.start_date.clone());
    }
    now != then
}

/// "Rings tomorrow, in 7 h 12 min" for the settings as they are right now.
fn rings_text(tr: &Translations, current: &AlarmData, one_time: bool, date_value: &str) -> String {
    let now = chrono::Local::now();
    let days = if one_time { [false; 7] } else { current.days };
    let date = if one_time {
        chrono::NaiveDate::parse_from_str(date_value, "%Y-%m-%d").ok()
    } else {
        None
    };
    schedule::next_ring_moment(
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
        let time = crate::text::time_left(tr, schedule::countdown(0, left_ms));
        let day = match schedule::day_label(now.date_naive(), moment.date()) {
            schedule::DayLabel::Today => t(tr, "home.today_lower"),
            schedule::DayLabel::Tomorrow => t(tr, "home.tomorrow_lower"),
            schedule::DayLabel::Weekday(_) => crate::text::short_date(tr, moment.date()),
        };
        t(tr, "config.rings_on")
            .replace("{day}", &day)
            .replace("{time}", &time)
    })
    .unwrap_or_default()
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

pub fn snooze_interval(ctx: &Ctx, longer: bool) {
    edit(ctx, |ed| {
        ed.draft.snooze_interval = longer_or_shorter(ed.draft.snooze_interval, longer);
    });
}

pub fn snooze_count(ctx: &Ctx, more: bool) {
    edit(ctx, |ed| {
        ed.draft.snooze_count = if more {
            ed.draft.snooze_count.saturating_add(1).min(10)
        } else {
            ed.draft.snooze_count.saturating_sub(1)
        };
    });
}

const PRESETS: [(&str, [bool; 7]); 3] = [
    (
        "config.weekdays",
        [true, true, true, true, true, false, false],
    ),
    (
        "config.weekend",
        [false, false, false, false, false, true, true],
    ),
    ("config.every_day", [true; 7]),
];

fn sound_ids() -> Vec<String> {
    crate::custom_sounds::list_default_sounds()
        .into_iter()
        .chain(crate::custom_sounds::list_custom_sounds())
        .collect()
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

/// Show the editor's state on screen.
pub fn sync(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let editor = ctx.editor.borrow();
    let Some(ed) = editor.as_ref() else {
        return;
    };
    let settings = ctx.settings.borrow();
    let tr = ctx.tr.borrow();
    let current = &ed.draft;
    let twenty_four = crate::text::uses_24_hours(&settings);
    ui.set_ed_title(
        t(
            &tr,
            if ed.is_new {
                "alarm.create"
            } else {
                "alarm.edit"
            },
        )
        .into(),
    );
    ui.set_ed_deletable(!ed.is_new);
    ui.set_ed_time_text(
        crate::text::clock(current.time_hour, current.time_minute, twenty_four).into(),
    );
    ui.set_ed_twelve(!twenty_four);
    ui.set_ed_rings_text(rings_text(&tr, current, ed.one_time, &ed.date_value).into());
    ui.set_ed_days(model(
        settings
            .prefs
            .week_start
            .day_order()
            .into_iter()
            .map(|day| DayCell {
                letter: crate::text::day_letter(&tr, day).into(),
                on: current.days[day],
            })
            .collect(),
    ));
    let mut labels: Vec<SharedString> = PRESETS.iter().map(|(key, _)| t(&tr, key).into()).collect();
    labels.push(t(&tr, "config.one_date").into());
    ui.set_ed_preset_labels(model(labels));
    let active = if ed.one_time {
        3
    } else {
        PRESETS
            .iter()
            .position(|(_, days)| current.days == *days)
            .map_or(-1, |i| i as i32)
    };
    ui.set_ed_preset_active(active);
    ui.set_ed_one_time(ed.one_time);
    let date_text = chrono::NaiveDate::parse_from_str(&ed.date_value, "%Y-%m-%d")
        .map(|date| crate::text::short_date(&tr, date))
        .unwrap_or_default();
    ui.set_ed_once_text(
        t(&tr, "config.once_on")
            .replace("{date}", &date_text)
            .into(),
    );
    ui.set_ed_error(ed.error.clone().into());
    ui.set_ed_label_summary(
        if current.label.is_empty() {
            t(&tr, "config.none")
        } else {
            current.label.clone()
        }
        .into(),
    );
    ui.set_ed_label_text(current.label.clone().into());
    ui.set_ed_snooze_summary(
        if current.snooze_count == 0 {
            t(&tr, "config.no_snooze")
        } else {
            crate::text::counted(
                &tr,
                "config.snooze_summary",
                u32::from(current.snooze_count),
            )
            .replace("{minutes}", &current.snooze_interval.to_string())
        }
        .into(),
    );
    ui.set_ed_snooze_interval_text(
        t(&tr, "text.minutes")
            .replace("{minutes}", &current.snooze_interval.to_string())
            .into(),
    );
    ui.set_ed_snooze_count_text(
        crate::text::counted(&tr, "text.times", u32::from(current.snooze_count)).into(),
    );
    ui.set_ed_sound_summary(
        if current.sound_mode == 2 {
            t(&tr, "config.vibration_only")
        } else {
            crate::custom_sounds::display_name(&current.sound, &settings.prefs.sound_names)
        }
        .into(),
    );
    ui.set_ed_hour_labels(model(
        (0..24u8)
            .map(|h| SharedString::from(crate::text::hour_label(h, twenty_four)))
            .collect(),
    ));
    ui.set_ed_minute_labels(model(
        (0..60)
            .map(|m| SharedString::from(format!("{m:02}")))
            .collect(),
    ));
    ui.set_ed_hour(i32::from(current.time_hour));
    ui.set_ed_minute(i32::from(current.time_minute));
    let vibrate_only = current.sound_mode == 2;
    ui.set_ed_sound_none(vibrate_only);
    ui.set_ed_sounds(model(
        sound_ids()
            .into_iter()
            .map(|sound| ChoiceRow {
                text: crate::custom_sounds::display_name(&sound, &settings.prefs.sound_names)
                    .into(),
                selected: !vibrate_only && sound == current.sound,
            })
            .collect(),
    ));
    ui.set_ed_sounding(ed.preview != 0);
}

fn edit(ctx: &Ctx, apply: impl FnOnce(&mut EditorState)) {
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        apply(ed);
    }
    sync(ctx);
}

pub fn shift(ctx: &Ctx, minutes: i32) {
    edit(ctx, |ed| {
        let total = i32::from(ed.draft.time_hour) * 60 + i32::from(ed.draft.time_minute) + minutes;
        let total = total.rem_euclid(24 * 60);
        ed.draft.time_hour = (total / 60) as u8;
        ed.draft.time_minute = (total % 60) as u8;
    });
}

pub fn set_hour(ctx: &Ctx, hour: i32) {
    edit(ctx, |ed| ed.draft.time_hour = hour.rem_euclid(24) as u8);
}

pub fn set_minute(ctx: &Ctx, minute: i32) {
    edit(ctx, |ed| ed.draft.time_minute = minute.rem_euclid(60) as u8);
}

pub fn toggle_day(ctx: &Ctx, index: usize) {
    let day = ctx
        .settings
        .borrow()
        .prefs
        .week_start
        .day_order()
        .get(index)
        .copied();
    let Some(day) = day else {
        return;
    };
    edit(ctx, |ed| {
        ed.draft.days[day] = !ed.draft.days[day];
        ed.error.clear();
    });
}

pub fn preset(ctx: &Ctx, index: usize) {
    edit(ctx, |ed| {
        ed.error.clear();
        if let Some((_, days)) = PRESETS.get(index) {
            ed.one_time = false;
            ed.draft.days = *days;
            return;
        }
        // One date: today while its time is still ahead, else the next day it comes.
        let now = chrono::Local::now().naive_local();
        let (hour, minute) = (ed.draft.time_hour, ed.draft.time_minute);
        let ahead = chrono::NaiveDate::parse_from_str(&ed.date_value, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(u32::from(hour), u32::from(minute), 0))
            .is_some_and(|at| at > now);
        if !ahead {
            ed.date_value = schedule::next_occurrence(hour, minute, now);
        }
        ed.one_time = true;
    });
}

pub fn set_label(ctx: &Ctx, label: &str) {
    // No `sync`: it would replace what is being typed.
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        ed.draft.label = label.to_owned();
    }
    if let Some(ui) = ctx.ui.upgrade() {
        let editor = ctx.editor.borrow();
        if let Some(ed) = editor.as_ref() {
            let tr = ctx.tr.borrow();
            ui.set_ed_label_summary(
                if ed.draft.label.is_empty() {
                    t(&tr, "config.none")
                } else {
                    ed.draft.label.clone()
                }
                .into(),
            );
        }
    }
}

// ---- Sounds -------------------------------------------------------------------------------

fn stop_handle(handle: u64) {
    if handle != 0 {
        let handle = mobile_sentinel::PlaybackHandle::from_id(handle);
        let _ = mobile_sentinel::audio::stop(&handle);
    }
}

/// Play a sound as the alarm will sound, and also with the media volume at zero. Round and
/// round, as it would ring: a short sound still lasts as long as the preview.
fn play_handle(sound: &str) -> u64 {
    let Some(path) = crate::custom_sounds::get_playback_path(sound) else {
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
        ed.preview_token += 1;
    }
    sync(ctx);
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
    sync(ctx);
}

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
    stop_preview(ctx);
    std::thread::spawn(|| {
        let picked = crate::custom_sounds::pick_audio_file_native();
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
                Ok(None) => {}
                Err(e) => {
                    log::warn!("[AlarmFree] adding a sound failed: {e}");
                    let message = if e.contains("not a sound") {
                        t(&ctx.tr.borrow(), "sound.not_a_sound")
                    } else {
                        e
                    };
                    super::toast::error(ctx, message);
                }
            });
        });
    });
}

// ---- Leaving ------------------------------------------------------------------------------

fn leave(ctx: &Rc<Ctx>) {
    stop_preview(ctx);
    *ctx.editor.borrow_mut() = None;
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_ed_sheet(EditorSheet::None);
        ui.set_screen(0);
    }
    super::home::refresh(ctx);
}

/// Back, or the arrow: close what is open, ask before dropping changes, else leave.
pub fn back(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    if ui.get_ed_sheet() != EditorSheet::None {
        ui.set_ed_sheet(EditorSheet::None);
        stop_preview(ctx);
        return;
    }
    let changed = ctx
        .editor
        .borrow()
        .as_ref()
        .is_some_and(|ed| has_changes(ctx, ed));
    if changed {
        ui.set_ed_sheet(EditorSheet::Discard);
    } else {
        leave(ctx);
    }
}

pub fn discard(ctx: &Rc<Ctx>) {
    leave(ctx);
}

pub fn sheet_closed(ctx: &Rc<Ctx>) {
    stop_preview(ctx);
}

pub fn save(ctx: &Rc<Ctx>) {
    stop_preview(ctx);
    let alarm = {
        let editor = ctx.editor.borrow();
        let Some(ed) = editor.as_ref() else {
            return;
        };
        build(ctx, ed)
    };
    let (one_time, date_value) = {
        let editor = ctx.editor.borrow();
        let ed = editor.as_ref().expect("checked above");
        (ed.one_time, ed.date_value.clone())
    };
    let error = if one_time {
        let ahead = chrono::NaiveDate::parse_from_str(&date_value, "%Y-%m-%d")
            .ok()
            .and_then(|d| {
                d.and_hms_opt(u32::from(alarm.time_hour), u32::from(alarm.time_minute), 0)
            })
            .is_some_and(|at| at > chrono::Local::now().naive_local());
        (!ahead).then(|| t(&ctx.tr.borrow(), "validation.date_past"))
    } else {
        (!alarm.days.iter().any(|d| *d)).then(|| t(&ctx.tr.borrow(), "validation.select_day"))
    };
    if let Some(error) = error {
        edit(ctx, |ed| ed.error = error);
        return;
    }
    let rings = crate::text::rings_in(&ctx.tr.borrow(), &alarm);
    {
        let mut list = ctx.alarms.borrow_mut();
        if let Some(existing) = list.iter_mut().find(|a| a.id == alarm.id) {
            *existing = alarm;
        } else {
            list.push(alarm);
        }
    }
    crate::persistence::save_alarms(&ctx.alarms.borrow());
    leave(ctx);
    if !rings.is_empty() {
        super::toast::show(ctx, rings, None);
    }
}

pub fn delete(ctx: &Rc<Ctx>) {
    stop_preview(ctx);
    let Some(id) = ctx.editor.borrow().as_ref().map(|ed| ed.id.clone()) else {
        return;
    };
    let before = ctx.alarms.borrow().clone();
    let name = before
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.label.clone())
        .unwrap_or_default();
    ctx.alarms.borrow_mut().retain(|a| a.id != id);
    crate::persistence::save_alarms(&ctx.alarms.borrow());
    let message = {
        let tr = ctx.tr.borrow();
        if name.is_empty() {
            t(&tr, "home.deleted_plain")
        } else {
            t(&tr, "home.deleted").replace("{alarm}", &name)
        }
    };
    leave(ctx);
    super::toast::show(ctx, message, Some(before));
}

#[cfg(test)]
mod tests {
    use super::longer_or_shorter;

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
}
