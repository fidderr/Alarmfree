//! The edit lock: shortly before an alarm rings it cannot be switched off or weakened, unless
//! the user does its challenges. One function decides whether an alarm is locked
//! ([`locked_until`]); every action that changes an alarm asks it.

use std::sync::Mutex;

use crate::models::AlarmData;

/// What the setting offers, in minutes; 0 is off.
pub const LOCK_MINUTES: [u16; 11] = [0, 1, 5, 10, 15, 30, 60, 120, 180, 360, 600];

/// How long an alarm stays open after its challenges were done.
pub const OPEN_MS: i64 = 5 * 60_000;

/// Alarms opened by their challenges, until when (unix ms). In memory only: gone with the app.
static OPEN: Mutex<Vec<(String, i64)>> = Mutex::new(Vec::new());

/// Alarms whose Skip on the reminder notice was refused, for the screen to say so.
static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// When the locked alarm rings, or `None` if it is not locked. Locked is: the lock is on, the
/// alarm is on and not paused, it is not a quick alarm, a wake-up check or a test, it was not
/// opened by its challenges, and its next ring is at most `lock_minutes` away.
pub fn locked_until(
    alarm: &AlarmData,
    now_ms: i64,
    next_ring_ms: Option<i64>,
    open_until_ms: Option<i64>,
) -> Option<i64> {
    let free = alarm.lock_minutes == 0
        || !alarm.enabled
        || alarm.hidden
        || alarm.test
        || alarm.delete_after_ring
        || alarm.pause_until_ms.is_some_and(|until| until > now_ms)
        || open_until_ms.is_some_and(|until| until > now_ms);
    let ring = next_ring_ms?;
    (!free && ring - now_ms <= i64::from(alarm.lock_minutes) * 60_000).then_some(ring)
}

/// Whether the alarm is already locked when its reminder comes, `before_ms` ahead of the ring:
/// that reminder has no Skip button.
pub fn locked_when_reminded(alarm: &AlarmData, before_ms: i64) -> bool {
    alarm.lock_minutes > 0
        && alarm.enabled
        && !alarm.hidden
        && !alarm.test
        && !alarm.delete_after_ring
        && before_ms <= i64::from(alarm.lock_minutes) * 60_000
}

/// The next ring as far as the app can tell: AlarmKit knows a snooze in progress, the settings
/// know the rest.
pub fn next_ring_ms(alarm: &AlarmData, now: chrono::DateTime<chrono::Local>) -> Option<i64> {
    crate::platform::alarm_kit_optional()
        .and_then(|kit| {
            kit.next_fire(&mobile_sentinel::InstanceId::new(alarm.id.clone()))
                .ok()
                .flatten()
        })
        .or_else(|| crate::schedule::next_ring_ms(alarm, now))
}

/// [`locked_until`] for this alarm right now.
pub fn locked_now(alarm: &AlarmData) -> Option<i64> {
    if alarm.lock_minutes == 0 {
        return None;
    }
    let now = crate::clock::local_now();
    locked_until(
        alarm,
        now.timestamp_millis(),
        next_ring_ms(alarm, now),
        open_until(&alarm.id),
    )
}

/// The first locked alarm of the list.
pub fn first_locked(alarms: &[AlarmData]) -> Option<&AlarmData> {
    alarms.iter().find(|alarm| locked_now(alarm).is_some())
}

pub fn open_until(id: &str) -> Option<i64> {
    OPEN.lock()
        .ok()?
        .iter()
        .find(|(open, _)| open == id)
        .map(|(_, until)| *until)
}

/// The alarm's challenges were done: it can be changed for [`OPEN_MS`].
pub fn unlock(id: &str, now_ms: i64) {
    if let Ok(mut open) = OPEN.lock() {
        open.retain(|(open, until)| open != id && *until > now_ms);
        open.push((id.to_owned(), now_ms + OPEN_MS));
    }
}

/// Take the lock back, as if its challenges had not been done.
pub fn relock(id: &str) {
    if let Ok(mut open) = OPEN.lock() {
        open.retain(|(open, _)| open != id);
    }
}

pub fn note_refused_skip(name: String) {
    if let Ok(mut refused) = REFUSED.lock() {
        refused.push(name);
    }
}

