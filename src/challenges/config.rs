//! Setting up an alarm's challenges: the limits of every setting, what a challenge is
//! summed up as, and what it needs from the phone.

use crate::i18n::{t, Translations};
use crate::models::setup::difficulty_name;
use crate::models::{ChallengeSetup, Difficulty};

/// The most challenges an alarm takes.
pub const MAX_CHALLENGES: usize = 10;

pub const DIFFICULTIES: [Difficulty; 4] = [
    Difficulty::Easy,
    Difficulty::Medium,
    Difficulty::Hard,
    Difficulty::Extreme,
];

/// The permission a challenge needs (steps, camera) before it is added or tried.
pub fn permission_for(setup: &ChallengeSetup) -> Option<&'static str> {
    match setup {
        ChallengeSetup::Steps { .. } => Some("android.permission.ACTIVITY_RECOGNITION"),
        ChallengeSetup::Scan { .. } => Some("android.permission.CAMERA"),
        _ => None,
    }
}

/// What an alarm's challenge is set to, in a few words.
pub fn summary(translations: &Translations, setup: &ChallengeSetup) -> String {
    let difficulty = |d: Difficulty| t(translations, &format!("difficulty.{}", difficulty_name(d)));
    match setup {
        ChallengeSetup::Math { difficulty: d }
        | ChallengeSetup::Memory { difficulty: d }
        | ChallengeSetup::Typing { difficulty: d } => difficulty(*d),
        ChallengeSetup::Shake { count } => {
            t(translations, "challenge.summary.shakes").replace("{count}", &count.to_string())
        }
        ChallengeSetup::Steps { count } => {
            t(translations, "challenge.summary.steps").replace("{count}", &count.to_string())
        }
        ChallengeSetup::Hold { count, secs } => {
            crate::text::counted(translations, "challenge.summary.hold", *count)
                .replace("{secs}", &secs.to_string())
        }
        ChallengeSetup::Reaction {
            targets,
            secs,
            rounds,
        } => t(
            translations,
            if *rounds == 1 {
                "challenge.summary.reaction_one"
            } else {
                "challenge.summary.reaction"
            },
        )
        .replace("{count}", &targets.to_string())
        .replace("{secs}", &secs.to_string())
        .replace("{rounds}", &rounds.to_string()),
        ChallengeSetup::Scan { code } if code.is_empty() => {
            t(translations, "challenge.summary.any")
        }
        ChallengeSetup::Scan { code } => {
            t(translations, "challenge.summary.code").replace("{code}", code)
        }
    }
}

/// An example of what a difficulty means for this challenge.
pub fn difficulty_example(translations: &Translations, setup: &ChallengeSetup) -> String {
    let (name, difficulty) = match setup {
        ChallengeSetup::Math { difficulty } => ("math", difficulty),
        ChallengeSetup::Memory { difficulty } => ("memory", difficulty),
        ChallengeSetup::Typing { difficulty } => ("typing", difficulty),
        _ => return String::new(),
    };
    t(
        translations,
        &format!("challenge.example.{name}_{}", difficulty_name(*difficulty)),
    )
}

/// The difficulty of a challenge that has one.
pub fn difficulty_of(setup: &ChallengeSetup) -> Option<Difficulty> {
    match setup {
        ChallengeSetup::Math { difficulty }
        | ChallengeSetup::Memory { difficulty }
        | ChallengeSetup::Typing { difficulty } => Some(*difficulty),
        _ => None,
    }
}

pub fn set_difficulty(setup: &mut ChallengeSetup, picked: Difficulty) {
    if let ChallengeSetup::Math { difficulty }
    | ChallengeSetup::Memory { difficulty }
    | ChallengeSetup::Typing { difficulty } = setup
    {
        *difficulty = picked;
    }
}

/// A setting with a number and a step either way: the title's translation key, the
/// unit text key if it is in seconds, and the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stepper {
    pub title_key: &'static str,
    pub seconds: bool,
    pub value: u32,
}

