//! Database access layer for AlarmFree.

pub mod migrations;
pub mod schema;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rusqlite::{params, Connection};

use crate::app::AlarmData;
use crate::models::*;

/// Database wrapper providing CRUD operations.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open a database at the given path, running migrations.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        migrations::run_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory database for testing.
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        migrations::run_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    // --- Alarm CRUD ---

    pub fn upsert_alarm(&self, alarm: &AlarmData) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let json = serde_json::to_string(alarm)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        conn.execute(
            "INSERT OR REPLACE INTO alarms (id, data) VALUES (?1, ?2)",
            params![alarm.id, json],
        )?;
        Ok(())
    }

    pub fn list_alarms(&self) -> Result<Vec<AlarmData>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare("SELECT data FROM alarms")?;
        let alarms = stmt
            .query_map([], |row| {
                let data: String = row.get(0)?;
                serde_json::from_str::<AlarmData>(&data).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        // Sort by time for stable display order.
        let mut alarms = alarms;
        alarms.sort_by_key(|a| (a.time_hour, a.time_minute));
        Ok(alarms)
    }

    pub fn delete_alarm(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute("DELETE FROM alarms WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- History ---

    pub fn insert_event(&self, event: &AlarmEvent) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let snooze_intervals_json = serde_json::to_string(&event.snooze_intervals_minutes)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let challenge_solves_json = serde_json::to_string(&event.challenge_solves)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        conn.execute(
            "INSERT INTO alarm_events (id, alarm_id, alarm_label, fire_timestamp, \
             first_interaction_timestamp, dismiss_timestamp, snooze_count, snooze_intervals, \
             challenge_solves, total_dismissal_time_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                event.id,
                event.alarm_id.0,
                event.alarm_label,
                event.fire_timestamp.to_rfc3339(),
                event.first_interaction_timestamp.map(|t| t.to_rfc3339()),
                event.dismiss_timestamp.to_rfc3339(),
                event.snooze_count,
                snooze_intervals_json,
                challenge_solves_json,
                event.total_dismissal_time.as_millis() as i64,
            ],
        )?;
        Ok(())
    }

    pub fn query_events(&self, filter: &HistoryFilter) -> Result<Vec<AlarmEvent>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let (start, end) = filter_to_date_range(filter);
        let mut stmt = conn.prepare(
            "SELECT id, alarm_id, alarm_label, fire_timestamp, first_interaction_timestamp, \
             dismiss_timestamp, snooze_count, snooze_intervals, challenge_solves, \
             total_dismissal_time_ms \
             FROM alarm_events WHERE fire_timestamp >= ?1 AND fire_timestamp <= ?2 \
             ORDER BY fire_timestamp DESC",
        )?;

        let events = stmt
            .query_map(params![start.to_rfc3339(), end.to_rfc3339()], row_to_event)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(events)
    }

    pub fn aggregate_metrics(
        &self,
        filter: &HistoryFilter,
    ) -> Result<AggregateMetrics, rusqlite::Error> {
        let events = self.query_events(filter)?;
        Ok(compute_aggregate_metrics(&events))
    }

    pub fn delete_events_before(&self, cutoff: &DateTime<Utc>) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "DELETE FROM alarm_events WHERE fire_timestamp < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn delete_all_events(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute("DELETE FROM alarm_events", [])?;
        Ok(())
    }

    // --- Settings ---

    pub fn load_settings(&self) -> Result<AppSettings, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let result = conn.query_row(
            "SELECT theme, locale, default_snooze_count, default_snooze_interval_minutes \
             FROM settings WHERE id = 1",
            [],
            |row| {
                let theme_json: String = row.get(0)?;
                let locale_json: String = row.get(1)?;
                Ok(AppSettings {
                    theme: serde_json::from_str(&theme_json).unwrap_or_default(),
                    locale: serde_json::from_str(&locale_json).unwrap_or_default(),
                    default_snooze_count: row.get(2)?,
                    default_snooze_interval_minutes: row.get(3)?,
                })
            },
        );

        match result {
            Ok(settings) => Ok(settings),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(AppSettings::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let theme_json = serde_json::to_string(&settings.theme)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let locale_json = serde_json::to_string(&settings.locale)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (id, theme, locale, default_snooze_count, \
             default_snooze_interval_minutes) VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                theme_json,
                locale_json,
                settings.default_snooze_count,
                settings.default_snooze_interval_minutes,
            ],
        )?;
        Ok(())
    }
}

// --- History helpers ---

fn row_to_event(row: &rusqlite::Row) -> Result<AlarmEvent, rusqlite::Error> {
    let id: String = row.get(0)?;
    let alarm_id: String = row.get(1)?;
    let alarm_label: String = row.get(2)?;
    let fire_ts_str: String = row.get(3)?;
    let first_interaction_str: Option<String> = row.get(4)?;
    let dismiss_ts_str: String = row.get(5)?;
    let snooze_count: u8 = row.get(6)?;
    let snooze_intervals_json: String = row.get(7)?;
    let challenge_solves_json: String = row.get(8)?;
    let total_ms: i64 = row.get(9)?;

    let fire_timestamp = DateTime::parse_from_rfc3339(&fire_ts_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);
    let first_interaction_timestamp = first_interaction_str
        .as_deref()
        .map(|s| DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc)))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let dismiss_timestamp = DateTime::parse_from_rfc3339(&dismiss_ts_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);

    let snooze_intervals_minutes: Vec<u8> =
        serde_json::from_str(&snooze_intervals_json).unwrap_or_default();
    let challenge_solves: Vec<crate::models::ChallengeSolve> =
        serde_json::from_str(&challenge_solves_json).unwrap_or_default();

    Ok(AlarmEvent {
        id,
        alarm_id: AlarmId(alarm_id),
        alarm_label,
        fire_timestamp,
        first_interaction_timestamp,
        dismiss_timestamp,
        snooze_count,
        snooze_intervals_minutes,
        challenge_solves,
        total_dismissal_time: Duration::from_millis(total_ms as u64),
    })
}

