use crate::app::{t, AlarmData, Route, Translations};
use crate::components::challenges::{
    HoldButtonChallenge, MathChallenge, MemoryGameChallenge, ReactionChallenge, ScanChallenge,
    ShakeChallenge, StepCountChallenge, TypingChallenge,
};
use crate::components::{Icon, IconName};
use dioxus::prelude::*;

use crate::models::{
    ChallengeAnswer, ChallengeConfig, ChallengeEngine, ChallengePrompt, ChallengeType, Difficulty,
};

/// Alarm firing states
#[derive(Clone, PartialEq)]
enum FiringPhase {
    /// Alarm is ringing — show snooze/dismiss buttons
    Ringing,
    /// User tapped dismiss — show challenge to solve
    Challenge,
}

/// Rate limiter to prevent brute-forcing challenge answers.
#[derive(Clone, Debug)]
struct RateLimiter {
    consecutive_wrong: u8,
    /// Timestamp (seconds since component mount) when cooldown ends
    cooldown_until_secs: Option<u32>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            consecutive_wrong: 0,
            cooldown_until_secs: None,
        }
    }

    /// Record a wrong answer. Returns cooldown duration in seconds if threshold hit.
    fn record_wrong(&mut self, current_secs: u32) -> Option<u32> {
        self.consecutive_wrong += 1;
        let cooldown = match self.consecutive_wrong {
            3..=4 => Some(5u32),
            5..=9 => Some(15u32),
            10.. => Some(30u32),
            _ => None,
        };
        if let Some(d) = cooldown {
            self.cooldown_until_secs = Some(current_secs + d);
        }
        cooldown
    }

    /// Record a correct answer — resets the counter.
    fn record_correct(&mut self) {
        self.consecutive_wrong = 0;
        self.cooldown_until_secs = None;
    }

    /// Check if currently in cooldown.
    fn is_cooling_down(&self, current_secs: u32) -> bool {
        self.cooldown_until_secs
            .is_some_and(|until| current_secs < until)
    }

    /// Get remaining cooldown seconds.
    fn remaining_cooldown(&self, current_secs: u32) -> u32 {
        self.cooldown_until_secs
            .map(|until| until.saturating_sub(current_secs))
            .unwrap_or(0)
    }
}

/// Generated challenge data stored in signals (pre-generated to avoid StdRng Send issues)
#[derive(Clone, Debug)]
struct GeneratedChallenge {
    challenge_type: String,
    /// Enum form of challenge type for history records.
    challenge_type_enum: ChallengeType,
    /// Difficulty for history records.
    difficulty: Difficulty,
    /// For math: the expression string
    expression: Option<String>,
    /// For math: the expected numeric answer
    expected_number: Option<i64>,
    /// For memory: the sequence
    memory_sequence: Option<Vec<u8>>,
    /// For memory: grid size (3 or 4 columns)
    memory_grid_size: Option<u8>,
    /// For typing: the passage
    typing_passage: Option<String>,
    /// For typing: minimum accuracy
    typing_min_accuracy: Option<f32>,
    /// For shake/step/hold: required count
    required_count: Option<u32>,
    /// For barcode/qr: expected value (empty = any)
    expected_value: Option<String>,
    /// For hold-button: how long each bubble must be held (ms)
    hold_duration_ms: Option<u32>,
    /// For reaction: time limit per round (ms)
    reaction_time_ms: Option<u32>,
    /// For reaction: number of rounds
    reaction_rounds: Option<u32>,
}

/// Map challenge type key string to ChallengeType enum
fn parse_challenge_type(key: &str) -> Option<ChallengeType> {
    match key {
        "challenge.math" => Some(ChallengeType::Math),
        "challenge.memory" => Some(ChallengeType::MemoryGame),
        "challenge.typing" => Some(ChallengeType::Typing),
        "challenge.shake" => Some(ChallengeType::ShakeToWake),
        "challenge.steps" => Some(ChallengeType::StepCount),
        "challenge.scan" => Some(ChallengeType::Scan),
        "challenge.hold" => Some(ChallengeType::HoldButton),
        "challenge.reaction" => Some(ChallengeType::Reaction),
        _ => None,
    }
}

