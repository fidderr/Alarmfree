use chrono::{Datelike, NaiveDate};
use dioxus::prelude::*;

use crate::app::{t, Route, Translations};
use crate::components::{BottomNav, Icon, IconName, Sheet, SheetDone, SheetItem, ToastMessage};
use crate::models::setup::difficulty_name;
use crate::models::{
    AggregateMetrics, AlarmEvent, AlarmOutcome, AppSettings, ChallengeType, HistoryFilter,
    WeekStart,
};

/// How far back a streak is looked for.
/// For "all of it": a period is a number of days.
const EVER_DAYS: u32 = 36_500;
const STREAK_LOOKBACK_DAYS: u32 = 400;

/// The periods, as their translation keys.
const PERIODS: [&str; 7] = [
    "filter.this_week",
    "filter.last_week",
    "filter.this_month",
    "filter.last_month",
    "filter.this_year",
    "filter.last_year",
    "filter.custom_days",
];

fn period_filter(key: &str, custom_days: u16, week_start: WeekStart) -> HistoryFilter {
    match key {
        "filter.last_week" => HistoryFilter::LastWeek(week_start),
        "filter.this_month" => HistoryFilter::ThisMonth,
        "filter.last_month" => HistoryFilter::LastMonth,
        "filter.this_year" => HistoryFilter::ThisYear,
        "filter.last_year" => HistoryFilter::LastYear,
        "filter.custom_days" => HistoryFilter::LastDays(u32::from(custom_days)),
        _ => HistoryFilter::ThisWeek(week_start),
    }
}

/// How many days in a row every alarm was dealt with: at least one solved, nothing missed
/// or left ringing. A day without alarms is no break, and does not count either.
fn streak(events: &[AlarmEvent], today: NaiveDate) -> u32 {
    use std::collections::BTreeMap;
    let mut days: BTreeMap<NaiveDate, bool> = BTreeMap::new();
    for event in events {
        let day = event
            .fire_timestamp
            .with_timezone(&chrono::Local)
            .date_naive();
        let fine = matches!(event.outcome, AlarmOutcome::Solved | AlarmOutcome::Backup);
        let entry = days.entry(day).or_insert(true);
        *entry = *entry && fine;
    }
    let mut count = 0;
    for (day, fine) in days.iter().rev() {
        if *day > today {
            continue;
        }
        if !fine {
            break;
        }
        count += 1;
    }
    count
}

/// One bar of the chart.
#[derive(Debug, Clone, PartialEq)]
struct Bar {
    label: String,
    /// Average seconds to stop the alarm, 0 when nothing rang.
    seconds: u64,
    current: bool,
}

/// Group events into bars for the period: by weekday for a week, by week for a month, by
/// month for a year. `None` for a period of the user's own, which has no natural groups.
fn chart(
    translations: &Translations,
    filter: &HistoryFilter,
    events: &[AlarmEvent],
    week_start: WeekStart,
    today: NaiveDate,
) -> Option<Vec<Bar>> {
    let mut groups: Vec<(String, Vec<u64>, bool)> = match filter {
        HistoryFilter::ThisWeek(_) | HistoryFilter::LastWeek(_) => week_start
            .day_order()
            .iter()
            .map(|day| {
                let is_today = matches!(filter, HistoryFilter::ThisWeek(_))
                    && today.weekday().num_days_from_monday() as usize == *day;
                (
                    crate::text::day_letter(translations, *day),
                    Vec::new(),
                    is_today,
                )
            })
            .collect(),
        HistoryFilter::ThisMonth | HistoryFilter::LastMonth => (0..5)
            .map(|week| {
                let current = *filter == HistoryFilter::ThisMonth && today.day0() / 7 == week;
                ((week * 7 + 1).to_string(), Vec::new(), current)
            })
            .collect(),
        HistoryFilter::ThisYear | HistoryFilter::LastYear => (0..12)
            .map(|month| {
                let current = *filter == HistoryFilter::ThisYear && today.month0() == month;
                (
                    crate::text::month_short(translations, month as usize)
                        .chars()
                        .next()
                        .map(|c| c.to_uppercase().to_string())
                        .unwrap_or_default(),
                    Vec::new(),
                    current,
                )
            })
            .collect(),
        HistoryFilter::LastDays(_) => return None,
    };

    // Only what was timed, as in the figures next to it.
    for event in events.iter().filter(|e| e.outcome == AlarmOutcome::Solved) {
        let day = event
            .fire_timestamp
            .with_timezone(&chrono::Local)
            .date_naive();
        let group = match filter {
            HistoryFilter::ThisWeek(_) | HistoryFilter::LastWeek(_) => {
                let monday_based = day.weekday().num_days_from_monday() as usize;
                week_start
                    .day_order()
                    .iter()
                    .position(|d| *d == monday_based)
            }
            HistoryFilter::ThisMonth | HistoryFilter::LastMonth => Some((day.day0() / 7) as usize),
            _ => Some(day.month0() as usize),
        };
        if let Some(group) = group.and_then(|g| groups.get_mut(g)) {
            group.1.push(event.total_dismissal_time.as_secs());
        }
    }

    Some(
        groups
            .into_iter()
            .map(|(label, seconds, current)| Bar {
                label,
                seconds: if seconds.is_empty() {
                    0
                } else {
                    seconds.iter().sum::<u64>() / seconds.len() as u64
                },
                current,
            })
            .collect(),
    )
}

