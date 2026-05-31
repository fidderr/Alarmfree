//! Schema versioning and migrations.
//! Uses SQLite's `PRAGMA user_version` to track which schema version is on disk.
//! On first launch, creates all tables at the current version.
//!
//! No legacy data is kept — this is a single-user dev app. When the schema
//! changes incompatibly, we drop the affected tables and recreate.
//!
//! To add a future migration:
//! 1. Increment `CURRENT_VERSION`
//! 2. Add an `if version < N { migrate_vN_minus_1_to_vN(conn)?; }` block
//! 3. Implement the migration function

use rusqlite::Connection;

use crate::db::schema;

/// Current schema version.
const CURRENT_VERSION: u32 = 1;

/// Run all pending migrations to bring the database up to `CURRENT_VERSION`.
/// Safe to call on every app launch — it's a no-op if already at the latest version.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version = get_user_version(conn)?;

    if version < 1 {
        create_initial_schema(conn)?;
    }

    set_user_version(conn, CURRENT_VERSION)?;
    Ok(())
}

fn get_user_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn set_user_version(conn: &Connection, version: u32) -> Result<(), rusqlite::Error> {
    conn.execute_batch(&format!("PRAGMA user_version = {}", version))
}

/// v0 → v1: Create all tables from scratch.
fn create_initial_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(schema::CREATE_ALARMS_TABLE)?;
    conn.execute_batch(schema::CREATE_ALARM_EVENTS_TABLE)?;
    conn.execute_batch(schema::CREATE_ALARM_EVENTS_INDEXES)?;
    conn.execute_batch(schema::CREATE_SETTINGS_TABLE)?;
    Ok(())
}
