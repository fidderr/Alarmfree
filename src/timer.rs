//! Shared async timer.
//!
//! One tokio timer instead of spawning an OS thread per sleep (the previous
//! pattern created ~10 threads/second on the scan screen alone).

use std::time::Duration;

/// Sleep for `d` without blocking a thread.
pub async fn sleep(d: Duration) {
    tokio::time::sleep(d).await;
}

/// Sleep for `ms` milliseconds without blocking a thread.
pub async fn sleep_ms(ms: u64) {
    sleep(Duration::from_millis(ms)).await;
}
