//! Alarm and settings persistence.
//!
//! Alarms live in SQLite as JSON-blob rows keyed by id (`AlarmData` is
//! both the UI signal type and the wire format on disk). On every save
//! we sync the matching ContextStore record via AlarmKit so the
//! universal SDK has a fresh view; on load we re-arm via AlarmKit.

use std::collections::HashSet;
use std::sync::Arc;

use mobile_sentinel::InstanceId;
use once_cell::sync::Lazy;

use crate::app::AlarmData;
use crate::db::Database;
use crate::models::AppSettings;

static DB: Lazy<Arc<Database>> = Lazy::new(|| {
    let db_path = mobile_sentinel::app_files_dir().join("alarmfree.db");
    Arc::new(Database::open(&db_path).unwrap_or_else(|e| {
        eprintln!(
            "[AlarmFree] Failed to open database: {}, falling back to in-memory",
            e
        );
        Database::open_in_memory().expect("in-memory DB must succeed")
    }))
});

pub fn database() -> &'static Arc<Database> {
    &DB
}

// ---------------------------------------------------------------------------
// Public API used by screens
// ---------------------------------------------------------------------------

/// Save all alarms to the database (full replace strategy). Also pushes
/// every enabled alarm through AlarmKit so the universal SDK's
/// ContextStore + exact-alarm registration stays in sync.
pub fn save_alarms(alarms: &[AlarmData]) {
    let db = &*DB;
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
    DB.list_alarms().unwrap_or_default()
}

pub fn save_settings(settings: &AppSettings) {
    if let Err(e) = DB.save_settings(settings) {
        eprintln!("[AlarmFree] Failed to save settings: {}", e);
    }
}

pub fn load_settings() -> AppSettings {
    DB.load_settings().unwrap_or_default()
}
