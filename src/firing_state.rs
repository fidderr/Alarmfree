//! Firing-state subscription for the MAIN process UI.
//!
//! The Recipe state machine writes `state: firing` to the ContextStore;
//! MAIN observes the change by polling. The cross-process read is a
//! plain `current_session()` call against the shared on-disk store.
//!
//! Polling at 500 ms is battery-cheap and only has to switch the UI
//! to the firing screen — the user already sees and hears the alarm
//! via the foreground service.

use futures_util::stream::{Stream, StreamExt};

/// Subscribe to firing-state changes. The stream yields the currently
/// firing instance id (or `None` when nothing is firing).
pub fn init_notifier() -> impl Stream<Item = Option<String>> + Unpin + Send {
    use futures_channel::mpsc;
    let (tx, rx) = mpsc::unbounded::<Option<String>>();

    std::thread::spawn(move || {
        let mut last: Option<String> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let current = peek_firing_alarm();
            if current != last {
                if tx.unbounded_send(current.clone()).is_err() {
                    break;
                }
                last = current;
            }
        }
    });

    rx.boxed()
}

/// Peek the currently-firing instance id, if any. Reads the ContextStore
/// directly from disk and returns the first record in `Firing` state.
pub fn peek_firing_alarm() -> Option<String> {
    let kit = crate::platform::alarm_kit_optional()?;
    match kit.current_session() {
        Ok(Some(session)) => {
            #[cfg(target_os = "android")]
            log::trace!(
                "[AlarmFree] peek_firing_alarm: firing instance {}",
                session.instance_id.0
            );
            Some(session.instance_id.0)
        }
        Ok(None) => None,
        Err(_e) => {
            #[cfg(target_os = "android")]
            log::warn!("[AlarmFree] peek_firing_alarm: error: {:?}", _e);
            None
        }
    }
}