fn challenge_name(translations: &Translations, kind: ChallengeType) -> String {
    let key = match kind {
        ChallengeType::Math => "challenge.math",
        ChallengeType::MemoryGame => "challenge.memory",
        ChallengeType::Typing => "challenge.typing",
        ChallengeType::ShakeToWake => "challenge.shake",
        ChallengeType::StepCount => "challenge.steps",
        ChallengeType::Scan => "challenge.scan",
        ChallengeType::HoldButton => "challenge.hold",
        ChallengeType::Reaction => "challenge.reaction",
    };
    t(translations, key)
}

/// A solved challenge's name, with its difficulty where it has one.
fn solve_name(
    translations: &Translations,
    kind: ChallengeType,
    difficulty: crate::models::Difficulty,
) -> String {
    let name = challenge_name(translations, kind);
    if matches!(
        kind,
        ChallengeType::Math | ChallengeType::MemoryGame | ChallengeType::Typing
    ) {
        let level = t(
            translations,
            &format!("difficulty.{}", difficulty_name(difficulty)),
        );
        format!("{name}, {level}")
    } else {
        name
    }
}

#[component]
pub fn History() -> Element {
    let translations = use_context::<Translations>();
    let settings = use_context::<Signal<AppSettings>>();
    let mut period = use_signal(|| PERIODS[0]);
    let mut custom_days = use_signal(|| 30u16);
    let mut deleting = use_signal(|| false);
    let mut show_all = use_signal(|| false);
    let mut open_event: Signal<Option<String>> = use_signal(|| None);
    // The entry whose deletion is being asked about.
    let mut confirm_delete: Signal<Option<String>> = use_signal(|| None);
    let mut refresh = use_signal(|| 0u32);

    // Not read again for opening a row.
    let loaded = use_memo(move || {
        let _ = refresh();
        let filter = period_filter(period(), custom_days(), settings.read().prefs.week_start);
        let events = crate::history_persistence::query_history(&filter);
        let metrics = crate::history_persistence::aggregate_history(&filter);
        let lookback = HistoryFilter::LastDays(STREAK_LOOKBACK_DAYS);
        let streak_days = streak(
            &crate::history_persistence::query_history(&lookback),
            chrono::Local::now().date_naive(),
        );
        let nothing_at_all = events.is_empty()
            && crate::history_persistence::query_history(&HistoryFilter::LastDays(EVER_DAYS))
                .is_empty();
        (filter, events, metrics, streak_days, nothing_at_all)
    });
    let (filter, events, metrics, streak_days, nothing_at_all) = loaded();
    let today = chrono::Local::now().date_naive();
    let week_start = settings.read().prefs.week_start;
    let bars = chart(&translations, &filter, &events, week_start, today);
    let twenty_four = crate::text::uses_24_hours(&settings.read());

    let delete_aria = t(&translations, "history.delete_history");

    rsx! {
        div { class: "screen",
            header { class: "main-header main-header-row",
                h1 { class: "screen-title-large", {t(&translations, "history.title")} }
                button {
                    class: "icon-btn",
                    "aria-label": "{delete_aria}",
                    onclick: move |_| deleting.set(true),
                    Icon { name: IconName::Trash, size: 20 }
                }
            }

            div { class: "chip-scroll",
                for key in PERIODS {
                    button {
                        class: if period() == key { "chip-pill selected" } else { "chip-pill" },
                        "aria-pressed": if period() == key { "true" } else { "false" },
                        onclick: move |_| period.set(key),
                        {t(&translations, key)}
                    }
                }
            }
            if period() == "filter.custom_days" {
                label { class: "custom-days",
                    {t(&translations, "filter.last_prefix")}
                    input {
                        class: "field field-short",
                        r#type: "number",
                        min: "1",
                        max: "365",
                        value: "{custom_days}",
                        onchange: move |evt| {
                            if let Ok(days) = evt.value().parse::<u16>() {
                                custom_days.set(days.clamp(1, 365));
                            }
                        },
                    }
                    {t(&translations, "filter.days_unit")}
                }
            }

            div { class: "scroll",
                if events.is_empty() {
                    div { class: "empty",
                        div { class: "empty-icon", Icon { name: IconName::ChartBar, size: 40 } }
                        if nothing_at_all {
                            p { class: "empty-title", {t(&translations, "history.no_events")} }
                            p { class: "empty-sub", {t(&translations, "history.events_subtitle")} }
                        } else {
                            p { class: "empty-title", {t(&translations, "history.none_in_period")} }
                        }
                    }
                } else {
                    Summary {
                        metrics: metrics.clone(),
                        streak_days,
                        bars,
                        show_all: show_all(),
                        on_toggle: move |_| show_all.set(!show_all()),
                    }
                    ul { class: "list",
                        for event in events.iter().cloned() {
                            {
                                let id = event.id.clone();
                                let open = open_event() == Some(id.clone());
                                let on_delete_id = id.clone();
                                rsx! {
                                    EventRow {
                                        key: "{id}",
                                        event,
                                        open,
                                        twenty_four,
                                        on_toggle: move |_| {
                                            open_event.set(if open { None } else { Some(id.clone()) });
                                        },
                                        on_delete: move |_| confirm_delete.set(Some(on_delete_id.clone())),
                                    }
                                }
                            }
                        }
                    }
                }
            }

            BottomNav { current: Route::History {} }

            if deleting() {
                DeleteHistorySheet {
                    on_close: move |_| deleting.set(false),
                    on_deleted: move |_| refresh += 1,
                }
            }
            if let Some(id) = confirm_delete() {
                AskDeleteSheet {
                    title: t(&translations, "history.delete_one_title"),
                    on_confirm: move |_| {
                        crate::history_persistence::delete_event(&id);
                        confirm_delete.set(None);
                        refresh += 1;
                    },
                    on_close: move |_| confirm_delete.set(None),
                }
            }
        }
    }
}

