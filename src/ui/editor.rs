//! Making and changing an alarm. The time and the days are right there; everything else is
//! one summary row that opens a sheet.

use std::rc::Rc;

use chrono::TimeZone;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::datepicker::{DateView, Move};
use super::editor_sound::stop_preview;
use super::{Ctx, DayCell, Ed, EditorSheet, SCREEN_SETTINGS};
use crate::i18n::{t, Translations};
use crate::models::{AlarmData, ChallengeSetup};
use crate::{alarm_ops, schedule};

pub const FADE_SECONDS: [u16; 6] = [0, 15, 30, 60, 120, 300];
pub const SOUND_DELAY_SECONDS: [u16; 4] = [0, 15, 30, 60];
pub const LIMIT_MINUTES: [u32; 4] = [30, 60, 90, 120];
pub const CHECK_MINUTES: [u8; 4] = [0, 3, 5, 10];
pub const NOTICE_MINUTES: [u8; 5] = [0, 5, 10, 30, 60];

/// Translation keys of the vibration patterns, in the order of `platform::VIBRATION_PATTERNS`.
pub const VIBRATION_KEYS: [&str; 3] =
    ["vibration.pulse", "vibration.steady", "vibration.heartbeat"];

pub fn seconds_label(tr: &Translations, secs: u16) -> String {
    match secs {
        0 => t(tr, "config.fade_off"),
        s if s < 60 => t(tr, "text.seconds").replace("{seconds}", &s.to_string()),
        s => t(tr, "text.minutes").replace("{minutes}", &(s / 60).to_string()),
    }
}

/// The alarm being edited, and what it started from.
pub struct EditorState {
    pub id: String,
    pub is_new: bool,
    /// Editing what new alarms start from, reached from the settings.
    pub for_defaults: bool,
    /// What the editor starts from, once; comparing with it tells if anything changed.
    pub original: AlarmData,
    pub draft: AlarmData,
    /// Its challenges, in the order they are done.
    pub setups: Vec<ChallengeSetup>,
    /// The challenge whose settings are open.
    pub detail: usize,
    /// An alarm with no weekday selected rings once, on a date.
    pub one_time: bool,
    pub date_value: String,
    pub(super) start_date: String,
    pub(super) error: String,
    /// The month shown in the date sheet.
    pub(super) calendar: DateView,
    /// The user's own sound being renamed (file, name so far), or asked about deleting.
    pub(super) renaming: Option<(String, String)>,
    pub(super) deleting: Option<String>,
    /// The handle of the sound being tried, 0 for none.
    pub(super) preview: u64,
    pub(super) vibrating: bool,
    pub(super) preview_token: u64,
    /// Bumped to make a field or the wheels start over from the draft.
    pub(super) time_rev: i32,
    pub(super) label_rev: i32,
    /// The colour sheet of the ring look.
    pub(super) wheel: super::colour::Wheel,
    pub(super) typed_colour: Option<String>,
    /// The ring photo as loaded, so choosing among the looks does not decode it again.
    pub(super) photo: Option<(String, slint::Image)>,
}

/// "copy-<id>" opens a new alarm prefilled from the alarm <id>.
pub fn open(ctx: &Rc<Ctx>, id: &str) {
    // Next to the list another alarm can be picked while one is open: the editor starts over
    // for it, and back returns to the one before.
    let previous = ctx.editor.borrow().as_ref().map(|ed| ed.id.clone());
    if let Some(previous) = previous {
        stop_preview(ctx);
        ctx.editor_trail.borrow_mut().push(previous);
    }
    start(ctx, id);
}

