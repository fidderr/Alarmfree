//! The order alarms are listed in on the home screen.
//!
//! Alarms are listed by `AlarmData::order`, then by clock time, then by id,
//! so alarms nobody has moved (order 0, e.g. saved before reordering existed)
//! keep the familiar by-time order. Dragging an alarm numbers all of them
//! 1..n in the new sequence.

use crate::app::AlarmData;

/// The alarms in display order.
pub fn sorted_for_display(alarms: &[AlarmData]) -> Vec<AlarmData> {
    let mut list = alarms.to_vec();
    list.sort_by(|a, b| {
        (a.order, a.time_hour, a.time_minute, &a.id).cmp(&(
            b.order,
            b.time_hour,
            b.time_minute,
            &b.id,
        ))
    });
    list
}

/// Move the alarm shown at position `from` to position `to` (positions in
/// display order) and store the new order in every alarm. Returns whether
/// anything moved.
pub fn apply_reorder(alarms: &mut [AlarmData], from: usize, to: usize) -> bool {
    let mut ids: Vec<String> = sorted_for_display(alarms)
        .into_iter()
        .map(|a| a.id)
        .collect();
    if from >= ids.len() || to >= ids.len() || from == to {
        return false;
    }
    let moved = ids.remove(from);
    ids.insert(to, moved);
    for alarm in alarms.iter_mut() {
        if let Some(position) = ids.iter().position(|id| *id == alarm.id) {
            alarm.order = position as i64 + 1;
        }
    }
    true
}

/// The order value for an alarm that is being added: at the end once the user
/// has arranged the list by hand, otherwise 0 so it sorts by time with the rest.
pub fn order_for_new(alarms: &[AlarmData]) -> i64 {
    let highest = alarms.iter().map(|a| a.order).max().unwrap_or(0);
    if highest > 0 {
        highest + 1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alarm(id: &str, hour: u8, minute: u8, order: i64) -> AlarmData {
        AlarmData {
            id: id.into(),
            time_hour: hour,
            time_minute: minute,
            label: String::new(),
            days: [true; 7],
            date: None,
            skip_until_ms: None,
            fade_in_secs: 0,
            ring_limit_secs: None,
            order,
            enabled: true,
            sound: "air".into(),
            sound_mode: 0,
            challenges: vec![],
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

    fn ids(list: &[AlarmData]) -> Vec<&str> {
        list.iter().map(|a| a.id.as_str()).collect()
    }

    #[test]
    fn unarranged_alarms_are_listed_by_time() {
        let list = [
            alarm("c", 9, 0, 0),
            alarm("a", 6, 30, 0),
            alarm("b", 7, 0, 0),
        ];
        assert_eq!(ids(&sorted_for_display(&list)), ["a", "b", "c"]);
    }

    #[test]
    fn a_stored_order_beats_the_clock() {
        let list = [
            alarm("a", 6, 30, 3),
            alarm("b", 7, 0, 1),
            alarm("c", 9, 0, 2),
        ];
        assert_eq!(ids(&sorted_for_display(&list)), ["b", "c", "a"]);
    }

    #[test]
    fn moving_an_alarm_renumbers_the_whole_list() {
        let mut list = vec![
            alarm("a", 6, 30, 0),
            alarm("b", 7, 0, 0),
            alarm("c", 9, 0, 0),
        ];
        assert!(apply_reorder(&mut list, 2, 0)); // c to the top
        assert_eq!(ids(&sorted_for_display(&list)), ["c", "a", "b"]);
        assert_eq!(
            sorted_for_display(&list)
                .iter()
                .map(|a| a.order)
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(apply_reorder(&mut list, 0, 1)); // and back down one
        assert_eq!(ids(&sorted_for_display(&list)), ["a", "c", "b"]);
    }

    #[test]
    fn a_move_to_the_same_place_or_out_of_range_does_nothing() {
        let mut list = vec![alarm("a", 6, 0, 0), alarm("b", 7, 0, 0)];
        assert!(!apply_reorder(&mut list, 1, 1));
        assert!(!apply_reorder(&mut list, 0, 5));
        assert!(!apply_reorder(&mut list, 5, 0));
        assert!(list.iter().all(|a| a.order == 0), "nothing was renumbered");
    }

    #[test]
    fn a_new_alarm_joins_the_time_order_until_the_user_arranges_the_list() {
        let untouched = [alarm("a", 6, 0, 0), alarm("b", 7, 0, 0)];
        assert_eq!(order_for_new(&untouched), 0);
        let arranged = [alarm("a", 6, 0, 2), alarm("b", 7, 0, 1)];
        assert_eq!(
            order_for_new(&arranged),
            3,
            "goes to the end of a hand-made list"
        );
        assert_eq!(order_for_new(&[]), 0);
    }
}