/// How much of the history to delete. Every choice is asked about once more: nothing
/// brings it back.
#[component]
pub fn DeleteHistorySheet(
    on_close: EventHandler<()>,
    #[props(default)] on_deleted: Option<EventHandler<()>>,
) -> Element {
    let translations = use_context::<Translations>();
    let mut toast = use_context::<Signal<Option<ToastMessage>>>();
    // The choice being asked about: everything, or what is older than so many days.
    let mut asking: Signal<Option<Option<u32>>> = use_signal(|| None);
    let mut delete = move |older_than_days: Option<u32>| {
        crate::history_persistence::delete_history(older_than_days);
        toast.set(Some(ToastMessage::info(t(
            &translations,
            "history.deleted",
        ))));
        if let Some(on_deleted) = on_deleted {
            on_deleted.call(());
        }
        on_close.call(());
    };
    if let Some(choice) = asking() {
        let title = match choice {
            None => t(&translations, "history.delete_all_title"),
            Some(days) => {
                t(&translations, "history.delete_older_title").replace("{days}", &days.to_string())
            }
        };
        return rsx! {
            AskDeleteSheet { title, on_confirm: move |_| delete(choice), on_close }
        };
    }
    rsx! {
        Sheet {
            title: t(&translations, "history.delete_history"),
            on_close,
            SheetItem {
                label: t(&translations, "history.delete_30"),
                on_click: move |_| asking.set(Some(Some(30))),
            }
            SheetItem {
                label: t(&translations, "history.delete_90"),
                on_click: move |_| asking.set(Some(Some(90))),
            }
            SheetItem {
                label: t(&translations, "history.delete_all"),
                danger: true,
                on_click: move |_| asking.set(Some(None)),
            }
        }
    }
}

/// "Really?" before history goes: a mis-tap must not take a record with it.
#[component]
fn AskDeleteSheet(
    title: String,
    on_confirm: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let translations = use_context::<Translations>();
    rsx! {
        Sheet {
            title,
            note: t(&translations, "history.delete_all_note"),
            on_close,
            SheetItem {
                label: t(&translations, "action.delete"),
                danger: true,
                on_click: move |_| on_confirm.call(()),
            }
            SheetDone {
                label: t(&translations, "action.cancel"),
                on_click: move |_| on_close.call(()),
            }
        }
    }
}

