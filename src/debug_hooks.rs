//! What the test scenarios (`devtest/`) drive the app with. Only in a debug build and the
//! release built for testing (`RELEASE_TEST`), never in one for users.
//!
//! The harness writes `<files>/debug_action.txt`, and the file goes once it was seen:
//!
//!   `snooze:<id>`, `dismiss:<id>`: the same calls as the screens make.
//!   `js:<script>`: run in the page, the result is left in `debug_script_result.txt`.
//!     It is how the scenarios press buttons and read the screen on any phone size.
//!   `hang_ui`: freeze the UI thread, to exercise the guardian.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use mobile_sentinel::InstanceId;

use crate::platform::alarm_kit_optional;

static HANG_UI: AtomicBool = AtomicBool::new(false);
static SCRIPT: Mutex<Option<String>> = Mutex::new(None);

/// Watch for what the harness writes.
pub fn start() {
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
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
        } else if let Some(script) = contents.strip_prefix("js:") {
            *SCRIPT.lock().unwrap_or_else(PoisonError::into_inner) = Some(script.to_owned());
        } else if contents == "hang_ui" {
            HANG_UI.store(true, Ordering::SeqCst);
        }
        let _ = std::fs::remove_file(&path);
    });
}

/// Run what the harness sent in the page.
pub async fn script_loop() {
    loop {
        crate::timer::sleep_ms(200).await;
        let script = SCRIPT.lock().unwrap_or_else(PoisonError::into_inner).take();
        let Some(script) = script else {
            continue;
        };
        let mut eval = dioxus::document::eval(&format!(
            "try {{ dioxus.send(String((function () {{ {script} }})())); }} \
             catch (e) {{ dioxus.send('error: ' + e); }}"
        ));
        let result = eval.recv::<String>().await.unwrap_or_default();
        let path = mobile_sentinel::app_files_dir().join("debug_script_result.txt");
        let _ = std::fs::write(path, format!("={result}"));
    }
}

/// Freeze the calling thread, the UI's, if `hang_ui` asked for it.
pub fn block_if_requested() {
    if HANG_UI.swap(false, Ordering::SeqCst) {
        log::warn!("[AlarmFree] DEBUG blocking the UI thread");
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
