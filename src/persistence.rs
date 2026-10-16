//! Alarm and settings persistence.
//!
//! Alarms live in SQLite as JSON-blob rows keyed by id (`AlarmData` is
//! both the UI signal type and the wire format on disk). On every save
//! we sync the matching ContextStore record via AlarmKit so the
//! universal SDK has a fresh view; on load we re-arm via AlarmKit.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mobile_sentinel::InstanceId;
use once_cell::sync::Lazy;

use crate::app::AlarmData;
use crate::db::Database;
use crate::models::AppSettings;

/// What happened when the database was opened at startup.
#[derive(Debug, Clone, PartialEq)]
pub enum DbRecovery {
    /// Opened normally.
    None,
    /// The file was unreadable/corrupt. It was moved aside to `backup` and a
    /// fresh database was created, so the user's old alarms are not lost
    /// silently.
    Recovered { backup: PathBuf },
    /// Even a fresh file could not be created; running on a throw-away
    /// in-memory database. Nothing the user does will persist.
    Volatile,
}

struct Opened {
    db: Arc<Database>,
    recovery: DbRecovery,
}

static DB: Lazy<Opened> = Lazy::new(|| {
    let path = mobile_sentinel::app_files_dir().join("alarmfree.db");
    let (db, recovery) = open_with_recovery(&path);
    Opened {
        db: Arc::new(db),
        recovery,
    }
});

pub fn database() -> &'static Arc<Database> {
    &DB.db
}

/// How the database was opened at startup, so the UI can tell the user when
/// their data was reset or is not being saved.
pub fn db_recovery() -> &'static DbRecovery {
    &DB.recovery
}

fn open_healthy(path: &Path) -> Result<Database, String> {
    let db = Database::open(path).map_err(|e| e.to_string())?;
    if db.quick_check() {
        Ok(db)
    } else {
        Err("integrity check failed".to_string())
    }
}

/// Move a bad database (and its WAL/SHM sidecars) out of the way, keeping it
/// for manual recovery. Returns the path the main file now lives at.
fn quarantine(path: &Path) -> Option<PathBuf> {
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let name = path.file_name()?.to_string_lossy().into_owned();
    let backup = path.with_file_name(format!("{name}.corrupt-{ts}"));
    std::fs::rename(path, &backup).ok()?;
    for ext in ["-wal", "-shm"] {
        let side = path.with_file_name(format!("{name}{ext}"));
        let _ = std::fs::rename(
            &side,
            path.with_file_name(format!("{name}.corrupt-{ts}{ext}")),
        );
    }
    Some(backup)
}

/// Open the database at `path`. Never silently discards data: an unreadable
/// file is retried once, then quarantined to `<name>.corrupt-<timestamp>` and
/// replaced by a fresh database, which is reported via [`DbRecovery`].
pub(crate) fn open_with_recovery(path: &Path) -> (Database, DbRecovery) {
    if !path.exists() {
        // First run: nothing to recover.
        if let Ok(db) = open_healthy(path) {
            return (db, DbRecovery::None);
        }
    } else {
        for attempt in 1..=2 {
            match open_healthy(path) {
                Ok(db) => return (db, DbRecovery::None),
                Err(e) => log::error!("[AlarmFree] database open attempt {attempt} failed: {e}"),
            }
        }
    }

    let backup = if path.exists() {
        quarantine(path)
    } else {
        None
    };
    match open_healthy(path) {
        Ok(db) => match backup {
            Some(backup) => {
                log::error!(
                    "[AlarmFree] database reset; old file kept at {}",
                    backup.display()
                );
                (db, DbRecovery::Recovered { backup })
            }
            // Fresh create succeeded without having to move anything.
            None => (db, DbRecovery::None),
        },
        Err(e) => {
            log::error!("[AlarmFree] cannot create a database ({e}); running in memory");
            let db = Database::open_in_memory()
                .unwrap_or_else(|e| panic!("in-memory SQLite must always open: {e}"));
            (db, DbRecovery::Volatile)
        }
    }
}

// ---------------------------------------------------------------------------
// Public API used by screens
// ---------------------------------------------------------------------------