/// The period at a glance, with everything else one tap away.
#[component]
fn Summary(
    metrics: AggregateMetrics,
    streak_days: u32,
    bars: Option<Vec<Bar>>,
    show_all: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let translations = use_context::<Translations>();
    let duration = |d| crate::text::duration(&translations, d);
    let streak_text = crate::text::counted(&translations, "history.streak_days", streak_days);
    let stats = [
        (
            t(&translations, "history.alarms"),
            metrics.total_events.to_string(),
        ),
        (
            t(&translations, "history.avg_reaction"),
            duration(metrics.avg_reaction_time),
        ),
        (
            t(&translations, "history.avg_total"),
            duration(metrics.avg_total_dismissal_time),
        ),
        (
            t(&translations, "history.avg_time_snoozed"),
            duration(metrics.avg_total_snoozed),
        ),
        (
            t(&translations, "history.avg_snooze_gap"),
            duration(std::time::Duration::from_secs_f64(
                metrics.avg_snooze_interval_minutes.max(0.0) * 60.0,
            )),
        ),
        (
            t(&translations, "history.avg_challenge"),
            duration(metrics.avg_total_challenge_time),
        ),
    ];
    let solves: Vec<(String, String)> = metrics
        .avg_solve_per_challenge
        .iter()
        .map(|((kind, difficulty), avg)| {
            (
                solve_name(&translations, *kind, *difficulty),
                duration(*avg),
            )
        })
        .collect();
    let longest = bars
        .iter()
        .flatten()
        .map(|bar| bar.seconds)
        .max()
        .unwrap_or(0)
        .max(1);
    let chart_aria = t(&translations, "history.chart_aria");
    let toggle_text = if show_all {
        t(&translations, "history.fewer_stats")
    } else {
        t(&translations, "history.all_stats")
    };

    rsx! {
        section { class: "summary",
            div { class: "summary-stats",
                div {
                    p { class: "stat-value", "{streak_text}" }
                    p { class: "stat-name", {t(&translations, "history.streak")} }
                }
                div {
                    p { class: "stat-value", {duration(metrics.avg_total_dismissal_time)} }
                    p { class: "stat-name", {t(&translations, "history.time_to_stop")} }
                }
                div {
                    p { class: "stat-value", {crate::text::one_decimal(&translations, metrics.avg_snooze_count)} }
                    p { class: "stat-name", {t(&translations, "history.snoozes")} }
                }
            }
            if let Some(bars) = bars {
                div { class: "chart", role: "img", "aria-label": "{chart_aria}",
                    for bar in bars {
                        {
                            let height = if bar.seconds == 0 { 0 } else { (bar.seconds * 56 / longest).max(3) };
                            rsx! {
                                span { class: "chart-col",
                                    span {
                                        class: if bar.current { "chart-bar current" } else { "chart-bar" },
                                        style: "height: {height}px",
                                    }
                                    span { class: "chart-label", "{bar.label}" }
                                }
                            }
                        }
                    }
                }
            }
            if show_all {
                dl { class: "stat-list",
                    for (name, value) in stats {
                        dt { "{name}" }
                        dd { "{value}" }
                    }
                    for (name, value) in solves {
                        dt { "{name}" }
                        dd { "{value}" }
                    }
                }
            }
            button {
                class: "link-btn summary-toggle",
                "aria-expanded": if show_all { "true" } else { "false" },
                onclick: move |_| on_toggle.call(()),
                "{toggle_text}"
            }
        }
    }
}