fn start(ctx: &Rc<Ctx>, id: &str) {
    let copy_of = id.strip_prefix("copy-").map(str::to_owned);
    let for_defaults = id == "defaults";
    let is_new = id == "new" || copy_of.is_some();
    let defaults = ctx.settings.borrow().prefs.alarm_defaults.clone();
    let original = if id == "new" || for_defaults {
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
        for_defaults,
        one_time: schedule::is_one_time(&original),
        calendar: DateView::new(
            chrono::NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
                .unwrap_or_else(|_| now.date()),
            chrono::Local::now().date_naive(),
        ),
        date_value: start_date.clone(),
        start_date,
        setups: ChallengeSetup::list_from_stored(&original.challenges),
        detail: 0,
        draft: original.clone(),
        original,
        error: String::new(),
        renaming: None,
        deleting: None,
        preview: 0,
        vibrating: false,
        preview_token: 0,
        time_rev: 0,
        label_rev: 0,
        wheel: super::colour::Wheel::of(crate::models::Rgb::new(0, 0, 0)),
        typed_colour: None,
        photo: None,
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
    alarm.challenges = ed.setups.iter().map(ChallengeSetup::to_stored).collect();
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
    // Challenges in the form they are saved in, so unknown types and spelling do not count.
    then.challenges = ChallengeSetup::list_from_stored(&then.challenges)
        .iter()
        .map(ChallengeSetup::to_stored)
        .collect();
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

pub(super) fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
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
            if ed.for_defaults {
                "config.defaults_title"
            } else if ed.is_new {
                "alarm.create"
            } else {
                "alarm.edit"
            },
        )
        .into(),
    );
    ui.set_ed_defaults(ed.for_defaults);
    ui.set_ed_deletable(!ed.is_new && !ed.for_defaults);
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

    let rows = ui.global::<Ed>();
    rows.set_label_summary(
        if current.label.is_empty() {
            t(&tr, "config.none")
        } else {
            current.label.clone()
        }
        .into(),
    );
    rows.set_vibration_summary(
        if current.sound_mode == 1 {
            t(&tr, "vibration.off")
        } else {
            t(
                &tr,
                VIBRATION_KEYS[usize::from(current.vibration).min(VIBRATION_KEYS.len() - 1)],
            )
        }
        .into(),
    );
    rows.set_snooze_summary(
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
    rows.set_look_summary(t(&tr, current.ring_look.backdrop.name_key()).into());
    rows.set_limit_summary(
        match current.ring_limit_secs {
            None => t(&tr, "config.limit_forever"),
            Some(secs) => t(&tr, "text.minutes").replace("{minutes}", &(secs / 60).to_string()),
        }
        .into(),
    );
    rows.set_check_summary(
        if current.wake_check_minutes == 0 {
            t(&tr, "config.fade_off")
        } else {
            t(&tr, "config.check_after")
                .replace("{minutes}", &current.wake_check_minutes.to_string())
        }
        .into(),
    );
    rows.set_notice_summary(
        if current.notify_before_minutes == 0 {
            t(&tr, "config.fade_off")
        } else {
            t(&tr, "text.minutes").replace("{minutes}", &current.notify_before_minutes.to_string())
        }
        .into(),
    );
    rows.set_bright(current.max_brightness);

    // The sheets.
    rows.set_snooze_interval_text(
        t(&tr, "text.minutes")
            .replace("{minutes}", &current.snooze_interval.to_string())
            .into(),
    );
    rows.set_snooze_count_text(
        crate::text::counted(&tr, "text.times", u32::from(current.snooze_count)).into(),
    );
    rows.set_hour_labels(model(
        (0..24u8)
            .map(|h| SharedString::from(crate::text::hour_label(h, twenty_four)))
            .collect(),
    ));
    rows.set_minute_labels(model(
        (0..60)
            .map(|m| SharedString::from(format!("{m:02}")))
            .collect(),
    ));
    rows.set_hour(i32::from(current.time_hour));
    rows.set_minute(i32::from(current.time_minute));
    rows.set_time_rev(ed.time_rev);
    rows.set_label_text(current.label.clone().into());
    rows.set_note_text(current.note.clone().into());
    rows.set_label_rev(ed.label_rev);
    let today = chrono::Local::now().date_naive();
    rows.set_quick_labels(model(
        ["home.today", "home.tomorrow", "config.next_week"]
            .iter()
            .map(|key| SharedString::from(t(&tr, key)))
            .collect(),
    ));
    let picked = chrono::NaiveDate::parse_from_str(&ed.date_value, "%Y-%m-%d").ok();
    rows.set_quick_selected(
        [0, 1, 7]
            .iter()
            .position(|days| picked == Some(today + chrono::Duration::days(*days)))
            .map_or(-1, |i| i as i32),
    );
    super::editor_sound::sync(&ui, ed, &settings, &tr, &ctx.alarms.borrow());
    drop(settings);
    drop(tr);
    drop(editor);
    super::challenge_sheets::sync_rows(ctx);
}

pub(super) fn edit(ctx: &Ctx, apply: impl FnOnce(&mut EditorState)) {
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
    crate::feedback::tick(8);
    edit(ctx, |ed| ed.draft.time_hour = hour.rem_euclid(24) as u8);
}

pub fn set_minute(ctx: &Ctx, minute: i32) {
    crate::feedback::tick(8);
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

pub fn preset(ctx: &Rc<Ctx>, index: usize) {
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
    if index >= PRESETS.len() {
        open_sheet(ctx, EditorSheet::Date);
    }
}

/// Open a sheet from Rust, as a tap on its row does.
pub(super) fn open_sheet(ctx: &Rc<Ctx>, sheet: EditorSheet) {
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_ed_sheet(sheet);
    }
    sheet_opened(ctx, sheet);
}