/// The number settings of a challenge, in the order the sheet shows them.
pub fn steppers(setup: &ChallengeSetup) -> Vec<Stepper> {
    let stepper = |title_key, seconds, value| Stepper {
        title_key,
        seconds,
        value,
    };
    match setup {
        ChallengeSetup::Shake { count } => {
            vec![stepper("challenge.config.how_many_shakes", false, *count)]
        }
        ChallengeSetup::Steps { count } => {
            vec![stepper("challenge.config.how_many_steps", false, *count)]
        }
        ChallengeSetup::Hold { count, secs } => vec![
            stepper("challenge.config.how_many_holds", false, *count),
            stepper("challenge.config.hold_duration", true, *secs),
        ],
        ChallengeSetup::Reaction {
            targets,
            secs,
            rounds,
        } => vec![
            stepper("challenge.config.how_many_targets", false, *targets),
            stepper("challenge.config.time_limit", true, *secs),
            stepper("challenge.config.how_many_rounds", false, *rounds),
        ],
        _ => Vec::new(),
    }
}

/// One step of number setting `which` (as listed by [`steppers`]) up or down, inside its limits.
pub fn step(setup: &mut ChallengeSetup, which: usize, up: bool) {
    let by = |value: &mut u32, step: u32, min: u32, max: u32| {
        *value = if up {
            (*value + step).min(max)
        } else {
            value.saturating_sub(step).max(min)
        };
    };
    match (setup, which) {
        (ChallengeSetup::Shake { count } | ChallengeSetup::Steps { count }, 0) => {
            by(count, 5, 5, 200)
        }
        (ChallengeSetup::Hold { count, .. }, 0) => by(count, 1, 1, 30),
        (ChallengeSetup::Hold { secs, .. }, 1) => by(secs, 1, 1, 15),
        (ChallengeSetup::Reaction { targets, .. }, 0) => by(targets, 1, 3, 30),
        (ChallengeSetup::Reaction { secs, .. }, 1) => by(secs, 1, 2, 10),
        (ChallengeSetup::Reaction { rounds, .. }, 2) => by(rounds, 1, 1, 5),
        _ => {}
    }
}

