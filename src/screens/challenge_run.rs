//! Doing an alarm's challenges one after another: the same for a ringing alarm and for
//! practice, so practice is exactly what will happen.

use dioxus::prelude::*;

use crate::app::{t, Translations};
use crate::components::challenges::{
    HoldButtonChallenge, MathChallenge, MemoryGameChallenge, ReactionChallenge, ScanChallenge,
    ShakeChallenge, StepCountChallenge, TypingChallenge,
};
use crate::components::{Celebration, Sheet, SheetDone, SheetItem};
use crate::models::{
    AppSettings, ChallengeEngine, ChallengeSetup, ChallengeSolve, ChallengeType, Difficulty, Locale,
};

/// One challenge, ready to be done.
#[derive(Clone, Debug, PartialEq)]
pub enum Challenge {
    Math {
        expression: String,
        expected: i64,
    },
    Memory {
        sequence: Vec<u8>,
        grid_size: u8,
    },
    Typing {
        passage: String,
        min_accuracy: f32,
    },
    Shake {
        count: u32,
    },
    Steps {
        count: u32,
    },
    Hold {
        count: u32,
        hold_ms: u32,
    },
    Reaction {
        targets: u32,
        time_ms: u32,
        rounds: u32,
    },
    /// Empty: any code counts.
    Scan {
        expected: String,
    },
}

/// A challenge and what history keeps of it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChallengeToRun {
    pub challenge: Challenge,
    pub kind: ChallengeType,
    pub difficulty: Difficulty,
}

/// Make the actual sums and passages for an alarm's challenges.
pub fn prepare(
    setups: &[ChallengeSetup],
    random_order: bool,
    locale: Locale,
) -> Vec<ChallengeToRun> {
    let mut engine = ChallengeEngine::new(locale);
    let mut prepared: Vec<ChallengeToRun> = setups
        .iter()
        .map(|setup| prepare_one(&mut engine, setup))
        .collect();
    if random_order {
        use rand::seq::SliceRandom;
        prepared.shuffle(&mut rand::thread_rng());
    }
    prepared
}

fn prepare_one(engine: &mut ChallengeEngine, setup: &ChallengeSetup) -> ChallengeToRun {
    let plain = |challenge, kind| ChallengeToRun {
        challenge,
        kind,
        // History needs a value; these have no difficulty to speak of.
        difficulty: Difficulty::Easy,
    };
    match setup {
        ChallengeSetup::Shake { count } => plain(
            Challenge::Shake { count: *count },
            ChallengeType::ShakeToWake,
        ),
        ChallengeSetup::Steps { count } => {
            plain(Challenge::Steps { count: *count }, ChallengeType::StepCount)
        }
        ChallengeSetup::Hold { count, secs } => plain(
            Challenge::Hold {
                count: *count,
                hold_ms: secs * 1000,
            },
            ChallengeType::HoldButton,
        ),
        ChallengeSetup::Reaction {
            targets,
            secs,
            rounds,
        } => plain(
            Challenge::Reaction {
                targets: *targets,
                time_ms: secs * 1000,
                rounds: *rounds,
            },
            ChallengeType::Reaction,
        ),
        ChallengeSetup::Scan { code } => plain(
            Challenge::Scan {
                expected: code.clone(),
            },
            ChallengeType::Scan,
        ),
        ChallengeSetup::Math { difficulty } => {
            let (expression, expected) = engine.math(*difficulty);
            ChallengeToRun {
                challenge: Challenge::Math {
                    expression,
                    expected,
                },
                kind: ChallengeType::Math,
                difficulty: *difficulty,
            }
        }
        ChallengeSetup::Memory { difficulty } => {
            let (sequence, grid_size) = engine.memory(*difficulty);
            ChallengeToRun {
                challenge: Challenge::Memory {
                    sequence,
                    grid_size,
                },
                kind: ChallengeType::MemoryGame,
                difficulty: *difficulty,
            }
        }
        ChallengeSetup::Typing { difficulty } => {
            let (passage, min_accuracy) = engine.typing(*difficulty);
            ChallengeToRun {
                challenge: Challenge::Typing {
                    passage,
                    min_accuracy,
                },
                kind: ChallengeType::Typing,
                difficulty: *difficulty,
            }
        }
    }
}

/// Without doing anything for this long, a ringing alarm's challenge gives way to the
/// ringing again.
const IDLE_LIMIT_SECS: u32 = 60;

/// A solved challenge stays on screen this long before the next one starts.
const CELEBRATION_MS: u64 = 1800;

/// The universal way out for a challenge that cannot be done (a broken sensor, a
/// missing code): nothing but touch, and slow on purpose so it is not the easy way.
const FALLBACK_HOLDS: u32 = 20;
const FALLBACK_HOLD_MS: u32 = 3000;

