//! Firing-state observation for the MAIN process UI.
//!
//! The Recipe state machine writes `state: firing` to the ContextStore; MAIN
//! observes the change by polling from a single async task in `app.rs`, which
//! only re-reads the store when its directory changed (see [`FiringWatch`]).
//! No dedicated thread. The poll only has to switch the UI to
//! the firing screen — the user already sees and hears the alarm via the
//! foreground service.

/// How often the UI checks whether an alarm is firing.
pub const POLL_MS: u64 = 500;

/// Full re-read at least this often even when the change marker is unchanged
/// (guards against filesystems with coarse mtimes). 20 × 500 ms = 10 s.
const FORCE_REREAD_EVERY: u32 = 20;

/// Decides when the poll actually needs to re-read the alarm store.
///
/// Each tick costs one `stat` of the store directory; the store's records are
/// only read and parsed when that directory changed (or every 10 s).
#[derive(Default)]
pub struct FiringWatch {
    last_marker: Option<std::time::SystemTime>,
    ticks_since_read: u32,
}

impl FiringWatch {
    /// Whether this tick should re-read, given the store's current marker.
    pub fn should_reread(&mut self, marker: Option<std::time::SystemTime>) -> bool {
        self.ticks_since_read += 1;
        let changed = marker != self.last_marker;
        if changed || marker.is_none() || self.ticks_since_read >= FORCE_REREAD_EVERY {
            self.last_marker = marker;
            self.ticks_since_read = 0;
            true
        } else {
            false
        }
    }
}

/// The alarm store's change marker (`None` before AlarmKit is installed).
pub fn store_marker() -> Option<std::time::SystemTime> {
    crate::platform::alarm_kit_optional()?
        .store()
        .change_marker()
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn rereads_only_on_change_or_periodically() {
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let t1 = t0 + Duration::from_secs(1);
        let mut w = FiringWatch::default();

        assert!(w.should_reread(Some(t0)), "first tick reads");
        assert!(!w.should_reread(Some(t0)), "unchanged marker skips");
        assert!(w.should_reread(Some(t1)), "changed marker reads");

        // Unchanged for a while: forced re-read every FORCE_REREAD_EVERY ticks.
        let reads = (0..FORCE_REREAD_EVERY * 2)
            .filter(|_| w.should_reread(Some(t1)))
            .count();
        assert_eq!(reads, 2);
    }

    #[test]
    fn unknown_marker_always_reads() {
        let mut w = FiringWatch::default();
        assert!(w.should_reread(None));
        assert!(w.should_reread(None));
    }
}
