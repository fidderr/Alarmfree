//! Alarm-related model types.

use serde::{Deserialize, Serialize};

/// Stable opaque alarm identifier (UUID v4 string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlarmId(pub String);

/// Shared alarm data stored in global state.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub struct AlarmData {
    pub id: String,
    pub time_hour: u8,
    pub time_minute: u8,
    pub label: String,
    pub days: [bool; 7],
    /// Date (`YYYY-MM-DD`) of a one-time alarm; `None` for weekday alarms and
    /// for alarms saved before this field existed.
    #[serde(default)]
    pub date: Option<String>,
    /// Skip every occurrence at or before this instant (unix ms): one skipped
    /// occurrence of a repeating alarm. In the past it has no effect.
    #[serde(default)]
    pub skip_until_ms: Option<i64>,
    /// Seconds the sound takes to fade in (0 = off).
    #[serde(default)]
    pub fade_in_secs: u16,
    /// Stop by itself after ringing this long with nobody doing anything, in
    /// seconds. `None` rings until it is dealt with.
    #[serde(default = "default_ring_limit_secs")]
    pub ring_limit_secs: Option<u32>,
    /// Position in the home list once the user has dragged alarms around (see
    /// [`crate::ordering`]); 0 = not arranged, listed by time.
    #[serde(default)]
    pub order: i64,
    pub enabled: bool,
    pub sound: String,
    pub sound_mode: usize,                 // 0=Sound+Vib, 1=Sound, 2=Vib
    pub challenges: Vec<(String, String)>, // (type, settings), see ChallengeSetup
    pub snooze_count: u8,
    pub snooze_interval: u8,
    /// Shown under the label while it rings ("Take medication").
    #[serde(default)]
    pub note: String,
    /// Vibration pattern, see [`crate::platform::VIBRATION_PATTERNS`]. Whether it
    /// vibrates at all is `sound_mode`.
    #[serde(default)]
    pub vibration: u8,
    /// The alarm volume while it rings is at least this; `None` is the general
    /// minimum from the settings.
    #[serde(default)]
    pub volume_percent: Option<u8>,
    /// This long after it was turned off, confirm being up or it rings again (0 = off).
    #[serde(default)]
    pub wake_check_minutes: u8,
    /// The screen goes to full brightness while it rings.
    #[serde(default)]
    pub max_brightness: bool,
    /// A silent notice this long before it rings, from which the ring can be skipped (0 = off).
    #[serde(default)]
    pub notify_before_minutes: u8,
    /// How the screen looks while it rings.
    #[serde(default)]
    pub ring_look: crate::models::RingLook,
    /// Only vibrate this long before the sound starts. Needs both on (`sound_mode` 0).
    #[serde(default)]
    pub sound_delay_secs: u16,
    /// Nothing rings until then, for a holiday. Separate from a skipped ring, so
    /// undoing one leaves the other.
    #[serde(default)]
    pub pause_until_ms: Option<i64>,
    /// Gone instead of switched off when over: quick alarms, wake-up checks.
    #[serde(default)]
    pub delete_after_ring: bool,
    /// The ring from the reliability screen's test button, which can be called off.
    #[serde(default)]
    pub test: bool,
    /// Not in the list: wake-up checks and test alarms.
    #[serde(default)]
    pub hidden: bool,
}

fn default_ring_limit_secs() -> Option<u32> {
    Some(mobile_sentinel::context::schema::DEFAULT_RING_LIMIT_SECS)
}