/// A sheet came up: fill what only it shows.
pub fn sheet_opened(ctx: &Rc<Ctx>, sheet: EditorSheet) {
    match sheet {
        EditorSheet::Date => {
            if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
                if let Ok(day) = chrono::NaiveDate::parse_from_str(&ed.date_value, "%Y-%m-%d") {
                    ed.calendar.select(day);
                }
                ed.calendar.show(ctx);
            }
            sync(ctx);
        }
        EditorSheet::Limit | EditorSheet::WakeCheck | EditorSheet::Notice => {
            super::editor_sound::sync_options(ctx, sheet)
        }
        EditorSheet::RingLook | EditorSheet::RingColour => {
            super::editor_look::sheet_opened(ctx, sheet)
        }
        _ => {}
    }
}

/// A tap in the date sheet's month.
pub fn date_action(ctx: &Rc<Ctx>, action: Move) {
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        match action {
            Move::Prev => ed.calendar.prev(),
            Move::Next => ed.calendar.next(),
            Move::Pick(index) => {
                if ed.calendar.pick(ctx, index) {
                    ed.date_value = ed.calendar.selected.format("%Y-%m-%d").to_string();
                    ed.error.clear();
                }
            }
        }
        ed.calendar.show(ctx);
    }
    sync(ctx);
}

/// Today, tomorrow or a week on, in the date sheet.
pub fn date_quick(ctx: &Rc<Ctx>, index: usize) {
    let days = [0, 1, 7].get(index).copied().unwrap_or(0);
    let day = chrono::Local::now().date_naive() + chrono::Duration::days(days);
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        ed.date_value = day.format("%Y-%m-%d").to_string();
        ed.error.clear();
        ed.calendar.select(day);
        ed.calendar.show(ctx);
    }
    sync(ctx);
}

/// The time typed in the time sheet: the wheels follow it as soon as it is a time.
pub fn time_typed(ctx: &Rc<Ctx>, text: &str) {
    let Some((hour, minute)) = crate::text::parse_typed_time(text) else {
        return;
    };
    edit(ctx, |ed| {
        ed.draft.time_hour = hour;
        ed.draft.time_minute = minute;
        ed.time_rev += 1;
    });
}

pub fn set_note(ctx: &Ctx, note: &str) {
    // No `sync`: it would replace what is being typed.
    if let Some(ed) = ctx.editor.borrow_mut().as_mut() {
        ed.draft.note = note.to_owned();
    }
}

/// A label used before, picked in the label sheet.
pub fn use_label(ctx: &Ctx, index: usize) {
    let label = super::editor_sound::used_labels(ctx).get(index).cloned();
    if let Some(label) = label {
        edit(ctx, |ed| {
            ed.draft.label = label;
            ed.label_rev += 1;
        });
    }
}

pub fn set_max_brightness(ctx: &Ctx, on: bool) {
    edit(ctx, |ed| ed.draft.max_brightness = on);
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
            ui.global::<Ed>().set_label_summary(
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

// ---- Leaving ------------------------------------------------------------------------------

fn leave(ctx: &Rc<Ctx>) {
    stop_preview(ctx);
    let from_defaults = ctx
        .editor
        .borrow_mut()
        .take()
        .is_some_and(|ed| ed.for_defaults);
    let earlier = ctx.editor_trail.borrow_mut().pop();
    if let Some(earlier) = earlier {
        super::home::refresh(ctx);
        start(ctx, &earlier);
        return;
    }
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_ed_sheet(EditorSheet::None);
        // The defaults are opened from the settings.
        ui.set_screen(if from_defaults { SCREEN_SETTINGS } else { 0 });
    }
    super::home::refresh(ctx);
}

/// Back, or the arrow: close what is open, ask before dropping changes, else leave.
pub fn back(ctx: &Rc<Ctx>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    match ui.get_ed_sheet() {
        EditorSheet::None => {}
        // Their back is the list of challenges.
        EditorSheet::AddChallenge | EditorSheet::Detail => {
            ui.set_ed_sheet(EditorSheet::Challenges);
            return;
        }
        // The colour came from the look.
        EditorSheet::RingColour => {
            open_sheet(ctx, EditorSheet::RingLook);
            return;
        }
        _ => {
            ui.set_ed_sheet(EditorSheet::None);
            stop_preview(ctx);
            return;
        }
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
    if alarm_is_defaults(ctx) {
        let mut defaults = alarm;
        defaults.id = String::new();
        ctx.settings.borrow_mut().prefs.alarm_defaults = Some(defaults);
        crate::persistence::save_settings(&ctx.settings.borrow());
        leave(ctx);
        super::settings::refresh(ctx);
        return;
    }
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

fn alarm_is_defaults(ctx: &Ctx) -> bool {
    ctx.editor
        .borrow()
        .as_ref()
        .is_some_and(|ed| ed.for_defaults)
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
