//! Touch feedback, in one place so one setting turns it all off. No-ops off Android
//! and when no vibrator is present.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// A short tick.
pub fn tick(_ms: u64) {
    #[cfg(target_os = "android")]
    if ENABLED.load(Ordering::Relaxed) {
        mobile_sentinel::haptics::vibrate(std::time::Duration::from_millis(_ms));
    }
}

/// A pattern of alternating wait and vibrate times.
pub fn pattern(_pattern: &[i64]) {
    #[cfg(target_os = "android")]
    if ENABLED.load(Ordering::Relaxed) {
        let pattern: Vec<u64> = _pattern.iter().map(|ms| *ms as u64).collect();
        mobile_sentinel::haptics::vibrate_pattern(&pattern);
    }
}
