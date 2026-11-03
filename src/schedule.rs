//! One-time ("specific date") alarms: which date they ring on.
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

/// [`next_occurrence_date`], the way a one-time alarm stores it.
pub fn next_occurrence(hour: u8, minute: u8, now: NaiveDateTime) -> String {
    next_occurrence_date(hour, minute, now)
        .format(DATE_FORMAT)
        .to_string()
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
    Some(match one_time_moment(alarm) {
        Some(at) if at > now => at.date().format(DATE_FORMAT).to_string(),
        _ => next_occurrence(alarm.time_hour, alarm.time_minute, now),
    })
}

/// A one-time alarm is over once dealt with: it switches off, or goes altogether if
/// it was made to delete itself. Returns whether something changed.
pub fn finish_if_one_time(alarms: &mut Vec<AlarmData>, id: &str) -> bool {
    let Some(index) = alarms.iter().position(|a| a.id == id) else {
        return false;
    };
    if alarms[index].delete_after_ring {
        alarms.remove(index);
        return true;
    }
    let alarm = &mut alarms[index];
    if is_one_time(alarm) && alarm.enabled {
        alarm.enabled = false;
        return true;
    }
    false
}

/// Give a one-time alarm that is being switched on a date still ahead. Its old
/// date may have passed long ago, and an alarm's date is what it rings on.
pub fn refresh_one_time_date(alarm: &mut AlarmData, now: NaiveDateTime) {
    if let Some(date) = resolve_one_time_date(alarm, now) {
        alarm.date = Some(date);
    }
}

/// One-time alarms saved before they had a date get one. Returns whether
/// anything changed.
pub fn date_undated_one_time(alarms: &mut [AlarmData], now: NaiveDateTime) -> bool {
    let mut changed = false;
    for alarm in alarms.iter_mut() {
        if alarm.enabled && is_one_time(alarm) && alarm.date.is_none() {
            refresh_one_time_date(alarm, now);
            changed = true;
        }
    }
    changed
}

/// When an alarm with these settings next rings, as seen from `now`: on the
/// chosen date for a one-time alarm, otherwise on the first selected weekday
/// (today included while the time is still ahead). `days` is Monday first.
/// `None` for a one-time alarm whose date is past, or no day selected and no
/// date given.
pub fn next_ring_moment(
    hour: u8,
    minute: u8,
    days: [bool; 7],
    date: Option<NaiveDate>,
    now: NaiveDateTime,
) -> Option<NaiveDateTime> {
    use chrono::Datelike;
    let at = |day: NaiveDate| day.and_hms_opt(u32::from(hour), u32::from(minute), 0);
    if days.iter().any(|&d| d) {
        return (0..=7)
            .map(|offset| now.date() + Duration::days(offset))
            .filter(|day| days[day.weekday().num_days_from_monday() as usize])
            .filter_map(at)
            .find(|moment| *moment > now);
    }
    at(date?).filter(|moment| *moment > now)
}

/// Time left until an alarm rings, in whole days, hours and minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Countdown {
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
}

/// The countdown from `now_ms` to `at_ms` (unix ms), rounded UP to a whole
/// minute: 59 s left reads "1m", never "0m" for an alarm that has not rung.
/// Zero once `at_ms` is reached.
pub fn countdown(now_ms: i64, at_ms: i64) -> Countdown {
    let remaining_ms = (at_ms - now_ms).max(0);
    let total_minutes = ((remaining_ms + 59_999) / 60_000) as u32;
    Countdown {
        days: total_minutes / (24 * 60),
        hours: total_minutes % (24 * 60) / 60,
        minutes: total_minutes % 60,
    }
}

/// The soonest of `times` that is still ahead of `now_ms`.
pub fn earliest_future(now_ms: i64, times: impl IntoIterator<Item = i64>) -> Option<i64> {
    times.into_iter().filter(|&t| t > now_ms).min()
}

