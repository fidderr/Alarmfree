//! Firing-state observation for the MAIN process UI.
//!
//! The Recipe state machine writes `state: firing` to the ContextStore; MAIN
//! observes the change by polling `peek_firing_alarm` from a single async task
//! in `app.rs` (no dedicated thread). The poll only has to switch the UI to
//! the firing screen — the user already sees and hears the alarm via the
//! foreground service.

/// How often the UI checks whether an alarm is firing.
pub const POLL_MS: u64 = 500;

/// Peek the currently-firing instance id, if any. Reads the ContextStore
/// directly from disk and returns the first record in `Firing` state.
pub fn peek_firing_alarm() -> Option<String> {
    let kit = crate::platform::alarm_kit_optional()?;
    match kit.current_session() {
        Ok(Some(session)) => {
            log::trace!(
                "[AlarmFree] peek_firing_alarm: firing instance {}",
                session.instance_id.0
            );
            Some(session.instance_id.0)
        }
        Ok(None) => None,
        Err(e) => {
            log::warn!("[AlarmFree] peek_firing_alarm: error: {:?}", e);
            None
        }
    }
}
