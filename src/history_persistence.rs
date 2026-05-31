//! History persistence — records alarm events and provides query/aggregation.
//! Uses the shared SQLite database (via `persistence::database()`) for
//! reliable, queryable storage of alarm firing history.

use chrono::Utc;

use crate::models::{AggregateMetrics, AlarmEvent, HistoryFilter};
use crate::persistence;

/// Record an alarm event after dismissal.
pub fn record_alarm_event(event: &AlarmEvent) {
    let db = persistence::database();
    if let Err(e) = db.insert_event(event) {
        eprintln!("[AlarmFree] Failed to record history event: {}", e);
    }
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

/// Delete history events older than the given number of days.
/// If `older_than_days` is None, deletes all history.
pub fn delete_history(older_than_days: Option<u32>) {
    let db = persistence::database();
    match older_than_days {
        Some(days) => {
            let cutoff = Utc::now() - chrono::Duration::days(days as i64);
            if let Err(e) = db.delete_events_before(&cutoff) {
                eprintln!("[AlarmFree] Failed to delete old history: {}", e);
            }
        }
        None => {
            if let Err(e) = db.delete_all_events() {
                eprintln!("[AlarmFree] Failed to delete all history: {}", e);
            }
        }
    }
}
