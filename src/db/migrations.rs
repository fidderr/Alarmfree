//! Schema versioning and migrations.
//! Uses SQLite's `PRAGMA user_version` to track which schema version is on disk.
//! On first launch, creates all tables at the current version. An existing database
//! is brought up to date and keeps what is in it.
//!
//! To add a future migration:
//! 1. Increment `CURRENT_VERSION`
//! 2. Add an `if version < N { migrate_vN_minus_1_to_vN(conn)?; }` block
//! 3. Implement the migration function

use rusqlite::Connection;

use crate::db::schema;

/// Current schema version.
const CURRENT_VERSION: u32 = 5;

/// Run all pending migrations to bring the database up to `CURRENT_VERSION`.
/// Safe to call on every app launch — it's a no-op if already at the latest version.
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version = get_user_version(conn)?;
    if version == CURRENT_VERSION {
        return Ok(());
    }

    // All or nothing: a kill halfway must not leave a half-migrated database that
    // the next launch would try to migrate again.
    let tx = conn.unchecked_transaction()?;
    if version < 1 {
        // A fresh database is created at the current layout.
        create_initial_schema(&tx)?;
    } else {
        if version < 2 {
            migrate_v1_to_v2(&tx)?;
        }
        if version < 3 {
            migrate_v2_to_v3(&tx)?;
        }
        if version < 4 {
            migrate_v3_to_v4(&tx)?;
        }
        if version < 5 {
            migrate_v4_to_v5(&tx)?;
        }
    }
    set_user_version(&tx, CURRENT_VERSION)?;
    tx.commit()
}

/// v1 → v2: the minimum alarm volume setting (existing users get the default).
fn migrate_v1_to_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE settings ADD COLUMN min_volume_percent INTEGER NOT NULL DEFAULT 50",
    )
}

/// v2 → v3: how an alarm ended, and whether the user has been told. Existing rows
/// are alarms the user solved.
fn migrate_v2_to_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE alarm_events ADD COLUMN outcome TEXT NOT NULL DEFAULT 'solved';
         ALTER TABLE alarm_events ADD COLUMN seen INTEGER NOT NULL DEFAULT 1;",
    )
}

/// v3 → v4: everything the settings got since, as one JSON column.
fn migrate_v3_to_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("ALTER TABLE settings ADD COLUMN prefs TEXT NOT NULL DEFAULT '{}'")
}

/// v4 → v5: what a new alarm starts from moved to the prefs long ago.
fn migrate_v4_to_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "ALTER TABLE settings DROP COLUMN default_snooze_count;
         ALTER TABLE settings DROP COLUMN default_snooze_interval_minutes;",
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
             CREATE TABLE alarm_events (id TEXT PRIMARY KEY);
             INSERT INTO alarm_events (id) VALUES ('old');
             PRAGMA user_version = 1;",
        )
        .unwrap();

        run_migrations(&conn).unwrap();

        assert_eq!(min_volume(&conn), 50);
        let theme: String = conn
            .query_row("SELECT theme FROM settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(theme, "\"Light\"", "existing settings survive");
        let (outcome, seen): (String, u32) = conn
            .query_row("SELECT outcome, seen FROM alarm_events", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(
            (outcome.as_str(), seen),
            ("solved", 1),
            "not all migrations ran, or history was reinterpreted"
        );
        assert_eq!(get_user_version(&conn).unwrap(), CURRENT_VERSION);
        run_migrations(&conn).unwrap(); // idempotent
    }

    #[test]
    fn a_failed_migration_changes_nothing() {
        let conn = Connection::open_in_memory().unwrap();
        // v2 without the table v3 alters, so the migration fails.
        conn.execute_batch(
            "CREATE TABLE settings (id INTEGER PRIMARY KEY);
             PRAGMA user_version = 2;",
        )
        .unwrap();

        assert!(run_migrations(&conn).is_err());

        assert_eq!(get_user_version(&conn).unwrap(), 2);
    }
}