/// Generate all challenges eagerly from the alarm's challenge list
fn generate_challenges(challenge_list: &[(String, String)]) -> Vec<GeneratedChallenge> {
    use crate::models::ReferenceData;
    let mut engine = ChallengeEngine::new();
    let mut generated = Vec::new();

    for (type_key, config_json) in challenge_list {
        let Some(challenge_type) = parse_challenge_type(type_key) else {
            continue;
        };

        // Parse the challenge config JSON
        let config: serde_json::Value =
            serde_json::from_str(config_json).unwrap_or(serde_json::json!({}));

        let difficulty = config
            .get("difficulty")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "easy" => Difficulty::Easy,
                "medium" => Difficulty::Medium,
                "hard" => Difficulty::Hard,
                "extreme" => Difficulty::Extreme,
                _ => Difficulty::Easy,
            })
            .unwrap_or(Difficulty::Easy);

        // For shake/step/hold: use custom count from config
        let custom_count = config
            .get("count")
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);

        // For hold-button: how long each bubble must be held (ms).
        let hold_ms = config
            .get("hold_ms")
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);

        // For reaction: time limit per round (ms) + number of rounds.
        let reaction_time_ms = config
            .get("time_ms")
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);
        let reaction_rounds = config
            .get("rounds")
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);

        // For scan: use custom value from config
        let custom_value = config
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        // Shake / step are pure count-only challenges — no difficulty involved.
        // Build directly from `custom_count` and skip the engine entirely,
        // exactly like scan builds directly from `value` without consulting
        // difficulty params. If the user never set a count, fall back to 5 —
        // never to a difficulty-derived default.
        if matches!(challenge_type, ChallengeType::ShakeToWake) {
            let count = custom_count.unwrap_or(5);
            generated.push(GeneratedChallenge {
                challenge_type: "shake".to_string(),
                challenge_type_enum: ChallengeType::ShakeToWake,
                // Shake doesn't use difficulty for any decision, but the
                // history record requires a value — record `Easy` so the row
                // is deterministic.
                difficulty: Difficulty::Easy,
                expression: None,
                expected_number: None,
                memory_sequence: None,
                memory_grid_size: None,
                typing_passage: None,
                typing_min_accuracy: None,
                required_count: Some(count),
                expected_value: None,
                hold_duration_ms: None,
                reaction_time_ms: None,
                reaction_rounds: None,
            });
            continue;
        }
        if matches!(challenge_type, ChallengeType::StepCount) {
            let count = custom_count.unwrap_or(5);
            generated.push(GeneratedChallenge {
                challenge_type: "step_count".to_string(),
                challenge_type_enum: ChallengeType::StepCount,
                difficulty: Difficulty::Easy,
                expression: None,
                expected_number: None,
                memory_sequence: None,
                memory_grid_size: None,
                typing_passage: None,
                typing_min_accuracy: None,
                required_count: Some(count),
                expected_value: None,
                hold_duration_ms: None,
                reaction_time_ms: None,
                reaction_rounds: None,
            });
            continue;
        }
        // Hold-button: pure count + duration, no difficulty. Build directly
        // from config (count = number of bubbles, hold_ms = fill time).
        if matches!(challenge_type, ChallengeType::HoldButton) {
            let count = custom_count.unwrap_or(10);
            let duration = hold_ms.unwrap_or(3000);
            generated.push(GeneratedChallenge {
                challenge_type: "hold".to_string(),
                challenge_type_enum: ChallengeType::HoldButton,
                difficulty: Difficulty::Easy,
                expression: None,
                expected_number: None,
                memory_sequence: None,
                memory_grid_size: None,
                typing_passage: None,
                typing_min_accuracy: None,
                required_count: Some(count),
                expected_value: None,
                hold_duration_ms: Some(duration),
                reaction_time_ms: None,
                reaction_rounds: None,
            });
            continue;
        }
        // Reaction: pure count + time + rounds, no difficulty. Build directly
        // from config (count = targets per round, time_ms = per-round limit,
        // rounds = number of rounds).
        if matches!(challenge_type, ChallengeType::Reaction) {
            let count = custom_count.unwrap_or(10);
            let time_ms = reaction_time_ms.unwrap_or(3000);
            let rounds = reaction_rounds.unwrap_or(2);
            generated.push(GeneratedChallenge {
                challenge_type: "reaction".to_string(),
                challenge_type_enum: ChallengeType::Reaction,
                difficulty: Difficulty::Easy,
                expression: None,
                expected_number: None,
                memory_sequence: None,
                memory_grid_size: None,
                typing_passage: None,
                typing_min_accuracy: None,
                required_count: Some(count),
                expected_value: None,
                hold_duration_ms: None,
                reaction_time_ms: Some(time_ms),
                reaction_rounds: Some(rounds),
            });
            continue;
        }

        let challenge_config = ChallengeConfig {
            challenge_type,
            difficulty,
            reference_data: match challenge_type {
                ChallengeType::Scan => custom_value.clone().map(ReferenceData::Scan),
                _ => None,
            },
        };

        let instance = engine.generate(&challenge_config);
        let gen = match instance.prompt {
            ChallengePrompt::Math { expression } => {
                let expected = match instance.expected_answer {
                    ChallengeAnswer::Numeric(n) => n,
                    _ => 0,
                };
                GeneratedChallenge {
                    challenge_type: "math".to_string(),
                    challenge_type_enum: ChallengeType::Math,
                    difficulty,
                    expression: Some(expression),
                    expected_number: Some(expected),
                    memory_sequence: None,
                    memory_grid_size: None,
                    typing_passage: None,
                    typing_min_accuracy: None,
                    required_count: None,
                    expected_value: None,
                    hold_duration_ms: None,
                    reaction_time_ms: None,
                    reaction_rounds: None,
                }
            }
            ChallengePrompt::MemoryGame { sequence, .. } => {
                let grid = match difficulty {
                    Difficulty::Hard | Difficulty::Extreme => 4u8,
                    _ => 3u8,
                };
                GeneratedChallenge {
                    challenge_type: "memory".to_string(),
                    challenge_type_enum: ChallengeType::MemoryGame,
                    difficulty,
                    expression: None,
                    expected_number: None,
                    memory_sequence: Some(sequence),
                    memory_grid_size: Some(grid),
                    typing_passage: None,
                    typing_min_accuracy: None,
                    required_count: None,
                    expected_value: None,
                    hold_duration_ms: None,
                    reaction_time_ms: None,
                    reaction_rounds: None,
                }
            }
            ChallengePrompt::Typing { passage } => {
                let min_acc = match instance.expected_answer {
                    ChallengeAnswer::TypedText { min_accuracy, .. } => min_accuracy,
                    _ => 0.80,
                };
                GeneratedChallenge {
                    challenge_type: "typing".to_string(),
                    challenge_type_enum: ChallengeType::Typing,
                    difficulty,
                    expression: None,
                    expected_number: None,
                    memory_sequence: None,
                    memory_grid_size: None,
                    typing_passage: Some(passage),
                    typing_min_accuracy: Some(min_acc),
                    required_count: None,
                    expected_value: None,
                    hold_duration_ms: None,
                    reaction_time_ms: None,
                    reaction_rounds: None,
                }
            }
            ChallengePrompt::ShakeToWake { .. } | ChallengePrompt::StepCount { .. } => {
                // Already handled above before the engine was invoked.
                continue;
            }
            ChallengePrompt::HoldButton { .. } | ChallengePrompt::Reaction { .. } => {
                // Already handled above before the engine was invoked.
                continue;
            }
            ChallengePrompt::Scan { expected_value } => GeneratedChallenge {
                challenge_type: "scan".to_string(),
                challenge_type_enum: ChallengeType::Scan,
                difficulty,
                expression: None,
                expected_number: None,
                memory_sequence: None,
                memory_grid_size: None,
                typing_passage: None,
                typing_min_accuracy: None,
                required_count: None,
                expected_value: Some(custom_value.unwrap_or(expected_value)),
                hold_duration_ms: None,
                reaction_time_ms: None,
                reaction_rounds: None,
            },
        };
        generated.push(gen);
    }

    generated
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
    let alarms = use_context::<Signal<Vec<AlarmData>>>();
    // One-shot navigation intent — set on successful dismiss so the app
    // lands on History when the Router remounts (the firing screen is
    // rendered outside the Router and can't navigate directly).
    let mut nav_intent = use_context::<Signal<Option<crate::app::Route>>>();

    // Find the alarm data
    let alarm = alarms.read().iter().find(|a| a.id == id).cloned();
    let alarm_label = alarm
        .as_ref()
        .map(|a| a.label.clone())
        .unwrap_or_else(|| t(&translations, "alarm.fallback_label"));
    let alarm_time = alarm.as_ref().map_or("--:--".to_string(), |a| {
        format!("{:02}:{:02}", a.time_hour, a.time_minute)
    });
    let max_snoozes = alarm.as_ref().map_or(3u8, |a| a.snooze_count);
    let snooze_interval_minutes = alarm.as_ref().map_or(1u8, |a| a.snooze_interval);
    let has_challenges = alarm.as_ref().is_some_and(|a| !a.challenges.is_empty());
    let challenge_list = alarm
        .as_ref()
        .map(|a| a.challenges.clone())
        .unwrap_or_default();

    let mut phase = use_signal(|| FiringPhase::Ringing);
    let mut challenge_timeout = use_signal(|| 0u32);

    // Persist snooze count across re-fires using a simple file
    let snooze_file = mobile_sentinel::app_files_dir().join(format!("snooze_{}.txt", id));
    let snooze_file2 = snooze_file.clone();
    let snooze_file3 = snooze_file.clone();
    // Persist snooze intervals across re-fires too — each re-fire remounts this
    // component, so an in-memory vec would be reset and `total_snoozed` would
    // come out to zero even though `snooze_count` is correct from disk.
    let snooze_intervals_file =
        mobile_sentinel::app_files_dir().join(format!("snooze_intervals_{}.txt", id));
    let snooze_intervals_file2 = snooze_intervals_file.clone();
    let snooze_intervals_file3 = snooze_intervals_file.clone();
    let snoozed_so_far = std::fs::read_to_string(&snooze_file)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok())
        .unwrap_or(0);
    let intervals_so_far: Vec<u8> = std::fs::read_to_string(&snooze_intervals_file)
        .ok()
        .map(|s| {
            s.split(',')
                .filter_map(|v| v.trim().parse::<u8>().ok())
                .collect()
        })
        .unwrap_or_default();
    let remaining = max_snoozes.saturating_sub(snoozed_so_far);
    let mut snoozes_remaining = use_signal(|| remaining);
    let mut snooze_count_used = use_signal(|| snoozed_so_far);

    // Challenge sequence state — generated eagerly when entering challenge phase
    let mut challenges: Signal<Vec<GeneratedChallenge>> = use_signal(Vec::new);
    let mut current_challenge_index = use_signal(|| 0usize);
    let mut answer_feedback = use_signal(|| ""); // "wrong" or "correct" or ""
    let rate_limiter = use_signal(RateLimiter::new);
    // Fallback mode: when the user can't complete the configured challenge
    // (broken sensor, missing QR code, unsolvable on this device, …) they
    // can switch the CURRENT challenge to the universal hold-the-bubble
    // fallback, which needs nothing but touch. Solving it advances exactly
    // like a normal solve. Reset to false on every challenge change.
    let mut using_fallback = use_signal(|| false);
    // Two-step arm for the fallback danger button: first tap reveals an
    // explanatory tooltip, second tap (while armed) actually starts the
    // fallback. Prevents accidental activation.
    let mut fallback_armed = use_signal(|| false);
    // Celebration state: when true, the success overlay is shown briefly
    // before advancing to the next challenge.
    let celebrating: Signal<bool> = use_signal(|| false);

    // Per-challenge solve tracking: pushed when on_complete fires.
    let challenge_solves: Signal<Vec<crate::models::ChallengeSolve>> = use_signal(Vec::new);
    // When the current challenge mounted (so we can compute its solve duration).
    let challenge_started_at: Signal<Option<chrono::DateTime<chrono::Utc>>> = use_signal(|| None);
    // Per-snooze interval tracking (minutes used per snooze tap).
    // Seeded from disk so re-fires after a snooze keep the prior intervals.
    let mut snooze_intervals_used: Signal<Vec<u8>> = use_signal(|| intervals_so_far.clone());

    // History metrics — track timestamps for recording alarm events
    let fire_timestamp = use_signal(chrono::Utc::now);
    let mut first_interaction: Signal<Option<chrono::DateTime<chrono::Utc>>> = use_signal(|| None);
    let alarm_id_for_history = id.clone();
    let alarm_label_for_history = alarm_label.clone();

    // Translation strings
    let snooze_text = t(&translations, "snooze.button");
    let dismiss_text = t(&translations, "action.dismiss");
    let snooze_left_text = t(&translations, "snooze.left");
    let alarm_ringing_text = t(&translations, "alarm.ringing");
    let loading_text = t(&translations, "common.loading");
    let incorrect_text = t(&translations, "challenge.incorrect");
    let fallback_title_text = t(&translations, "challenge.fallback.title");
    let fallback_explain_text = t(&translations, "challenge.fallback.explain");
    let fallback_confirm_text = t(&translations, "challenge.fallback.confirm");
    let fallback_cancel_text = t(&translations, "action.cancel");
    let fallback_aria_text = t(&translations, "challenge.fallback.aria");

    // 60-second inactivity timeout on challenge screen → back to ringing.
    // Resets whenever the user interacts (click/touch/input on the challenge div)
    // or when shake/step sensor counts change.
    let phase_clone = phase;
    let id_for_timeout = id.clone();
    let mut last_shake = use_signal(|| 0i32);
    let mut last_steps = use_signal(|| 0i32);
    use_future(move || {
        let id_for_timeout = id_for_timeout.clone();
        async move {
            loop {
                let (tx, rx) = futures_channel::oneshot::channel::<()>();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let _ = tx.send(());
                });
                let _ = rx.await;

                if *phase_clone.read() == FiringPhase::Challenge {
                    // Check if sensor counts changed (shake/step activity)
                    let current_shake = mobile_sentinel::sensors::shake_count();
                    let current_steps = mobile_sentinel::sensors::step_count();
                    if current_shake != *last_shake.read() || current_steps != *last_steps.read() {
                        last_shake.set(current_shake);
                        last_steps.set(current_steps);
                        challenge_timeout.set(0);
                    }

                    let current = challenge_timeout();
                    challenge_timeout.set(current + 1);
                    if current >= 60 {
                        phase.set(FiringPhase::Ringing);
                        challenge_timeout.set(0);
                        current_challenge_index.set(0);
                        challenges.set(Vec::new());
                        using_fallback.set(false);
                        fallback_armed.set(false);
                        // Universal SDK resume: AlarmKit owns the audio resume
                        // via the FiringSink.
                        if let Some(kit) = crate::platform::alarm_kit_optional() {
                            let _ = kit
                                .resume(&mobile_sentinel::InstanceId::new(id_for_timeout.clone()));
                        }
                    }
                } else {
                    challenge_timeout.set(0);
                }
            }
        }
    });

    rsx! {
           div {
               class: if *phase.read() == FiringPhase::Challenge { "screen alarm-firing-screen in-challenge" } else { "screen alarm-firing-screen" },
               match &*phase.read() {
                   FiringPhase::Ringing => rsx! {
    // Hero: animated pulsing bell + time + label, centered in the
    // flexible space at the top.
                       div { class: "firing-hero",
                           div { class: "firing-bell",
                               span { class: "firing-bell-ring" }
                               span { class: "firing-bell-ring firing-bell-ring-2" }
                               Icon { name: IconName::Bell, size: 44 }
                           }
                           h1 { class: "time-display", "{alarm_time}" }
                           p { class: "firing-label", "{alarm_label}" }
                           p { class: "firing-status", "{alarm_ringing_text}" }
                       }

    // Action buttons. Snooze sits high and is small/secondary; dismiss is
    // the large primary button anchored at the bottom with a big gap
    // between them so neither is hit by accident.
                       div { class: "firing-actions",
    // Snooze button
                           if *snoozes_remaining.read() > 0 {
                               {
                                   let id_for_snooze = id.clone();
                                   rsx! {
                                       button {
                                           class: "snooze-btn",
                                           onclick: move |_| {
                                               let remaining = *snoozes_remaining.read();
                                               if remaining > 0 {
                                                   if first_interaction.read().is_none() {
                                                       first_interaction.set(Some(chrono::Utc::now()));
                                                   }
                                                   snooze_count_used.set(snooze_count_used() + 1);
                                                   snoozes_remaining.set(remaining - 1);
                                                   // Record snooze interval for history metrics.
                                                   snooze_intervals_used.write().push(snooze_interval_minutes);
                                                   let _ = std::fs::write(&snooze_file, snooze_count_used().to_string());
                                                   // Persist intervals so they survive the re-fire remount.
                                                   let intervals_str: String = snooze_intervals_used
                                                       .read()
                                                       .iter()
                                                       .map(|n| n.to_string())
                                                       .collect::<Vec<_>>()
                                                       .join(",");
                                                   let _ = std::fs::write(&snooze_intervals_file, intervals_str);

                                                   let kit = crate::platform::alarm_kit();
                                                   let instance_id = mobile_sentinel::InstanceId::new(id_for_snooze.clone());
                                                   if let Err(_e) = kit.snooze(&instance_id) {
                                                       #[cfg(target_os = "android")]
                                                       log::warn!("[AlarmFree] AlarmKit snooze failed for {}: {:?}", id_for_snooze, _e);
                                                   }
                                                   #[cfg(target_os = "android")]
                                                   {
                                                       mobile_sentinel::foregrounding::finish_activity();
                                                   }
                                               }
                                           },
                                           Icon { name: IconName::Stopwatch, size: 16 }
                                           " {snooze_text} · {snoozes_remaining} {snooze_left_text}"
                                       }
                                   }
                               }
                           }

    // Dismiss button
                           {
                               let id_for_dismiss = id.clone();
                               rsx! {
                                   button {
                                       class: "dismiss-btn",
                                       onclick: move |_| {
                                           if first_interaction.read().is_none() {
                                               first_interaction.set(Some(chrono::Utc::now()));
                                           }

                                           if has_challenges {
                                               let kit = crate::platform::alarm_kit();
                                               let instance_id = mobile_sentinel::InstanceId::new(id_for_dismiss.clone());
                                               let _ = kit.pause(&instance_id);
                                               let generated = generate_challenges(&challenge_list);
                                               if generated.is_empty() {
                                                   let _ = kit.dismiss(&instance_id);
                                                   let now = chrono::Utc::now();
                                                   let event = crate::models::AlarmEvent {
                                                       id: uuid::Uuid::new_v4().to_string(),
                                                       alarm_id: crate::models::AlarmId(alarm_id_for_history.clone()),
                                                       alarm_label: alarm_label_for_history.clone(),
                                                       fire_timestamp: *fire_timestamp.read(),
                                                       first_interaction_timestamp: *first_interaction.read(),
                                                       dismiss_timestamp: now,
                                                       snooze_count: *snooze_count_used.read(),
                                                       snooze_intervals_minutes: snooze_intervals_used.read().clone(),
                                                       challenge_solves: challenge_solves.read().clone(),
                                                       total_dismissal_time: (now - *fire_timestamp.read())
                                                           .to_std()
                                                           .unwrap_or(std::time::Duration::from_secs(0)),
                                                   };
                                                   crate::history_persistence::record_alarm_event(&event);
                                                   let _ = std::fs::remove_file(&snooze_file2);
                                                   let _ = std::fs::remove_file(&snooze_intervals_file2);
                                                   // Successful dismiss → land on History when the
                                                   // Router remounts (don't close the app).
                                                   nav_intent.set(Some(Route::History {}));
                                               } else {
                                                   challenges.set(generated);
                                                   current_challenge_index.set(0);
                                                   phase.set(FiringPhase::Challenge);
                                                   challenge_timeout.set(0);
                                                   answer_feedback.set("");
                                               }
                                           } else {
                                               let kit = crate::platform::alarm_kit();
                                               let instance_id = mobile_sentinel::InstanceId::new(id_for_dismiss.clone());
                                               let _ = kit.dismiss(&instance_id);
                                               let now = chrono::Utc::now();
                                               let event = crate::models::AlarmEvent {
                                                   id: uuid::Uuid::new_v4().to_string(),
                                                   alarm_id: crate::models::AlarmId(alarm_id_for_history.clone()),
                                                   alarm_label: alarm_label_for_history.clone(),
                                                   fire_timestamp: *fire_timestamp.read(),
                                                   first_interaction_timestamp: *first_interaction.read(),
                                                   dismiss_timestamp: now,
                                                   snooze_count: *snooze_count_used.read(),
                                                   snooze_intervals_minutes: snooze_intervals_used.read().clone(),
                                                   challenge_solves: challenge_solves.read().clone(),
                                                   total_dismissal_time: (now - *fire_timestamp.read())
                                                       .to_std()
                                                       .unwrap_or(std::time::Duration::from_secs(0)),
                                               };
                                               crate::history_persistence::record_alarm_event(&event);
                                               let _ = std::fs::remove_file(&snooze_file3);
                                               let _ = std::fs::remove_file(&snooze_intervals_file3);
                                               // Successful dismiss (no challenges) → land on
                                               // History when the Router remounts.
                                               nav_intent.set(Some(Route::History {}));
                                           }
                                       },
                                       Icon { name: IconName::Check, size: 20 }
                                       " {dismiss_text}"
                                   }
                               }
                           }
                       }
                   },

                   FiringPhase::Challenge => rsx! {
                       div {
                           class: "challenge-area",
                           // Reset inactivity timer on any interaction
                           onclick: move |_| { challenge_timeout.set(0); },
                           oninput: move |_| { challenge_timeout.set(0); },
                           ontouchstart: move |_| { challenge_timeout.set(0); },

    // Compact meta row: progress + timer pills, centered. The challenge's
    // own instruction (rendered inside the card below) is the real heading,
    // so there's no redundant generic title competing for space here.
                           div { class: "challenge-meta",
                               {
                                   let total = challenges.read().len();
                                   let current = *current_challenge_index.read() + 1;
                                   rsx! {
                                       if total > 1 {
                                           span { class: "challenge-pill challenge-pill-progress", "{current} / {total}" }
                                       }
                                   }
                               }
                               {
                                   let secs_left = 60u32.saturating_sub(*challenge_timeout.read());
                                   let low = secs_left <= 10;
                                   let pill_class = if low { "challenge-pill challenge-pill-timer low" } else { "challenge-pill challenge-pill-timer" };
                                   rsx! {
                                       span { class: "{pill_class}",
                                           Icon { name: IconName::Stopwatch, size: 13 }
                                           " {secs_left}s"
                                       }
                                   }
                               }
                           }


    // Render current challenge
                           {
                               let idx = *current_challenge_index.read();
                               let challenge_list = challenges.read();
                               if let Some(challenge) = challenge_list.get(idx) {
                                   let challenge = challenge.clone();
                                   drop(challenge_list);
                                   let aid = alarm_id_for_history.clone();
                                   let alabel = alarm_label_for_history.clone();
                                   let aid_for_kit = alarm_id_for_history.clone();
                                   render_challenge(
                                       challenge,
                                       idx,
                                       challenges.read().len(),
                                       current_challenge_index,
                                       answer_feedback,
                                       EventHandler::new(move |_| {
                                           let now = chrono::Utc::now();
                                           let event = crate::models::AlarmEvent {
                                               id: uuid::Uuid::new_v4().to_string(),
                                               alarm_id: crate::models::AlarmId(aid.clone()),
                                               alarm_label: alabel.clone(),
                                               fire_timestamp: *fire_timestamp.read(),
                                               first_interaction_timestamp: *first_interaction.read(),
                                               dismiss_timestamp: now,
                                               snooze_count: *snooze_count_used.read(),
                                               snooze_intervals_minutes: snooze_intervals_used.read().clone(),
                                               challenge_solves: challenge_solves.read().clone(),
                                               total_dismissal_time: (now - *fire_timestamp.read())
                                                   .to_std()
                                                   .unwrap_or(std::time::Duration::from_secs(0)),
                                           };
                                           crate::history_persistence::record_alarm_event(&event);
    // Clean up snooze persistence file
                                           let snooze_cleanup = mobile_sentinel::app_files_dir().join(format!("snooze_{}.txt", aid));
                                           let _ = std::fs::remove_file(snooze_cleanup);
                                           // All challenges solved → land on History when the
                                           // Router remounts (the firing screen exits once the
                                           // ContextStore poll sees the dismissed state).
                                           nav_intent.set(Some(Route::History {}));
                                       }),
                                       rate_limiter,
                                       challenge_timeout,
                                       celebrating,
                                       challenge_solves,
                                       challenge_started_at,
                                       using_fallback,
                                       fallback_armed,
                                       aid_for_kit,
                                       t(&translations, "challenge.unknown_type"),
                                   )
                               } else {
                                   rsx! { p { "{loading_text}" } }
                               }
                           }

    // Celebration overlay — shown after a challenge solve before advancing
                           if *celebrating.read() {
                               {
                                   let total = challenges.read().len();
                                   let current = *current_challenge_index.read() + 1;
                                   let label = if total > 1 {
                                       format!("{} / {}", current, total)
                                   } else {
                                       String::new()
                                   };
                                   rsx! {
                                       crate::components::Celebration {
                                           label: label,
                                       }
                                   }
                               }
                           }

    // Feedback overlay — fixed-position toast so it never shifts the
    // challenge layout when it appears/disappears.
                           if *answer_feedback.read() == "wrong" {
                               {
                                   let timeout_secs = *challenge_timeout.read();
                                   let cooling = rate_limiter.read().is_cooling_down(timeout_secs);
                                   let remaining = rate_limiter.read().remaining_cooldown(timeout_secs);
                                   if cooling {
                                       let cooldown_text = t(&translations, "challenge.try_again_in")
                                           .replace("{seconds}", &remaining.to_string());
                                       rsx! {
                                           div { class: "answer-feedback-toast",
                                               "{cooldown_text}"
                                           }
                                       }
                                   } else {
                                       rsx! {
                                           div { class: "answer-feedback-toast",
                                               "{incorrect_text}"
                                           }
                                       }
                                   }
                               }
                           }

    // Fallback escape hatch — a small red danger button anchored bottom-right
    // so it never pushes the challenge layout. First tap arms it and shows an
    // overlay popup explaining what it does; tapping again while armed switches
    // the current challenge to the universal touch-only hold challenge. Hidden
    // once in fallback or celebrating.
                           if !*using_fallback.read() && !*celebrating.read() {
                               button {
                                   class: if *fallback_armed.read() { "fallback-fab armed" } else { "fallback-fab" },
                                   "aria-label": "{fallback_aria_text}",
                                   onclick: move |_| {
                                       challenge_timeout.set(0);
                                       if *fallback_armed.read() {
                                           fallback_armed.set(false);
                                           answer_feedback.set("");
                                           using_fallback.set(true);
                                       } else {
                                           fallback_armed.set(true);
                                       }
                                   },
                                   "!"
                               }
                           }

    // Fallback explanation — floating overlay popup so it never shifts
    // the challenge layout. Tap outside to dismiss; the danger button
    // itself confirms (second tap).
                           if *fallback_armed.read() && !*using_fallback.read() {
                               div {
                                   class: "fallback-overlay",
                                   onclick: move |_| { fallback_armed.set(false); },
                                   div {
                                       class: "fallback-popup",
                                       onclick: move |evt| { evt.stop_propagation(); },
                                       p { class: "fallback-popup-title", "{fallback_title_text}" }
                                       p { class: "fallback-popup-body", "{fallback_explain_text}" }
                                       div { class: "fallback-popup-actions",
                                           button {
                                               class: "fallback-popup-cancel",
                                               onclick: move |_| { fallback_armed.set(false); },
                                               "{fallback_cancel_text}"
                                           }
                                           button {
                                               class: "fallback-popup-confirm",
                                               onclick: move |_| {
                                                   challenge_timeout.set(0);
                                                   fallback_armed.set(false);
                                                   answer_feedback.set("");
                                                   using_fallback.set(true);
                                               },
                                               "{fallback_confirm_text}"
                                           }
                                       }
                                   }
                               }
                           }
                       }
                   },
               }
           }
       }
}

