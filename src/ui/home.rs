//! The alarm list: what the cards say and what can be done to them in one tap.

use std::collections::HashMap;
use std::rc::Rc;

use chrono::{TimeZone, Timelike};
use slint::{ModelRc, SharedString, VecModel};

use super::{AlarmRow, Ctx, DayCell};
use crate::i18n::t;
use crate::models::{AlarmData, SortMode};
use crate::{alarm_ops, schedule};

/// What the quick row offers, in minutes.
pub const QUICK_MINUTES: [i64; 5] = [5, 10, 15, 30, 60];

/// The two starting points the empty list offers: key, hour, days.
const TEMPLATES: [(&str, u8, [bool; 7]); 2] = [
    (
        "home.template_weekdays",
        7,
        [true, true, true, true, true, false, false],
    ),
    (
        "home.template_weekend",
        9,
        [false, false, false, false, false, true, true],
    ),
];

fn local_date(at_ms: i64) -> Option<chrono::NaiveDate> {
    Some(
        chrono::DateTime::from_timestamp_millis(at_ms)?
            .with_timezone(&chrono::Local)
            .date_naive(),
    )
}

/// What an alarm's card says about when it rings: only what the days and the switch do not
/// show already.
fn when_text(
    ctx: &Ctx,
    alarm: &AlarmData,
    next_fire: Option<i64>,
    now_ms: i64,
    twenty_four: bool,
) -> String {
    let tr = ctx.tr.borrow();
    if !alarm.enabled && !schedule::is_one_time(alarm) {
        return String::new();
    }
    let snoozed_until = crate::platform::alarm_kit_optional()
        .and_then(|kit| kit.snoozed_until(&mobile_sentinel::InstanceId::new(alarm.id.clone())))
        .and_then(|at| chrono::Local.timestamp_millis_opt(at).single());
    if let Some(at) = snoozed_until {
        let time = crate::text::clock(at.hour() as u8, at.minute() as u8, twenty_four);
        return t(&tr, "home.snoozed_until").replace("{time}", &time);
    }
    if let Some(until) = alarm.pause_until_ms.filter(|until| *until > now_ms) {
        return local_date(until)
            .map(|date| {
                t(&tr, "home.paused_until").replace("{date}", &crate::text::day_month(&tr, date))
            })
            .unwrap_or_default();
    }
    if alarm.skip_until_ms.is_some_and(|skip| skip > now_ms) {
        if let Some(date) = alarm.skip_until_ms.and_then(local_date) {
            return t(&tr, "home.skips").replace("{day}", &crate::text::day_name(&tr, date));
        }
    }
    // The days say when a repeating alarm rings, and the top of the screen which is next.
    if !schedule::is_one_time(alarm) {
        return String::new();
    }
    let date = next_fire.and_then(local_date).or_else(|| {
        alarm
            .date
            .as_deref()
            .and_then(|date| chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
    });
    let Some(date) = date else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    match schedule::day_label(today, date) {
        schedule::DayLabel::Today => t(&tr, "home.today"),
        schedule::DayLabel::Tomorrow => t(&tr, "home.tomorrow"),
        schedule::DayLabel::Weekday(_) => crate::text::short_date(&tr, date),
    }
}

/// The alarms as the list shows them.
fn sorted_for_list(
    alarms: &[AlarmData],
    sort: SortMode,
    next: &HashMap<String, i64>,
) -> Vec<AlarmData> {
    let mut list = crate::ordering::sorted_for_display(alarms);
    list.retain(|a| !a.hidden);
    match sort {
        SortMode::Own => {}
        SortMode::Time => list.sort_by_key(|a| (a.time_hour, a.time_minute)),
        SortMode::NextRing => list.sort_by_key(|a| next.get(&a.id).copied().unwrap_or(i64::MAX)),
    }
    list
}

/// When each enabled alarm really rings next, from AlarmKit (it knows the weekdays, a snooze
/// in progress and one-time dates).
pub fn next_fire_times(alarms: &[AlarmData]) -> HashMap<String, i64> {
    let kit = crate::platform::alarm_kit_optional();
    alarms
        .iter()
        .filter(|a| a.enabled && !a.hidden)
        .filter_map(|a| {
            let at = kit?
                .next_fire(&mobile_sentinel::InstanceId::new(a.id.clone()))
                .ok()
                .flatten()?;
            Some((a.id.clone(), at))
        })
        .collect()
}

/// Draw the list and its header again from the current state.
pub fn refresh(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let settings = ctx.settings.borrow();
    let alarms = ctx.alarms.borrow();
    let tr = ctx.tr.borrow();
    let twenty_four = crate::text::uses_24_hours(&settings);
    let week_start = settings.prefs.week_start;
    let now_ms = chrono::Local::now().timestamp_millis();
    let next_fire = next_fire_times(&alarms);
    let list = sorted_for_list(&alarms, settings.prefs.sort, &next_fire);

    let rows: Vec<AlarmRow> = list
        .iter()
        .map(|alarm| {
            let next = next_fire.get(&alarm.id).copied();
            let one_time = schedule::is_one_time(alarm);
            let skip_active = alarm.skip_until_ms.is_some_and(|skip| skip > now_ms);
            AlarmRow {
                id: alarm.id.clone().into(),
                time: crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four).into(),
                twelve: !twenty_four,
                label: alarm.label.clone().into(),
                when: when_text(ctx, alarm, next, now_ms, twenty_four).into(),
                days: ModelRc::from(Rc::new(VecModel::from(
                    week_start
                        .day_order()
                        .into_iter()
                        .map(|day| DayCell {
                            letter: crate::text::day_letter(&tr, day).into(),
                            on: alarm.days[day],
                        })
                        .collect::<Vec<_>>(),
                ))),
                on: alarm.enabled,
                can_skip: alarm.enabled && !one_time && (skip_active || next.is_some()),
                skip_active,
            }
        })
        .collect();
    ui.set_rows(ModelRc::from(Rc::new(VecModel::from(rows))));

    let next_ms = schedule::earliest_future(now_ms, next_fire.values().copied());
    let (title, sub) = match next_ms {
        Some(at) => {
            let local = chrono::DateTime::from_timestamp_millis(at)
                .map(|at| at.with_timezone(&chrono::Local));
            let title = local
                .map(|at| {
                    format!(
                        "{} {}",
                        crate::text::day_name(&tr, at.date_naive()),
                        crate::text::clock(at.hour() as u8, at.minute() as u8, twenty_four)
                    )
                })
                .unwrap_or_default();
            (title, countdown_text_with(&tr, now_ms, at))
        }
        None => (t(&tr, "home.none_on"), String::new()),
    };
    ui.set_next_title(title.into());
    ui.set_next_sub(sub.into());
    ui.set_paused(alarm_ops::all_paused_until(&alarms, now_ms).is_some());

    let labels: Vec<SharedString> = QUICK_MINUTES
        .iter()
        .map(|minutes| format!("+{minutes}").into())
        .collect();
    ui.set_quick_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
    let template_label = |(key, hour, _): &(&str, u8, [bool; 7])| -> SharedString {
        t(&tr, key)
            .replace("{time}", &crate::text::clock(*hour, 0, twenty_four))
            .into()
    };
    ui.set_empty_weekdays(template_label(&TEMPLATES[0]));
    ui.set_empty_weekend(template_label(&TEMPLATES[1]));
    drop(tr);
    super::reliability::show_banner(ctx);
}

