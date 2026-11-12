//! What the history screen says: the periods, the streak, the chart and the words of each
//! alarm in the list. No UI in here.

use chrono::{Datelike, NaiveDate, Timelike};

use crate::i18n::{t, Translations};
use crate::models::setup::difficulty_name;
use crate::models::{
    AggregateMetrics, AlarmEvent, AlarmOutcome, ChallengeType, Difficulty, HistoryFilter, WeekStart,
};

/// For "all of it": a period is a number of days.
pub const EVER_DAYS: u32 = 36_500;
/// How far back a streak is looked for.
pub const STREAK_LOOKBACK_DAYS: u32 = 400;
/// The tallest bar of the chart, in px.
pub const BAR_MAX_PX: u64 = 56;

/// The periods, as their translation keys.
pub const PERIODS: [&str; 7] = [
    "filter.this_week",
    "filter.last_week",
    "filter.this_month",
    "filter.last_month",
    "filter.this_year",
    "filter.last_year",
    "filter.custom_days",
];

/// The index of the period of the user's own days.
pub const CUSTOM_PERIOD: usize = 6;

pub fn period_filter(key: &str, custom_days: u16, week_start: WeekStart) -> HistoryFilter {
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
pub fn streak(events: &[AlarmEvent], today: NaiveDate) -> u32 {
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
pub struct Bar {
    pub label: String,
    /// Average seconds to stop the alarm, 0 when nothing rang.
    pub seconds: u64,
    pub current: bool,
}

/// The height of a bar in px, the longest being the tallest.
pub fn bar_height(seconds: u64, longest: u64) -> u64 {
    if seconds == 0 {
        0
    } else {
        (seconds * BAR_MAX_PX / longest.max(1)).max(3)
    }
}

/// Group events into bars for the period: by weekday for a week, by week for a month, by
/// month for a year. `None` for a period of the user's own, which has no natural groups.
pub fn chart(
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

pub fn challenge_name(translations: &Translations, kind: ChallengeType) -> String {
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
pub fn solve_name(
    translations: &Translations,
    kind: ChallengeType,
    difficulty: Difficulty,
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

/// The figures of a period, as name and value: the ones under "All stats".
pub fn stat_rows(translations: &Translations, metrics: &AggregateMetrics) -> Vec<(String, String)> {
    let duration = |d| crate::text::duration(translations, d);
    let mut rows = vec![
        (
            t(translations, "history.alarms"),
            metrics.total_events.to_string(),
        ),
        (
            t(translations, "history.avg_reaction"),
            duration(metrics.avg_reaction_time),
        ),
        (
            t(translations, "history.avg_total"),
            duration(metrics.avg_total_dismissal_time),
        ),
        (
            t(translations, "history.avg_time_snoozed"),
            duration(metrics.avg_total_snoozed),
        ),
        (
            t(translations, "history.avg_snooze_gap"),
            duration(std::time::Duration::from_secs_f64(
                metrics.avg_snooze_interval_minutes.max(0.0) * 60.0,
            )),
        ),
        (
            t(translations, "history.avg_challenge"),
            duration(metrics.avg_total_challenge_time),
        ),
    ];
    rows.extend(
        metrics
            .avg_solve_per_challenge
            .iter()
            .map(|((kind, difficulty), avg)| {
                (solve_name(translations, *kind, *difficulty), duration(*avg))
            }),
    );
    rows
}

/// How one alarm of the list reads, closed and opened.
#[derive(Debug, Clone, PartialEq)]
pub struct EventText {
    /// "12 Nov 07:00 Label"
    pub title: String,
    pub sub: String,
    /// 0 normal, 1 danger (missed), 2 warn (limit, reboot).
    pub flag: i32,
    pub value: String,
    pub details: Vec<(String, String)>,
}

pub fn event_text(translations: &Translations, event: &AlarmEvent, twenty_four: bool) -> EventText {
    let local = event.fire_timestamp.with_timezone(&chrono::Local);
    let when = format!(
        "{} {}",
        crate::text::short_date(translations, local.date_naive()),
        crate::text::clock(local.hour() as u8, local.minute() as u8, twenty_four),
    );
    let flag = match event.outcome {
        AlarmOutcome::Solved | AlarmOutcome::Backup => 0,
        AlarmOutcome::Missed => 1,
        AlarmOutcome::Limit | AlarmOutcome::Reboot => 2,
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
        Some(key) => t(translations, key),
        None if event.snooze_count == 1 => t(translations, "history.snooze_pill_one"),
        None if event.snooze_count > 1 => t(translations, "history.snooze_pill_other")
            .replace("{count}", &event.snooze_count.to_string()),
        None => event
            .challenge_solves
            .iter()
            .map(|solve| challenge_name(translations, solve.challenge_type))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let value = if measured {
        crate::text::duration(translations, event.total_dismissal_time)
    } else {
        String::new()
    };

    let mut details: Vec<(String, String)> = Vec::new();
    if measured {
        if let Some(reaction) = event.reaction_time() {
            details.push((
                t(translations, "history.reaction"),
                crate::text::duration(translations, reaction),
            ));
        }
        for (i, minutes) in event.snooze_intervals_minutes.iter().enumerate() {
            details.push((
                t(translations, "history.snooze_n").replace("{n}", &(i + 1).to_string()),
                t(translations, "text.minutes").replace("{minutes}", &minutes.to_string()),
            ));
        }
        for solve in &event.challenge_solves {
            details.push((
                solve_name(translations, solve.challenge_type, solve.difficulty),
                crate::text::duration(translations, solve.duration),
            ));
        }
    }
    EventText {
        title: format!("{when} {}", event.alarm_label),
        sub,
        flag,
        value,
        details,
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

    #[test]
    fn a_bar_is_as_tall_as_its_time_against_the_longest() {
        assert_eq!(bar_height(0, 100), 0);
        assert_eq!(bar_height(100, 100), BAR_MAX_PX);
        assert_eq!(bar_height(50, 100), BAR_MAX_PX / 2);
        assert_eq!(bar_height(1, 1000), 3, "never thinner than can be seen");
    }

    #[test]
    fn the_chart_groups_a_week_by_weekday_and_a_custom_period_has_none() {
        let en = crate::i18n::load_translations(crate::models::Locale::English);
        let today = NaiveDate::from_ymd_opt(2026, 11, 11).unwrap();
        let events = vec![event_on(today, AlarmOutcome::Solved)];
        let filter = HistoryFilter::ThisWeek(WeekStart::Monday);
        let bars = chart(&en, &filter, &events, WeekStart::Monday, today).unwrap();
        assert_eq!(bars.len(), 7);
        let wednesday = today.weekday().num_days_from_monday() as usize;
        assert_eq!(bars[wednesday].seconds, 60);
        assert!(bars[wednesday].current);
        assert_eq!(bars.iter().filter(|b| b.current).count(), 1);
        assert!(chart(
            &en,
            &HistoryFilter::LastDays(30),
            &events,
            WeekStart::Monday,
            today
        )
        .is_none());
        let year = chart(
            &en,
            &HistoryFilter::ThisYear,
            &events,
            WeekStart::Monday,
            today,
        )
        .unwrap();
        assert_eq!(year.len(), 12);
        assert_eq!(year[10].seconds, 60, "november");
    }

    #[test]
    fn a_missed_alarm_is_flagged_and_has_no_time() {
        let en = crate::i18n::load_translations(crate::models::Locale::English);
        let day = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        let missed = event_text(&en, &event_on(day, AlarmOutcome::Missed), true);
        assert_eq!(missed.flag, 1);
        assert_eq!(missed.value, "");
        assert!(missed.details.is_empty());
        let solved = event_text(&en, &event_on(day, AlarmOutcome::Solved), true);
        assert_eq!(solved.flag, 0);
        assert_eq!(solved.value, "1 min 00");
    }
}
