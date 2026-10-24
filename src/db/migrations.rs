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
const CURRENT_VERSION: u32 = 2;

/// Run all pending migrations to bring the database up to `CURRENT_VERSION`.
/// Safe to call on every app launch — it's a no-op if already at the latest version.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version = get_user_version(conn)?;

    if version < 1 {
        // A fresh database is created at the current layout.
        create_initial_schema(conn)?;
    } else if version < 2 {
        migrate_v1_to_v2(conn)?;
    }

    set_user_version(conn, CURRENT_VERSION)?;
    Ok(())
}

/// v1 → v2: the minimum alarm volume setting (existing users get the default).
fn migrate_v1_to_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE settings ADD COLUMN min_volume_percent INTEGER NOT NULL DEFAULT 50",
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn min_volume(conn: &Connection) -> u32 {
        conn.query_row(
            "SELECT min_volume_percent FROM settings WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn a_fresh_database_gets_the_current_layout() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO settings (id, theme, locale) VALUES (1, '\"Dark\"', '\"English\"')",
            [],
        )
        .unwrap();
        assert_eq!(min_volume(&conn), 50);
        assert_eq!(get_user_version(&conn).unwrap(), CURRENT_VERSION);
    }

    #[test]
    fn a_v1_database_keeps_its_settings_and_gets_the_default_volume() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                theme TEXT NOT NULL,
                locale TEXT NOT NULL,
                default_snooze_count INTEGER NOT NULL DEFAULT 3,
                default_snooze_interval_minutes INTEGER NOT NULL DEFAULT 1
             );
             INSERT INTO settings (id, theme, locale, default_snooze_count)
                VALUES (1, '\"Light\"', '\"Dutch\"', 7);
             PRAGMA user_version = 1;",
        )
        .unwrap();

        run_migrations(&conn).unwrap();

        assert_eq!(min_volume(&conn), 50);
        let snooze: u32 = conn
            .query_row("SELECT default_snooze_count FROM settings", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(snooze, 7, "existing settings survive");
        run_migrations(&conn).unwrap(); // idempotent
    }
}
