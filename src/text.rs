//! Times, dates and durations as the user reads them: in their language and their
//! time format. Translated through the word lists, without a locale library for a
//! handful of words.

use chrono::{Datelike, NaiveDate};

use crate::app::{t, Translations};
use crate::models::{AppSettings, TimeFormat};

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

/// "07:30" or "7:30 AM".
pub fn clock(hour: u8, minute: u8, twenty_four: bool) -> String {
    if twenty_four {
        format!("{hour:02}:{minute:02}")
    } else {
        let period = if hour < 12 { "AM" } else { "PM" };
        let shown = match hour % 12 {
            0 => 12,
            other => other,
        };
        format!("{shown}:{minute:02} {period}")
    }
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
}
