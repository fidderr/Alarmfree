use chrono::Timelike;
use dioxus::prelude::*;

use crate::app::{t, AlarmData, Route, Translations};
use crate::models::{AppSettings, ChallengeSetup, ChallengeSolve};
use crate::screens::challenge_run::{prepare, ChallengeRun, ChallengeToRun};
use crate::screens::ring_face::{ring_date, RingFace};

/// How the ring on screen stands.
#[derive(Clone, Copy)]
struct RingTimes {
    /// How often this session was snoozed so far.
    snoozed: u8,
    /// When its first ring started, across snoozes.
    first_fired: chrono::DateTime<chrono::Utc>,
    /// When the ring on screen started.
    ring_started: chrono::DateTime<chrono::Utc>,
}

impl RingTimes {
    fn load(alarm_id: &str) -> Self {
        let now = chrono::Utc::now();
        let session = crate::platform::alarm_kit_optional()
            .and_then(|kit| kit.session(&mobile_sentinel::InstanceId::new(alarm_id.to_owned())));
        let at = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).unwrap_or(now);
        match session {
            Some(s) => Self {
                snoozed: u8::try_from(s.snooze_count).unwrap_or(u8::MAX),
                first_fired: at(s.first_fired_unix_ms),
                ring_started: at(s.fired_at_unix_ms),
            },
            None => Self {
                snoozed: 0,
                first_fired: now,
                ring_started: now,
            },
        }
    }

    /// What history keeps of an alarm the user dealt with just now. The total time
    /// counts from the first ring; the reaction time is to the last one, since
    /// sleeping through a snooze is no reaction.
    fn solved_event(
        self,
        alarm_id: &str,
        label: &str,
        first_interaction: Option<chrono::DateTime<chrono::Utc>>,
        snooze_interval_minutes: u8,
        challenge_solves: Vec<ChallengeSolve>,
    ) -> crate::models::AlarmEvent {
        let now = chrono::Utc::now();
        crate::models::AlarmEvent {
            id: uuid::Uuid::new_v4().to_string(),
            alarm_id: crate::models::AlarmId(alarm_id.to_owned()),
            alarm_label: label.to_owned(),
            fire_timestamp: self.first_fired,
            first_interaction_timestamp: first_interaction
                .map(|at| self.first_fired + (at - self.ring_started)),
            dismiss_timestamp: now,
            snooze_count: self.snoozed,
            snooze_intervals_minutes: vec![snooze_interval_minutes; usize::from(self.snoozed)],
            challenge_solves,
            total_dismissal_time: (now - self.first_fired)
                .to_std()
                .unwrap_or(std::time::Duration::ZERO),
            outcome: crate::models::AlarmOutcome::Solved,
        }
    }
}

/// The alarm is over: what it wants afterwards (a wake-up check), and a one-time alarm
/// switches off or goes.
fn finish_alarm(alarms: &mut Signal<Vec<AlarmData>>, id: &str, check_label: String) {
    let mut list = alarms.write();
    let check = list
        .iter()
        .find(|a| a.id == id && a.wake_check_minutes > 0)
        .map(|a| {
            crate::alarm_ops::wake_check_alarm(a, chrono::Local::now().naive_local(), check_label)
        });
    let mut changed = crate::schedule::finish_if_one_time(&mut list, id);
    if let Some(check) = check {
        list.push(check);
        changed = true;
    }
    drop(list);
    if changed {
        crate::persistence::save_alarms(&alarms.read());
    }
}

