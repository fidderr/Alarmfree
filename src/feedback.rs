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

/// Timings and strengths of a haptic with strength (alternating wait and vibrate, 0..=255).
pub fn waveform(timings: &[u64], amplitudes: &[u8]) {
    #[cfg(test)]
    record(Haptic::Waveform(timings.to_vec(), amplitudes.to_vec()));
    if ENABLED.load(Ordering::Relaxed) {
        mobile_sentinel::haptics::vibrate_waveform(timings, amplitudes);
    }
}

pub const BEATEN_TIMINGS: [u64; 6] = [0, 70, 80, 70, 80, 260];
pub const BEATEN_AMPLITUDES: [u8; 6] = [0, 170, 0, 210, 0, 255];
pub const SOFT_TICK_TIMINGS: [u64; 2] = [0, 30];
pub const SOFT_TICK_AMPLITUDES: [u8; 2] = [0, 70];

/// A challenge is beaten: three rising pulses, the last one long and at full strength.
pub fn challenge_beaten() {
    waveform(&BEATEN_TIMINGS, &BEATEN_AMPLITUDES);
}

/// One counted repetition: a short, low pulse.
pub fn soft_tick() {
    waveform(&SOFT_TICK_TIMINGS, &SOFT_TICK_AMPLITUDES);
}

/// What the tests see of the waveforms requested.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub enum Haptic {
    Waveform(Vec<u64>, Vec<u8>),
}

#[cfg(test)]
thread_local! {
    static RECORDED: std::cell::RefCell<Vec<Haptic>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
fn record(haptic: Haptic) {
    RECORDED.with(|r| r.borrow_mut().push(haptic));
}

/// Take the waveforms requested since the last call.
#[cfg(test)]
pub fn take_recorded() -> Vec<Haptic> {
    RECORDED.with(|r| std::mem::take(&mut *r.borrow_mut()))
}
