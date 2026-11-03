//! What can be done to alarms, apart from any screen: new alarms and what they start
//! from, quick alarms, wake-up checks, pausing, backups.

use chrono::{Duration, NaiveDateTime, Timelike};

use crate::app::AlarmData;

/// The most alarms the list takes.
pub const MAX_ALARMS: usize = 50;

/// How a new alarm starts when the user did not set up their own defaults.
pub fn factory_alarm() -> AlarmData {
    AlarmData {
        id: String::new(),
        time_hour: 7,
        time_minute: 0,
        label: String::new(),
        days: [true, true, true, true, true, false, false],
        date: None,
        skip_until_ms: None,
        fade_in_secs: 60,
        ring_limit_secs: Some(mobile_sentinel::context::schema::DEFAULT_RING_LIMIT_SECS),
        order: 0,
        enabled: true,
        sound: crate::custom_sounds::list_default_sounds()
            .first()
            .cloned()
            .unwrap_or_default(),
        sound_mode: 0,
        challenges: Vec::new(),
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

/// A new alarm, from the user's defaults if they have any.
pub fn new_alarm(defaults: Option<&AlarmData>) -> AlarmData {
    let mut alarm = defaults.cloned().unwrap_or_else(factory_alarm);
    alarm.id = String::new();
    alarm.enabled = true;
    alarm.skip_until_ms = None;
    alarm.pause_until_ms = None;
    alarm
}

/// A one-time alarm at this moment. An alarm rings on whole minutes, so seconds round
/// up: a nap is at least as long as asked for.
fn one_time_at(mut alarm: AlarmData, at: NaiveDateTime) -> AlarmData {
    let at = if at.second() > 0 {
        at + Duration::minutes(1)
    } else {
        at
    };
    alarm.time_hour = at.hour() as u8;
    alarm.time_minute = at.minute() as u8;
    alarm.days = [false; 7];
    alarm.date = Some(at.date().format("%Y-%m-%d").to_string());
    alarm.id = uuid::Uuid::new_v4().to_string();
    alarm
}

/// An alarm for a nap: rings `minutes` from now, and deletes itself when done.
pub fn quick_alarm(
    now: NaiveDateTime,
    minutes: i64,
    label: String,
    defaults: Option<&AlarmData>,
) -> AlarmData {
    let mut alarm = one_time_at(new_alarm(defaults), now + Duration::minutes(minutes));
    alarm.label = label;
    alarm.delete_after_ring = true;
    alarm.wake_check_minutes = 0;
    alarm
}

/// A little later, an alarm the user only has to confirm: if they fell asleep again it
/// goes off by the same path as any alarm. Not listed, and gone when done.
pub fn wake_check_alarm(source: &AlarmData, now: NaiveDateTime, label: String) -> AlarmData {
    let mut alarm = source.clone();
    alarm.challenges = Vec::new();
    alarm.snooze_count = 0;
    alarm.wake_check_minutes = 0;
    alarm.fade_in_secs = 0;
    alarm.note = String::new();
    alarm.label = label;
    alarm.delete_after_ring = true;
    alarm.hidden = true;
    alarm.skip_until_ms = None;
    alarm.pause_until_ms = None;
    alarm.enabled = true;
    one_time_at(
        alarm,
        now + Duration::minutes(i64::from(source.wake_check_minutes)),
    )
}

/// An alarm to try the whole ring path with: real, but not listed, without challenges,
/// and gone when done.
pub fn test_alarm(now: NaiveDateTime, seconds: i64, label: String) -> AlarmData {
    let mut alarm = factory_alarm();
    alarm.label = label;
    alarm.fade_in_secs = 0;
    alarm.snooze_count = 0;
    alarm.delete_after_ring = true;
    alarm.hidden = true;
    one_time_at(alarm, now + Duration::seconds(seconds))
}

/// Nothing rings until `until_ms`; afterwards everything carries on by itself.
pub fn pause_all(alarms: &mut [AlarmData], until_ms: i64) {
    for alarm in alarms.iter_mut().filter(|a| !a.hidden) {
        alarm.pause_until_ms = Some(until_ms);
    }
}

pub fn resume_all(alarms: &mut [AlarmData]) {
    for alarm in alarms.iter_mut() {
        alarm.pause_until_ms = None;
    }
}

/// When pausing is in force for every listed alarm, until when.
pub fn all_paused_until(alarms: &[AlarmData], now_ms: i64) -> Option<i64> {
    alarms
        .iter()
        .filter(|a| a.enabled && !a.hidden)
        .map(|a| a.pause_until_ms.filter(|until| *until > now_ms))
        .collect::<Option<Vec<i64>>>()?
        .into_iter()
        .min()
}

/// The alarms to put in a backup file: what the user has made, without what rings on
/// its own schedule right now.
pub fn export_json(alarms: &[AlarmData]) -> String {
    let listed: Vec<&AlarmData> = alarms.iter().filter(|a| !a.hidden).collect();
    serde_json::to_string_pretty(&listed).unwrap_or_else(|_| "[]".to_owned())
}

/// Alarms from a backup file, each with a fresh id so it never collides with an alarm
/// already there. Sounds that are not on this phone fall back to the first bundled one.
pub fn import_json(json: &str, known_sound: impl Fn(&str) -> bool) -> Option<Vec<AlarmData>> {
    let mut alarms: Vec<AlarmData> = serde_json::from_str(json).ok()?;
    for alarm in &mut alarms {
        alarm.id = uuid::Uuid::new_v4().to_string();
        alarm.hidden = false;
        if !known_sound(&alarm.sound) {
            alarm.sound = factory_alarm().sound;
        }
    }
    Some(alarms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(hour: u32, minute: u32, second: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 3)
            .unwrap()
            .and_hms_opt(hour, minute, second)
            .unwrap()
    }

    #[test]
    fn a_quick_alarm_is_never_shorter_than_asked_and_deletes_itself() {
        let alarm = quick_alarm(at(7, 0, 30), 5, "Nap".into(), None);
        assert_eq!((alarm.time_hour, alarm.time_minute), (7, 6));
        assert_eq!(alarm.date.as_deref(), Some("2026-11-03"));
        assert_eq!(alarm.days, [false; 7]);
        assert!(alarm.delete_after_ring);
        assert!(!alarm.id.is_empty());
        let exact = quick_alarm(at(7, 0, 0), 5, "Nap".into(), None);
        assert_eq!(exact.time_minute, 5);
    }

    #[test]
    fn a_quick_alarm_past_midnight_rings_tomorrow() {
        let alarm = quick_alarm(at(23, 50, 0), 30, "Nap".into(), None);
        assert_eq!((alarm.time_hour, alarm.time_minute), (0, 20));
        assert_eq!(alarm.date.as_deref(), Some("2026-11-04"));
    }

    #[test]
    fn a_wake_check_rings_later_and_only_needs_confirming() {
        let mut source = factory_alarm();
        source.wake_check_minutes = 5;
        source.challenges = vec![("challenge.math".into(), "{}".into())];
        source.snooze_count = 3;
        let check = wake_check_alarm(&source, at(7, 0, 0), "Up?".into());
        assert_eq!((check.time_hour, check.time_minute), (7, 5));
        assert!(check.challenges.is_empty());
        assert_eq!(check.snooze_count, 0);
        assert_eq!(check.wake_check_minutes, 0, "or it would never end");
        assert!(check.hidden && check.delete_after_ring);
    }

    #[test]
    fn pausing_all_leaves_hidden_alarms_ringing_and_can_be_undone() {
        let mut alarms = vec![factory_alarm(), factory_alarm()];
        alarms[1].hidden = true;
        pause_all(&mut alarms, 5_000);
        assert_eq!(alarms[0].pause_until_ms, Some(5_000));
        assert_eq!(alarms[1].pause_until_ms, None);
        assert_eq!(all_paused_until(&alarms, 1_000), Some(5_000));
        assert_eq!(all_paused_until(&alarms, 6_000), None, "the pause is over");
        resume_all(&mut alarms);
        assert_eq!(all_paused_until(&alarms, 1_000), None);
    }

    #[test]
    fn a_backup_comes_back_with_fresh_ids_and_known_sounds() {
        let mut alarm = factory_alarm();
        alarm.id = "a".into();
        alarm.label = "Work".into();
        alarm.sound = "gone.mp3".into();
        let mut hidden = factory_alarm();
        hidden.hidden = true;
        let json = export_json(&[alarm, hidden]);
        let back = import_json(&json, |sound| sound != "gone.mp3").unwrap();
        assert_eq!(back.len(), 1, "a wake-up check is not part of a backup");
        assert_eq!(back[0].label, "Work");
        assert_ne!(back[0].id, "a");
        assert_ne!(back[0].sound, "gone.mp3");
        assert!(import_json("not json", |_| true).is_none());
    }
}