/// Render the appropriate challenge component based on type
#[allow(clippy::too_many_arguments)]
fn render_challenge(
    challenge: GeneratedChallenge,
    current_idx: usize,
    total: usize,
    mut current_challenge_index: Signal<usize>,
    mut answer_feedback: Signal<&'static str>,
    on_all_solved: EventHandler<()>,
    mut rate_limiter: Signal<RateLimiter>,
    mut challenge_timeout: Signal<u32>,
    mut celebrating: Signal<bool>,
    mut challenge_solves: Signal<Vec<crate::models::ChallengeSolve>>,
    mut challenge_started_at: Signal<Option<chrono::DateTime<chrono::Utc>>>,
    mut using_fallback: Signal<bool>,
    mut fallback_armed: Signal<bool>,
    instance_id: String,
    unknown_type_text: String,
) -> Element {
    // Mark the start time on first render of each challenge.
    if challenge_started_at.read().is_none() {
        challenge_started_at.set(Some(chrono::Utc::now()));
    }

    // Celebration timing: 200ms initial pause + 2700ms overlay = 2900ms total before advance.
    const CELEBRATION_TOTAL_MS: u64 = 2900;

    // In fallback mode the recorded solve reflects the hold challenge that
    // was actually completed, not the configured (unsolvable) one.
    let fallback = *using_fallback.read();
    let solve_type = if fallback {
        ChallengeType::HoldButton
    } else {
        challenge.challenge_type_enum
    };
    let solve_difficulty = challenge.difficulty;

    let mut advance_or_dismiss = move || {
        // Guard: don't fire twice if already celebrating.
        if *celebrating.read() {
            return;
        }
        // Leaving this challenge — clear fallback so the next one starts
        // with its configured challenge.
        using_fallback.set(false);
        fallback_armed.set(false);

        // Record the solve duration (excludes celebration).
        if let Some(start) = *challenge_started_at.read() {
            let duration = (chrono::Utc::now() - start)
                .to_std()
                .unwrap_or(std::time::Duration::ZERO);
            challenge_solves
                .write()
                .push(crate::models::ChallengeSolve {
                    challenge_type: solve_type,
                    difficulty: solve_difficulty,
                    duration,
                });
        }
        // Reset start time for next challenge.
        challenge_started_at.set(None);

        // Trigger celebration overlay first — with a satisfying double-buzz
        // haptic for that little hit of dopamine.
        celebrate_haptic();
        celebrating.set(true);

        let instance_id = instance_id.clone();
        let on_all_solved = on_all_solved;

        // After the celebration completes, actually advance.
        spawn(async move {
            sleep_ms(CELEBRATION_TOTAL_MS).await;
            celebrating.set(false);

            let next = current_idx + 1;
            if next >= total {
                // All challenges solved — dismiss via AlarmKit, then let
                // `on_all_solved` record history and request navigation to
                // the History screen.
                let kit = crate::platform::alarm_kit();
                let id = mobile_sentinel::InstanceId::new(instance_id.clone());
                let _ = kit.mark_solved(&id);
                let _ = kit.dismiss(&id);
                on_all_solved.call(());
            } else {
                current_challenge_index.set(next);
                challenge_timeout.set(0);
                answer_feedback.set("");
            }
        });
    };

    // Fallback mode overrides the configured challenge with the universal
    // hold-the-bubble challenge (touch-only, works on any device). Solving
    // it advances exactly like a normal solve. A back button (on_cancel)
    // lets the user undo an accidental fallback tap.
    const FALLBACK_REQUIRED_HOLDS: u32 = 20;
    const FALLBACK_HOLD_MS: u32 = 3000;
    if fallback {
        return rsx! {
            HoldButtonChallenge {
                key: "fallback-{current_idx}",
                required: FALLBACK_REQUIRED_HOLDS,
                hold_ms: FALLBACK_HOLD_MS,
                on_complete: move |_| {
                    answer_feedback.set("correct");
                    advance_or_dismiss();
                },
                on_cancel: move |_| {
                    using_fallback.set(false);
                },
            }
        };
    }

    match challenge.challenge_type.as_str() {
        "math" => {
            let expression = challenge.expression.unwrap_or_else(|| "0 + 0".to_string());
            let expected = challenge.expected_number.unwrap_or(0);

            rsx! {
                           MathChallenge {
                               key: "math-{current_idx}",
                               expression: expression,
                               expected: expected,
                               on_answer: move |answer: i64| {
            // Check cooldown
                                   let secs = challenge_timeout();
                                   if rate_limiter.read().is_cooling_down(secs) {
                                       return;
                                   }
                                   if answer == expected {
                                       rate_limiter.write().record_correct();
                                       answer_feedback.set("correct");
                                       advance_or_dismiss();
                                   } else {
                                       rate_limiter.write().record_wrong(secs);
                                       answer_feedback.set("wrong");
                                   }
                               },
                           }
                       }
        }
        "memory" => {
            let sequence = challenge.memory_sequence.unwrap_or_default();
            let grid_size = challenge.memory_grid_size.unwrap_or(3);

            rsx! {
                MemoryGameChallenge {
                    key: "memory-{current_idx}",
                    sequence: sequence,
                    grid_size: grid_size,
                    on_complete: move |_seq: Vec<u8>| {
                        answer_feedback.set("correct");
                        advance_or_dismiss();
                    },
                }
            }
        }
        "typing" => {
            let passage = challenge
                .typing_passage
                .unwrap_or_else(|| "hello world".to_string());
            let min_accuracy = challenge.typing_min_accuracy.unwrap_or(0.80);

            rsx! {
                           TypingChallenge {
                               key: "typing-{current_idx}",
                               passage: passage.clone(),
                               min_accuracy: min_accuracy,
                               on_complete: move |typed: String| {
            // Calculate accuracy
                                   let passage_chars: Vec<char> = passage.chars().collect();
                                   let typed_chars: Vec<char> = typed.chars().collect();
                                   let correct = passage_chars.iter()
                                       .zip(typed_chars.iter())
                                       .filter(|(p, t)| p == t)
                                       .count();
                                   let accuracy = if !passage_chars.is_empty() {
                                       correct as f32 / passage_chars.len() as f32
                                   } else {
                                       1.0
                                   };

                                   if accuracy >= min_accuracy {
                                       rate_limiter.write().record_correct();
                                       answer_feedback.set("correct");
                                       advance_or_dismiss();
                                   } else {
                                       let secs = challenge_timeout();
                                       rate_limiter.write().record_wrong(secs);
                                       answer_feedback.set("wrong");
                                   }
                               },
                           }
                       }
        }
        "shake" => {
            let required = challenge.required_count.unwrap_or(10);

            rsx! {
                ShakeChallenge {
                    key: "shake-{current_idx}",
                    required: required,
                    on_complete: move |_| {
                        answer_feedback.set("correct");
                        advance_or_dismiss();
                    },
                }
            }
        }
        "step_count" => {
            let required = challenge.required_count.unwrap_or(10);

            rsx! {
                StepCountChallenge {
                    key: "step-{current_idx}",
                    required: required,
                    on_complete: move |_| {
                        answer_feedback.set("correct");
                        advance_or_dismiss();
                    },
                }
            }
        }
        "scan" => {
            let expected = challenge.expected_value.unwrap_or_default();
            rsx! {
                ScanChallenge {
                    key: "scan-{current_idx}",
                    expected_value: expected,
                    on_complete: move |_| {
                        advance_or_dismiss();
                    },
                }
            }
        }
        "hold" => {
            let required = challenge.required_count.unwrap_or(10);
            let hold_ms = challenge.hold_duration_ms.unwrap_or(3000);
            rsx! {
                HoldButtonChallenge {
                    key: "hold-{current_idx}",
                    required: required,
                    hold_ms: hold_ms,
                    on_complete: move |_| {
                        answer_feedback.set("correct");
                        advance_or_dismiss();
                    },
                }
            }
        }
        "reaction" => {
            let targets = challenge.required_count.unwrap_or(10);
            let time_ms = challenge.reaction_time_ms.unwrap_or(3000);
            let rounds = challenge.reaction_rounds.unwrap_or(2);
            rsx! {
                ReactionChallenge {
                    key: "reaction-{current_idx}",
                    targets_per_round: targets,
                    time_limit_ms: time_ms,
                    rounds: rounds,
                    on_complete: move |_| {
                        answer_feedback.set("correct");
                        advance_or_dismiss();
                    },
                }
            }
        }
        _ => {
            // Unknown challenge type — auto-advance
            advance_or_dismiss();
            rsx! { p { "{unknown_type_text}" } }
        }
    }
}

/// Async sleep helper using a oneshot channel + std::thread::sleep on a
/// background thread. Avoids needing tokio in the UI layer.
async fn sleep_ms(ms: u64) {
    let (tx, rx) = futures_channel::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let _ = tx.send(());
    });
    let _ = rx.await;
}

/// Celebration haptic: a short, punchy double-buzz pattern via the gated
/// `haptics` capability. No-op on host / when no vibrator is present.
fn celebrate_haptic() {
    #[cfg(target_os = "android")]
    mobile_sentinel::haptics::vibrate_pattern(&[0, 45, 60, 90]);
}
