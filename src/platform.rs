//! Platform initialisation for AlarmFree.
//!
//! All firing behavior (FGS, audio, kiosk, exact-alarm) is owned by the mobile-sentinel
//! SDK and its AlarmKit. This file says what AlarmFree wants of it: the policy (lockdown,
//! texts, limits), and how a saved alarm becomes an [`AlarmSpec`].

use chrono::Local;
use mobile_sentinel::context::schema::{ChallengeSpec, ScheduleSpec, SnoozePolicy, SoundIdSpec};
use mobile_sentinel::jobs::RingEnd;
use mobile_sentinel::recipes::alarm_class::AlarmClassConfig;
use mobile_sentinel::{AlarmKit, AlarmSpec, InstanceId, ReminderButton, ReminderTap};
use std::sync::OnceLock;

use crate::app::AlarmData;
use crate::models::{AlarmOutcome, Locale};

/// Fully-qualified name of the Dioxus mobile entry activity. The same value is in
/// `sentinel.toml`, for the Android manifest.
const ALARMFREE_ACTIVITY_FQCN: &str = "dev.dioxus.main.MainActivity";

/// Notification channel id for the firing FGS notification.
pub const FOREGROUND_CHANNEL_ID: &str = "alarmfree_firing";

/// How an alarm can vibrate, as alternating wait and vibrate times.
pub const VIBRATION_PATTERNS: [&[i64]; 3] = [
    // Pulse: pause, long buzz, short gap, long buzz.
    &[0, 500, 300, 500],
    // Steady: hardly a break.
    &[0, 2000, 100],
    // Heartbeat: two quick beats and a rest.
    &[0, 100, 100, 100, 700],
];

/// A pattern over and over, for as long as `total_ms`, in one list of wait and vibrate times.
/// A list of odd length ends on a wait: laid end to end as it is, the next round's wait would
/// land on a place for vibrating and swap the two, so what is the same kind is added up.
pub fn repeated_pattern(pattern: &[i64], total_ms: u64) -> Vec<u64> {
    let mut times: Vec<u64> = Vec::new();
    let mut left = total_ms;
    if pattern.iter().all(|ms| *ms <= 0) {
        return times;
    }
    while left > 0 {
        for (position, ms) in pattern.iter().enumerate() {
            let ms = (*ms).max(0) as u64;
            let taken = ms.min(left);
            left -= taken;
            // Even places wait, odd places vibrate, in the pattern and in the list.
            if times.len() % 2 == position % 2 {
                times.push(taken);
            } else if let Some(last) = times.last_mut() {
                *last += taken;
            }
            if left == 0 {
                break;
            }
        }
    }
    times
}

/// The backup screen, for when the app's own keeps failing: as many long holds as
/// the slowest challenge takes, because it must not be the easy way out.
const BACKUP_HOLDS: u32 = 20;
const BACKUP_HOLD_MS: u32 = 3_000;

/// A phone that restarted and was not unlocked yet cannot show this app's screen. The alarm
/// rings all the same, and a screen of the SDK asks for the unlock. After its button the ring
/// is quiet for this long, to unlock in peace, and it is back if the phone is still locked then.
const UNLOCK_QUIET_SECS: u32 = 60;

/// The UI may fail this often during a ring before the backup screen takes over.
const UI_FAILURES_BEFORE_BACKUP: u32 = 2;

/// An alarm whose time passed during a restart still rings if it is less than
/// this late; later than that it is reported as missed.
const LATE_RING_WINDOW_MS: i64 = 15 * 60 * 1000;

/// Global AlarmKit handle; set by [`init_platform`] or [`on_wake`].
static ALARM_KIT: OnceLock<AlarmKit> = OnceLock::new();

/// Borrow the global AlarmKit handle. Panics if called before [`init_platform`].
pub fn alarm_kit() -> &'static AlarmKit {
    ALARM_KIT
        .get()
        .expect("AlarmKit not initialised: call init_platform() first")
}

/// Borrow the global AlarmKit handle if initialised.
pub fn alarm_kit_optional() -> Option<&'static AlarmKit> {
    ALARM_KIT.get()
}