/// The editor's row: the names of the challenges.
pub fn names_summary(translations: &Translations, setups: &[ChallengeSetup]) -> String {
    let names: Vec<String> = setups
        .iter()
        .map(|s| t(translations, s.type_key()))
        .collect();
    match names.len() {
        0 => t(translations, "config.none"),
        1 | 2 => names.join(", "),
        n => format!("{} +{}", names[0], n - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::load_translations;
    use crate::models::Locale;

    fn english() -> Translations {
        load_translations(Locale::English)
    }

    #[test]
    fn the_sensor_challenges_ask_for_their_permission() {
        assert_eq!(
            permission_for(&ChallengeSetup::Steps { count: 5 }),
            Some("android.permission.ACTIVITY_RECOGNITION")
        );
        assert_eq!(
            permission_for(&ChallengeSetup::Scan {
                code: String::new()
            }),
            Some("android.permission.CAMERA")
        );
        assert_eq!(permission_for(&ChallengeSetup::Shake { count: 5 }), None);
    }

    #[test]
    fn shake_goes_by_five_between_five_and_two_hundred() {
        let mut setup = ChallengeSetup::Shake { count: 20 };
        step(&mut setup, 0, false);
        assert_eq!(setup, ChallengeSetup::Shake { count: 15 });
        for _ in 0..10 {
            step(&mut setup, 0, false);
        }
        assert_eq!(setup, ChallengeSetup::Shake { count: 5 });
        for _ in 0..60 {
            step(&mut setup, 0, true);
        }
        assert_eq!(setup, ChallengeSetup::Shake { count: 200 });
    }

    #[test]
    fn hold_and_reaction_stay_inside_their_limits() {
        let mut hold = ChallengeSetup::Hold { count: 1, secs: 15 };
        step(&mut hold, 0, false);
        step(&mut hold, 1, true);
        assert_eq!(hold, ChallengeSetup::Hold { count: 1, secs: 15 });
        for _ in 0..40 {
            step(&mut hold, 0, true);
            step(&mut hold, 1, false);
        }
        assert_eq!(hold, ChallengeSetup::Hold { count: 30, secs: 1 });

        let mut reaction = ChallengeSetup::new("challenge.reaction").unwrap();
        for _ in 0..40 {
            step(&mut reaction, 0, false);
            step(&mut reaction, 1, false);
            step(&mut reaction, 2, false);
        }
        assert_eq!(
            reaction,
            ChallengeSetup::Reaction {
                targets: 3,
                secs: 2,
                rounds: 1
            }
        );
        for _ in 0..40 {
            step(&mut reaction, 0, true);
            step(&mut reaction, 1, true);
            step(&mut reaction, 2, true);
        }
        assert_eq!(
            reaction,
            ChallengeSetup::Reaction {
                targets: 30,
                secs: 10,
                rounds: 5
            }
        );
    }

    #[test]
    fn a_stepper_that_does_not_exist_changes_nothing() {
        let mut setup = ChallengeSetup::Shake { count: 20 };
        step(&mut setup, 3, true);
        let mut math = ChallengeSetup::new("challenge.math").unwrap();
        step(&mut math, 0, true);
        assert_eq!(setup, ChallengeSetup::Shake { count: 20 });
        assert_eq!(math, ChallengeSetup::new("challenge.math").unwrap());
    }

    #[test]
    fn each_type_lists_its_own_number_settings() {
        let count = |key: &str| steppers(&ChallengeSetup::new(key).unwrap()).len();
        assert_eq!(count("challenge.math"), 0);
        assert_eq!(count("challenge.scan"), 0);
        assert_eq!(count("challenge.shake"), 1);
        assert_eq!(count("challenge.hold"), 2);
        assert_eq!(count("challenge.reaction"), 3);
    }

    #[test]
    fn the_difficulty_is_set_only_where_there_is_one() {
        let mut math = ChallengeSetup::new("challenge.math").unwrap();
        set_difficulty(&mut math, Difficulty::Extreme);
        assert_eq!(difficulty_of(&math), Some(Difficulty::Extreme));
        let mut shake = ChallengeSetup::new("challenge.shake").unwrap();
        set_difficulty(&mut shake, Difficulty::Extreme);
        assert_eq!(difficulty_of(&shake), None);
        assert_eq!(shake, ChallengeSetup::Shake { count: 20 });
    }

    #[test]
    fn summaries_say_what_is_set() {
        let tr = english();
        assert_eq!(
            summary(&tr, &ChallengeSetup::Shake { count: 25 }),
            "25 shakes"
        );
        assert_eq!(
            summary(&tr, &ChallengeSetup::Hold { count: 1, secs: 3 }),
            "1 hold of 3 s"
        );
        assert_eq!(
            summary(&tr, &ChallengeSetup::Hold { count: 4, secs: 3 }),
            "4 holds of 3 s"
        );
        assert_eq!(
            summary(
                &tr,
                &ChallengeSetup::Reaction {
                    targets: 10,
                    secs: 3,
                    rounds: 1
                }
            ),
            "10 targets, 3 s, 1 round"
        );
        assert_eq!(
            summary(
                &tr,
                &ChallengeSetup::Scan {
                    code: String::new()
                }
            ),
            "Any code"
        );
        assert_eq!(
            summary(
                &tr,
                &ChallengeSetup::Scan {
                    code: "abc".to_owned()
                }
            ),
            "Code abc"
        );
        assert_eq!(
            summary(&tr, &ChallengeSetup::new("challenge.math").unwrap()),
            "Medium"
        );
    }

    #[test]
    fn the_example_follows_the_difficulty() {
        let tr = english();
        let mut typing = ChallengeSetup::new("challenge.typing").unwrap();
        set_difficulty(&mut typing, Difficulty::Hard);
        assert_eq!(
            difficulty_example(&tr, &typing),
            "25-35 words (90% accuracy)"
        );
        assert_eq!(
            difficulty_example(&tr, &ChallengeSetup::Shake { count: 5 }),
            ""
        );
    }

    #[test]
    fn the_editor_row_names_up_to_two_then_counts() {
        let tr = english();
        assert_eq!(names_summary(&tr, &[]), "None");
        let math = ChallengeSetup::new("challenge.math").unwrap();
        let shake = ChallengeSetup::new("challenge.shake").unwrap();
        assert_eq!(names_summary(&tr, std::slice::from_ref(&math)), "Math");
        assert_eq!(
            names_summary(&tr, &[math.clone(), shake.clone()]),
            "Math, Shake"
        );
        assert_eq!(
            names_summary(&tr, &[math.clone(), shake.clone(), shake]),
            "Math +2"
        );
    }
}