fn filter_to_date_range(filter: &HistoryFilter) -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now();
    let safe_midnight = |date: NaiveDate| -> DateTime<Utc> {
        date.and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc())
            .unwrap_or(now)
    };
    match filter {
        HistoryFilter::ThisWeek => {
            let weekday_num = now.weekday().num_days_from_monday();
            let start = (now - chrono::Duration::days(weekday_num as i64)).date_naive();
            (safe_midnight(start), now)
        }
        HistoryFilter::LastWeek => {
            let weekday_num = now.weekday().num_days_from_monday();
            let this_week_start = (now - chrono::Duration::days(weekday_num as i64)).date_naive();
            let this_week_start = safe_midnight(this_week_start);
            let last_week_start = this_week_start - chrono::Duration::days(7);
            (last_week_start, this_week_start)
        }
        HistoryFilter::ThisMonth => {
            let start =
                NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap_or(now.date_naive());
            (safe_midnight(start), now)
        }
        HistoryFilter::LastMonth => {
            let this_month_start =
                NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap_or(now.date_naive());
            let this_month_start = safe_midnight(this_month_start);
            let (prev_year, prev_month) = if now.month() == 1 {
                (now.year() - 1, 12)
            } else {
                (now.year(), now.month() - 1)
            };
            let last_month_start =
                NaiveDate::from_ymd_opt(prev_year, prev_month, 1).unwrap_or(now.date_naive());
            let last_month_start = safe_midnight(last_month_start);
            (last_month_start, this_month_start)
        }
        HistoryFilter::ThisYear => {
            let start = NaiveDate::from_ymd_opt(now.year(), 1, 1).unwrap_or(now.date_naive());
            (safe_midnight(start), now)
        }
        HistoryFilter::LastYear => {
            let this_year_start =
                NaiveDate::from_ymd_opt(now.year(), 1, 1).unwrap_or(now.date_naive());
            let this_year_start = safe_midnight(this_year_start);
            let last_year_start =
                NaiveDate::from_ymd_opt(now.year() - 1, 1, 1).unwrap_or(now.date_naive());
            let last_year_start = safe_midnight(last_year_start);
            (last_year_start, this_year_start)
        }
        HistoryFilter::LastDays(days) => {
            let start = now - chrono::Duration::days(*days as i64);
            (start, now)
        }
    }
}

fn compute_aggregate_metrics(events: &[AlarmEvent]) -> AggregateMetrics {
    use std::collections::BTreeMap;

    if events.is_empty() {
        return AggregateMetrics::default();
    }

    let total = events.len() as f64;
    let avg_snooze = events.iter().map(|e| e.snooze_count as f64).sum::<f64>() / total;

    let avg_reaction = avg_durations(
        events
            .iter()
            .filter_map(|e| e.reaction_time())
            .collect::<Vec<_>>()
            .as_slice(),
    );

    let all_intervals: Vec<u8> = events
        .iter()
        .flat_map(|e| e.snooze_intervals_minutes.iter().copied())
        .collect();
    let avg_snooze_interval = if all_intervals.is_empty() {
        0.0
    } else {
        all_intervals.iter().map(|m| *m as f64).sum::<f64>() / all_intervals.len() as f64
    };

    let avg_total_snoozed = avg_durations(
        events
            .iter()
            .map(|e| e.total_snoozed())
            .collect::<Vec<_>>()
            .as_slice(),
    );

    let avg_total_challenge = avg_durations(
        events
            .iter()
            .map(|e| e.total_challenge_time())
            .collect::<Vec<_>>()
            .as_slice(),
    );

    let avg_dismissal = avg_durations(
        events
            .iter()
            .map(|e| e.total_dismissal_time)
            .collect::<Vec<_>>()
            .as_slice(),
    );

    // Per-(type, difficulty) average solve time. Difficulty-less challenges
    // (shake/steps/scan/hold) always bucket under a single difficulty so they
    // aggregate into one row per type instead of splitting by a difficulty
    // value that has no meaning for them.
    fn challenge_has_difficulty(t: ChallengeType) -> bool {
        matches!(
            t,
            ChallengeType::Math | ChallengeType::MemoryGame | ChallengeType::Typing
        )
    }
    let mut solve_buckets: BTreeMap<(ChallengeType, Difficulty), Vec<Duration>> = BTreeMap::new();
    for event in events {
        for solve in &event.challenge_solves {
            let diff = if challenge_has_difficulty(solve.challenge_type) {
                solve.difficulty
            } else {
                Difficulty::Easy
            };
            solve_buckets
                .entry((solve.challenge_type, diff))
                .or_default()
                .push(solve.duration);
        }
    }
    let avg_solve_per_challenge: BTreeMap<_, _> = solve_buckets
        .into_iter()
        .map(|(k, v)| (k, avg_durations(&v)))
        .collect();

    AggregateMetrics {
        total_events: events.len() as u32,
        avg_snooze_count: avg_snooze,
        avg_snooze_interval_minutes: avg_snooze_interval,
        avg_total_snoozed,
        avg_reaction_time: avg_reaction,
        avg_total_challenge_time: avg_total_challenge,
        avg_total_dismissal_time: avg_dismissal,
        avg_solve_per_challenge,
    }
}

fn avg_durations(durations: &[Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }
    let total_ms: u128 = durations.iter().map(|d| d.as_millis()).sum();
    Duration::from_millis((total_ms / durations.len() as u128) as u64)
}
