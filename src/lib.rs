//! AlarmFree library crate - exposes modules for integration testing.

pub mod alarm_ops;
pub mod custom_sounds;
pub mod db;
#[cfg(debug_assertions)]
pub mod debug_hooks;
pub mod feedback;
pub mod firing_state;
pub mod history_persistence;
pub mod i18n;
pub mod list_edit;
pub mod models;
pub mod ordering;
pub mod persistence;
pub mod platform;
pub mod ring_photos;
pub mod sanitize;
pub mod schedule;
pub mod text;
pub mod ui;

/// The phone starts the app here: the NativeActivity loads this library.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    slint::android::init(app).expect("start the UI backend");
    if let Err(e) = ui::run() {
        log::error!("[AlarmFree] the UI stopped: {e}");
    }
}