fn countdown_text_with(tr: &crate::i18n::Translations, now_ms: i64, at_ms: i64) -> String {
    let time = crate::text::time_left(tr, schedule::countdown(now_ms, at_ms));
    t(tr, "home.in").replace("{time}", &time)
}

/// Change the list, save it, tell the user, and offer to take it back.
pub fn change(ctx: &Rc<Ctx>, message: String, apply: &mut dyn FnMut(&mut Vec<AlarmData>)) {
    let before = ctx.alarms.borrow().clone();
    apply(&mut ctx.alarms.borrow_mut());
    crate::persistence::save_alarms(&ctx.alarms.borrow());
    super::toast::show(ctx, message, Some(before));
    refresh(ctx);
}

/// Whether there is room for another alarm; says so when there is not.
fn has_room(ctx: &Rc<Ctx>) -> bool {
    if ctx.alarms.borrow().len() >= alarm_ops::MAX_ALARMS {
        let message = t(&ctx.tr.borrow(), "alarm.limit_reached");
        super::toast::error(ctx, message);
        return false;
    }
    true
}

pub fn toggle(ctx: &Rc<Ctx>, id: &str) {
    let now = chrono::Local::now().naive_local();
    let mut message = String::new();
    change(ctx, String::new(), &mut |list| {
        let tr = ctx.tr.borrow();
        if let Some(alarm) = list.iter_mut().find(|a| a.id == id) {
            alarm.enabled = !alarm.enabled;
            if alarm.enabled {
                schedule::refresh_one_time_date(alarm, now);
                message = crate::text::rings_in(&tr, alarm);
            } else {
                message = t(&tr, "home.switched_off");
            }
        }
    });
    super::toast::set_message(ctx, message);
}

