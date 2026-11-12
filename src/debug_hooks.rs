//! What the test scenarios (`devtest/`) drive the app with. Only in a debug build and the
//! release built for testing (`RELEASE_TEST`), never in one for users.
//!
//! The harness writes `<files>/debug_action.txt`, and the file goes once it was seen:
//!
//!   `snooze:<id>`, `dismiss:<id>`: the same calls as the screens make.
//!   `hang_ui`: freeze the UI thread, to exercise the guardian.
//!
//! `<files>/debug_<name>.txt` holds a number to use instead of a wait that takes too long
//! for a test, see [`crate::platform::shortened`].

use std::sync::atomic::{AtomicBool, Ordering};

use mobile_sentinel::InstanceId;

use crate::platform::alarm_kit_optional;

static HANG_UI: AtomicBool = AtomicBool::new(false);

/// Watch for what the harness writes.
pub fn start() {
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_millis(30));
        let path = mobile_sentinel::app_files_dir().join("debug_action.txt");
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let contents = contents.trim();
        let Some(kit) = alarm_kit_optional() else {
            continue;
        };
        if let Some(id) = contents.strip_prefix("snooze:") {
            if let Err(e) = kit.snooze(&InstanceId::new(id.to_owned())) {
                log::warn!("[AlarmFree] DEBUG snooze({id}) failed: {e:?}");
            }
        } else if let Some(id) = contents.strip_prefix("dismiss:") {
            if let Err(e) = kit.dismiss(&InstanceId::new(id.to_owned())) {
                log::warn!("[AlarmFree] DEBUG dismiss({id}) failed: {e:?}");
            }
        } else if contents == "hang_ui" {
            HANG_UI.store(true, Ordering::SeqCst);
        }
        let _ = std::fs::remove_file(&path);
    });
}

/// The number the harness left for `name`, if any.
pub fn test_value(name: &str) -> Option<u32> {
    let path = mobile_sentinel::app_files_dir().join(format!("debug_{name}.txt"));
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Freeze the calling thread, the UI's, if `hang_ui` asked for it.
pub fn block_if_requested() {
    if HANG_UI.swap(false, Ordering::SeqCst) {
        log::warn!("[AlarmFree] DEBUG blocking the UI thread");
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
