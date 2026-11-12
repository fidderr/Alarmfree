//! The month a date picker shows: which days fill the grid and which month comes next.

use chrono::{Datelike, Duration, NaiveDate};

use crate::models::WeekStart;

/// The first day of the month `by` months from the one `date` is in.
pub fn shift_month(date: NaiveDate, by: i32) -> NaiveDate {
    let months = date.year() * 12 + date.month0() as i32 + by;
    NaiveDate::from_ymd_opt(months.div_euclid(12), months.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(date)
}

/// The cells of a month, six weeks of seven: `None` before the first and after the last day.
pub fn month_grid(month: NaiveDate, week_start: WeekStart) -> Vec<Option<NaiveDate>> {
    let first = NaiveDate::from_ymd_opt(month.year(), month.month(), 1).unwrap_or(month);
    let lead = week_start.days_into_week(first.weekday()) as usize;
    (0..42)
        .map(|cell| {
            let day = first + Duration::days(cell as i64 - lead as i64);
            (day.month() == first.month() && day.year() == first.year()).then_some(day)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn months_wrap_over_the_year() {
        assert_eq!(shift_month(date(2026, 11, 12), 1), date(2026, 12, 1));
        assert_eq!(shift_month(date(2026, 12, 31), 1), date(2027, 1, 1));
        assert_eq!(shift_month(date(2026, 1, 15), -1), date(2025, 12, 1));
        assert_eq!(shift_month(date(2026, 3, 31), 0), date(2026, 3, 1));
    }

    #[test]
    fn the_grid_starts_in_the_column_of_the_first_weekday() {
        // 1 Nov 2026 is a Sunday.
        let monday = month_grid(date(2026, 11, 20), WeekStart::Monday);
        assert_eq!(monday.len(), 42);
        assert_eq!(monday.iter().position(Option::is_some), Some(6));
        let sunday = month_grid(date(2026, 11, 20), WeekStart::Sunday);
        assert_eq!(sunday.iter().position(Option::is_some), Some(0));
        let saturday = month_grid(date(2026, 11, 20), WeekStart::Saturday);
        assert_eq!(saturday.iter().position(Option::is_some), Some(1));
        assert_eq!(monday.iter().flatten().count(), 30);
        assert_eq!(monday.iter().flatten().last(), Some(&date(2026, 11, 30)));
    }

    #[test]
    fn a_leap_february_has_twenty_nine_days() {
        let grid = month_grid(date(2028, 2, 10), WeekStart::Monday);
        assert_eq!(grid.iter().flatten().count(), 29);
    }
}
