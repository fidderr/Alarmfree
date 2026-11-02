//! Platform initialisation for AlarmFree.
//!
//! All firing behavior (FGS, audio, kiosk, exact-alarm) is owned by
//! mobile-sentinel's universal SDK + AlarmKit. This file only wires the
//! runtime: install AlarmKit with the Android `FiringSink`, the Android
//! sound backend, and the ContextStore that lives next to AlarmFree's
//! other persistence files.

use std::sync::Arc;

use chrono::Local;
use mobile_sentinel::context::schema::{ChallengeSpec, ScheduleSpec, SnoozePolicy, SoundIdSpec};
use mobile_sentinel::{AlarmKit, AlarmKitConfig, AlarmSpec, ContextStore, InstanceId};
use once_cell::sync::OnceCell;

use crate::app::AlarmData;

/// Fully-qualified name of the Dioxus mobile entry activity. The same
/// value is injected into the Android manifest by `prepare_android` and
/// referenced by the AlarmClass runtime config below.
const ALARMFREE_ACTIVITY_FQCN: &str = "dev.dioxus.main.MainActivity";

/// Notification channel id for the firing FGS notification.
const FOREGROUND_CHANNEL_ID: &str = "alarmfree_firing";

/// Notification channel display name.
const FOREGROUND_CHANNEL_NAME: &str = "Alarms";

/// Notification body shown while firing.
const FOREGROUND_BODY: &str = "Tap to open";

/// An alarm whose time passed during a restart still rings if it is less than
/// this late; later than that it is reported as missed.
const LATE_RING_WINDOW_MS: i64 = 15 * 60 * 1000;

/// Global AlarmKit handle; populated during `init_sentinel`.
static ALARM_KIT: OnceCell<AlarmKit> = OnceCell::new();

/// Borrow the global AlarmKit handle. Panics if called before `init_sentinel`.
pub fn alarm_kit() -> &'static AlarmKit {
    ALARM_KIT
        .get()
        .expect("AlarmKit not initialised — call init_sentinel() first")
}

/// Borrow the global AlarmKit handle if initialised.
pub fn alarm_kit_optional() -> Option<&'static AlarmKit> {
    ALARM_KIT.get()
}

/// Names of the alarms that were ringing when the phone restarted and were
/// ended by [`init_platform`], for a notice to the user.
static ENDED_BY_REBOOT: OnceCell<Vec<String>> = OnceCell::new();

/// Alarms ended by a reboot during startup (empty if none, or before
/// [`init_platform`] ran).
pub fn ended_by_reboot() -> &'static [String] {
    ENDED_BY_REBOOT.get().map_or(&[], Vec::as_slice)
}

/// Names of the alarms whose time passed without ringing, found during startup.
static MISSED: OnceCell<Vec<String>> = OnceCell::new();

/// Alarms that did not ring at their time (empty if none, or before
/// [`init_platform`] ran).
pub fn missed_alarms() -> &'static [String] {
    MISSED.get().map_or(&[], Vec::as_slice)
}

/// One-shot platform init plus the permission prompts (which need the UI).
pub fn init_sentinel() {
    init_platform();

    // Permission prompts (POST_NOTIFICATIONS, SYSTEM_ALERT_WINDOW), but never
    // over a ringing alarm: this also runs when an alarm starts the app.
    #[cfg(target_os = "android")]
    if crate::firing_state::peek_firing_alarm().is_none() {
        request_runtime_permissions();
    }
}

/// Platform setup, idempotent and free of UI. Initialises mobile-sentinel
/// (logger + crash mitigation), installs the Android `FiringSink`, installs
/// AlarmKit with that sink + the Android sound backend, then re-arms every
/// persisted alarm via the universal SDK and switches off the one-time alarms
/// it reports as over.
pub fn init_platform() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(setup_platform);
}

/// Change the minimum alarm volume for rings from now on.
pub fn set_min_volume(percent: u8) {
    let runtime = mobile_sentinel::alarm_class_runtime();
    let mut config = runtime.config();
    config.min_volume_percent = percent;
    runtime.configure(config);
}

/// Per-session files the firing screen keeps for an alarm.
fn clear_session_files(alarm_id: &str) {
    let dir = mobile_sentinel::app_files_dir();
    let _ = std::fs::remove_file(dir.join(format!("snooze_{alarm_id}.txt")));
    let _ = std::fs::remove_file(dir.join(format!("snooze_intervals_{alarm_id}.txt")));
}

