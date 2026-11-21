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
    /// This long before it rings the alarm cannot be switched off or weakened without doing its
    /// challenges, see [`crate::edit_lock`] (0 = off).
    #[serde(default)]
    pub lock_minutes: u16,
    /// The screen goes to full brightness while it rings.
    #[serde(default)]
    pub max_brightness: bool,
    /// What alarms saved before the reminder had: a silent notice this many minutes before.
    /// Read once and turned into `reminder` by [`AlarmData::migrate`]; never written again.
    #[serde(default, skip_serializing)]
    pub notify_before_minutes: u8,
    /// A notification before it rings, from which the ring can be skipped.
    #[serde(default)]
    pub reminder: Reminder,
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

/// How long before the ring a reminder can come, in minutes.
pub const REMINDER_MINUTES: [u16; 19] = [
    5, 10, 15, 30, 45, 60, 120, 180, 240, 300, 360, 420, 480, 540, 600, 660, 720, 960, 1440,
];
/// The longest text a reminder can carry, in characters.
pub const REMINDER_TEXT_MAX: usize = 120;

/// The notification before an alarm: when, what it says, and whether it makes a sound.
#[derive(Clone, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Reminder {
    #[serde(default)]
    pub on: bool,
    /// How long before the ring, one of [`REMINDER_MINUTES`].
    #[serde(default)]
    pub minutes: u16,
    /// What it says under the alarm's time; empty is the alarm's label.
    #[serde(default)]
    pub text: String,
    /// The phone's notification sound, instead of silence.
    #[serde(default)]
    pub sound: bool,
}

impl Reminder {
    /// How long before the ring it comes, in milliseconds; `None` when there is none.
    pub fn before_ms(&self) -> Option<i64> {
        (self.on && self.minutes > 0).then(|| i64::from(self.minutes) * 60_000)
    }
}

impl AlarmData {
    /// Bring an alarm saved by an older version up to date. Every way an alarm is read goes
    /// through here.
    pub fn migrate(&mut self) {
        if self.notify_before_minutes > 0 && !self.reminder.on {
            self.reminder = Reminder {
                on: true,
                minutes: u16::from(self.notify_before_minutes),
                text: String::new(),
                sound: false,
            };
        }
        self.notify_before_minutes = 0;
    }
}

fn default_ring_limit_secs() -> Option<u32> {
    Some(mobile_sentinel::context::schema::DEFAULT_RING_LIMIT_SECS)
}
