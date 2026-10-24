//! One-time ("specific date") alarms: which date they ring on, and when they
//! are finished.
//!
//! An alarm with no weekday selected rings once. Its date is stored in
//! `AlarmData::date` (`YYYY-MM-DD`). The pure functions here take the current
//! time as an argument so they are testable.

use chrono::{Duration, NaiveDate, NaiveDateTime};

use crate::app::AlarmData;

const DATE_FORMAT: &str = "%Y-%m-%d";

/// An alarm rings once when no weekday is selected.
pub fn is_one_time(alarm: &AlarmData) -> bool {
    !alarm.days.iter().any(|&d| d)
}

/// The date a clock time next occurs on: today while it is still ahead,
/// otherwise tomorrow.
pub fn next_occurrence_date(hour: u8, minute: u8, now: NaiveDateTime) -> NaiveDate {
    let today = now.date();
    match today.and_hms_opt(u32::from(hour), u32::from(minute), 0) {
        Some(at) if at > now => today,
        _ => today + Duration::days(1),
    }
}

/// The moment a one-time alarm is set for, from its stored date.
pub fn one_time_moment(alarm: &AlarmData) -> Option<NaiveDateTime> {
    let date = NaiveDate::parse_from_str(alarm.date.as_deref()?, DATE_FORMAT).ok()?;
    date.and_hms_opt(u32::from(alarm.time_hour), u32::from(alarm.time_minute), 0)
}

/// The date a one-time alarm should ring on: its stored date while that
/// moment is still ahead, otherwise the next occurrence of its clock time.
/// `None` for an alarm that repeats on weekdays.
pub fn resolve_one_time_date(alarm: &AlarmData, now: NaiveDateTime) -> Option<String> {
    if !is_one_time(alarm) {
        return None;
    }
    let date = match one_time_moment(alarm) {
        Some(at) if at > now => at.date(),
        _ => next_occurrence_date(alarm.time_hour, alarm.time_minute, now),
    };
    Some(date.format(DATE_FORMAT).to_string())
}

/// Switch off a one-time alarm after it was dismissed. Returns whether
/// something changed.
pub fn disable_if_one_time(alarms: &mut [AlarmData], id: &str) -> bool {
    match alarms.iter_mut().find(|a| a.id == id) {
        Some(alarm) if is_one_time(alarm) && alarm.enabled => {
            alarm.enabled = false;
            true
        }
        _ => false,
    }
}