fn setup_platform() {
    mobile_sentinel::init(mobile_sentinel::InitConfig {
        log_tag: "AlarmFree".to_string(),
        ..Default::default()
    });

    let store = Arc::new(ContextStore::new(
        mobile_sentinel::app_files_dir().join("sentinel/context"),
    ));

    let alarm_class_config = mobile_sentinel::recipes::alarm_class::AlarmClassConfig {
        channel_id: FOREGROUND_CHANNEL_ID.into(),
        channel_name: FOREGROUND_CHANNEL_NAME.into(),
        firing_body: FOREGROUND_BODY.into(),
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
        // Show the app normally while firing — keep both system bars
        // visible (status bar with clock/battery, and the nav bar).
        kiosk_hide_status_bar: false,
        kiosk_hide_nav_bar: false,
        // Use a full-screen intent so the alarm can wake a locked screen,
        // but the platform suppresses the on-screen heads-up banner when
        // the firing UI is already in the foreground (no redundant popup).
        firing_full_screen: true,
        // Alarm-style buzz for vibration-enabled alarms: pause, long buzz,
        // short gap, long buzz — looped by the firing sink while ringing.
        vibration_pattern: vec![0, 500, 300, 500],
        // The user's minimum alarm volume (Settings); updated live by
        // `set_min_volume` when it changes.
        min_volume_percent: crate::persistence::load_settings().min_volume_percent,
        late_ring_window_ms: LATE_RING_WINDOW_MS,
    };

    #[cfg(target_os = "android")]
    let firing_sink: Option<Arc<dyn mobile_sentinel::firing::FiringSink>> = Some(Arc::new(
        mobile_sentinel::platform::android::AndroidFiringSink::new(),
    ));
    #[cfg(not(target_os = "android"))]
    let firing_sink: Option<Arc<dyn mobile_sentinel::firing::FiringSink>> = None;

    // Install the firing sink as the process-wide singleton so background
    // tasks (e.g. cross-process reengage) can reach it.
    if let Some(ref s) = firing_sink {
        mobile_sentinel::firing::install_firing_sink(s.clone());
    }

    #[cfg(target_os = "android")]
    let sound_resolver: Option<Arc<dyn mobile_sentinel::recipes::alarm_class::SoundResolver>> = {
        let bundled = crate::custom_sounds::default_sounds_dir();
        let custom = mobile_sentinel::app_files_dir().join("sounds/custom");
        let lib = mobile_sentinel::SoundLibrary::new(
            bundled,
            custom,
            mobile_sentinel::platform::android::AndroidSoundBackend,
        );
        Some(Arc::new(lib))
    };
    #[cfg(not(target_os = "android"))]
    let sound_resolver: Option<Arc<dyn mobile_sentinel::recipes::alarm_class::SoundResolver>> =
        None;

    let kit = AlarmKit::install(AlarmKitConfig {
        store,
        sink: firing_sink,
        sound_resolver,
        alarm_class_config,
    });
    let _ = ALARM_KIT.set(kit);

    let kit = alarm_kit();
    let stored = crate::persistence::database().list_alarms();
    let readable = stored.is_ok();
    let mut list = stored.unwrap_or_else(|e| {
        log::error!("[AlarmFree] alarms could not be read: {e}");
        Vec::new()
    });
    if crate::schedule::date_undated_one_time(&mut list, Local::now().naive_local()) {
        crate::persistence::save_alarms(&list);
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
    let report = kit.on_startup(&alarms);

    let name_of = |id: &str| {
        list.iter()
            .find(|a| a.id == id)
            .map(display_name)
            .unwrap_or_default()
    };
    let ended_names: Vec<String> = report
        .ended_by_reboot
        .iter()
        .map(|id| name_of(&id.0))
        .collect();
    let missed_names: Vec<String> = report.missed.iter().map(|m| name_of(&m.id.0)).collect();
    let over = report
        .ended_by_reboot
        .iter()
        .chain(report.missed.iter().map(|m| &m.id));
    for id in over {
        clear_session_files(&id.0);
    }
    let finished = report
        .finished
        .iter()
        .chain(&report.ended_by_reboot)
        .chain(report.missed.iter().map(|m| &m.id));
    let mut switched_off = false;
    for id in finished {
        switched_off |= crate::schedule::disable_if_one_time(&mut list, &id.0);
    }
    if switched_off {
        crate::persistence::save_alarms(&list);
    }
    let _ = ENDED_BY_REBOOT.set(ended_names);
    let _ = MISSED.set(missed_names);

    // Debug helpers (test harness only)
    #[cfg(debug_assertions)]
    debug_auto_schedule();
    #[cfg(debug_assertions)]
    debug_action_poller();

    #[cfg(target_os = "android")]
    log::info!("[AlarmFree] Platform fully configured");
}

/// The name to tell the user an alarm by: its label, else its time.
fn display_name(alarm: &AlarmData) -> String {
    if alarm.label.is_empty() {
        format!("{:02}:{:02}", alarm.time_hour, alarm.time_minute)
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

#[cfg(target_os = "android")]
fn request_runtime_permissions() {
    let _ = mobile_sentinel::permissions::request("android.permission.POST_NOTIFICATIONS");
    if !mobile_sentinel::overlay::is_granted() {
        mobile_sentinel::overlay::request();
    }
}

/// Debug-only auto-schedule. If `<files>/debug_alarm_secs.txt` exists,
/// reads an integer (seconds-from-now) and creates a one-shot AlarmKit
/// alarm at `now + N seconds`. Used by the integration test harness.
#[cfg(debug_assertions)]
fn debug_auto_schedule() {
    #[cfg(not(target_os = "android"))]
    return;

    #[cfg(target_os = "android")]
    {
        use chrono::Timelike;

        let marker = mobile_sentinel::app_files_dir().join("debug_alarm_secs.txt");
        let contents = match std::fs::read_to_string(&marker) {
            Ok(s) => s,
            Err(_) => return,
        };
        let secs: i64 = match contents.trim().parse() {
            Ok(n) => n,
            Err(_) => {
                log::warn!("[AlarmFree] debug_alarm_secs.txt: invalid integer");
                return;
            }
        };
        let now = chrono::Local::now();
        // Round up to next full minute so the OneTime schedule has a future
        // hour:minute (the schedule has minute granularity).
        let target = (now + chrono::Duration::seconds(secs))
            .with_second(0)
            .unwrap_or(now)
            + chrono::Duration::minutes(1);

        let kit = match alarm_kit_optional() {
            Some(k) => k,
            None => return,
        };
        let id = InstanceId::new(format!("debug-{}", target.timestamp_millis()));
        let spec = AlarmSpec {
            label: format!("Debug +{secs}s"),
            schedule: ScheduleSpec::Weekdays {
                days_mask: 0b0111_1111,
                hour: target.hour() as u8,
                minute: target.minute() as u8,
            },
            time_zone: AlarmKit::detect_timezone(),
            sound_id: SoundIdSpec::Bundled("alarm".into()),
            snooze_policy: SnoozePolicy {
                max_count: 3,
                interval_minutes: 1,
                escalation: None,
            },
            challenges: Vec::<ChallengeSpec>::new(),
            vibration_enabled: true,
            // Inherit alarmfree's app-wide pattern (AlarmClassConfig default).
            vibration_pattern: None,
            kiosk_mode: true,
            bypass_dnd: true,
            skip_until_unix_ms: None,
            fade_in_secs: 0,
        };
        match kit.create_with_id(id.clone(), spec) {
            Ok(()) => log::info!(
                "[AlarmFree] DEBUG auto-scheduled {} for {} ({}s from now)",
                id.0,
                target.format("%Y-%m-%d %H:%M:%S"),
                secs
            ),
            Err(e) => log::warn!("[AlarmFree] DEBUG auto-schedule failed: {:?}", e),
        }
        // Remove marker so the next launch doesn't re-create it.
        let _ = std::fs::remove_file(&marker);
    }
}

/// Set by the `hang_ui` debug action; consumed by the UI's heartbeat loop.
#[cfg(debug_assertions)]
static DEBUG_HANG_UI: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Debug-only: freeze the calling thread (the UI's) for an hour when the
/// `hang_ui` debug action asked for it, to exercise the `:sentinel` liveness
/// watchdog.
#[cfg(debug_assertions)]
pub fn debug_block_if_requested() {
    if DEBUG_HANG_UI.swap(false, std::sync::atomic::Ordering::SeqCst) {
        log::warn!("[AlarmFree] DEBUG blocking the UI thread");
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

/// Background poll for the integration-test debug action file. The
/// harness writes `<files>/debug_action.txt` with one of:
///
///   `snooze:<instance_id>` — triggers `kit.snooze(id)` on the next poll.
///   `dismiss:<instance_id>` — triggers `kit.dismiss(id)`.
///
/// Each successful action removes the file. Used to drive snooze /
/// dismiss without relying on flaky UI tap automation.
#[cfg(debug_assertions)]
fn debug_action_poller() {
    #[cfg(not(target_os = "android"))]
    return;

    #[cfg(target_os = "android")]
    {
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let path = mobile_sentinel::app_files_dir().join("debug_action.txt");
            let contents = match std::fs::read_to_string(&path) {
                Ok(s) => s.trim().to_string(),
                Err(_) => continue,
            };
            let kit = match alarm_kit_optional() {
                Some(k) => k,
                None => continue,
            };
            if let Some(id) = contents.strip_prefix("snooze:") {
                let inst = InstanceId::new(id.to_owned());
                if let Err(e) = kit.snooze(&inst) {
                    log::warn!("[AlarmFree] DEBUG snooze({id}) failed: {e:?}");
                } else {
                    log::info!("[AlarmFree] DEBUG snooze({id}) ok");
                }
            } else if contents == "hang_ui" {
                DEBUG_HANG_UI.store(true, std::sync::atomic::Ordering::SeqCst);
                log::info!("[AlarmFree] DEBUG hang_ui requested");
            } else if let Some(id) = contents.strip_prefix("dismiss:") {
                let inst = InstanceId::new(id.to_owned());
                if let Err(e) = kit.dismiss(&inst) {
                    log::warn!("[AlarmFree] DEBUG dismiss({id}) failed: {e:?}");
                } else {
                    log::info!("[AlarmFree] DEBUG dismiss({id}) ok");
                }
            }
            let _ = std::fs::remove_file(&path);
        });
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
        if let Err(e) = kit.delete(&InstanceId::new(alarm_id.to_owned())) {
            log::warn!("[AlarmFree] AlarmKit delete({alarm_id}) failed: {e:?}");
        }
    }
    let _ = mobile_sentinel::remove_job(alarm_id);
    clear_session_files(alarm_id);
}

/// How long the alarm screen may go without proving it is alive before the
/// `:sentinel` guardian restarts the app (see `app::ui_round_trip`). The first
/// beat has to follow the activity coming on screen within this time, so it
/// is generous: a cold start extracts sounds and loads the WebView first.
const UI_LIVENESS_TIMEOUT_MS: u64 = 15_000;

/// Guardian settings for an alarm's job.
fn job_config() -> mobile_sentinel::JobConfig {
    mobile_sentinel::JobConfig {
        poll_interval_ms: 100,
        ui_liveness_timeout_ms: UI_LIVENESS_TIMEOUT_MS,
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
        if job.config.ui_liveness_timeout_ms == UI_LIVENESS_TIMEOUT_MS {
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
            crate::schedule::next_occurrence_date(alarm.time_hour, alarm.time_minute, now)
                .format("%Y-%m-%d")
                .to_string()
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
            .map(|(challenge_type, _difficulty)| ChallengeSpec {
                challenge_type: challenge_type.clone(),
                difficulty: 0,
                config: serde_json::json!({}),
            })
            .collect(),
        vibration_enabled: matches!(alarm.sound_mode, 0 | 2),
        // alarmfree uses one app-wide buzz (the AlarmClassConfig default), so
        // every alarm inherits it. `None` = inherit; a future per-alarm
        // "vibration style" picker would set `Some(pattern)` here.
        vibration_pattern: None,
        kiosk_mode: true,
        bypass_dnd: true,
        skip_until_unix_ms: alarm.skip_until_ms,
        fade_in_secs: alarm.fade_in_secs,
    }
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
            order: 0,
            enabled: true,
            sound: "happy".into(),
            sound_mode: 0,
            challenges: vec![],
            snooze_count: 3,
            snooze_interval: 5,
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
