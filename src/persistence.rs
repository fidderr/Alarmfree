//! Alarm and settings persistence.
//!
//! Alarms live in SQLite as JSON-blob rows keyed by id (`AlarmData` is
//! both the UI signal type and the wire format on disk). A save tells
//! AlarmKit about exactly the alarms that changed; startup re-arms them all.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::sync::LazyLock;

use crate::app::AlarmData;
use crate::db::Database;
use crate::models::{AppSettings, Locale};

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

static DB: LazyLock<Opened> = LazyLock::new(|| {
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

/// What saving `alarms` over `existing` changes: the ids that are gone, and the
/// alarms that are new or different. Everything else is left alone, so saving
/// one alarm can't disturb another that is armed, snoozed or ringing.
fn changes<'a>(
    existing: &'a [AlarmData],
    alarms: &'a [AlarmData],
) -> (Vec<&'a str>, Vec<&'a AlarmData>) {
    let removed = existing
        .iter()
        .filter(|old| !alarms.iter().any(|a| a.id == old.id))
        .map(|old| old.id.as_str())
        .collect();
    let changed = alarms
        .iter()
        .filter(|a| existing.iter().find(|old| old.id == a.id) != Some(*a))
        .collect();
    (removed, changed)
}

/// Save the alarm list and bring AlarmKit in line with whatever changed.
pub fn save_alarms(alarms: &[AlarmData]) {
    let db = DB.db.as_ref();
    let existing = db.list_alarms().unwrap_or_default();
    let (removed, changed) = changes(&existing, alarms);

    for id in removed {
        if let Err(e) = db.delete_alarm(id) {
            log::error!("[AlarmFree] Failed to delete alarm {id}: {e}");
            note_save_failure();
        }
        crate::platform::forget_alarm(id);
    }
    for alarm in changed {
        if let Err(e) = db.upsert_alarm(alarm) {
            log::error!("[AlarmFree] Failed to upsert alarm {}: {e}", alarm.id);
            note_save_failure();
        }
        crate::platform::apply_alarm(alarm);
    }
    crate::platform::refresh_reminders();
}

/// Load all alarms from the database.
pub fn load_alarms() -> Vec<AlarmData> {
    DB.db.list_alarms().unwrap_or_default()
}

pub fn save_settings(settings: &AppSettings) {
    if let Err(e) = DB.db.save_settings(settings) {
        log::error!("[AlarmFree] Failed to save settings: {e}");
    }
}

pub fn load_settings() -> AppSettings {
    match DB.db.load_settings() {
        Ok(Some(mut settings)) => {
            if settings.prefs.language_follows_phone {
                if let Some(locale) =
                    Locale::from_language_code(&mobile_sentinel::device_language())
                {
                    settings.locale = locale;
                }
            }
            settings
        }
        // Nothing chosen yet: start in the phone's language.
        Ok(None) => AppSettings {
            locale: Locale::from_language_code(&mobile_sentinel::device_language())
                .unwrap_or_default(),
            ..AppSettings::default()
        },
        Err(e) => {
            log::error!("[AlarmFree] Settings could not be read: {e}");
            AppSettings::default()
        }
    }
}

/// How many saved alarms cannot be read.
pub fn unreadable_alarms() -> usize {
    DB.db.list_alarms_reporting().map_or(0, |(_, n)| n)
}

/// Set when something could not be saved; see [`take_save_failure`].
static SAVE_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether something could not be saved since the last call, for telling the user.
pub fn take_save_failure() -> bool {
    SAVE_FAILED.swap(false, std::sync::atomic::Ordering::Relaxed)
}

fn note_save_failure() {
    SAVE_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
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
            date: None,
            skip_until_ms: None,
            fade_in_secs: 0,
            ring_limit_secs: None,
            order: 0,
            enabled: true,
            sound: "happy".into(),
            sound_mode: 0,
            challenges: vec![],
            snooze_count: 1,
            snooze_interval: 5,
            note: String::new(),
            vibration: 0,
            volume_percent: None,
            random_order: false,
            wake_check_minutes: 0,
            max_brightness: false,
            sound_delay_secs: 0,
            pause_until_ms: None,
            delete_after_ring: false,
            hidden: false,
        }
    }

    #[test]
    fn a_save_only_touches_what_changed() {
        let same = alarm("same");
        let gone = alarm("gone");
        let mut edited = alarm("edited");
        let existing = vec![same.clone(), gone, edited.clone()];
        edited.time_hour = 9;
        let added = alarm("added");
        let saved = vec![same, edited.clone(), added.clone()];

        let (removed, changed) = changes(&existing, &saved);

        assert_eq!(removed, ["gone"]);
        assert_eq!(changed, [&edited, &added]);
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