/// Switch off enabled one-time alarms whose moment has passed and that are no
/// longer in play (`in_play`: ringing or snoozed). That covers alarms that
/// already rang and ones whose time passed while the phone was off. Returns
/// the ids that were switched off.
pub fn sweep_finished_one_time(
    alarms: &mut [AlarmData],
    now: NaiveDateTime,
    in_play: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut switched_off = Vec::new();
    for alarm in alarms.iter_mut() {
        let finished = alarm.enabled
            && is_one_time(alarm)
            && one_time_moment(alarm).is_some_and(|at| at <= now)
            && !in_play(&alarm.id);
        if finished {
            alarm.enabled = false;
            switched_off.push(alarm.id.clone());
        }
    }
    switched_off
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(date: &str, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::parse_from_str(date, DATE_FORMAT)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    fn alarm(days: [bool; 7], date: Option<&str>, hour: u8, minute: u8) -> AlarmData {
        AlarmData {
            id: "a".into(),
            time_hour: hour,
            time_minute: minute,
            label: String::new(),
            days,
            date: date.map(str::to_owned),
            enabled: true,
            sound: "air".into(),
            sound_mode: 0,
            challenges: vec![],
            snooze_count: 3,
            snooze_interval: 5,
        }
    }

    const NO_DAYS: [bool; 7] = [false; 7];

    #[test]
    fn next_occurrence_is_today_while_the_time_is_ahead_and_tomorrow_after() {
        let now = at("2026-10-24", 15, 0);
        assert_eq!(next_occurrence_date(15, 1, now).to_string(), "2026-10-24");
        assert_eq!(
            next_occurrence_date(15, 0, now).to_string(),
            "2026-10-25",
            "the current minute has already begun"
        );
        assert_eq!(next_occurrence_date(7, 0, now).to_string(), "2026-10-25");
    }

    #[test]
    fn next_occurrence_rolls_over_a_month_end() {
        assert_eq!(
            next_occurrence_date(7, 0, at("2026-10-31", 23, 0)).to_string(),
            "2026-11-01"
        );
    }

    #[test]
    fn a_future_stored_date_is_kept() {
        let a = alarm(NO_DAYS, Some("2026-12-24"), 7, 0);
        assert_eq!(
            resolve_one_time_date(&a, at("2026-10-24", 15, 0)).as_deref(),
            Some("2026-12-24")
        );
    }

    #[test]
    fn a_past_or_missing_date_resolves_to_the_next_occurrence() {
        let now = at("2026-10-24", 15, 0);
        let past = alarm(NO_DAYS, Some("2026-10-01"), 7, 0);
        assert_eq!(
            resolve_one_time_date(&past, now).as_deref(),
            Some("2026-10-25")
        );
        let missing = alarm(NO_DAYS, None, 16, 0);
        assert_eq!(
            resolve_one_time_date(&missing, now).as_deref(),
            Some("2026-10-24")
        );
        let garbled = alarm(NO_DAYS, Some("not a date"), 16, 0);
        assert_eq!(
            resolve_one_time_date(&garbled, now).as_deref(),
            Some("2026-10-24")
        );
    }

    #[test]
    fn a_weekday_alarm_has_no_one_time_date() {
        let mut days = NO_DAYS;
        days[5] = true;
        let a = alarm(days, Some("2026-12-24"), 7, 0);
        assert_eq!(resolve_one_time_date(&a, at("2026-10-24", 15, 0)), None);
    }

    #[test]
    fn dismissing_a_one_time_alarm_switches_it_off_but_not_a_repeating_one() {
        let mut days = NO_DAYS;
        days[0] = true;
        let mut list = vec![alarm(NO_DAYS, Some("2026-10-24"), 7, 0)];
        assert!(disable_if_one_time(&mut list, "a"));
        assert!(!list[0].enabled);
        assert!(!disable_if_one_time(&mut list, "a"), "already off");

        let mut repeating = vec![alarm(days, None, 7, 0)];
        assert!(!disable_if_one_time(&mut repeating, "a"));
        assert!(repeating[0].enabled);
        assert!(!disable_if_one_time(&mut repeating, "missing"));
    }

    #[test]
    fn the_sweep_switches_off_finished_one_time_alarms_only() {
        let now = at("2026-10-24", 15, 0);
        let mut days = NO_DAYS;
        days[2] = true;
        let mut done = alarm(NO_DAYS, Some("2026-10-24"), 14, 0);
        done.id = "done".into();
        let mut upcoming = alarm(NO_DAYS, Some("2026-10-24"), 16, 0);
        upcoming.id = "upcoming".into();
        let mut ringing = alarm(NO_DAYS, Some("2026-10-24"), 14, 59);
        ringing.id = "ringing".into();
        let mut repeating = alarm(days, None, 14, 0);
        repeating.id = "repeating".into();
        let mut legacy = alarm(NO_DAYS, None, 14, 0);
        legacy.id = "legacy".into();
        let mut list = vec![done, upcoming, ringing, repeating, legacy];

        let off = sweep_finished_one_time(&mut list, now, |id| id == "ringing");

        assert_eq!(off, vec!["done".to_string()]);
        let enabled: Vec<_> = list
            .iter()
            .filter(|a| a.enabled)
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(enabled, ["upcoming", "ringing", "repeating", "legacy"]);
    }
}
