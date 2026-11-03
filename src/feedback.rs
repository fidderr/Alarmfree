//! Touch feedback, in one place so one setting turns it all off. No-ops off Android
//! and when no vibrator is present.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// A short tick.
pub fn tick(ms: u64) {
    if ENABLED.load(Ordering::Relaxed) {
        mobile_sentinel::haptics::vibrate(std::time::Duration::from_millis(ms));
    }
}

/// A pattern of alternating wait and vibrate times.
pub fn pattern(pattern: &[i64]) {
    if ENABLED.load(Ordering::Relaxed) {
        let pattern: Vec<u64> = pattern.iter().map(|ms| *ms as u64).collect();
        mobile_sentinel::haptics::vibrate_pattern(&pattern);
    }
}