/// Set when something changed that the UI shows or has to tell, if there is a UI.
static RING_END_NEWS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether rings ended since the last call, for the UI to catch up.
pub fn take_ring_end_news() -> bool {
    RING_END_NEWS.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Record the rings that just ended without the app having a say, while it runs, and
/// switch off the one-time alarms among them.
fn handle_ring_ends() {
    let Some(kit) = alarm_kit_optional() else {
        return;
    };
    let ended = kit.take_ring_ends();
    if ended.is_empty() {
        return;
    }
    let mut list = crate::persistence::load_alarms();
    let mut switched_off = false;
    for (id, end) in &ended {
        record_outcome(&list, &id.0, outcome_of(*end), chrono::Utc::now());
        switched_off |= crate::schedule::finish_if_one_time(&mut list, &id.0);
    }
    if switched_off {
        crate::persistence::save_alarms(&list);
    }
    RING_END_NEWS.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn outcome_of(end: RingEnd) -> AlarmOutcome {
    match end {
        RingEnd::Limit => AlarmOutcome::Limit,
        RingEnd::BackupScreen => AlarmOutcome::Backup,
    }
}

/// An alarm ended some other way than the user solving it: history keeps it, and
/// the user is told next time the app is on screen.
fn record_outcome(
    list: &[AlarmData],
    alarm_id: &str,
    outcome: AlarmOutcome,
    at: chrono::DateTime<chrono::Utc>,
) {
    let alarm = list.iter().find(|a| a.id == alarm_id);
    // A wake-up check or a test alarm is not one of the user's alarms.
    if alarm.is_some_and(|a| a.hidden) {
        return;
    }
    let label = alarm.map(display_name).unwrap_or_default();
    crate::history_persistence::record_outcome(alarm_id, &label, outcome, at);
}

/// Platform setup happens once per process, whichever comes first: the UI, or
/// something else that started the app (see [`on_wake`]).
static SETUP: std::sync::Once = std::sync::Once::new();

/// Platform setup, idempotent and free of UI. Initialises mobile-sentinel (logger +
/// crash mitigation), installs AlarmKit, then re-arms every persisted alarm through it
/// and switches off the one-time alarms it reports as over.
pub fn init_platform() {
    // `force`: a setup that panicked must not take the app down for good.
    SETUP.call_once_force(|_| setup_platform(false));
}

/// The app was started for something other than the UI, or something happened
/// while it runs without one: alarms may have to be armed again. Needs no Activity.
fn on_wake() {
    let mut first = false;
    SETUP.call_once_force(|_| {
        first = true;
        setup_platform(true);
    });
    if !first {
        sync_alarms(alarm_kit(), true);
    }
}

mobile_sentinel::wake_hook!(on_wake);

/// Bring what shows outside the app in line with the alarms and the settings: the
/// notifications before alarms and the home-screen widget. An alarm that has gone is taken
/// care of by [`forget_alarm`].
pub fn refresh_outside_app() {
    let Some(kit) = alarm_kit_optional() else {
        return;
    };
    let settings = crate::persistence::load_settings();
    let translations = crate::i18n::load_translations(settings.locale);
    let text = |key: &str| translations.get(key).cloned().unwrap_or_default();
    let twenty_four = crate::text::uses_24_hours(&settings);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut soonest: Option<(i64, AlarmData)> = None;
    for alarm in crate::persistence::load_alarms() {
        let id = InstanceId::new(alarm.id.clone());
        let next = kit.next_fire(&id).ok().flatten();
        if let Some(at) = next.filter(|_| alarm.enabled && !alarm.hidden) {
            if soonest.as_ref().map_or(true, |(first, _)| at < *first) {
                soonest = Some((at, alarm.clone()));
            }
        }
        let before_ms = i64::from(alarm.notify_before_minutes) * 60_000;
        let wanted = next.filter(|_| {
            before_ms > 0 && alarm.enabled && !alarm.hidden && kit.snoozed_until(&id).is_none()
        });
        match wanted {
            Some(at) if at - before_ms > now_ms => {
                let time = crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four);
                let done_title = text("reminder.skipped").replace("{time}", &time);
                let (skip, undo) = (text("reminder.skip"), text("action.undo"));
                kit.set_reminder(
                    &id,
                    at - before_ms,
                    &text("reminder.title").replace("{time}", &time),
                    &alarm.label,
                    &text("reminder.channel"),
                    before_ms,
                    Some(ReminderButton {
                        label: &skip,
                        done_title: &done_title,
                        undo_label: &undo,
                    }),
                );
            }
            // Its time has come: what shows now stays until the alarm takes over.
            Some(_) => {}
            None => kit.clear_reminder(&id),
        }
    }
    let (title, detail) = widget_text(
        soonest
            .as_ref()
            .map(|(at, alarm)| (*at, alarm.label.as_str())),
        &text,
        twenty_four,
        now_ms,
    );
    mobile_sentinel::home_widget::set_text(&title, &detail);
}

/// What the home-screen widget says about the next ring. It stays up with the app gone,
/// so nothing that stops being true by itself, like "tomorrow".
fn widget_text(
    next: Option<(i64, &str)>,
    text: &dyn Fn(&str) -> String,
    twenty_four: bool,
    now_ms: i64,
) -> (String, String) {
    use chrono::{Datelike, TimeZone, Timelike};
    let Some((at, label)) = next.and_then(|(at, label)| {
        let at = chrono::Local.timestamp_millis_opt(at).single();
        at.map(|at| (at, label))
    }) else {
        return ("--:--".to_owned(), text("widget.none"));
    };
    let title = crate::text::clock(at.hour() as u8, at.minute() as u8, twenty_four);
    let mut day = text(crate::text::FULL_DAY_KEYS[at.weekday().num_days_from_monday() as usize]);
    // A weekday alone is only clear for the coming week.
    if at.timestamp_millis() - now_ms >= 6 * 24 * 60 * 60 * 1000 {
        day = format!("{day} {}-{}", at.day(), at.month());
    }
    let detail = if label.is_empty() {
        day
    } else {
        format!("{day} \u{b7} {label}")
    };
    (title, detail)
}

/// Change the minimum alarm volume for rings from now on. An alarm rings by the
/// plan it was armed with, so the alarms already armed get a new one.
pub fn set_min_volume(percent: u8) {
    let runtime = mobile_sentinel::alarm_class_runtime();
    let mut config = runtime.config();
    config.min_volume_percent = percent;
    runtime.configure(config);
    if let Some(kit) = alarm_kit_optional() {
        kit.refresh_ring_plans();
    }
}

/// The texts an alarm rings with, in the user's language.
fn apply_ring_texts(config: &mut AlarmClassConfig, locale: Locale) {
    let translations = crate::i18n::load_translations(locale);
    let text = |key: &str| translations.get(key).cloned().unwrap_or_default();
    config.channel_name = text("ring.channel");
    config.firing_body = text("ring.body");
    config.fallback_title = text("ring.fallback_title");
    config.backup_text = text("ring.backup_hold");
    config.unlock_text = text("ring.unlock_text");
    config.unlock_button = text("ring.unlock_button");
}

/// The user picked another language. An alarm rings by the plan it was armed with,
/// so the alarms already armed get a new one.
pub fn set_ring_language(locale: Locale) {
    let runtime = mobile_sentinel::alarm_class_runtime();
    let mut config = runtime.config();
    apply_ring_texts(&mut config, locale);
    runtime.configure(config);
    if let Some(kit) = alarm_kit_optional() {
        kit.refresh_ring_plans();
    }
}

fn setup_platform(woken: bool) {
    mobile_sentinel::init(mobile_sentinel::InitConfig {
        log_tag: "AlarmFree".to_string(),
        ..Default::default()
    });

    // Before arming: an alarm's plan names the file its sound plays from.
    crate::custom_sounds::extract_bundled_sounds();

    let settings = crate::persistence::load_settings();
    let mut alarm_class_config = AlarmClassConfig {
        channel_id: FOREGROUND_CHANNEL_ID.into(),
        channel_name: String::new(),
        firing_body: String::new(),
        importance: 5, // IMPORTANCE_MAX
        audio_usage: "alarm".into(),
        audio_content_type: "sonification".into(),
        activity_fqcn: Some(ALARMFREE_ACTIVITY_FQCN.into()),
        kiosk_debounce_ms: 50,
        // Full lockdown while firing: relaunch on HOME/Recents and consume
        // BACK. These are policy knobs the SDK exposes; alarmfree wants the
        // most aggressive setting so the alarm can't be escaped.
        kiosk_block_home: true,
        kiosk_block_back: true,
        kiosk_block_recents: true,
        kiosk_block_volume: true,
        // Show the app normally while firing — keep both system bars
        // visible (status bar with clock/battery, and the nav bar).
        kiosk_hide_status_bar: false,
        kiosk_hide_nav_bar: false,
        // A full-screen intent, so the alarm can wake a locked screen.
        firing_full_screen: true,
        // And no banner over the alarm's own screen once that is up: on ColorOS it covered the
        // date for some seconds.
        firing_banner_over_ui: false,
        // Every alarm names its own; this is for the SDK's sake.
        vibration_pattern: VIBRATION_PATTERNS[0].to_vec(),
        // The user's minimum alarm volume (Settings); updated live by
        // `set_min_volume` when it changes.
        min_volume_percent: settings.min_volume_percent,
        late_ring_window_ms: LATE_RING_WINDOW_MS,
        fallback_sound_id: Some(SoundIdSpec::Bundled(
            crate::custom_sounds::FALLBACK_SOUND.into(),
        )),
        fallback_title: String::new(),
        backup_holds: shortened("backup_holds", BACKUP_HOLDS),
        backup_hold_ms: BACKUP_HOLD_MS,
        backup_text: String::new(),
        unlock_text: String::new(),
        unlock_button: String::new(),
        unlock_quiet_secs: shortened("unlock_quiet_secs", UNLOCK_QUIET_SECS),
    };
    apply_ring_texts(&mut alarm_class_config, settings.locale);

    let kit = AlarmKit::install_on_platform(
        crate::custom_sounds::default_sounds_dir(),
        crate::custom_sounds::custom_sounds_dir(),
        alarm_class_config,
    );
    kit.on_ring_end(handle_ring_ends);
    let _ = ALARM_KIT.set(kit);

    sync_alarms(alarm_kit(), woken);

    #[cfg(debug_assertions)]
    crate::debug_hooks::start();

    log::info!("[AlarmFree] Platform fully configured");
}

/// Bring what AlarmKit has in line with the saved alarms: end what cannot go on,
/// arm everything, ring what is due, and switch off the one-time alarms it reports
/// as over.
fn sync_alarms(kit: &'static AlarmKit, woken: bool) {
    // A boot and a clock change can arrive together.
    static SYNC: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _sync = SYNC
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Not sure what is saved? Then what AlarmKit has must stay: it may be all that
    // still rings.
    let stored = crate::persistence::database().list_alarms_reporting();
    let readable = matches!(stored, Ok((_, 0)));
    let mut list = stored.map(|(alarms, _)| alarms).unwrap_or_else(|e| {
        log::error!("[AlarmFree] alarms could not be read: {e}");
        Vec::new()
    });
    if crate::schedule::date_undated_one_time(&mut list, Local::now().naive_local()) {
        crate::persistence::save_alarms(&list);
    }
    // Only with a list that can be trusted. Otherwise the alarm simply rings, and what was
    // tapped has no effect.
    if readable && (hand_notice_to_alarms(&mut list) | apply_reminder_taps(kit, &mut list)) {
        crate::persistence::save_alarms(&list);
        RING_END_NEWS.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    // Single call: end alarms a reboot interrupted + rearm all alarms +
    // process active jobs + reengage firing
    let alarms: Vec<(String, AlarmSpec)> = list
        .iter()
        .filter(|a| a.enabled)
        .map(|a| (a.id.clone(), alarm_data_to_spec(a)))
        .collect();
    for (id, _) in &alarms {
        ensure_alarm_job(id);
    }
    if readable {
        forget_unlisted_alarms(kit, &alarms);
    }
    let report = if woken {
        kit.on_wake(&alarms)
    } else {
        kit.on_startup(&alarms)
    };

    let now = chrono::Utc::now();
    for id in &report.ended_by_reboot {
        record_outcome(&list, &id.0, AlarmOutcome::Reboot, now);
    }
    for (id, end) in &report.ring_ends {
        record_outcome(&list, &id.0, outcome_of(*end), now);
    }
    for missed in &report.missed {
        let due = chrono::DateTime::from_timestamp_millis(missed.due_unix_ms).unwrap_or(now);
        record_outcome(&list, &missed.id.0, AlarmOutcome::Missed, due);
    }
    // A sync can run next to the UI: Android hands over the boot signals when a
    // force-stopped app is opened, so this may come after the UI looked for news.
    if !report.ended_by_reboot.is_empty()
        || !report.ring_ends.is_empty()
        || !report.missed.is_empty()
    {
        RING_END_NEWS.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let finished = report
        .finished
        .iter()
        .chain(&report.ended_by_reboot)
        .chain(report.ring_ends.iter().map(|(id, _)| id))
        .chain(report.missed.iter().map(|m| &m.id));
    let mut switched_off = false;
    for id in finished {
        switched_off |= crate::schedule::finish_if_one_time(&mut list, &id.0);
    }
    if switched_off {
        crate::persistence::save_alarms(&list);
        RING_END_NEWS.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    refresh_outside_app();
}

/// The notice before an alarm used to be one setting for all of them: each has its own now.
fn hand_notice_to_alarms(list: &mut [AlarmData]) -> bool {
    let mut settings = crate::persistence::load_settings();
    let minutes = settings.prefs.notify_before_minutes;
    if minutes == 0 {
        return false;
    }
    crate::alarm_ops::give_notice_to_all(list, settings.prefs.alarm_defaults.as_mut(), minutes);
    settings.prefs.notify_before_minutes = 0;
    crate::persistence::save_settings(&settings);
    true
}

/// What was tapped on the notices before alarms. Whether the app ran or not: the tap wakes it.
fn apply_reminder_taps(kit: &AlarmKit, list: &mut [AlarmData]) -> bool {
    let mut changed = false;
    for (id, tap) in kit.take_reminder_taps() {
        let next_fire = kit.next_fire(&id).ok().flatten();
        match list.iter_mut().find(|alarm| alarm.id == id.0) {
            Some(alarm) => {
                changed |= match tap {
                    ReminderTap::Act => crate::alarm_ops::skip_from_notice(alarm, next_fire),
                    ReminderTap::Undo => crate::alarm_ops::undo_skip_from_notice(alarm),
                };
            }
            // Deleted since: taking the skip back brought its notice back, for nothing.
            None => kit.clear_reminder(&id),
        }
    }
    changed
}

/// Write `text` to a file in the phone's Downloads. Returns where it went.
pub fn save_to_downloads(text: &str, name: &str, mime_type: &str) -> Result<String, String> {
    let path = mobile_sentinel::app_files_dir().join(name);
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let result =
        mobile_sentinel::share::save_to_downloads(&path.to_string_lossy(), name, mime_type)
            .map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&path);
    result
}

/// A wait or a count as it is, unless the scenarios asked for less, in a build for testing.
pub fn shortened(name: &str, normal: u32) -> u32 {
    #[cfg(debug_assertions)]
    if let Some(value) = crate::debug_hooks::test_value(name) {
        return value;
    }
    let _ = name;
    normal
}

/// The name to tell the user an alarm by: its label, else its time.
fn display_name(alarm: &AlarmData) -> String {
    if alarm.label.is_empty() {
        let twenty_four = crate::text::uses_24_hours(&crate::persistence::load_settings());
        crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four)
    } else {
        alarm.label.clone()
    }
}

/// Drop what the SDK still has for alarms that are no longer enabled (the app
/// was killed between saving and telling the SDK). A ringing one stays until it
/// is dealt with. Only call this when the alarm list was read successfully: a
/// read error must not cancel every alarm.
fn forget_unlisted_alarms(kit: &AlarmKit, listed: &[(String, AlarmSpec)]) {
    let Ok(known) = kit.list() else {
        return;
    };
    for (id, _) in known {
        let ringing = matches!(kit.current_session(), Ok(Some(s)) if s.instance_id == id);
        if !ringing && !listed.iter().any(|(listed_id, _)| *listed_id == id.0) {
            forget_alarm(&id.0);
        }
    }
}

// ---------------------------------------------------------------------------
// UI-facing helpers — every save / cancel from the screens funnels through
// these, which forward to AlarmKit.
// ---------------------------------------------------------------------------

/// Bring the SDK in line with a saved alarm: scheduled when enabled, gone when
/// not. Registers a sentinel job so the job guardian can keep MAIN alive when
/// the alarm fires.
pub(crate) fn apply_alarm(alarm: &AlarmData) {
    let Some(kit) = alarm_kit_optional() else {
        return;
    };
    if !alarm.enabled {
        forget_alarm(&alarm.id);
        return;
    }
    ensure_alarm_job(&alarm.id);
    let id = InstanceId::new(alarm.id.clone());
    let spec = alarm_data_to_spec(alarm);
    let res = if kit.get(&id).is_ok() {
        kit.update(&id, spec)
    } else {
        kit.create_with_id(id, spec)
    };
    if let Err(e) = res {
        log::warn!(
            "[AlarmFree] AlarmKit schedule failed for {}: {e:?}",
            alarm.id
        );
    }
}

/// Remove everything but the saved alarm itself: nothing may ring for it any
/// more, and a later alarm with its id starts clean.
pub(crate) fn forget_alarm(alarm_id: &str) {
    if let Some(kit) = alarm_kit_optional() {
        kit.clear_reminder(&InstanceId::new(alarm_id.to_owned()));
        if let Err(e) = kit.delete(&InstanceId::new(alarm_id.to_owned())) {
            log::warn!("[AlarmFree] AlarmKit delete({alarm_id}) failed: {e:?}");
        }
    }
    let _ = mobile_sentinel::remove_job(alarm_id);
}

/// How long the alarm screen may go without proving it is alive before the
/// `:sentinel` guardian restarts the app (see `app::ui_round_trip`). The first
/// beat has to follow the activity coming on screen within this time, so it
/// is generous: a cold start extracts sounds and loads the WebView first.
const UI_LIVENESS_TIMEOUT_MS: u32 = 15_000;

/// How often the guardian checks on a job. The sound does not depend on it.
const POLL_INTERVAL_MS: u64 = 500;

/// Guardian settings for an alarm's job.
fn job_config() -> mobile_sentinel::JobConfig {
    mobile_sentinel::JobConfig {
        poll_interval_ms: POLL_INTERVAL_MS,
        ui_liveness_timeout_ms: u64::from(shortened("ui_timeout_ms", UI_LIVENESS_TIMEOUT_MS)),
        ui_failures_before_backup: UI_FAILURES_BEFORE_BACKUP,
        ..Default::default()
    }
}

fn register_alarm_job(alarm_id: &str) -> Result<(), mobile_sentinel::jobs::JobGuardianError> {
    let payload = serde_json::json!({
        "instance_id": alarm_id,
        "activity_fqcn": ALARMFREE_ACTIVITY_FQCN
    });
    mobile_sentinel::register_job(alarm_id, payload, job_config()).map(|_| ())
}

/// Make sure an enabled alarm's job exists with the current settings.
fn ensure_alarm_job(alarm_id: &str) {
    if let Ok(Some(job)) = mobile_sentinel::jobs::get_job(alarm_id) {
        if job.config == job_config() {
            return;
        }
    }
    if let Err(e) = register_alarm_job(alarm_id) {
        log::warn!("[AlarmFree] ensure job {alarm_id} failed: {e:?}");
    }
}

/// Map AlarmFree's UI `AlarmData` to the universal SDK's `AlarmSpec`.
pub fn alarm_data_to_spec(alarm: &AlarmData) -> AlarmSpec {
    let any_days = alarm.days.iter().any(|&d| d);
    let schedule = if any_days {
        let mut mask: u8 = 0;
        for (idx, &enabled) in alarm.days.iter().enumerate() {
            if enabled {
                mask |= 1u8 << idx;
            }
        }
        ScheduleSpec::Weekdays {
            days_mask: mask,
            hour: alarm.time_hour,
            minute: alarm.time_minute,
        }
    } else {
        // The stored date as it is, also when it has passed: an alarm's spec
        // must not change by itself as time goes on, or AlarmKit would see every
        // ringing or snoozed one-time alarm as edited. Dates are set when an
        // alarm is saved or switched on, and at startup for ones that have none.
        let date = alarm.date.clone().unwrap_or_else(|| {
            let now = Local::now().naive_local();
            crate::schedule::next_occurrence(alarm.time_hour, alarm.time_minute, now)
        });
        ScheduleSpec::OneTime {
            date,
            hour: alarm.time_hour,
            minute: alarm.time_minute,
        }
    };
    // sound_mode: 0 = sound + vibrate, 1 = sound only, 2 = vibrate only.
    // Mode 2 plays no sound, so resolve to Silent regardless of the picked
    // tone; modes 0/1 use the chosen tone. Vibration is enabled for 0 and 2.
    let vibrate_only = alarm.sound_mode == 2;
    let sound_id = if vibrate_only {
        SoundIdSpec::Silent
    } else if crate::custom_sounds::is_custom_sound(&alarm.sound) {
        SoundIdSpec::Custom(alarm.sound.clone())
    } else if alarm.sound.is_empty() {
        SoundIdSpec::SystemDefault
    } else {
        SoundIdSpec::Bundled(alarm.sound.clone())
    };
    AlarmSpec {
        label: alarm.label.clone(),
        schedule,
        time_zone: AlarmKit::detect_timezone(),
        sound_id,
        snooze_policy: SnoozePolicy {
            max_count: alarm.snooze_count as u32,
            interval_minutes: alarm.snooze_interval as u32,
            escalation: None,
        },
        challenges: alarm
            .challenges
            .iter()
            .map(|(challenge_type, _settings)| ChallengeSpec {
                challenge_type: challenge_type.clone(),
                difficulty: 0,
                config: serde_json::json!({}),
            })
            .collect(),
        vibration_enabled: matches!(alarm.sound_mode, 0 | 2),
        vibration_pattern: Some(
            VIBRATION_PATTERNS[usize::from(alarm.vibration).min(VIBRATION_PATTERNS.len() - 1)]
                .to_vec(),
        ),
        kiosk_mode: true,
        bypass_dnd: true,
        // A skipped ring and a holiday are separate, and the later one holds.
        skip_until_unix_ms: alarm.skip_until_ms.max(alarm.pause_until_ms),
        fade_in_secs: alarm.fade_in_secs,
        ring_limit_secs: alarm.ring_limit_secs,
        min_volume_percent: alarm.volume_percent,
        sound_delay_secs: if alarm.sound_mode == 0 {
            alarm.sound_delay_secs
        } else {
            0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_keeps_its_rhythm_when_it_repeats() {
        // Heartbeat: buzz, pause, buzz, rest, and the next beat starts right after the rest.
        assert_eq!(
            repeated_pattern(VIBRATION_PATTERNS[2], 2_000),
            vec![0, 100, 100, 100, 700, 100, 100, 100, 700]
        );
        // Steady: the short gap stays a gap, and does not turn into a buzz.
        assert_eq!(
            repeated_pattern(VIBRATION_PATTERNS[1], 4_200),
            vec![0, 2000, 100, 2000, 100]
        );
        // Pulse ends on a buzz, so the next round has its own wait in between.
        assert_eq!(
            repeated_pattern(VIBRATION_PATTERNS[0], 2_600),
            vec![0, 500, 300, 500, 0, 500, 300, 500]
        );
    }

    #[test]
    fn a_repeated_pattern_lasts_as_long_as_was_asked() {
        for pattern in VIBRATION_PATTERNS {
            let total: u64 = repeated_pattern(pattern, 15_000).iter().sum();
            assert_eq!(total, 15_000);
        }
        assert!(
            repeated_pattern(&[0, 0], 15_000).is_empty(),
            "or it never ends"
        );
        assert!(repeated_pattern(&[], 15_000).is_empty());
    }

    #[test]
    fn the_widget_says_when_the_next_alarm_rings_and_nothing_that_goes_stale() {
        use chrono::TimeZone;
        let text = |key: &str| key.to_owned();
        let at = chrono::Local
            .with_ymd_and_hms(2026, 11, 3, 7, 5, 0)
            .unwrap();
        let at_ms = at.timestamp_millis();
        let soon = at_ms - 3_600_000;
        assert_eq!(
            widget_text(Some((at_ms, "Work")), &text, true, soon),
            ("07:05".to_owned(), "day.tuesday \u{b7} Work".to_owned())
        );
        assert_eq!(
            widget_text(Some((at_ms, "")), &text, true, soon).1,
            "day.tuesday"
        );
        let far = at_ms - 7 * 24 * 3_600_000;
        assert_eq!(
            widget_text(Some((at_ms, "")), &text, true, far).1,
            "day.tuesday 3-11"
        );
        assert_eq!(
            widget_text(None, &text, true, soon),
            ("--:--".to_owned(), "widget.none".to_owned())
        );
    }

    fn weekday_alarm() -> AlarmData {
        AlarmData {
            id: "a".into(),
            time_hour: 7,
            time_minute: 30,
            label: "Wake".into(),
            days: [true, true, true, true, true, false, false],
            date: None,
            skip_until_ms: None,
            fade_in_secs: 0,
            ring_limit_secs: None,
            order: 0,
            enabled: true,
            sound: "happy".into(),
            sound_mode: 0,
            challenges: vec![],
            snooze_count: 3,
            snooze_interval: 5,
            note: String::new(),
            vibration: 0,
            volume_percent: None,
            notify_before_minutes: 0,
            ring_look: Default::default(),
            wake_check_minutes: 0,
            max_brightness: false,
            sound_delay_secs: 0,
            pause_until_ms: None,
            delete_after_ring: false,
            hidden: false,
            test: false,
        }
    }

    #[test]
    fn alarm_data_to_spec_maps_weekday_days() {
        let alarm = weekday_alarm();
        let spec = alarm_data_to_spec(&alarm);
        match spec.schedule {
            ScheduleSpec::Weekdays {
                days_mask,
                hour,
                minute,
            } => {
                assert_eq!(days_mask, 0b0001_1111);
                assert_eq!(hour, 7);
                assert_eq!(minute, 30);
            }
            other => panic!("expected weekdays, got {other:?}"),
        }
        assert!(matches!(spec.sound_id, SoundIdSpec::Bundled(ref s) if s == "happy"));
        assert_eq!(spec.snooze_policy.max_count, 3);
        assert_eq!(spec.snooze_policy.interval_minutes, 5);
    }

    #[test]
    fn alarm_data_to_spec_handles_custom_sound() {
        let mut alarm = weekday_alarm();
        alarm.sound = "abc.mp3".into();
        let spec = alarm_data_to_spec(&alarm);
        assert!(matches!(spec.sound_id, SoundIdSpec::Custom(ref s) if s == "abc.mp3"));
    }

    #[test]
    fn alarm_data_to_spec_handles_empty_sound_as_system_default() {
        let mut alarm = weekday_alarm();
        alarm.sound = "".into();
        let spec = alarm_data_to_spec(&alarm);
        assert!(matches!(spec.sound_id, SoundIdSpec::SystemDefault));
    }

    #[test]
    fn alarm_data_to_spec_one_time_when_no_days_selected() {
        let mut alarm = weekday_alarm();
        alarm.days = [false; 7];
        let spec = alarm_data_to_spec(&alarm);
        assert!(matches!(spec.schedule, ScheduleSpec::OneTime { .. }));
    }

    /// If the spec moved by itself once the alarm's time passed, AlarmKit would
    /// take a snoozed one-time alarm for an edited one and cancel the snooze.
    #[test]
    fn a_one_time_alarm_keeps_its_date_after_its_time_has_passed() {
        let mut alarm = weekday_alarm();
        alarm.days = [false; 7];
        alarm.date = Some("2020-01-01".into());
        match alarm_data_to_spec(&alarm).schedule {
            ScheduleSpec::OneTime { date, .. } => assert_eq!(date, "2020-01-01"),
            other => panic!("expected one-time, got {other:?}"),
        }
    }
}