/// Alarm firing screen.
/// This screen is a **pure view** over the firing state. All platform
/// orchestration (audio, kiosk, wake-lock, foreground service, notification,
/// exact-alarm scheduling) is owned by mobile-sentinel via AlarmKit + the
/// AlarmClass Recipe. The UI only calls AlarmKit lifecycle methods:
/// - `kit.pause(id)` when entering a challenge
/// - `kit.resume(id)` when a challenge times out back to ringing
/// - `kit.snooze(id)` when the user snoozes (arms the next fire)
/// - `kit.dismiss(id)` when the user dismisses / finishes the challenge
#[component]
pub fn AlarmFiring(id: String) -> Element {
    let translations = use_context::<Translations>();
    let settings = use_context::<Signal<AppSettings>>();
    let mut alarms = use_context::<Signal<Vec<AlarmData>>>();
    // One-shot navigation intent — the firing screen is rendered outside the Router
    // and can't navigate directly.
    let mut nav_intent = use_context::<Signal<Option<Route>>>();

    let alarm = alarms.read().iter().find(|a| a.id == id).cloned();
    let label = alarm
        .as_ref()
        .map(|a| a.label.clone())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| t(&translations, "alarm.fallback_label"));
    let note = alarm.as_ref().map(|a| a.note.clone()).unwrap_or_default();
    let look = alarm
        .as_ref()
        .map(|a| a.ring_look.clone())
        .unwrap_or_default();
    let hidden = alarm.as_ref().is_some_and(|a| a.hidden);
    let max_snoozes = alarm.as_ref().map_or(0u8, |a| a.snooze_count);
    let snooze_minutes = alarm.as_ref().map_or(1u8, |a| a.snooze_interval);
    let setups = alarm
        .as_ref()
        .map(|a| ChallengeSetup::list_from_stored(&a.challenges))
        .unwrap_or_default();

    // How this ring stands, from the SDK, which still knows after a snooze or a
    // restart. Read once: the record is gone after a dismiss, while this still shows.
    let ring = use_hook(|| RingTimes::load(&id));
    let mut snoozes_left = use_signal(|| max_snoozes.saturating_sub(ring.snoozed));
    let mut first_interaction = use_signal(|| None);
    // The challenges being done, if the user got that far.
    let mut running: Signal<Option<Vec<ChallengeToRun>>> = use_signal(|| None);

    // The screen came up: part of the record of how fast this ring was.
    use_hook({
        let id = id.clone();
        move || {
            if let Some(kit) = crate::platform::alarm_kit_optional() {
                kit.note_ui_up(&mobile_sentinel::InstanceId::new(id));
            }
        }
    });

    // Full brightness if this alarm asks for it, as long as this screen is up.
    let bright = alarm.as_ref().is_some_and(|a| a.max_brightness);
    use_hook(move || {
        if bright {
            mobile_sentinel::display::set_max_brightness();
        }
    });
    use_drop(move || {
        if bright {
            mobile_sentinel::display::restore_brightness();
        }
    });

    // A live clock, so a glance tells how long this has been going.
    let mut now = use_signal(chrono::Local::now);
    use_future(move || async move {
        loop {
            crate::timer::sleep_ms(5000).await;
            now.set(chrono::Local::now());
            // Again: the window may only be on top by now, or be a new one.
            if bright {
                mobile_sentinel::display::set_max_brightness();
            }
        }
    });

    // Everything that happens when the alarm is over, whichever way that went.
    let check_label = t(&translations, "wake_check.label");
    let finished = {
        let id = id.clone();
        let label = label.clone();
        use_callback(move |solves: Vec<ChallengeSolve>| {
            if !hidden {
                crate::history_persistence::record_alarm_event(&ring.solved_event(
                    &id,
                    &label,
                    *first_interaction.peek(),
                    snooze_minutes,
                    solves,
                ));
            }
            finish_alarm(&mut alarms, &id, check_label.clone());
            nav_intent.set(Some(if hidden {
                Route::Home {}
            } else {
                Route::History {}
            }));
        })
    };

    if let Some(challenges) = running() {
        let solved_id = id.clone();
        let idle_id = id.clone();
        return rsx! {
            ChallengeRun {
                challenges,
                on_solved: move |solves: Vec<ChallengeSolve>| {
                    let kit = crate::platform::alarm_kit();
                    let instance = mobile_sentinel::InstanceId::new(solved_id.clone());
                    let _ = kit.mark_solved(&instance);
                    let _ = kit.dismiss(&instance);
                    finished.call(solves);
                },
                on_idle: move |_| {
                    running.set(None);
                    // AlarmKit brings the sound back.
                    if let Some(kit) = crate::platform::alarm_kit_optional() {
                        let _ = kit.resume(&mobile_sentinel::InstanceId::new(idle_id.clone()));
                    }
                },
            }
        };
    }

    let moment = now();
    let twenty_four = crate::text::uses_24_hours(&settings.read());
    let clock = crate::text::clock(moment.hour() as u8, moment.minute() as u8, twenty_four);
    let date_text = ring_date(&translations, moment);
    let snooze_text = t(&translations, "ring.snooze")
        .replace("{minutes}", &snooze_minutes.to_string())
        .replace("{left}", &snoozes_left().to_string());
    let turn_off_text = t(&translations, "ring.turn_off");
    let total_challenges = setups.len();
    let to_go_text = if total_challenges == 1 {
        t(&translations, "ring.one_challenge")
    } else {
        t(&translations, "ring.challenges").replace("{count}", &total_challenges.to_string())
    };

    rsx! {
        RingFace {
            look,
            date: date_text,
            clock,
            twenty_four,
            label,
            note,
            div { class: "grow" }

            if snoozes_left() > 0 {
                button {
                    class: "btn ring-snooze",
                    onclick: {
                        let id = id.clone();
                        move |_| {
                            let left = snoozes_left();
                            if left == 0 {
                                return;
                            }
                            snoozes_left.set(left - 1);
                            let kit = crate::platform::alarm_kit();
                            let instance = mobile_sentinel::InstanceId::new(id.clone());
                            if let Err(e) = kit.snooze(&instance) {
                                log::warn!("[AlarmFree] AlarmKit snooze failed for {id}: {e:?}");
                            }
                            // With another alarm still ringing, the screen moves on to
                            // that one.
                            if matches!(kit.current_session(), Ok(None)) {
                                mobile_sentinel::foregrounding::finish_activity();
                            }
                        }
                    },
                    "{snooze_text}"
                }
            }

            if total_challenges > 0 {
                div { class: "ring-challenges",
                    span { "{to_go_text}" }
                    // One dot after "1 challenge" only looks like a stray full stop.
                    if total_challenges > 1 {
                        span { class: "dots", "aria-hidden": "true",
                            for _ in 0..total_challenges {
                                span { class: "dot" }
                            }
                        }
                    }
                }
            } else {
                div { class: "ring-gap" }
            }

            button {
                class: "btn btn-primary ring-off",
                onclick: {
                    let id = id.clone();
                    move |_| {
                        if first_interaction.peek().is_none() {
                            first_interaction.set(Some(chrono::Utc::now()));
                        }
                        let kit = crate::platform::alarm_kit();
                        let instance = mobile_sentinel::InstanceId::new(id.clone());
                        let prepared = prepare(&setups, settings.peek().locale);
                        if prepared.is_empty() {
                            // No challenge, or none this version knows: the SDK refuses
                            // to dismiss a challenge alarm until it is marked solved, so
                            // mark it first or the user would be stuck in the lockdown.
                            let _ = kit.mark_solved(&instance);
                            let _ = kit.dismiss(&instance);
                            finished.call(Vec::new());
                        } else {
                            let _ = kit.pause(&instance);
                            running.set(Some(prepared));
                        }
                    }
                },
                "{turn_off_text}"
            }
        }
    }
}
