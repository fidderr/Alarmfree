//! Times, dates and durations as the user reads them: in their language and their
//! time format. Translated through the word lists, without a locale library for a
//! handful of words.

use chrono::{Datelike, NaiveDate, TimeZone};

use crate::app::{t, AlarmData, Translations};
use crate::models::{AppSettings, TimeFormat};
use crate::schedule;

pub const DAY_KEYS: [&str; 7] = [
    "day.mon", "day.tue", "day.wed", "day.thu", "day.fri", "day.sat", "day.sun",
];

pub const FULL_DAY_KEYS: [&str; 7] = [
    "day.monday",
    "day.tuesday",
    "day.wednesday",
    "day.thursday",
    "day.friday",
    "day.saturday",
    "day.sunday",
];

const MONTH_KEYS: [&str; 12] = [
    "month.jan",
    "month.feb",
    "month.mar",
    "month.apr",
    "month.may",
    "month.jun",
    "month.jul",
    "month.aug",
    "month.sep",
    "month.oct",
    "month.nov",
    "month.dec",
];

/// Whether times are written in 24 hours for this user.
pub fn uses_24_hours(settings: &AppSettings) -> bool {
    match settings.prefs.time_format {
        TimeFormat::H24 => true,
        TimeFormat::H12 => false,
        TimeFormat::System => mobile_sentinel::display::uses_24_hour_clock(),
    }
}

/// The hour as a 12-hour clock has it, and which half of the day.
fn twelve_hour(hour: u8) -> (u8, &'static str) {
    let period = if hour < 12 { "AM" } else { "PM" };
    let shown = match hour % 12 {
        0 => 12,
        other => other,
    };
    (shown, period)
}

/// "07:30" or "7:30 AM".
pub fn clock(hour: u8, minute: u8, twenty_four: bool) -> String {
    if twenty_four {
        format!("{hour:02}:{minute:02}")
    } else {
        let (shown, period) = twelve_hour(hour);
        format!("{shown}:{minute:02} {period}")
    }
}

/// An hour on its own, on the time wheel: "07" or "7 AM".
pub fn hour_label(hour: u8, twenty_four: bool) -> String {
    if twenty_four {
        format!("{hour:02}")
    } else {
        let (shown, period) = twelve_hour(hour);
        format!("{shown} {period}")
    }
}

/// "Rings in 7 h 12 min", for an alarm that is on, or until when it is paused. Empty if
/// nothing is ahead.
pub fn rings_in(translations: &Translations, alarm: &AlarmData) -> String {
    let now = chrono::Local::now();
    let now_ms = now.timestamp_millis();
    if let Some(until) = alarm.pause_until_ms.filter(|until| *until > now_ms) {
        return chrono::Local
            .timestamp_millis_opt(until)
            .single()
            .map(|at| {
                t(translations, "home.paused_until")
                    .replace("{date}", &day_month(translations, at.date_naive()))
            })
            .unwrap_or_default();
    }
    let date = alarm
        .date
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    // A ring that is skipped is not what comes next.
    let from = alarm
        .skip_until_ms
        .filter(|skip| *skip > now_ms)
        .and_then(|skip| chrono::Local.timestamp_millis_opt(skip).single())
        .map_or(now.naive_local(), |skip| skip.naive_local());
    schedule::next_ring_moment(alarm.time_hour, alarm.time_minute, alarm.days, date, from)
        .and_then(|moment| chrono::Local.from_local_datetime(&moment).earliest())
        .map(|at| {
            let time = time_left(
                translations,
                schedule::countdown(now_ms, at.timestamp_millis()),
            );
            t(translations, "config.rings_in").replace("{time}", &time)
        })
        .unwrap_or_default()
}

/// "3 d 12 h", "7 h 05 min" or "12 min".
pub fn time_left(translations: &Translations, left: schedule::Countdown) -> String {
    schedule::format_countdown(
        left,
        &t(translations, "home.next_in_d"),
        &t(translations, "home.next_in_dh"),
        &t(translations, "home.next_in_hm"),
        &t(translations, "home.next_in_m"),
    )
}

/// The first letter of a day's short name, for the rows of day letters.
pub fn day_letter(translations: &Translations, day: usize) -> String {
    t(translations, DAY_KEYS[day])
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

pub fn day_name(translations: &Translations, date: NaiveDate) -> String {
    t(
        translations,
        DAY_KEYS[date.weekday().num_days_from_monday() as usize],
    )
}

/// "6 Nov".
pub fn day_month(translations: &Translations, date: NaiveDate) -> String {
    format!(
        "{} {}",
        date.day(),
        t(translations, MONTH_KEYS[date.month0() as usize])
    )
}

/// "Fri 6 Nov".
pub fn short_date(translations: &Translations, date: NaiveDate) -> String {
    format!(
        "{} {}",
        day_name(translations, date),
        day_month(translations, date)
    )
}

/// A short month name, for the chart of a year.
pub fn month_short(translations: &Translations, month0: usize) -> String {
    t(translations, MONTH_KEYS[month0])
}

/// How long something took: "46 s", "1 min 40".
/// A text with a number in it, in its form for exactly one (`<key>_one`) if that is the number.
pub fn counted(translations: &Translations, key: &str, count: u32) -> String {
    let form = if count == 1 {
        format!("{key}_one")
    } else {
        key.to_owned()
    };
    t(translations, &form).replace("{count}", &count.to_string())
}

/// A number with one decimal, written as this language writes it.
pub fn one_decimal(translations: &Translations, value: f64) -> String {
    format!("{value:.1}").replace('.', &t(translations, "text.decimal_point"))
}

pub fn duration(translations: &Translations, d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs >= 3600 {
        t(translations, "text.hours_minutes")
            .replace("{hours}", &(secs / 3600).to_string())
            .replace("{minutes}", &((secs % 3600) / 60).to_string())
    } else if secs >= 60 {
        t(translations, "text.minutes_seconds")
            .replace("{minutes}", &(secs / 60).to_string())
            .replace("{seconds}", &format!("{:02}", secs % 60))
    } else {
        t(translations, "text.seconds").replace("{seconds}", &secs.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clock_in_either_format() {
        assert_eq!(clock(7, 5, true), "07:05");
        assert_eq!(clock(7, 5, false), "7:05 AM");
        assert_eq!(clock(0, 0, false), "12:00 AM");
        assert_eq!(clock(12, 30, false), "12:30 PM");
        assert_eq!(clock(23, 59, false), "11:59 PM");
    }

    #[test]
    fn an_hour_on_the_wheel_in_either_format() {
        assert_eq!(hour_label(7, true), "07");
        assert_eq!(hour_label(7, false), "7 AM");
        assert_eq!(hour_label(0, false), "12 AM");
        assert_eq!(hour_label(13, false), "1 PM");
    }
}
