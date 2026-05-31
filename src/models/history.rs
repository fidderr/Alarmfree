//! Alarm history types: AlarmEvent, ChallengeSolve, HistoryFilter, AggregateMetrics.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::alarm::AlarmId;
use crate::models::challenge::{ChallengeType, Difficulty};

/// A single challenge solve recorded during an alarm firing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeSolve {
    pub challenge_type: ChallengeType,
    pub difficulty: Difficulty,
    pub duration: Duration,
}

/// A recorded alarm firing event with all metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlarmEvent {
    pub id: String,
    pub alarm_id: AlarmId,
    pub alarm_label: String,
    pub fire_timestamp: DateTime<Utc>,
    pub first_interaction_timestamp: Option<DateTime<Utc>>,
    pub dismiss_timestamp: DateTime<Utc>,
    pub snooze_count: u8,
    /// Per-snooze interval in minutes (e.g. [1, 1, 5] = three snoozes of 1, 1, 5 minutes).
    pub snooze_intervals_minutes: Vec<u8>,
    /// Per-challenge solve records.
    pub challenge_solves: Vec<ChallengeSolve>,
    pub total_dismissal_time: Duration,
}

impl AlarmEvent {
    /// Time between alarm firing and the user's first interaction.
    pub fn reaction_time(&self) -> Option<Duration> {
        let first = self.first_interaction_timestamp?;
        (first - self.fire_timestamp).to_std().ok()
    }

    /// Total time spent in snooze (sum of intervals × 60s).
    pub fn total_snoozed(&self) -> Duration {
        let secs = self
            .snooze_intervals_minutes
            .iter()
            .map(|m| *m as u64 * 60)
            .sum();
        Duration::from_secs(secs)
    }

    /// Total time spent on all challenges.
    pub fn total_challenge_time(&self) -> Duration {
        self.challenge_solves.iter().map(|s| s.duration).sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HistoryFilter {
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    ThisYear,
    LastYear,
    LastDays(u32),
}

/// Aggregate metrics computed across a set of alarm events.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateMetrics {
    pub total_events: u32,
    pub avg_snooze_count: f64,
    pub avg_snooze_interval_minutes: f64,
    pub avg_total_snoozed: Duration,
    pub avg_reaction_time: Duration,
    pub avg_total_challenge_time: Duration,
    pub avg_total_dismissal_time: Duration,
    /// Average solve time grouped by (challenge type, difficulty).
    pub avg_solve_per_challenge: BTreeMap<(ChallengeType, Difficulty), Duration>,
}

impl Default for AggregateMetrics {
    fn default() -> Self {
        Self {
            total_events: 0,
            avg_snooze_count: 0.0,
            avg_snooze_interval_minutes: 0.0,
            avg_total_snoozed: Duration::ZERO,
            avg_reaction_time: Duration::ZERO,
            avg_total_challenge_time: Duration::ZERO,
            avg_total_dismissal_time: Duration::ZERO,
            avg_solve_per_challenge: BTreeMap::new(),
        }
    }
}