pub fn take_refused_skips() -> Vec<String> {
    REFUSED
        .lock()
        .map(|mut refused| std::mem::take(&mut *refused))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000_000_000;
    const MIN: i64 = 60_000;

    fn locked_alarm() -> AlarmData {
        let mut alarm = crate::alarm_ops::factory_alarm();
        alarm.id = "lock-test".into();
        alarm.lock_minutes = 10;
        alarm
    }

    fn locked(alarm: &AlarmData, ring_in_ms: Option<i64>) -> bool {
        locked_until(alarm, NOW, ring_in_ms.map(|ms| NOW + ms), None).is_some()
    }

    #[test]
    fn an_alarm_is_locked_from_the_edge_of_its_window_until_it_rings() {
        let alarm = locked_alarm();
        assert!(!locked(&alarm, Some(10 * MIN + 1)));
        assert!(locked(&alarm, Some(10 * MIN)));
        assert!(locked(&alarm, Some(MIN)));
        assert!(locked(&alarm, Some(0)));
        assert_eq!(
            locked_until(&alarm, NOW, Some(NOW + 5 * MIN), None),
            Some(NOW + 5 * MIN)
        );
    }

    #[test]
    fn nothing_is_locked_without_a_next_ring() {
        assert!(!locked(&locked_alarm(), None));
    }

    #[test]
    fn off_means_never_locked() {
        let mut alarm = locked_alarm();
        alarm.lock_minutes = 0;
        assert!(!locked(&alarm, Some(MIN)));
    }

    #[test]
    fn a_switched_off_or_paused_alarm_is_not_locked() {
        let mut alarm = locked_alarm();
        alarm.enabled = false;
        assert!(!locked(&alarm, Some(MIN)));
        let mut alarm = locked_alarm();
        alarm.pause_until_ms = Some(NOW + 1);
        assert!(!locked(&alarm, Some(MIN)));
        alarm.pause_until_ms = Some(NOW);
        assert!(locked(&alarm, Some(MIN)), "a pause that is over is none");
    }

    #[test]
    fn a_skipped_ring_counts_from_the_ring_after_it() {
        let mut alarm = locked_alarm();
        alarm.skip_until_ms = Some(NOW + 5 * MIN);
        // The next ring is the day after: far outside the window.
        assert!(!locked(&alarm, Some(24 * 60 * MIN)));
    }

    #[test]
    fn a_snooze_in_progress_is_the_next_ring() {
        let alarm = locked_alarm();
        assert!(locked(&alarm, Some(5 * MIN)), "snoozed for 5 minutes");
        let mut one_minute = alarm;
        one_minute.lock_minutes = 1;
        assert!(!locked(&one_minute, Some(5 * MIN)));
    }

    #[test]
    fn quick_alarms_wake_up_checks_and_tests_never_lock() {
        for change in [
            |a: &mut AlarmData| a.delete_after_ring = true,
            |a: &mut AlarmData| a.hidden = true,
            |a: &mut AlarmData| a.test = true,
        ] {
            let mut alarm = locked_alarm();
            change(&mut alarm);
            assert!(!locked(&alarm, Some(MIN)));
        }
        let now = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(7, 0, 0)
            .unwrap();
        let mut defaults = locked_alarm();
        defaults.lock_minutes = 600;
        assert_eq!(
            crate::alarm_ops::quick_alarm(now, 5, String::new(), Some(&defaults)).lock_minutes,
            0
        );
        assert_eq!(
            crate::alarm_ops::test_alarm(now, 30, String::new()).lock_minutes,
            0
        );
        assert_eq!(
            crate::alarm_ops::wake_check_alarm(&defaults, now, String::new()).lock_minutes,
            0
        );
    }

    #[test]
    fn doing_the_challenges_opens_the_alarm_for_five_minutes() {
        let alarm = locked_alarm();
        let ring = Some(NOW + MIN);
        let open = Some(NOW + OPEN_MS);
        assert!(locked_until(&alarm, NOW, ring, None).is_some());
        assert!(locked_until(&alarm, NOW, ring, open).is_none());
        assert!(locked_until(&alarm, NOW + OPEN_MS - 1, ring, open).is_none());
        assert!(locked_until(&alarm, NOW + OPEN_MS, ring, open).is_some());
    }

    #[test]
    fn unlocking_is_remembered_per_alarm_and_taken_back_by_relock() {
        unlock("lock-memory-a", NOW);
        assert_eq!(open_until("lock-memory-a"), Some(NOW + OPEN_MS));
        assert_eq!(open_until("lock-memory-b"), None);
        unlock("lock-memory-a", NOW + MIN);
        assert_eq!(open_until("lock-memory-a"), Some(NOW + MIN + OPEN_MS));
        relock("lock-memory-a");
        assert_eq!(open_until("lock-memory-a"), None);
    }

    #[test]
    fn a_reminder_inside_the_lock_window_has_no_skip_button() {
        let alarm = locked_alarm();
        assert!(locked_when_reminded(&alarm, 10 * MIN), "at the edge");
        assert!(locked_when_reminded(&alarm, 5 * MIN));
        assert!(!locked_when_reminded(&alarm, 10 * MIN + 1));
        assert!(!locked_when_reminded(&alarm, 30 * MIN));
        let mut off = locked_alarm();
        off.lock_minutes = 0;
        assert!(!locked_when_reminded(&off, MIN));
        let mut disabled = locked_alarm();
        disabled.enabled = false;
        assert!(!locked_when_reminded(&disabled, MIN));
    }

    #[test]
    fn the_five_minutes_are_what_the_texts_say() {
        assert_eq!(OPEN_MS, 5 * 60_000);
        for locale in crate::models::Locale::ALL {
            let tr = crate::i18n::load_translations(locale);
            assert!(tr["config.edit_lock_note"].contains('5'), "{locale:?}");
        }
    }

    #[test]
    fn the_choices_run_from_off_to_ten_hours() {
        assert_eq!(LOCK_MINUTES[0], 0);
        assert_eq!(*LOCK_MINUTES.last().unwrap(), 600);
        assert!(LOCK_MINUTES.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_saved_alarm_without_the_setting_loads_unlocked() {
        let mut saved = serde_json::to_value(crate::alarm_ops::factory_alarm()).unwrap();
        saved.as_object_mut().unwrap().remove("lock_minutes");
        let alarm: AlarmData = serde_json::from_value(saved).unwrap();
        assert_eq!(alarm.lock_minutes, 0);
    }
}
