//! The history screen: how the alarms went, for a period of the user's choice.

use std::cell::Cell;
use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};

use super::panel::{self, Act, Panel};
use super::{BarCell, Ctx, EventLine, StatLine, SCREEN_HISTORY};
use crate::history_view::{self as view, CUSTOM_PERIOD, EVER_DAYS, PERIODS, STREAK_LOOKBACK_DAYS};
use crate::i18n::t;
use crate::models::HistoryFilter;

/// The period and the number of days of the user's own, while the screen is up.
pub struct HistoryState {
    period: Cell<usize>,
    custom_days: Cell<u16>,
}

impl Default for HistoryState {
    fn default() -> Self {
        Self {
            period: Cell::new(0),
            custom_days: Cell::new(30),
        }
    }
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

/// Show the screen, on this week.
pub fn enter(ctx: &Ctx) {
    ctx.history.period.set(0);
    ctx.history.custom_days.set(30);
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_panel_open(false);
        ui.set_h_custom_text(ctx.history.custom_days.get().to_string().into());
        ui.set_screen(SCREEN_HISTORY);
    }
    refresh(ctx);
}

/// Draw the screen again, if it is the one up.
pub fn refresh_if_shown(ctx: &Ctx) {
    if ctx
        .ui
        .upgrade()
        .is_some_and(|ui| ui.get_screen() == SCREEN_HISTORY)
    {
        refresh(ctx);
    }
}

/// Read the period from the database and show it.
pub fn refresh(ctx: &Ctx) {
    let Some(ui) = ctx.ui.upgrade() else {
        return;
    };
    let (week_start, twenty_four) = {
        let settings = ctx.settings.borrow();
        (
            settings.prefs.week_start,
            crate::text::uses_24_hours(&settings),
        )
    };
    let tr = ctx.tr.borrow();
    let period = ctx.history.period.get();
    let filter = view::period_filter(PERIODS[period], ctx.history.custom_days.get(), week_start);
    let events = crate::history_persistence::query_history(&filter);
    let metrics = crate::history_persistence::aggregate_history(&filter);
    let today = chrono::Local::now().date_naive();
    let streak_days = view::streak(
        &crate::history_persistence::query_history(&HistoryFilter::LastDays(STREAK_LOOKBACK_DAYS)),
        today,
    );
    let nothing_at_all = events.is_empty()
        && crate::history_persistence::query_history(&HistoryFilter::LastDays(EVER_DAYS))
            .is_empty();

    ui.set_h_periods(model(
        PERIODS
            .iter()
            .map(|key| SharedString::from(t(&tr, key)))
            .collect(),
    ));
    ui.set_h_period(period as i32);
    ui.set_h_empty(match (events.is_empty(), nothing_at_all) {
        (false, _) => 0,
        (true, true) => 1,
        (true, false) => 2,
    });
    ui.set_h_streak(crate::text::counted(&tr, "history.streak_days", streak_days).into());
    ui.set_h_time_to_stop(crate::text::duration(&tr, metrics.avg_total_dismissal_time).into());
    ui.set_h_snoozes(crate::text::one_decimal(&tr, metrics.avg_snooze_count).into());

    let bars = view::chart(&tr, &filter, &events, week_start, today);
    ui.set_h_has_bars(bars.is_some());
    let bars = bars.unwrap_or_default();
    let longest = bars.iter().map(|bar| bar.seconds).max().unwrap_or(0).max(1);
    ui.set_h_bars(model(
        bars.into_iter()
            .map(|bar| BarCell {
                height: view::bar_height(bar.seconds, longest) as i32,
                label: bar.label.into(),
                current: bar.current,
            })
            .collect(),
    ));
    ui.set_h_stats(model(
        view::stat_rows(&tr, &metrics)
            .into_iter()
            .map(|(name, value)| StatLine {
                name: name.into(),
                value: value.into(),
            })
            .collect(),
    ));
    ui.set_h_events(model(
        events
            .iter()
            .map(|event| {
                let text = view::event_text(&tr, event, twenty_four);
                EventLine {
                    id: event.id.clone().into(),
                    title: text.title.into(),
                    sub: text.sub.into(),
                    flag: text.flag,
                    value: text.value.into(),
                    details: model(
                        text.details
                            .into_iter()
                            .map(|(name, value)| StatLine {
                                name: name.into(),
                                value: value.into(),
                            })
                            .collect(),
                    ),
                }
            })
            .collect(),
    ));
}

pub fn period_picked(ctx: &Ctx, index: usize) {
    if index < PERIODS.len() {
        ctx.history.period.set(index);
        refresh(ctx);
    }
}

/// The number of days of the user's own was typed.
pub fn custom_edited(ctx: &Ctx, text: &str) {
    if let Ok(days) = text.trim().parse::<u16>() {
        ctx.history.custom_days.set(days.clamp(1, 365));
        if ctx.history.period.get() == CUSTOM_PERIOD {
            refresh(ctx);
        }
    }
}

/// The field is left: show the number that was used.
pub fn custom_accepted(ctx: &Ctx) {
    if let Some(ui) = ctx.ui.upgrade() {
        ui.set_h_custom_text(ctx.history.custom_days.get().to_string().into());
    }
}

/// The trash button, and the settings row: how much to delete.
pub fn open_delete(ctx: &Ctx) {
    let tr = ctx.tr.borrow();
    panel::show(
        ctx,
        Panel {
            title: t(&tr, "history.delete_history"),
            rows: vec![
                (
                    t(&tr, "history.delete_30"),
                    false,
                    false,
                    Act::AskDelete(Some(30)),
                ),
                (
                    t(&tr, "history.delete_90"),
                    false,
                    false,
                    Act::AskDelete(Some(90)),
                ),
                (
                    t(&tr, "history.delete_all"),
                    false,
                    true,
                    Act::AskDelete(None),
                ),
            ],
            chip_selected: -1,
            ..Panel::default()
        },
    );
}

/// "Really?" before history goes: a mis-tap must not take a record with it.
fn ask(ctx: &Ctx, title: String, confirm: Act) {
    let tr = ctx.tr.borrow();
    panel::show(
        ctx,
        Panel {
            title,
            note: t(&tr, "history.delete_all_note"),
            rows: vec![(t(&tr, "action.delete"), false, true, confirm)],
            chip_selected: -1,
            done: Some((t(&tr, "action.cancel"), Act::Close)),
            ..Panel::default()
        },
    );
}

pub fn ask_delete(ctx: &Ctx, older_than_days: Option<u32>) {
    let title = {
        let tr = ctx.tr.borrow();
        match older_than_days {
            None => t(&tr, "history.delete_all_title"),
            Some(days) => t(&tr, "history.delete_older_title").replace("{days}", &days.to_string()),
        }
    };
    ask(ctx, title, Act::DeleteHistory(older_than_days));
}

pub fn ask_delete_event(ctx: &Ctx, id: &str) {
    let title = t(&ctx.tr.borrow(), "history.delete_one_title");
    ask(ctx, title, Act::DeleteEvent(id.to_owned()));
}

pub fn delete_history(ctx: &Rc<Ctx>, older_than_days: Option<u32>) {
    crate::history_persistence::delete_history(older_than_days);
    let message = t(&ctx.tr.borrow(), "history.deleted");
    super::toast::show(ctx, message, None);
    refresh_if_shown(ctx);
}

pub fn delete_event(ctx: &Ctx, id: &str) {
    crate::history_persistence::delete_event(id);
    refresh_if_shown(ctx);
}