pub fn quick(ctx: &Rc<Ctx>, index: usize) {
    let Some(minutes) = QUICK_MINUTES.get(index).copied() else {
        return;
    };
    if !has_room(ctx) {
        return;
    }
    let (label, message) = {
        let tr = ctx.tr.borrow();
        (
            t(&tr, "home.quick_alarm"),
            t(&tr, "home.quick_set").replace("{minutes}", &minutes.to_string()),
        )
    };
    let defaults = ctx.settings.borrow().prefs.alarm_defaults.clone();
    let now = chrono::Local::now().naive_local();
    let mut alarm = alarm_ops::quick_alarm(now, minutes, label, defaults.as_ref());
    alarm.order = crate::ordering::order_for_new(&ctx.alarms.borrow());
    change(ctx, message, &mut |list| list.push(alarm.clone()));
}

pub fn template(ctx: &Rc<Ctx>, index: usize) {
    let Some((_, hour, days)) = TEMPLATES.get(index).copied() else {
        return;
    };
    if !has_room(ctx) {
        return;
    }
    let defaults = ctx.settings.borrow().prefs.alarm_defaults.clone();
    let mut alarm = alarm_ops::new_alarm(defaults.as_ref());
    alarm.id = uuid::Uuid::new_v4().to_string();
    alarm.time_hour = hour;
    alarm.time_minute = 0;
    alarm.days = days;
    alarm.order = crate::ordering::order_for_new(&ctx.alarms.borrow());
    let message = crate::text::rings_in(&ctx.tr.borrow(), &alarm);
    change(ctx, message, &mut |list| list.push(alarm.clone()));
}

/// The + button: a new alarm, if there is room.
pub fn create(ctx: &Rc<Ctx>) {
    if has_room(ctx) {
        super::editor::open(ctx, "new");
    }
}

pub fn toggle_skip(ctx: &Rc<Ctx>, id: &str, next: Option<i64>, skip_active: bool) {
    let message = {
        let tr = ctx.tr.borrow();
        if skip_active {
            t(&tr, "home.skip_undone")
        } else {
            t(&tr, "home.skipped")
        }
    };
    change(ctx, message, &mut |list| {
        // Skipping remembers the occurrence to skip (the next ring); undoing clears it.
        if let Some(alarm) = list.iter_mut().find(|a| a.id == id) {
            alarm.skip_until_ms = if skip_active { None } else { next };
        }
    });
}

pub fn delete_alarm(ctx: &Rc<Ctx>, id: &str) {
    let name = ctx
        .alarms
        .borrow()
        .iter()
        .find(|a| a.id == id)
        .map(|a| a.label.clone())
        .unwrap_or_default();
    let message = {
        let tr = ctx.tr.borrow();
        if name.is_empty() {
            t(&tr, "home.deleted_plain")
        } else {
            t(&tr, "home.deleted").replace("{alarm}", &name)
        }
    };
    change(ctx, message, &mut |list| list.retain(|a| a.id != id));
}
