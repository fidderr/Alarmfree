//! History persistence — records alarm events and provides query/aggregation.
//! Uses the shared SQLite database (via `persistence::database()`) for
//! reliable, queryable storage of alarm firing history.

use chrono::Utc;

use crate::models::{AggregateMetrics, AlarmEvent, AlarmId, AlarmOutcome, HistoryFilter};
use crate::persistence;

/// Record an alarm event after dismissal.
pub fn record_alarm_event(event: &AlarmEvent) {
    let db = persistence::database();
    if let Err(e) = db.insert_event(event) {
        log::error!("[AlarmFree] Failed to record history event: {e}");
    }
}

/// Record an alarm that ended without the user. The user has not seen this yet;
/// see [`take_unseen_outcomes`].
pub fn record_outcome(
    alarm_id: &str,
    label: &str,
    outcome: AlarmOutcome,
    at: chrono::DateTime<Utc>,
) {
    let event = AlarmEvent {
        id: uuid::Uuid::new_v4().to_string(),
        alarm_id: AlarmId(alarm_id.to_owned()),
        alarm_label: label.to_owned(),
        fire_timestamp: at,
        first_interaction_timestamp: None,
        dismiss_timestamp: at,
        snooze_count: 0,
        snooze_intervals_minutes: Vec::new(),
        challenge_solves: Vec::new(),
        total_dismissal_time: std::time::Duration::ZERO,
        outcome,
    };
    if let Err(e) = persistence::database().insert_unseen_event(&event) {
        log::error!("[AlarmFree] Failed to record that alarm {alarm_id} ended: {e}");
    }
}

/// How and which alarms ended without the user since the last call, by name.
pub fn take_unseen_outcomes() -> Vec<(AlarmOutcome, String)> {
    persistence::database()
        .take_unseen_outcomes()
        .unwrap_or_else(|e| {
            log::error!("[AlarmFree] Failed to read what happened to alarms: {e}");
            Vec::new()
        })
}

/// Query history events with a filter.
pub fn query_history(filter: &HistoryFilter) -> Vec<AlarmEvent> {
    let db = persistence::database();
    db.query_events(filter).unwrap_or_default()
}

/// Compute aggregate metrics for a filter period.
pub fn aggregate_history(filter: &HistoryFilter) -> AggregateMetrics {
    let db = persistence::database();
    db.aggregate_metrics(filter).unwrap_or_default()
}

/// Delete one event.
pub fn delete_event(id: &str) {
    if let Err(e) = persistence::database().delete_event(id) {
        log::error!("[AlarmFree] Failed to delete a history event: {e}");
    }
}

/// Delete history events older than the given number of days.
/// If `older_than_days` is None, deletes all history.
pub fn delete_history(older_than_days: Option<u32>) {
    let db = persistence::database();
    match older_than_days {
        Some(days) => {
            let cutoff = Utc::now() - chrono::Duration::days(days as i64);
            if let Err(e) = db.delete_events_before(&cutoff) {
                log::error!("[AlarmFree] Failed to delete old history: {e}");
            }
        }
        None => {
            if let Err(e) = db.delete_all_events() {
                log::error!("[AlarmFree] Failed to delete all history: {e}");
            }
        }
    }
}
