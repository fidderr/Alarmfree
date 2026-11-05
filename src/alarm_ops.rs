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
        notify_before_minutes: 0,
        wake_check_minutes: 0,
        max_brightness: false,
        sound_delay_secs: 0,
        pause_until_ms: None,
        delete_after_ring: false,
        hidden: false,
        test: false,
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

/// What new alarms start from to be like this one: all but what makes it this alarm.
pub fn as_defaults(alarm: &AlarmData) -> AlarmData {
    let mut defaults = alarm.clone();
    defaults.id = String::new();
    defaults.label = String::new();
    defaults.note = String::new();
    defaults.date = None;
    defaults.order = 0;
    defaults.delete_after_ring = false;
    defaults
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
    alarm.notify_before_minutes = 0;
    alarm
}

/// A little later, an alarm the user only has to confirm: if they fell asleep again it
/// goes off by the same path as any alarm. Not listed, and gone when done.
pub fn wake_check_alarm(source: &AlarmData, now: NaiveDateTime, label: String) -> AlarmData {
    let mut alarm = source.clone();
    alarm.challenges = Vec::new();
    alarm.snooze_count = 0;
    alarm.wake_check_minutes = 0;
    alarm.notify_before_minutes = 0;
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

/// The next ring is not wanted, said on the notice before it. A repeating alarm skips that one,
/// the way its menu does, and an alarm with one date has nothing left to ring for.
pub fn skip_from_notice(alarm: &mut AlarmData, next_fire: Option<i64>) -> bool {
    if crate::schedule::is_one_time(alarm) {
        let was_on = alarm.enabled;
        alarm.enabled = false;
        return was_on;
    }
    if next_fire.is_none() || alarm.skip_until_ms == next_fire {
        return false;
    }
    alarm.skip_until_ms = next_fire;
    true
}

/// The notice's tap taken back.
pub fn undo_skip_from_notice(alarm: &mut AlarmData) -> bool {
    if crate::schedule::is_one_time(alarm) {
        let was_off = !alarm.enabled;
        alarm.enabled = true;
        return was_off;
    }
    alarm.skip_until_ms.take().is_some()
}

/// What was one setting for all alarms goes to each of them, and to what new ones start from.
pub fn give_notice_to_all(list: &mut [AlarmData], defaults: Option<&mut AlarmData>, minutes: u8) {
    for alarm in list.iter_mut().filter(|alarm| !alarm.hidden) {
        alarm.notify_before_minutes = minutes;
    }
    if let Some(defaults) = defaults {
        defaults.notify_before_minutes = minutes;
    }
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
    alarm.test = true;
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

/// The alarms to put in a backup file: what the user has made and keeps, so no wake-up
/// checks, test alarms or naps.
pub fn export_json(alarms: &[AlarmData]) -> String {
    let kept: Vec<&AlarmData> = alarms
        .iter()
        .filter(|a| !a.hidden && !a.delete_after_ring)
        .collect();
    serde_json::to_string_pretty(&kept).unwrap_or_else(|_| "[]".to_owned())
}

/// Alarms from a backup file, each with a fresh id so it never collides with an alarm
/// already there. A skip or a pause was about then, not now, and an alarm whose one date
/// has passed comes back switched off. Sounds that are not on this phone fall back to the
/// first bundled one.
pub fn import_json(
    json: &str,
    known_sound: impl Fn(&str) -> bool,
    now: NaiveDateTime,
) -> Option<Vec<AlarmData>> {
    let mut alarms: Vec<AlarmData> = serde_json::from_str(json).ok()?;
    alarms.retain(|a| !a.delete_after_ring);
    for alarm in &mut alarms {
        alarm.id = uuid::Uuid::new_v4().to_string();
        alarm.hidden = false;
        alarm.skip_until_ms = None;
        alarm.pause_until_ms = None;
        if crate::schedule::is_one_time(alarm)
            && !crate::schedule::one_time_moment(alarm).is_some_and(|at| at > now)
        {
            alarm.enabled = false;
        }
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

    #[test]
    fn an_alarm_saved_by_an_older_version_still_loads() {
        let mut saved = serde_json::to_value(factory_alarm()).unwrap();
        let fields = saved.as_object_mut().unwrap();
        fields.remove("notify_before_minutes");
        fields.insert("random_order".to_owned(), true.into());
        let alarm: AlarmData = serde_json::from_value(saved).unwrap();
        assert_eq!(alarm.notify_before_minutes, 0);
    }

    #[test]
    fn a_notice_skips_the_next_ring_and_can_take_it_back() {
        let mut alarm = factory_alarm();
        assert!(skip_from_notice(&mut alarm, Some(1_000)));
        assert_eq!(alarm.skip_until_ms, Some(1_000));
        assert!(alarm.enabled, "the rings after that one stay");
        assert!(
            !skip_from_notice(&mut alarm, Some(1_000)),
            "already skipped"
        );
        assert!(undo_skip_from_notice(&mut alarm));
        assert_eq!(alarm.skip_until_ms, None);
        assert!(!undo_skip_from_notice(&mut alarm), "nothing to take back");
        assert!(!skip_from_notice(&mut alarm, None), "no next ring to skip");
    }

    #[test]
    fn a_notice_turns_off_an_alarm_with_one_date_and_can_turn_it_on_again() {
        let mut alarm = factory_alarm();
        alarm.days = [false; 7];
        alarm.date = Some("2030-01-01".to_owned());
        assert!(skip_from_notice(&mut alarm, Some(1_000)));
        assert!(!alarm.enabled);
        assert_eq!(alarm.skip_until_ms, None);
        assert!(undo_skip_from_notice(&mut alarm));
        assert!(alarm.enabled);
    }

    #[test]
    fn what_was_one_notice_for_all_goes_to_each_alarm() {
        let mut list = vec![factory_alarm(), factory_alarm()];
        list[1].hidden = true;
        let mut defaults = factory_alarm();
        give_notice_to_all(&mut list, Some(&mut defaults), 10);
        assert_eq!(list[0].notify_before_minutes, 10);
        assert_eq!(
            list[1].notify_before_minutes, 0,
            "not one of the user's alarms"
        );
        assert_eq!(defaults.notify_before_minutes, 10);
    }

    #[test]
    fn alarms_made_along_the_way_give_no_notice() {
        let mut source = factory_alarm();
        source.notify_before_minutes = 30;
        let now = NaiveDate::from_ymd_opt(2030, 1, 1)
            .unwrap()
            .and_hms_opt(7, 0, 0)
            .unwrap();
        assert_eq!(
            quick_alarm(now, 10, String::new(), Some(&source)).notify_before_minutes,
            0
        );
        assert_eq!(
            wake_check_alarm(&source, now, String::new()).notify_before_minutes,
            0
        );
    }

    fn at(hour: u32, minute: u32, second: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 3)
            .unwrap()
            .and_hms_opt(hour, minute, second)
            .unwrap()
    }

    #[test]
    fn defaults_from_an_alarm_keep_how_it_rings_and_not_what_it_is() {
        let mut alarm = quick_alarm(at(7, 0, 0), 5, "Nap".into(), None);
        alarm.note = "Charger".into();
        alarm.fade_in_secs = 60;
        alarm.order = 3;
        alarm.challenges = vec![("challenge.math".into(), "{}".into())];
        let new = new_alarm(Some(&as_defaults(&alarm)));
        assert_eq!(new.fade_in_secs, 60);
        assert_eq!(new.challenges, alarm.challenges);
        assert_eq!(new.time_minute, alarm.time_minute);
        assert!(new.id.is_empty() && new.label.is_empty() && new.note.is_empty());
        assert_eq!(new.date, None);
        assert_eq!(new.order, 0);
        assert!(!new.delete_after_ring);
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
        alarm.pause_until_ms = Some(5_000);
        let mut hidden = factory_alarm();
        hidden.hidden = true;
        let nap = quick_alarm(at(7, 0, 0), 5, "Nap".into(), None);
        let json = export_json(&[alarm, hidden, nap]);
        let back = import_json(&json, |sound| sound != "gone.mp3", at(7, 0, 0)).unwrap();
        assert_eq!(
            back.len(),
            1,
            "a wake-up check or a nap is not part of a backup"
        );
        assert_eq!(back[0].label, "Work");
        assert_ne!(back[0].id, "a");
        assert_ne!(back[0].sound, "gone.mp3");
        assert_eq!(back[0].pause_until_ms, None);
        assert!(back[0].enabled);
        assert!(import_json("not json", |_| true, at(7, 0, 0)).is_none());
    }

    #[test]
    fn an_alarm_for_a_date_that_has_passed_comes_back_switched_off() {
        let mut once = factory_alarm();
        once.days = [false; 7];
        once.date = Some("2026-11-03".into());
        let json = export_json(&[once]);
        let earlier = import_json(&json, |_| true, at(6, 0, 0)).unwrap();
        assert!(earlier[0].enabled, "still ahead");
        let later = import_json(&json, |_| true, at(8, 0, 0)).unwrap();
        assert!(!later[0].enabled);
    }
}