/// Fill one of the countdown templates (`{days}`, `{hours}`, `{minutes}`):
/// `dhm` once a day or more is left, `hm` once an hour or more is, else `m`.
pub fn format_countdown(c: Countdown, dhm: &str, hm: &str, m: &str) -> String {
    let template = if c.days > 0 {
        dhm
    } else if c.hours > 0 {
        hm
    } else {
        m
    };
    let minutes = if c.days > 0 || c.hours > 0 {
        format!("{:02}", c.minutes)
    } else {
        c.minutes.to_string()
    };
    template
        .replace("{days}", &c.days.to_string())
        .replace("{hours}", &c.hours.to_string())
        .replace("{minutes}", &minutes)
}

/// How to name the day an alarm next rings on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayLabel {
    Today,
    Tomorrow,
    Weekday(chrono::Weekday),
}

pub fn day_label(today: NaiveDate, ring_day: NaiveDate) -> DayLabel {
    use chrono::Datelike;
    match (ring_day - today).num_days() {
        0 => DayLabel::Today,
        1 => DayLabel::Tomorrow,
        _ => DayLabel::Weekday(ring_day.weekday()),
    }
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
            skip_until_ms: None,
            fade_in_secs: 0,
            ring_limit_secs: None,
            order: 0,
            enabled: true,
            sound: "air".into(),
            sound_mode: 0,
            challenges: vec![],
            snooze_count: 3,
            snooze_interval: 5,
            note: String::new(),
            vibration: 0,
            volume_percent: None,
            random_order: false,
            wake_check_minutes: 0,
            max_brightness: false,
            sound_delay_secs: 0,
            pause_until_ms: None,
            delete_after_ring: false,
            hidden: false,
        }
    }

    const NO_DAYS: [bool; 7] = [false; 7];

    const DHM: &str = "{days}d {hours}h {minutes}m";
    const HM: &str = "{hours}h {minutes}m";
    const M: &str = "{minutes}m";

    fn text(now_ms: i64, at_ms: i64) -> String {
        format_countdown(countdown(now_ms, at_ms), DHM, HM, M)
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, DATE_FORMAT).unwrap()
    }

    #[test]
    fn the_next_ring_picks_the_first_selected_weekday_still_ahead() {
        // 2026-10-24 is a Saturday; days are Monday first.
        let mut days = NO_DAYS;
        days[0] = true; // Monday
        days[5] = true; // Saturday
        let now = at("2026-10-24", 15, 0);
        assert_eq!(
            next_ring_moment(16, 0, days, None, now),
            Some(at("2026-10-24", 16, 0)),
            "Saturday 16:00 is still ahead"
        );
        assert_eq!(
            next_ring_moment(14, 0, days, None, now),
            Some(at("2026-10-26", 14, 0)),
            "Saturday 14:00 has passed: Monday"
        );
        let saturday_only = {
            let mut d = NO_DAYS;
            d[5] = true;
            d
        };
        assert_eq!(
            next_ring_moment(14, 0, saturday_only, None, now),
            Some(at("2026-10-31", 14, 0)),
            "a week away"
        );
    }

    #[test]
    fn the_next_ring_of_a_one_time_alarm_is_its_date_when_that_is_ahead() {
        let now = at("2026-10-24", 15, 0);
        assert_eq!(
            next_ring_moment(7, 0, NO_DAYS, Some(day("2026-12-24")), now),
            Some(at("2026-12-24", 7, 0))
        );
        assert_eq!(
            next_ring_moment(7, 0, NO_DAYS, Some(day("2026-10-24")), now),
            None
        );
        assert_eq!(next_ring_moment(7, 0, NO_DAYS, None, now), None);
    }

    #[test]
    fn countdown_rounds_up_to_the_next_minute() {
        assert_eq!(text(0, 1), "1m");
        assert_eq!(text(0, 59_000), "1m");
        assert_eq!(text(0, 60_000), "1m");
        assert_eq!(text(0, 60_001), "2m");
        assert_eq!(text(0, 0), "0m");
        assert_eq!(text(10_000, 0), "0m", "an alarm in the past reads zero");
    }

    #[test]
    fn countdown_switches_to_hours_and_to_days() {
        let min = 60_000;
        assert_eq!(text(0, 59 * min), "59m");
        assert_eq!(text(0, 60 * min), "1h 00m");
        assert_eq!(text(0, (23 * 60 + 59) * min), "23h 59m");
        assert_eq!(text(0, 24 * 60 * min), "1d 0h 00m");
        assert_eq!(text(0, (6 * 24 * 60 + 23 * 60 + 45) * min), "6d 23h 45m");
    }

    #[test]
    fn the_earliest_future_time_wins_and_the_past_is_ignored() {
        assert_eq!(earliest_future(100, [500, 300, 50, 900]), Some(300));
        assert_eq!(earliest_future(100, [50, 100]), None);
        assert_eq!(earliest_future(100, []), None);
    }

    #[test]
    fn days_are_named_today_tomorrow_then_by_weekday() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap(); // Saturday
        let day = |n| today + Duration::days(n);
        assert_eq!(day_label(today, day(0)), DayLabel::Today);
        assert_eq!(day_label(today, day(1)), DayLabel::Tomorrow);
        assert_eq!(
            day_label(today, day(2)),
            DayLabel::Weekday(chrono::Weekday::Mon)
        );
        assert_eq!(
            day_label(today, day(7)),
            DayLabel::Weekday(chrono::Weekday::Sat)
        );
    }

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
        assert!(finish_if_one_time(&mut list, "a"));
        assert!(!list[0].enabled);
        assert!(!finish_if_one_time(&mut list, "a"), "already off");

        let mut repeating = vec![alarm(days, None, 7, 0)];
        assert!(!finish_if_one_time(&mut repeating, "a"));
        assert!(repeating[0].enabled);
        assert!(!finish_if_one_time(&mut repeating, "missing"));
    }

    #[test]
    fn an_alarm_made_to_delete_itself_goes_when_it_is_over() {
        let mut quick = alarm(NO_DAYS, Some("2026-10-24"), 7, 0);
        quick.delete_after_ring = true;
        let mut list = vec![quick];
        assert!(finish_if_one_time(&mut list, "a"));
        assert!(list.is_empty());
    }

    #[test]
    fn switching_an_expired_one_time_alarm_back_on_moves_it_ahead() {
        let now = at("2026-10-24", 15, 0);
        let mut expired = alarm(NO_DAYS, Some("2026-10-01"), 7, 0);
        refresh_one_time_date(&mut expired, now);
        assert_eq!(expired.date.as_deref(), Some("2026-10-25"));

        let mut ahead = alarm(NO_DAYS, Some("2026-12-24"), 7, 0);
        refresh_one_time_date(&mut ahead, now);
        assert_eq!(ahead.date.as_deref(), Some("2026-12-24"), "kept");

        let mut days = NO_DAYS;
        days[0] = true;
        let mut repeating = alarm(days, None, 7, 0);
        refresh_one_time_date(&mut repeating, now);
        assert_eq!(repeating.date, None, "repeating alarms have no date");
    }

    #[test]
    fn undated_one_time_alarms_get_a_date_once() {
        let now = at("2026-10-24", 15, 0);
        let mut list = vec![alarm(NO_DAYS, None, 16, 0)];
        assert!(date_undated_one_time(&mut list, now));
        assert_eq!(list[0].date.as_deref(), Some("2026-10-24"));
        assert!(!date_undated_one_time(&mut list, now), "nothing left to do");

        let mut off = vec![alarm(NO_DAYS, None, 16, 0)];
        off[0].enabled = false;
        assert!(!date_undated_one_time(&mut off, now));
    }
}