/// Runs `challenges` one after another and calls `on_solved` with what it took.
///
/// `on_idle` makes it the real thing: a clock runs that restarts on every touch and
/// sensor change, and ends in `on_idle`. `on_exit` makes it practice: there is an Exit
/// button instead, and the header says nothing is recorded.
#[component]
pub fn ChallengeRun(
    challenges: Vec<ChallengeToRun>,
    on_solved: EventHandler<Vec<ChallengeSolve>>,
    #[props(default)] on_idle: Option<EventHandler<()>>,
    #[props(default)] on_exit: Option<EventHandler<()>>,
) -> Element {
    let translations = use_context::<Translations>();
    let settings = use_context::<Signal<AppSettings>>();

    let total = challenges.len();
    let mut index = use_signal(|| 0usize);
    let mut idle_secs = use_signal(|| 0u32);
    let mut celebrating = use_signal(|| false);
    let mut solves: Signal<Vec<ChallengeSolve>> = use_signal(Vec::new);
    let mut started_at = use_signal(chrono::Utc::now);
    // This challenge was swapped for the touch-only one.
    let mut fallback = use_signal(|| false);
    let mut asking_fallback = use_signal(|| false);
    let idle_limit = use_hook(|| crate::platform::shortened("idle_secs", IDLE_LIMIT_SECS));

    use_future(move || async move {
        let Some(on_idle) = on_idle else {
            return;
        };
        let mut last_shake = mobile_sentinel::sensors::shake_count();
        let mut last_steps = mobile_sentinel::sensors::step_count();
        loop {
            crate::timer::sleep_ms(1000).await;
            // Finding the code to scan takes the time it takes.
            if crate::components::challenges::scanner_open() {
                idle_secs.set(0);
                continue;
            }
            let shakes = mobile_sentinel::sensors::shake_count();
            let steps = mobile_sentinel::sensors::step_count();
            if shakes != last_shake || steps != last_steps {
                last_shake = shakes;
                last_steps = steps;
                idle_secs.set(0);
                // A walk is no touch, and the ring limit should know somebody is up.
                if let Some(kit) = crate::platform::alarm_kit_optional() {
                    kit.note_activity();
                }
            }
            let idle = *idle_secs.peek() + 1;
            idle_secs.set(idle);
            if idle >= idle_limit {
                on_idle.call(());
                break;
            }
        }
    });

    let idx = index();
    let Some(current) = challenges.get(idx).cloned() else {
        return rsx! {};
    };

    let solve_kind = if fallback() {
        ChallengeType::HoldButton
    } else {
        current.kind
    };
    let solve_difficulty = current.difficulty;

    let mut solved = move || {
        if *celebrating.peek() {
            return;
        }
        let duration = (chrono::Utc::now() - *started_at.peek())
            .to_std()
            .unwrap_or_default();
        solves.write().push(ChallengeSolve {
            challenge_type: solve_kind,
            difficulty: solve_difficulty,
            duration,
        });
        crate::feedback::pattern(&[0, 45, 60, 90]);
        celebrating.set(true);
        spawn(async move {
            crate::timer::sleep_ms(CELEBRATION_MS).await;
            celebrating.set(false);
            fallback.set(false);
            let next = idx + 1;
            if next >= total {
                on_solved.call(solves.peek().clone());
            } else {
                index.set(next);
                idle_secs.set(0);
                started_at.set(chrono::Utc::now());
            }
        });
    };

    let progress = if on_exit.is_some() {
        t(&translations, "practice.progress")
    } else {
        t(&translations, "challenge.progress")
    }
    .replace("{current}", &(idx + 1).to_string())
    .replace("{total}", &total.to_string());
    let exit_text = t(&translations, "practice.exit");
    let fallback_link = t(&translations, "challenge.fallback.link");
    let fallback_title = t(&translations, "challenge.fallback.title");
    let fallback_note = t(&translations, "challenge.fallback.explain");
    let fallback_confirm = t(&translations, "challenge.fallback.confirm");
    let cancel_text = t(&translations, "action.cancel");
    let time_left_text = t(&translations, "challenge.time_left");
    let confetti = settings.read().prefs.confetti;
    let idle_left = 100_u32.saturating_sub(idle_secs() * 100 / idle_limit);
    let can_swap = !matches!(current.challenge, Challenge::Hold { .. });

    let body = if fallback() {
        rsx! {
            HoldButtonChallenge {
                key: "fallback-{idx}",
                required: FALLBACK_HOLDS,
                hold_ms: FALLBACK_HOLD_MS,
                on_complete: move |_| solved(),
                on_cancel: move |_| fallback.set(false),
            }
        }
    } else {
        match current.challenge {
            Challenge::Math {
                expression,
                expected,
            } => rsx! {
                MathChallenge {
                    key: "math-{idx}",
                    expression,
                    expected,
                    on_solved: move |_| solved(),
                }
            },
            Challenge::Memory {
                sequence,
                grid_size,
            } => rsx! {
                MemoryGameChallenge {
                    key: "memory-{idx}",
                    sequence,
                    grid_size,
                    on_complete: move |_| solved(),
                }
            },
            Challenge::Typing {
                passage,
                min_accuracy,
            } => rsx! {
                TypingChallenge {
                    key: "typing-{idx}",
                    passage,
                    min_accuracy,
                    on_complete: move |_| solved(),
                }
            },
            Challenge::Shake { count } => rsx! {
                ShakeChallenge { key: "shake-{idx}", required: count, on_complete: move |_| solved() }
            },
            Challenge::Steps { count } => rsx! {
                StepCountChallenge { key: "steps-{idx}", required: count, on_complete: move |_| solved() }
            },
            Challenge::Hold { count, hold_ms } => rsx! {
                HoldButtonChallenge {
                    key: "hold-{idx}",
                    required: count,
                    hold_ms,
                    on_complete: move |_| solved(),
                }
            },
            Challenge::Reaction {
                targets,
                time_ms,
                rounds,
            } => rsx! {
                ReactionChallenge {
                    key: "reaction-{idx}",
                    targets_per_round: targets,
                    time_limit_ms: time_ms,
                    rounds,
                    on_complete: move |_| solved(),
                }
            },
            Challenge::Scan { expected } => rsx! {
                ScanChallenge {
                    key: "scan-{idx}",
                    expected_value: expected,
                    on_complete: move |_| solved(),
                }
            },
        }
    };

    rsx! {
        div {
            class: "screen run-screen",
            onclick: move |_| idle_secs.set(0),
            oninput: move |_| idle_secs.set(0),
            ontouchstart: move |_| idle_secs.set(0),

            div { class: "run-top",
                p { class: "run-progress", "{progress}" }
                if let Some(on_exit) = on_exit {
                    button { class: "link-btn", onclick: move |_| on_exit.call(()), "{exit_text}" }
                }
                if total > 1 {
                    span { class: "dots", "aria-hidden": "true",
                        for dot in 0..total {
                            span { class: if dot <= idx { "dot on" } else { "dot" } }
                        }
                    }
                }
            }
            if on_idle.is_some() {
                div {
                    class: "run-timer",
                    role: "progressbar",
                    "aria-label": "{time_left_text}",
                    "aria-valuenow": "{idle_left}",
                    "aria-valuemin": "0",
                    "aria-valuemax": "100",
                    div { class: "run-timer-fill", style: "width: {idle_left}%" }
                }
            }

            div { class: "run-body", {body} }

            if can_swap && !fallback() && !celebrating() {
                button {
                    class: "link-btn run-fallback",
                    onclick: move |_| asking_fallback.set(true),
                    "{fallback_link}"
                }
            }

            if celebrating() {
                Celebration {
                    label: if total > 1 { format!("{} / {total}", idx + 1) } else { String::new() },
                    particles: if confetti { 28 } else { 0 },
                }
            }

            if asking_fallback() {
                Sheet {
                    title: fallback_title,
                    note: fallback_note,
                    on_close: move |_| asking_fallback.set(false),
                    SheetItem {
                        label: cancel_text,
                        on_click: move |_| asking_fallback.set(false),
                    }
                    SheetDone {
                        label: fallback_confirm,
                        on_click: move |_| {
                            asking_fallback.set(false);
                            fallback.set(true);
                        },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::setup::CHALLENGE_TYPES;

    #[test]
    fn every_type_of_challenge_gets_made() {
        let setups: Vec<ChallengeSetup> = CHALLENGE_TYPES
            .iter()
            .map(|key| ChallengeSetup::new(key).unwrap())
            .collect();
        let prepared = prepare(&setups, false, Locale::English);
        assert_eq!(prepared.len(), setups.len());
        assert!(prepared.iter().any(
            |c| matches!(&c.challenge, Challenge::Math { expression, .. } if !expression.is_empty())
        ));
        assert!(prepared.iter().any(
            |c| matches!(&c.challenge, Challenge::Typing { passage, .. } if !passage.is_empty())
        ));
        assert!(prepared.iter().any(
            |c| matches!(&c.challenge, Challenge::Memory { sequence, .. } if !sequence.is_empty())
        ));
    }

    #[test]
    fn random_order_keeps_all_the_challenges() {
        let setups = vec![
            ChallengeSetup::Shake { count: 1 },
            ChallengeSetup::Steps { count: 2 },
            ChallengeSetup::Hold { count: 3, secs: 1 },
        ];
        let mut counts: Vec<u32> = prepare(&setups, true, Locale::English)
            .into_iter()
            .map(|c| match c.challenge {
                Challenge::Shake { count }
                | Challenge::Steps { count }
                | Challenge::Hold { count, .. } => count,
                other => panic!("did not ask for {other:?}"),
            })
            .collect();
        counts.sort_unstable();
        assert_eq!(counts, [1, 2, 3]);
    }
}
