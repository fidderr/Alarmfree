//! SQLite schema definitions for AlarmFree.

/// SQL to create the alarms table. The full `AlarmData` is stored as a
/// JSON blob so schema changes only require bumping the migration
/// version, not adding columns.
pub const CREATE_ALARMS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS alarms (
    id TEXT PRIMARY KEY,
    data TEXT NOT NULL
)
"#;

/// SQL to create the alarm events (history) table.
pub const CREATE_ALARM_EVENTS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS alarm_events (
    id TEXT PRIMARY KEY,
    alarm_id TEXT NOT NULL,
    alarm_label TEXT NOT NULL,
    fire_timestamp TEXT NOT NULL,
    first_interaction_timestamp TEXT,
    dismiss_timestamp TEXT NOT NULL,
    snooze_count INTEGER NOT NULL DEFAULT 0,
    snooze_intervals TEXT NOT NULL DEFAULT '[]',
    challenge_solves TEXT NOT NULL DEFAULT '[]',
    total_dismissal_time_ms INTEGER NOT NULL
)
"#;

/// SQL to create indexes on alarm_events.
pub const CREATE_ALARM_EVENTS_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_alarm_events_fire_timestamp ON alarm_events(fire_timestamp);
CREATE INDEX IF NOT EXISTS idx_alarm_events_alarm_id ON alarm_events(alarm_id)
"#;

/// SQL to create the settings table (single-row pattern).
pub const CREATE_SETTINGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    theme TEXT NOT NULL,
    locale TEXT NOT NULL,
    default_snooze_count INTEGER NOT NULL DEFAULT 3,
    default_snooze_interval_minutes INTEGER NOT NULL DEFAULT 1
)
"#;
