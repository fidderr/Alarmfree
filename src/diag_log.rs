//! What AlarmFree writes to the diagnostic log (mobile-sentinel's `diag`), for a bug report.
//! Events only: times, repeat days, ids and outcomes. Never an alarm's label or note.

use chrono::{NaiveDateTime, Timelike};

use crate::models::AlarmData;

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Add an event to the log.
pub fn event(tag: &str, message: &str) {
    mobile_sentinel::diag::log(tag, message);
}

/// An alarm as the log and the report tell it: no label, no note, no sound name.
pub fn alarm_line(alarm: &AlarmData) -> String {
    let repeat = if alarm.days.iter().any(|d| *d) {
        DAYS.iter()
            .zip(alarm.days)
            .filter(|(_, on)| *on)
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(",")
    } else {
        format!("once {}", alarm.date.as_deref().unwrap_or("undated"))
    };
    let challenges = alarm
        .challenges
        .iter()
        .map(|(kind, _)| kind.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let mut line = format!(
        "id={} {:02}:{:02} {repeat} {}",
        short_id(&alarm.id),
        alarm.time_hour,
        alarm.time_minute,
        if alarm.enabled { "on" } else { "off" },
    );
    line.push_str(&format!(
        " snooze={}x{}min challenges=[{challenges}]",
        alarm.snooze_count, alarm.snooze_interval
    ));
    if alarm.skip_until_ms.is_some() {
        line.push_str(" skipped");
    }
    if alarm.pause_until_ms.is_some() {
        line.push_str(" paused");
    }
    if alarm.hidden {
        line.push_str(" hidden");
    }
    line
}

/// The start of an id: enough to tell alarms apart in a log.
pub fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

/// What changed between the saved alarms and the ones being saved, as log lines.
pub fn alarm_changes(old: &[AlarmData], new: &[AlarmData]) -> Vec<String> {
    let mut lines = Vec::new();
    for gone in old.iter().filter(|o| !new.iter().any(|n| n.id == o.id)) {
        lines.push(format!("alarm deleted {}", alarm_line(gone)));
    }
    for alarm in new {
        match old.iter().find(|o| o.id == alarm.id) {
            None => lines.push(format!("alarm added {}", alarm_line(alarm))),
            Some(before) if before != alarm => {
                let what = if before.enabled != alarm.enabled {
                    if alarm.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                } else {
                    "saved"
                };
                lines.push(format!("alarm {what} {}", alarm_line(alarm)));
            }
            Some(_) => {}
        }
    }
    lines
}

/// How many seconds after its time of day an alarm started to ring. The planned moment is
/// the alarm's hour and minute on the day it rang, or the day before when that is later.
pub fn lateness_secs(rang_at: NaiveDateTime, hour: u8, minute: u8) -> i64 {
    let planned = rang_at
        .with_hour(u32::from(hour))
        .and_then(|t| t.with_minute(u32::from(minute)))
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(rang_at);
    let planned = if planned > rang_at + chrono::Duration::minutes(1) {
        planned - chrono::Duration::days(1)
    } else {
        planned
    };
    (rang_at - planned).num_seconds()
}

/// The permission checks on one line.
pub fn permissions_line(report: &mobile_sentinel::health::HealthReport) -> String {
    report.summary().trim_end().replace('\n', ", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn alarm() -> AlarmData {
        let mut a = crate::alarm_ops::factory_alarm();
        a.id = "0123456789abcdef".into();
        a.time_hour = 7;
        a.time_minute = 5;
        a.days = [true, true, false, false, false, false, false];
        a.label = "Secret errand".into();
        a.note = "pills in the blue box".into();
        a
    }

    #[test]
    fn an_alarm_line_has_time_and_repeat_but_never_label_or_note() {
        let line = alarm_line(&alarm());
        assert!(line.contains("07:05 Mon,Tue on"), "{line}");
        assert!(line.starts_with("id=01234567 "));
        assert!(!line.contains("Secret"));
        assert!(!line.contains("pills"));
    }

    #[test]
    fn changes_name_added_enabled_disabled_deleted() {
        let on = alarm();
        let mut off = on.clone();
        off.enabled = false;
        let mut moved = on.clone();
        moved.time_minute = 6;
        let lines = |old: &[AlarmData], new: &[AlarmData]| alarm_changes(old, new);
        assert!(lines(&[], std::slice::from_ref(&on))[0].starts_with("alarm added"));
        assert!(
            lines(std::slice::from_ref(&on), std::slice::from_ref(&off))[0]
                .starts_with("alarm disabled")
        );
        assert!(
            lines(std::slice::from_ref(&off), std::slice::from_ref(&on))[0]
                .starts_with("alarm enabled")
        );
        assert!(lines(std::slice::from_ref(&on), &[])[0].starts_with("alarm deleted"));
        assert!(lines(std::slice::from_ref(&on), std::slice::from_ref(&on)).is_empty());
        assert!(
            lines(std::slice::from_ref(&on), std::slice::from_ref(&moved))[0]
                .starts_with("alarm saved")
        );
        for line in lines(&[], std::slice::from_ref(&on)) {
            assert!(!line.contains("Secret") && !line.contains("pills"));
        }
    }

    #[test]
    fn lateness_counts_from_the_planned_time_also_across_midnight() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
        let at = |h, m, s| day.and_hms_opt(h, m, s).unwrap();
        assert_eq!(lateness_secs(at(7, 0, 4), 7, 0), 4);
        assert_eq!(lateness_secs(at(7, 12, 0), 7, 0), 720);
        // Planned 23:59, rang at 00:00:30 the next day.
        assert_eq!(lateness_secs(at(0, 0, 30), 23, 59), 90);
        assert_eq!(lateness_secs(at(6, 59, 59), 7, 0), -1);
    }
}
