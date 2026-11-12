//! The panels over the list: the order, pausing, and what else can be done to one alarm.

use std::rc::Rc;

use chrono::{Duration, TimeZone};
use slint::{ModelRc, VecModel};

use super::datepicker::{DateView, Move};
use super::{Ctx, HomeSheet, SheetChoice};
use crate::i18n::{t, Translations};
use crate::models::{AlarmData, ChallengeSetup, SortMode};
use crate::{alarm_ops, schedule};

/// What a row of the open panel does.
#[derive(Clone)]
enum Act {
    Sort(SortMode),
    PauseEnd(Option<String>),
    PauseFor(i64),
    PauseDate(Option<String>),
    Practice(String),
    Skip {
        id: String,
        next: Option<i64>,
        active: bool,
    },
    Duplicate(String),
    MakeDefault(String),
    Delete(String),
}

#[derive(Default)]
pub struct SheetState {
    acts: Vec<Act>,
    /// Pausing until this date: everything, or only this alarm.
    pause_only: Option<String>,
    calendar: Option<DateView>,
}

/// Start of this day, `days` from today, in unix ms.
fn start_of_day_ms(days: i64) -> Option<i64> {
    let day = chrono::Local::now().date_naive() + Duration::days(days);
    start_of(day)
}

fn start_of(day: chrono::NaiveDate) -> Option<i64> {
    chrono::Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|at| at.timestamp_millis())
}

fn paused_message(tr: &Translations, until_ms: i64) -> String {
    let date = chrono::DateTime::from_timestamp_millis(until_ms)
        .map(|at| at.with_timezone(&chrono::Local).date_naive())
        .unwrap_or_default();
    t(tr, "home.paused_message").replace("{date}", &crate::text::day_month(tr, date))
}

fn show(ctx: &Ctx, sheet: HomeSheet, title: String, rows: Vec<(String, bool, bool, Act)>) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let choices: Vec<SheetChoice> = rows
        .iter()
        .map(|(text, selected, danger, _)| SheetChoice {
            text: text.clone().into(),
            selected: *selected,
            danger: *danger,
        })
        .collect();
    ctx.home_sheet.borrow_mut().acts = rows.into_iter().map(|(.., act)| act).collect();
    ui.set_hs_title(title.into());
    ui.set_hs_choices(ModelRc::from(Rc::new(VecModel::from(choices))));
    ui.set_home_sheet(sheet);
}

pub fn close(ctx: &Ctx) {
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_home_sheet(HomeSheet::None);
    }
}

pub fn open_sort(ctx: &Ctx) {
    let (rows, title) = {
        let tr = ctx.tr.borrow();
        let sort = ctx.settings.borrow().prefs.sort;
        let rows = [
            (SortMode::Own, "home.sort_own"),
            (SortMode::Time, "home.sort_time"),
            (SortMode::NextRing, "home.sort_next"),
        ]
        .into_iter()
        .map(|(mode, key)| (t(&tr, key), sort == mode, false, Act::Sort(mode)))
        .collect();
        (rows, t(&tr, "home.sort_title"))
    };
    show(ctx, HomeSheet::Sort, title, rows);
}

pub fn open_pause_all(ctx: &Ctx) {
    let now_ms = chrono::Local::now().timestamp_millis();
    let paused = alarm_ops::all_paused_until(&ctx.alarms.borrow(), now_ms).is_some();
    let (rows, title) = {
        let tr = ctx.tr.borrow();
        let mut rows = Vec::new();
        if paused {
            rows.push((t(&tr, "home.pause_end"), false, false, Act::PauseEnd(None)));
        }
        rows.push((
            t(&tr, "home.pause_tomorrow"),
            false,
            false,
            Act::PauseFor(1),
        ));
        rows.push((t(&tr, "home.pause_week"), false, false, Act::PauseFor(7)));
        rows.push((
            t(&tr, "home.pause_date"),
            false,
            false,
            Act::PauseDate(None),
        ));
        (rows, t(&tr, "home.pause_title"))
    };
    show(ctx, HomeSheet::PauseAll, title, rows);
}

pub fn open_menu(ctx: &Ctx, id: &str) {
    let Some(alarm) = ctx.alarms.borrow().iter().find(|a| a.id == id).cloned() else {
        return;
    };
    let now_ms = chrono::Local::now().timestamp_millis();
    let next = super::home::next_fire_times(std::slice::from_ref(&alarm))
        .get(id)
        .copied();
    let skip_active = alarm.skip_until_ms.is_some_and(|skip| skip > now_ms);
    let pause_active = alarm.pause_until_ms.is_some_and(|until| until > now_ms);
    let can_skip =
        alarm.enabled && !schedule::is_one_time(&alarm) && (skip_active || next.is_some());
    let twenty_four = crate::text::uses_24_hours(&ctx.settings.borrow());
    let tr = ctx.tr.borrow();
    let title = format!(
        "{} {}",
        crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four),
        alarm.label
    );
    let mut rows = Vec::new();
    if !alarm.challenges.is_empty() {
        rows.push((
            t(&tr, "home.practice"),
            false,
            false,
            Act::Practice(id.to_owned()),
        ));
    }
    if can_skip {
        let key = if skip_active {
            "action.undo_skip"
        } else {
            "action.skip_next"
        };
        rows.push((
            t(&tr, key),
            false,
            false,
            Act::Skip {
                id: id.to_owned(),
                next,
                active: skip_active,
            },
        ));
    }
    rows.push((
        t(&tr, "action.duplicate"),
        false,
        false,
        Act::Duplicate(id.to_owned()),
    ));
    if pause_active {
        rows.push((
            t(&tr, "home.pause_end"),
            false,
            false,
            Act::PauseEnd(Some(id.to_owned())),
        ));
    } else {
        rows.push((
            t(&tr, "home.pause_one"),
            false,
            false,
            Act::PauseDate(Some(id.to_owned())),
        ));
    }
    rows.push((
        t(&tr, "home.make_default"),
        false,
        false,
        Act::MakeDefault(id.to_owned()),
    ));
    rows.push((
        t(&tr, "action.delete"),
        false,
        true,
        Act::Delete(id.to_owned()),
    ));
    drop(tr);
    show(ctx, HomeSheet::Menu, title, rows);
}