/// Save all alarms to the database (full replace strategy). Also pushes
/// every enabled alarm through AlarmKit so the universal SDK's
/// ContextStore + exact-alarm registration stays in sync.
pub fn save_alarms(alarms: &[AlarmData]) {
    let db = DB.db.as_ref();
    let existing = db.list_alarms().unwrap_or_default();
    let existing_ids: HashSet<String> = existing.iter().map(|a| a.id.clone()).collect();
    let new_ids: HashSet<String> = alarms.iter().map(|a| a.id.clone()).collect();

    // Delete alarms that are no longer in the list.
    for id in existing_ids.difference(&new_ids) {
        if let Err(e) = db.delete_alarm(id) {
            eprintln!("[AlarmFree] Failed to delete alarm {}: {}", id, e);
        }
        if let Some(kit) = crate::platform::alarm_kit_optional() {
            let _ = kit.delete(&InstanceId::new(id.clone()));
        }
    }

    // Insert / update each alarm + push through AlarmKit.
    for alarm in alarms {
        if let Err(e) = db.upsert_alarm(alarm) {
            eprintln!("[AlarmFree] Failed to upsert alarm {}: {}", alarm.id, e);
        }

        if let Some(kit) = crate::platform::alarm_kit_optional() {
            let id = InstanceId::new(alarm.id.clone());
            if alarm.enabled {
                let spec = crate::platform::alarm_data_to_spec(alarm);
                let res = if kit.get(&id).is_ok() {
                    kit.update(&id, spec)
                } else {
                    kit.create_with_id(id, spec).map(|_| ())
                };
                if let Err(e) = res {
                    eprintln!("[AlarmFree] AlarmKit save failed for {}: {:?}", alarm.id, e);
                }
            } else {
                // Disabled in UI: drop from AlarmKit so no exact alarm is armed.
                let _ = kit.delete(&id);
            }
        }
    }
}

/// Load all alarms from the database.
pub fn load_alarms() -> Vec<AlarmData> {
    DB.db.list_alarms().unwrap_or_default()
}

pub fn save_settings(settings: &AppSettings) {
    if let Err(e) = DB.db.save_settings(settings) {
        eprintln!("[AlarmFree] Failed to save settings: {}", e);
    }
}

pub fn load_settings() -> AppSettings {
    DB.db.load_settings().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn alarm(id: &str) -> AlarmData {
        AlarmData {
            id: id.into(),
            time_hour: 7,
            time_minute: 0,
            label: "x".into(),
            days: [false; 7],
            enabled: true,
            sound: "happy".into(),
            sound_mode: 0,
            challenges: vec![],
            snooze_count: 1,
            snooze_interval: 5,
        }
    }

    #[test]
    fn fresh_path_opens_without_recovery() {
        let dir = TempDir::new().unwrap();
        let (_db, rec) = open_with_recovery(&dir.path().join("a.db"));
        assert_eq!(rec, DbRecovery::None);
    }

    #[test]
    fn healthy_db_keeps_its_data() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.db");
        {
            let (db, _) = open_with_recovery(&path);
            db.upsert_alarm(&alarm("one")).unwrap();
        }
        let (db, rec) = open_with_recovery(&path);
        assert_eq!(rec, DbRecovery::None);
        assert_eq!(db.list_alarms().unwrap().len(), 1);
    }

    #[test]
    fn corrupt_file_is_quarantined_not_silently_dropped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a.db");
        std::fs::write(
            &path,
            b"this is definitely not a sqlite database, just garbage bytes",
        )
        .unwrap();

        let (db, rec) = open_with_recovery(&path);
        let backup = match rec {
            DbRecovery::Recovered { backup } => backup,
            other => panic!("expected Recovered, got {other:?}"),
        };
        assert!(backup.exists(), "corrupt file must be kept for recovery");
        assert!(backup.to_string_lossy().contains(".corrupt-"));
        // Fresh DB is usable and persists.
        db.upsert_alarm(&alarm("two")).unwrap();
        assert_eq!(db.list_alarms().unwrap().len(), 1);
    }
}
