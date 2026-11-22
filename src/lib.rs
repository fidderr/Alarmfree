//! AlarmFree library crate - exposes modules for integration testing.
// The debug snapshot (src/ui/debug.rs) is one long json! literal.
#![recursion_limit = "256"]

pub mod alarm_ops;
pub mod calendar;
pub mod challenges;
pub mod clock;
pub mod custom_sounds;
pub mod db;
#[cfg(debug_assertions)]
pub mod debug_hooks;
pub mod diag_log;
pub mod edit_lock;
pub mod feedback;
pub mod firing_state;
pub mod history_persistence;
pub mod history_view;
pub mod i18n;
pub mod list_edit;
pub mod models;
pub mod ordering;
pub mod persistence;
pub mod platform;
pub mod reliability;
pub mod report;
pub mod ring_photos;
pub mod sanitize;
pub mod schedule;
pub mod text;
pub mod ui;

/// The phone starts the app here: the NativeActivity loads this library.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    // mobile-sentinel's `foregrounding::on_back` handles Back, so Slint registers no
    // `OnBackInvokedCallback` of its own.
    let options = slint::android::InitOptions::new()
        .with_back_invoked_callback(slint::android::BackInvokedCallback::Disabled);
    // The devtest harness checks that a real touch reached the app (`input_seen` in `ui:ready`).
    #[cfg(all(debug_assertions, feature = "devtest"))]
    let options = options.with_event_listener(|event| {
        use slint::android::android_activity::{MainEvent, PollEvent};
        if matches!(event, PollEvent::Main(MainEvent::InputAvailable)) {
            ui::note_input();
        }
    });
    slint::android::init_with_options(app, options).expect("start the UI backend");
    if let Err(e) = ui::run() {
        log::error!("[AlarmFree] the UI stopped: {e}");
    }
}
