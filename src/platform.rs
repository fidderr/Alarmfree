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
use mobile_sentinel::jobs::RingEnd;
use mobile_sentinel::recipes::alarm_class::AlarmClassConfig;
use mobile_sentinel::{AlarmKit, AlarmKitConfig, AlarmSpec, ContextStore, InstanceId};
use std::sync::OnceLock;

use crate::app::AlarmData;
use crate::models::{AlarmOutcome, Locale};

/// Fully-qualified name of the Dioxus mobile entry activity. The same
/// value is injected into the Android manifest by `prepare_android` and
/// referenced by the AlarmClass runtime config below.
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

/// The backup screen, for when the app's own cannot come up: as many long holds as
/// the slowest challenge takes, because it must not be the easy way out.
const BACKUP_HOLDS: u32 = 20;
const BACKUP_HOLD_MS: u32 = 3_000;

/// The UI may fail this often during a ring before the backup screen takes over.
const UI_FAILURES_BEFORE_BACKUP: u32 = 2;

/// An alarm whose time passed during a restart still rings if it is less than
/// this late; later than that it is reported as missed.
const LATE_RING_WINDOW_MS: i64 = 15 * 60 * 1000;

/// Global AlarmKit handle; populated during `init_sentinel`.
static ALARM_KIT: OnceLock<AlarmKit> = OnceLock::new();

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

/// Platform setup, idempotent and free of UI. Initialises mobile-sentinel
/// (logger + crash mitigation), installs the Android `FiringSink`, installs
/// AlarmKit with that sink + the Android sound backend, then re-arms every
/// persisted alarm via the universal SDK and switches off the one-time alarms
/// it reports as over.
pub fn init_platform() {
    // `force`: a setup that panicked must not take the app down for good.
    SETUP.call_once_force(|_| setup_platform(false));
}

/// The app was started for something other than the UI, or something happened
/// while it runs without one: alarms may have to be armed again. Needs no Activity.
#[cfg(target_os = "android")]
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

/// The SDK's hook for [`on_wake`]: a boot, an app update, the clock or time zone
/// changing, or an alarm going off with the app down.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_com_mobilesentinel_SentinelRearm_nativeRearm(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    if std::panic::catch_unwind(on_wake).is_err() {
        log::error!("[AlarmFree] re-arming panicked");
    }
}

/// Bring the notifications before alarms in line with the alarms and the setting. An
/// alarm that has gone is taken care of by [`forget_alarm`].
pub fn refresh_reminders() {
    let Some(kit) = alarm_kit_optional() else {
        return;
    };
    let settings = crate::persistence::load_settings();
    let before_ms = i64::from(settings.prefs.notify_before_minutes) * 60_000;
    let translations = crate::i18n::load_translations(settings.locale);
    let text = |key: &str| translations.get(key).cloned().unwrap_or_default();
    let twenty_four = crate::text::uses_24_hours(&settings);
    let now_ms = chrono::Utc::now().timestamp_millis();
    for alarm in crate::persistence::load_alarms() {
        let id = InstanceId::new(alarm.id.clone());
        let next = kit.next_fire(&id).ok().flatten();
        match next
            .filter(|at| before_ms > 0 && alarm.enabled && !alarm.hidden && at - before_ms > now_ms)
        {
            Some(at) => {
                let time = crate::text::clock(alarm.time_hour, alarm.time_minute, twenty_four);
                kit.set_reminder(
                    &id,
                    at - before_ms,
                    &text("reminder.title").replace("{time}", &time),
                    &alarm.label,
                    &text("reminder.channel"),
                    before_ms,
                );
            }
            None => kit.clear_reminder(&id),
        }
    }
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

    let store = Arc::new(ContextStore::new(
        mobile_sentinel::app_files_dir().join("sentinel/context"),
    ));

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
        // Use a full-screen intent so the alarm can wake a locked screen,
        // but the platform suppresses the on-screen heads-up banner when
        // the firing UI is already in the foreground (no redundant popup).
        firing_full_screen: true,
        // Alarm-style buzz for vibration-enabled alarms: pause, long buzz,
        // short gap, long buzz — looped by the firing sink while ringing.
        vibration_pattern: vec![0, 500, 300, 500],
        // The user's minimum alarm volume (Settings); updated live by
        // `set_min_volume` when it changes.
        min_volume_percent: settings.min_volume_percent,
        late_ring_window_ms: LATE_RING_WINDOW_MS,
        fallback_sound_id: Some(SoundIdSpec::Bundled(
            crate::custom_sounds::FALLBACK_SOUND.into(),
        )),
        fallback_title: String::new(),
        backup_holds: BACKUP_HOLDS,
        backup_hold_ms: BACKUP_HOLD_MS,
        backup_text: String::new(),
    };
    apply_ring_texts(&mut alarm_class_config, settings.locale);

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
    kit.on_ring_end(handle_ring_ends);
    let _ = ALARM_KIT.set(kit);

    sync_alarms(alarm_kit(), woken);

    // Debug helpers (test harness only)
    #[cfg(debug_assertions)]
    debug_auto_schedule();
    #[cfg(debug_assertions)]
    debug_action_poller();

    #[cfg(target_os = "android")]
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
    refresh_reminders();
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
            ring_limit_secs: None,
            min_volume_percent: None,
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

/// JavaScript from the harness, waiting for the UI to run it.
#[cfg(debug_assertions)]
static DEBUG_SCRIPT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Debug-only: run what the harness sent in the page, and leave the result in a file.
/// It is how the scenarios press buttons and read the screen on any phone size.
#[cfg(debug_assertions)]
pub async fn debug_script_loop() {
    loop {
        crate::timer::sleep_ms(200).await;
        let script = DEBUG_SCRIPT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let Some(script) = script else {
            continue;
        };
        let mut eval = dioxus::document::eval(&format!(
            "try {{ dioxus.send(String((function () {{ {script} }})())); }} \
             catch (e) {{ dioxus.send('error: ' + e); }}"
        ));
        let result = eval.recv::<String>().await.unwrap_or_default();
        let path = mobile_sentinel::app_files_dir().join("debug_script_result.txt");
        let _ = std::fs::write(path, format!("={result}"));
    }
}

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
            } else if let Some(script) = contents.strip_prefix("js:") {
                *DEBUG_SCRIPT
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(script.to_owned());
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
const UI_LIVENESS_TIMEOUT_MS: u64 = 15_000;

/// How often the guardian checks on a job. The sound does not depend on it.
const POLL_INTERVAL_MS: u64 = 500;

/// Guardian settings for an alarm's job.
fn job_config() -> mobile_sentinel::JobConfig {
    mobile_sentinel::JobConfig {
        poll_interval_ms: POLL_INTERVAL_MS,
        ui_liveness_timeout_ms: UI_LIVENESS_TIMEOUT_MS,
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
            random_order: false,
            wake_check_minutes: 0,
            pause_until_ms: None,
            delete_after_ring: false,
            hidden: false,
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
