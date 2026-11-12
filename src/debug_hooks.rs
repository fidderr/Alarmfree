//! What the test scenarios (`devtest/`) drive the app with. Only in a debug build and the
//! release built for testing (`RELEASE_TEST`), never in one for users.
//!
//! The harness writes `<files>/debug_action.txt`, and the file goes once it was seen:
//!
//!   `snooze:<id>`, `dismiss:<id>`: the same calls as the screens make.
//!   `hang_ui`: freeze the UI thread, to exercise the guardian.
//!   `ui:<command>`: ask or press something in the Slint window. The answer is written to
//!   `<files>/debug_script_result.txt` (one line, or JSON), after the harness removed the
//!   old one. The commands:
//!     `ui:screen`: which screen, sheet and panel are open (`screen=0 home_sheet=None ...`;
//!       screens: 0 alarms, 1 editor, 2 history, 3 settings, 4 reliability, 5 theme).
//!     `ui:snapshot`: the main properties of the window as JSON: screen, the alarm rows
//!       (id, time, label, when, on, can_skip), sheets and panel titles and choices, the
//!       editor's time and rings text, the ring screen and the toast.
//!     `ui:press:<callback>:<arg>`: call a callback of the window as a tap would. Callbacks:
//!       toggle, open, menu, skip, delete-alarm (arg: alarm id); quick, go, hs-picked,
//!       pn-picked, st-row, ed-shift, ed-day-toggled, ed-preset, ed-hour, ed-minute
//!       (arg: number); create, pause-all, sort, back, hs-close, pn-done-tapped, ed-back,
//!       ed-save, ed-delete-confirmed, ed-discard-confirmed, ring-snooze, ring-turn-off,
//!       toast-undo. The answer is `OK` or `ERR ...`.
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
        #[cfg(debug_assertions)]
        if let Some(command) = contents.strip_prefix("ui:") {
            let _ = std::fs::remove_file(&path);
            crate::ui::debug_command(command.to_owned(), write_result);
            continue;
        }
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

#[cfg(debug_assertions)]
fn write_result(answer: String) {
    let dir = mobile_sentinel::app_files_dir();
    let tmp = dir.join("debug_script_result.tmp");
    if std::fs::write(&tmp, answer).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("debug_script_result.txt"));
    }
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
