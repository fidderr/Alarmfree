//! Doing an alarm's challenges: what gets generated (`prepare`) and the rules of each
//! challenge as plain state machines. The screens in `ui/` only draw them and drive their
//! clocks, so practice and the real thing run the very same rules.

pub mod config;
pub mod hold;
pub mod math;
pub mod memory;
pub mod reaction;
pub mod typing;

use crate::models::{ChallengeEngine, ChallengeSetup, ChallengeType, Difficulty, Locale};

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
pub fn prepare(setups: &[ChallengeSetup], locale: Locale) -> Vec<ChallengeToRun> {
    let mut engine = ChallengeEngine::new(locale);
    setups
        .iter()
        .map(|setup| prepare_one(&mut engine, setup))
        .collect()
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
pub const IDLE_LIMIT_SECS: u32 = 60;

/// The camera may be up this long before the alarm rings again, camera or no camera. Getting to a
/// code in another room takes a while, but opening it and going back to sleep is no way out.
pub const SCAN_QUIET_SECS: u32 = 180;

/// A solved challenge stays on screen this long before the next one starts.
pub const CELEBRATION_MS: u64 = 1800;

/// The universal way out for a challenge that cannot be done (a broken sensor, a
/// missing code): nothing but touch, and slow on purpose so it is not the easy way.
pub const FALLBACK_HOLDS: u32 = 20;
pub const FALLBACK_HOLD_MS: u32 = 3000;

/// How far a counting challenge (shake, steps, the idle clock) has come, from 0 to 1.
pub fn fraction(count: u32, required: u32) -> f32 {
    (count as f32 / required.max(1) as f32).min(1.0)
}

/// How much of the idle limit is left, in percent.
pub fn idle_left_percent(idle_secs: u32, limit: u32) -> u32 {
    100u32.saturating_sub(idle_secs * 100 / limit.max(1))
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
        let prepared = prepare(&setups, Locale::English);
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
    fn challenges_come_in_the_order_they_were_set() {
        let setups = vec![
            ChallengeSetup::Shake { count: 1 },
            ChallengeSetup::Steps { count: 2 },
            ChallengeSetup::Hold { count: 3, secs: 1 },
        ];
        let counts: Vec<u32> = prepare(&setups, Locale::English)
            .into_iter()
            .map(|c| match c.challenge {
                Challenge::Shake { count }
                | Challenge::Steps { count }
                | Challenge::Hold { count, .. } => count,
                other => panic!("did not ask for {other:?}"),
            })
            .collect();
        assert_eq!(counts, [1, 2, 3]);
    }

    #[test]
    fn the_time_left_bar_drains_with_the_idle_clock() {
        assert_eq!(idle_left_percent(0, 60), 100);
        assert_eq!(idle_left_percent(30, 60), 50);
        assert_eq!(idle_left_percent(60, 60), 0);
        assert_eq!(idle_left_percent(90, 60), 0);
    }

    #[test]
    fn a_ring_never_fills_past_full() {
        assert_eq!(fraction(0, 20), 0.0);
        assert_eq!(fraction(10, 20), 0.5);
        assert_eq!(fraction(30, 20), 1.0);
        assert_eq!(fraction(1, 0), 1.0);
    }
}
