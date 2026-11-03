//! Shared async timer.

use std::time::Duration;

/// Sleep for `d` without blocking a thread.
pub async fn sleep(d: Duration) {
    tokio::time::sleep(d).await;
}

/// Sleep for `ms` milliseconds without blocking a thread.
pub async fn sleep_ms(ms: u64) {
    sleep(Duration::from_millis(ms)).await;
}