fn show_pause_date(ctx: &Ctx, only: Option<String>) {
    let today = chrono::Local::now().date_naive();
    let view = DateView::new(today + Duration::days(7), today + Duration::days(1));
    view.show(ctx);
    {
        let mut state = ctx.home_sheet.borrow_mut();
        state.pause_only = only;
        state.calendar = Some(view);
    }
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_home_sheet(HomeSheet::PauseDate);
    }
}

/// A tap in the month of the pause panel.
pub fn date_action(ctx: &Rc<Ctx>, action: Move) {
    let mut state = ctx.home_sheet.borrow_mut();
    if let Some(view) = state.calendar.as_mut() {
        match action {
            Move::Prev => view.prev(),
            Move::Next => view.next(),
            Move::Pick(index) => {
                view.pick(ctx, index);
            }
        }
        view.show(ctx);
    }
}

pub fn pause_apply(ctx: &Rc<Ctx>) {
    let (only, until) = {
        let state = ctx.home_sheet.borrow();
        (
            state.pause_only.clone(),
            state
                .calendar
                .as_ref()
                .map(|view| view.selected)
                .and_then(start_of),
        )
    };
    let now_ms = chrono::Local::now().timestamp_millis();
    if let Some(until) = until.filter(|until| *until > now_ms) {
        pause(ctx, only, until);
    }
    close(ctx);
}

fn pause(ctx: &Rc<Ctx>, only: Option<String>, until: i64) {
    let message = paused_message(&ctx.tr.borrow(), until);
    super::home::change(ctx, message, &mut |list| match &only {
        Some(id) => {
            if let Some(alarm) = list.iter_mut().find(|a| a.id == *id) {
                alarm.pause_until_ms = Some(until);
            }
        }
        None => alarm_ops::pause_all(list, until),
    });
}

fn end_pause(ctx: &Rc<Ctx>, only: Option<String>) {
    let message = t(&ctx.tr.borrow(), "home.pause_ended");
    super::home::change(ctx, message, &mut |list| match &only {
        Some(id) => {
            if let Some(alarm) = list.iter_mut().find(|a| a.id == *id) {
                alarm.pause_until_ms = None;
            }
        }
        None => alarm_ops::resume_all(list),
    });
}

/// A row of the open panel was tapped.
pub fn picked(ctx: &Rc<Ctx>, index: usize) {
    let Some(act) = ctx.home_sheet.borrow().acts.get(index).cloned() else {
        return;
    };
    match act {
        Act::PauseDate(only) => {
            show_pause_date(ctx, only);
            return;
        }
        Act::Sort(mode) => {
            ctx.settings.borrow_mut().prefs.sort = mode;
            crate::persistence::save_settings(&ctx.settings.borrow());
            super::home::refresh(ctx);
        }
        Act::PauseEnd(only) => end_pause(ctx, only),
        Act::PauseFor(days) => {
            if let Some(until) = start_of_day_ms(days) {
                pause(ctx, None, until);
            }
        }
        Act::Practice(id) => {
            close(ctx);
            let setups = ctx
                .alarms
                .borrow()
                .iter()
                .find(|a| a.id == id)
                .map(|a| ChallengeSetup::list_from_stored(&a.challenges))
                .unwrap_or_default();
            super::challenge_run::start_practice(ctx, &setups);
            return;
        }
        Act::Skip { id, next, active } => super::home::toggle_skip(ctx, &id, next, active),
        Act::Duplicate(id) => {
            close(ctx);
            if ctx.alarms.borrow().len() >= alarm_ops::MAX_ALARMS {
                let message = t(&ctx.tr.borrow(), "alarm.limit_reached");
                super::toast::error(ctx, message);
            } else {
                super::editor::open(ctx, &format!("copy-{id}"));
            }
            return;
        }
        Act::MakeDefault(id) => {
            let alarm: Option<AlarmData> = ctx.alarms.borrow().iter().find(|a| a.id == id).cloned();
            if let Some(alarm) = alarm {
                ctx.settings.borrow_mut().prefs.alarm_defaults =
                    Some(alarm_ops::as_defaults(&alarm));
                crate::persistence::save_settings(&ctx.settings.borrow());
                let message = t(&ctx.tr.borrow(), "home.made_default");
                super::toast::show(ctx, message, None);
            }
        }
        Act::Delete(id) => super::home::delete_alarm(ctx, &id),
    }
    close(ctx);
}
