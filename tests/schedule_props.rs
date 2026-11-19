//! The schedule math under random input (proptest): which moment an alarm rings next, the countdown
//! to it, and what the wall-clock moment is as an instant across daylight-saving changes, time
//! zones and leap days. The fixed cases are in `src/schedule.rs`; these look for the ones nobody
//! thought of.

use alarmfree::schedule::{
    countdown, earliest_future, format_countdown, instant_in, next_occurrence_date,
    next_ring_moment,
};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use chrono_tz::Tz;
use proptest::prelude::*;

/// Any minute between 2023 and 2031: it holds two leap days (2024-02-29, 2028-02-29) and every
/// month end.
fn any_now() -> impl Strategy<Value = NaiveDateTime> {
    let start = NaiveDate::from_ymd_opt(2023, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    (0i64..(8 * 366 * 24 * 60)).prop_map(move |m| start + Duration::minutes(m))
}

fn any_days() -> impl Strategy<Value = [bool; 7]> {
    proptest::array::uniform7(any::<bool>())
}

/// Zones with the changes that matter: forward and back, a half-hour change (Lord Howe), a change
/// at midnight (Sao Paulo used to), none at all (Kolkata), and a date line skip (Apia in 2011 is
/// before the tz data's recent rules, so Auckland stands for the southern summer time).
fn any_zone() -> impl Strategy<Value = Tz> {
    prop::sample::select(vec![
        chrono_tz::Europe::Amsterdam,
        chrono_tz::America::New_York,
        chrono_tz::Australia::Lord_Howe,
        chrono_tz::Asia::Kolkata,
        chrono_tz::Pacific::Auckland,
        chrono_tz::America::Sao_Paulo,
        chrono_tz::Africa::Casablanca,
    ])
}

fn selected(days: [bool; 7], day: NaiveDate) -> bool {
    days[day.weekday().num_days_from_monday() as usize]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// A repeating alarm always rings again: after now, within a week, on a chosen weekday, at its
    /// time, and with no earlier chosen moment in between.
    #[test]
    fn a_repeating_alarm_rings_next_on_the_first_chosen_moment_after_now(
        now in any_now(), hour in 0u8..24, minute in 0u8..60, days in any_days(),
    ) {
        prop_assume!(days.iter().any(|&d| d));
        let at = next_ring_moment(hour, minute, days, None, now).expect("a chosen day always has a next ring");
        prop_assert!(at > now);
        prop_assert!(at - now <= Duration::days(7));
        prop_assert!(selected(days, at.date()));
        prop_assert_eq!((at.hour(), at.minute(), at.second()), (u32::from(hour), u32::from(minute), 0));
        for offset in 0..=7 {
            let day = now.date() + Duration::days(offset);
            if let Some(earlier) = day.and_hms_opt(u32::from(hour), u32::from(minute), 0) {
                if selected(days, day) && earlier > now {
                    prop_assert!(earlier >= at, "{earlier} is chosen and sooner than {at}");
                }
            }
        }
    }

    /// A one-time alarm rings on its date while that is ahead, and never otherwise.
    #[test]
    fn a_one_time_alarm_rings_on_its_date_only_while_that_is_ahead(
        now in any_now(), hour in 0u8..24, minute in 0u8..60, ahead in -400i64..400,
    ) {
        let date = now.date() + Duration::days(ahead);
        let moment = date.and_hms_opt(u32::from(hour), u32::from(minute), 0).unwrap();
        let got = next_ring_moment(hour, minute, [false; 7], Some(date), now);
        prop_assert_eq!(got, (moment > now).then_some(moment));
        prop_assert_eq!(next_ring_moment(hour, minute, [false; 7], None, now), None);
    }

    /// The date a clock time next occurs on: today only while it is still ahead, else tomorrow.
    #[test]
    fn the_next_occurrence_is_today_or_tomorrow_and_after_now(
        now in any_now(), hour in 0u8..24, minute in 0u8..60,
    ) {
        let date = next_occurrence_date(hour, minute, now);
        let at = date.and_hms_opt(u32::from(hour), u32::from(minute), 0).unwrap();
        prop_assert!(at > now);
        prop_assert!(at - now <= Duration::days(1));
        if date != now.date() {
            let today = now.date().and_hms_opt(u32::from(hour), u32::from(minute), 0).unwrap();
            prop_assert!(today <= now, "tomorrow was chosen while today is still ahead");
        }
    }

    /// The countdown rounds up to a whole minute and is made of days, hours and minutes that add up.
    #[test]
    fn the_countdown_adds_up_and_rounds_up(now_ms in 0i64..4_000_000_000_000, left_ms in -5_000_000i64..4_000_000_000) {
        let c = countdown(now_ms, now_ms + left_ms);
        prop_assert!(c.hours < 24 && c.minutes < 60);
        let total = u64::from(c.days) * 1440 + u64::from(c.hours) * 60 + u64::from(c.minutes);
        let want = if left_ms <= 0 { 0 } else { (left_ms as u64).div_ceil(60_000) };
        prop_assert_eq!(total, want);
        // A little later never says more.
        let later = countdown(now_ms + 1, now_ms + left_ms);
        let later_total = u64::from(later.days) * 1440 + u64::from(later.hours) * 60 + u64::from(later.minutes);
        prop_assert!(later_total <= total);
        // And the text has no template left in it.
        let text = format_countdown(c, "{days}d", "{days}d {hours}h", "{hours}h {minutes}m", "{minutes}m");
        let open = text.contains('{');
        prop_assert!(!open && !text.is_empty());
    }

    /// The soonest of several times is one of them, ahead of now, and nothing ahead is sooner.
    #[test]
    fn the_earliest_future_time_is_the_soonest_one_ahead(
        now in -1000i64..1000, times in proptest::collection::vec(-2000i64..2000, 0..12),
    ) {
        let got = earliest_future(now, times.clone());
        prop_assert_eq!(got, times.iter().copied().filter(|t| *t > now).min());
    }

    /// In any zone, the next ring of a daily alarm is an instant after now, at the alarm's wall
    /// time, at most a day (plus the hour a change gives or takes) away, except when its time does
    /// not exist that day.
    #[test]
    fn a_daily_alarm_keeps_its_wall_time_across_clock_changes(
        zone in any_zone(), now in any_now(), hour in 0u8..24, minute in 0u8..60,
    ) {
        let Some(now_instant) = instant_in(&zone, now) else { return Ok(()) };
        let at = next_ring_moment(hour, minute, [true; 7], None, now).unwrap();
        match instant_in(&zone, at) {
            Some(instant) => {
                prop_assert!(instant > now_instant, "{instant} is not after {now_instant}");
                prop_assert_eq!((instant.hour(), instant.minute()), (u32::from(hour), u32::from(minute)));
                prop_assert!(instant.timestamp() - now_instant.timestamp() <= 25 * 3600 + 1800);
            }
            None => {
                // Only a time that the clocks skipped that day.
                prop_assert!(
                    zone.from_local_datetime(&at).single().is_none()
                        && zone.from_local_datetime(&(at - Duration::hours(3))).earliest().is_some(),
                    "{at} has no instant in {zone} and is not in a gap"
                );
            }
        }
    }
}

#[test]
fn a_time_that_happens_twice_rings_the_first_time() {
    // 2026-10-25: at 03:00 summer time the Netherlands go back to 02:00.
    let nl = chrono_tz::Europe::Amsterdam;
    let moment = NaiveDate::from_ymd_opt(2026, 10, 25)
        .unwrap()
        .and_hms_opt(2, 30, 0)
        .unwrap();
    let first = instant_in(&nl, moment).expect("an ambiguous time has an instant");
    assert_eq!(first.offset().to_string(), "CEST");
    let both = nl.from_local_datetime(&moment);
    let last = both.latest().unwrap();
    assert_eq!(last.timestamp() - first.timestamp(), 3600);
}

#[test]
fn a_time_the_clocks_skip_has_no_instant() {
    // 2026-03-29: at 02:00 the Netherlands go forward to 03:00.
    let nl = chrono_tz::Europe::Amsterdam;
    let moment = NaiveDate::from_ymd_opt(2026, 3, 29)
        .unwrap()
        .and_hms_opt(2, 30, 0)
        .unwrap();
    assert!(instant_in(&nl, moment).is_none());
    let after = NaiveDate::from_ymd_opt(2026, 3, 29)
        .unwrap()
        .and_hms_opt(3, 0, 0)
        .unwrap();
    assert!(instant_in(&nl, after).is_some());
}

#[test]
fn a_daily_alarm_follows_the_wall_across_a_clock_change_not_the_24_hours() {
    let nl = chrono_tz::Europe::Amsterdam;
    let ring = |day: u32, month: u32| {
        let now = NaiveDate::from_ymd_opt(2026, month, day)
            .unwrap()
            .and_hms_opt(8, 0, 0)
            .unwrap();
        let at = next_ring_moment(7, 0, [true; 7], None, now).unwrap();
        (instant_in(&nl, now).unwrap(), instant_in(&nl, at).unwrap())
    };
    // From 08:00 the day before to 07:00 on the day of the change: 23 hours on the wall, 22 when
    // the clocks go forward and 24 when they go back.
    let (now, at) = ring(28, 3);
    assert_eq!(at.timestamp() - now.timestamp(), 22 * 3600);
    let (now, at) = ring(24, 10);
    assert_eq!(at.timestamp() - now.timestamp(), 24 * 3600);
}

#[test]
fn a_leap_day_is_a_day_like_any_other() {
    let now = NaiveDate::from_ymd_opt(2028, 2, 28)
        .unwrap()
        .and_hms_opt(23, 0, 0)
        .unwrap();
    let at = next_ring_moment(6, 30, [true; 7], None, now).unwrap();
    assert_eq!(at.date(), NaiveDate::from_ymd_opt(2028, 2, 29).unwrap());
    let now = NaiveDate::from_ymd_opt(2028, 2, 29)
        .unwrap()
        .and_hms_opt(23, 0, 0)
        .unwrap();
    let at = next_ring_moment(6, 30, [true; 7], None, now).unwrap();
    assert_eq!(at.date(), NaiveDate::from_ymd_opt(2028, 3, 1).unwrap());
    // Not a leap year: February ends on the 28th.
    let now = NaiveDate::from_ymd_opt(2027, 2, 28)
        .unwrap()
        .and_hms_opt(23, 0, 0)
        .unwrap();
    assert_eq!(
        next_occurrence_date(6, 30, now),
        NaiveDate::from_ymd_opt(2027, 3, 1).unwrap()
    );
}
