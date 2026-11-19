//! The one place the app reads the current time. In a build for users it is the system clock. In
//! a debug build, tests and the devtest harness can put the app's clock elsewhere without moving
//! the phone's: a fixed moment (headless tests: no test depends on the weekday or on a minute
//! boundary) or an offset from the system clock (`ui:clock:` in `debug_hooks`).
//!
//! The phone's alarms fire on the system clock. A device scenario that waits for a ring moves
//! that one (`cmd alarm set-time`) and the app follows it; the app's own clock is for what the
//! screens show.

use chrono::{DateTime, Local, Utc};

#[cfg(debug_assertions)]
mod imp {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicI64, Ordering};

    static OFFSET_MS: AtomicI64 = AtomicI64::new(0);

    thread_local! {
        /// A moment this thread's clock stands at (unix ms), for a test.
        static FIXED_MS: Cell<Option<i64>> = const { Cell::new(None) };
    }

    pub fn now_ms() -> i64 {
        FIXED_MS
            .with(Cell::get)
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() + OFFSET_MS.load(Ordering::Relaxed))
    }
    pub fn set_fixed(ms: Option<i64>) {
        FIXED_MS.with(|f| f.set(ms));
    }
    pub fn set_offset(ms: i64) {
        OFFSET_MS.store(ms, Ordering::Relaxed);
    }
    pub fn offset() -> i64 {
        OFFSET_MS.load(Ordering::Relaxed)
    }
}

#[cfg(not(debug_assertions))]
mod imp {
    pub fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

/// Now, in the phone's time zone.
pub fn local_now() -> DateTime<Local> {
    DateTime::from_timestamp_millis(imp::now_ms())
        .unwrap_or_else(Utc::now)
        .with_timezone(&Local)
}

/// Now, in UTC.
pub fn utc_now() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(imp::now_ms()).unwrap_or_else(Utc::now)
}

/// Now, as unix milliseconds.
pub fn now_ms() -> i64 {
    imp::now_ms()
}

/// This thread's clock stands still at `ms` (unix ms); `None` lets it run again.
#[cfg(debug_assertions)]
pub fn set_fixed(ms: Option<i64>) {
    imp::set_fixed(ms);
}

/// The app's clock runs `ms` ahead of the system clock (negative: behind), on every thread.
#[cfg(debug_assertions)]
pub fn set_offset(ms: i64) {
    imp::set_offset(ms);
}

/// How far the app's clock is from the system clock, in ms.
#[cfg(debug_assertions)]
pub fn offset() -> i64 {
    imp::offset()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_clock_stands_still_and_lets_go() {
        set_fixed(Some(1_000_000_000_000));
        assert_eq!(now_ms(), 1_000_000_000_000);
        assert_eq!(utc_now().timestamp_millis(), 1_000_000_000_000);
        assert_eq!(local_now().timestamp_millis(), 1_000_000_000_000);
        set_fixed(None);
        assert!((now_ms() - chrono::Utc::now().timestamp_millis()).abs() < 5_000);
    }

    #[test]
    fn an_offset_moves_the_clock_for_every_thread() {
        // Own thread: the offset is global, and other tests only read a fixed moment or the real time.
        let before = chrono::Utc::now().timestamp_millis();
        set_offset(3_600_000);
        let during = std::thread::spawn(now_ms).join().unwrap();
        set_offset(0);
        assert!(during - before >= 3_600_000 - 100);
        assert!(offset() == 0);
    }
}