/// One alarm in the list. Tap for how it went.
#[component]
fn EventRow(
    event: AlarmEvent,
    open: bool,
    twenty_four: bool,
    on_toggle: EventHandler<()>,
    on_delete: EventHandler<()>,
) -> Element {
    use chrono::Timelike;
    let translations = use_context::<Translations>();
    let local = event.fire_timestamp.with_timezone(&chrono::Local);
    let when = format!(
        "{} {}",
        crate::text::short_date(&translations, local.date_naive()),
        crate::text::clock(local.hour() as u8, local.minute() as u8, twenty_four),
    );
    let label = event.alarm_label.clone();
    let flagged = match event.outcome {
        AlarmOutcome::Solved | AlarmOutcome::Backup => "",
        AlarmOutcome::Missed => "danger",
        AlarmOutcome::Limit | AlarmOutcome::Reboot => "warn",
    };
    let outcome_key = match event.outcome {
        AlarmOutcome::Solved => None,
        AlarmOutcome::Missed => Some("history.outcome.missed"),
        AlarmOutcome::Limit => Some("history.outcome.limit"),
        AlarmOutcome::Reboot => Some("history.outcome.reboot"),
        AlarmOutcome::Backup => Some("history.outcome.backup"),
    };
    let measured = matches!(event.outcome, AlarmOutcome::Solved);

    let sub = match outcome_key {
        Some(key) => t(&translations, key),
        None if event.snooze_count == 1 => t(&translations, "history.snooze_pill_one"),
        None if event.snooze_count > 1 => t(&translations, "history.snooze_pill_other")
            .replace("{count}", &event.snooze_count.to_string()),
        None => event
            .challenge_solves
            .iter()
            .map(|solve| challenge_name(&translations, solve.challenge_type))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let value = if measured {
        crate::text::duration(&translations, event.total_dismissal_time)
    } else {
        String::new()
    };

    let mut details: Vec<(String, String)> = Vec::new();
    if measured {
        if let Some(reaction) = event.reaction_time() {
            details.push((
                t(&translations, "history.reaction"),
                crate::text::duration(&translations, reaction),
            ));
        }
        for (i, minutes) in event.snooze_intervals_minutes.iter().enumerate() {
            details.push((
                t(&translations, "history.snooze_n").replace("{n}", &(i + 1).to_string()),
                t(&translations, "text.minutes").replace("{minutes}", &minutes.to_string()),
            ));
        }
        for solve in &event.challenge_solves {
            details.push((
                solve_name(&translations, solve.challenge_type, solve.difficulty),
                crate::text::duration(&translations, solve.duration),
            ));
        }
    }
    let delete_text = t(&translations, "history.delete_one");

    rsx! {
        li {
            button {
                class: "event-main",
                "aria-expanded": if open { "true" } else { "false" },
                onclick: move |_| on_toggle.call(()),
                span { class: "event-text",
                    span { class: "event-title", "{when} {label}" }
                    if !sub.is_empty() {
                        span { class: "event-sub {flagged}", "{sub}" }
                    }
                }
                span { class: "event-value", "{value}" }
                // Says the row opens: nobody guessed it.
                span { class: if open { "event-chevron open" } else { "event-chevron" },
                    Icon { name: IconName::ChevronRight, size: 16 }
                }
            }
            if open {
                div { class: "event-details",
                    if !details.is_empty() {
                        dl { class: "stat-list",
                            for (name, value) in details {
                                dt { "{name}" }
                                dd { "{value}" }
                            }
                        }
                    }
                    button {
                        class: "link-btn danger",
                        onclick: move |_| on_delete.call(()),
                        "{delete_text}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Local, TimeZone, Utc};

    fn event_on(day: NaiveDate, outcome: AlarmOutcome) -> AlarmEvent {
        let at = Local
            .from_local_datetime(&day.and_hms_opt(7, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&Utc);
        AlarmEvent {
            id: uuid::Uuid::new_v4().to_string(),
            alarm_id: crate::models::AlarmId("a".into()),
            alarm_label: String::new(),
            fire_timestamp: at,
            first_interaction_timestamp: None,
            dismiss_timestamp: at,
            snooze_count: 0,
            snooze_intervals_minutes: Vec::new(),
            challenge_solves: Vec::new(),
            total_dismissal_time: std::time::Duration::from_secs(60),
            outcome,
        }
    }

    #[test]
    fn a_streak_counts_days_in_a_row_and_a_miss_ends_it() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        let day = |back: i64| today - Duration::days(back);
        let events = vec![
            event_on(day(0), AlarmOutcome::Solved),
            event_on(day(1), AlarmOutcome::Solved),
            event_on(day(1), AlarmOutcome::Solved),
            event_on(day(3), AlarmOutcome::Backup),
            event_on(day(4), AlarmOutcome::Missed),
            event_on(day(5), AlarmOutcome::Solved),
        ];
        assert_eq!(
            streak(&events, today),
            3,
            "a day without alarms is no break"
        );
        assert_eq!(streak(&[], today), 0);
        assert_eq!(
            streak(
                &[
                    event_on(day(0), AlarmOutcome::Solved),
                    event_on(day(0), AlarmOutcome::Limit)
                ],
                today
            ),
            0,
            "one alarm left ringing spoils the day"
        );
    }
}
