//! What the user set up for one challenge of an alarm.
//!
//! An alarm keeps its challenges as `(type key, JSON settings)` pairs, the form the
//! SDK's challenge gate and older saved alarms use. Everything else works with this
//! type, so no screen deals in JSON or in fields that do not apply to a challenge.

use serde_json::json;

use crate::models::Difficulty;

#[derive(Debug, Clone, PartialEq)]
pub enum ChallengeSetup {
    Math {
        difficulty: Difficulty,
    },
    Memory {
        difficulty: Difficulty,
    },
    Typing {
        difficulty: Difficulty,
    },
    Shake {
        count: u32,
    },
    Steps {
        count: u32,
    },
    Hold {
        count: u32,
        secs: u32,
    },
    Reaction {
        targets: u32,
        secs: u32,
        rounds: u32,
    },
    Scan {
        code: String,
    },
}

/// The type keys, in the order the "add a challenge" sheet lists them. Each is also
/// the translation key of the challenge's name.
pub const CHALLENGE_TYPES: [&str; 8] = [
    "challenge.math",
    "challenge.typing",
    "challenge.memory",
    "challenge.reaction",
    "challenge.hold",
    "challenge.steps",
    "challenge.shake",
    "challenge.scan",
];

impl ChallengeSetup {
    /// A challenge of this type with the settings a new one starts with.
    pub fn new(type_key: &str) -> Option<Self> {
        Some(match type_key {
            "challenge.math" => Self::Math {
                difficulty: Difficulty::Medium,
            },
            "challenge.memory" => Self::Memory {
                difficulty: Difficulty::Medium,
            },
            "challenge.typing" => Self::Typing {
                difficulty: Difficulty::Medium,
            },
            "challenge.shake" => Self::Shake { count: 20 },
            "challenge.steps" => Self::Steps { count: 20 },
            "challenge.hold" => Self::Hold { count: 10, secs: 3 },
            "challenge.reaction" => Self::Reaction {
                targets: 10,
                secs: 3,
                rounds: 2,
            },
            "challenge.scan" => Self::Scan {
                code: String::new(),
            },
            _ => return None,
        })
    }

    pub fn type_key(&self) -> &'static str {
        match self {
            Self::Math { .. } => "challenge.math",
            Self::Memory { .. } => "challenge.memory",
            Self::Typing { .. } => "challenge.typing",
            Self::Shake { .. } => "challenge.shake",
            Self::Steps { .. } => "challenge.steps",
            Self::Hold { .. } => "challenge.hold",
            Self::Reaction { .. } => "challenge.reaction",
            Self::Scan { .. } => "challenge.scan",
        }
    }

    /// From an alarm's stored form. `None` for a type this version does not know.
    /// Settings missing from what was stored keep the new-challenge values.
    pub fn from_stored(type_key: &str, settings_json: &str) -> Option<Self> {
        let stored: serde_json::Value =
            serde_json::from_str(settings_json).unwrap_or_else(|_| json!({}));
        let number = |key: &str| {
            stored
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .map(|n| n as u32)
        };
        let millis_to_secs = |key: &str| number(key).map(|ms| ms / 1000);
        let difficulty = || match stored.get("difficulty").and_then(|v| v.as_str()) {
            Some("easy") => Difficulty::Easy,
            Some("hard") => Difficulty::Hard,
            Some("extreme") => Difficulty::Extreme,
            _ => Difficulty::Medium,
        };
        let mut setup = Self::new(type_key)?;
        match &mut setup {
            Self::Math { difficulty: d }
            | Self::Memory { difficulty: d }
            | Self::Typing { difficulty: d } => *d = difficulty(),
            Self::Shake { count } | Self::Steps { count } => {
                *count = number("count").unwrap_or(*count);
            }
            Self::Hold { count, secs } => {
                *count = number("count").unwrap_or(*count);
                *secs = millis_to_secs("hold_ms").unwrap_or(*secs);
            }
            Self::Reaction {
                targets,
                secs,
                rounds,
            } => {
                *targets = number("count").unwrap_or(*targets);
                *secs = millis_to_secs("time_ms").unwrap_or(*secs);
                *rounds = number("rounds").unwrap_or(*rounds);
            }
            Self::Scan { code } => {
                *code = stored
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
            }
        }
        Some(setup)
    }

    /// The alarm's stored form: type key and JSON settings.
    pub fn to_stored(&self) -> (String, String) {
        let settings = match self {
            Self::Math { difficulty }
            | Self::Memory { difficulty }
            | Self::Typing { difficulty } => {
                json!({ "difficulty": difficulty_name(*difficulty) })
            }
            Self::Shake { count } | Self::Steps { count } => json!({ "count": count }),
            Self::Hold { count, secs } => json!({ "count": count, "hold_ms": secs * 1000 }),
            Self::Reaction {
                targets,
                secs,
                rounds,
            } => json!({ "count": targets, "time_ms": secs * 1000, "rounds": rounds }),
            // Through serde so a quote or backslash in the code (a URL, JSON text in a
            // QR code) cannot break the settings, which would silently turn "this
            // specific code" into "any code".
            Self::Scan { code } => {
                json!({ "value": crate::sanitize::sanitize_reference_value(code) })
            }
        };
        (self.type_key().to_owned(), settings.to_string())
    }

    /// An alarm's challenges in this form, leaving out types this version does not
    /// know.
    pub fn list_from_stored(stored: &[(String, String)]) -> Vec<Self> {
        stored
            .iter()
            .filter_map(|(key, settings)| Self::from_stored(key, settings))
            .collect()
    }
}

pub fn difficulty_name(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Easy => "easy",
        Difficulty::Medium => "medium",
        Difficulty::Hard => "hard",
        Difficulty::Extreme => "extreme",
    }
}

/// The three ready-made sets of the challenges sheet.
pub fn preset(name: &str) -> Vec<ChallengeSetup> {
    use ChallengeSetup as C;
    use Difficulty::{Hard, Medium};
    match name {
        "gentle" => vec![C::Hold { count: 3, secs: 3 }],
        "normal" => vec![C::Math { difficulty: Medium }, C::Steps { count: 20 }],
        "hard" => vec![
            C::Math { difficulty: Hard },
            C::Typing { difficulty: Hard },
            C::Memory { difficulty: Hard },
            C::Steps { count: 50 },
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_challenge_type_stores_only_its_own_settings() {
        let stored = |setup: ChallengeSetup| setup.to_stored().1;
        assert_eq!(
            stored(ChallengeSetup::Math {
                difficulty: Difficulty::Hard
            }),
            r#"{"difficulty":"hard"}"#
        );
        assert_eq!(
            stored(ChallengeSetup::Shake { count: 25 }),
            r#"{"count":25}"#
        );
        assert_eq!(
            stored(ChallengeSetup::Hold { count: 8, secs: 4 }),
            r#"{"count":8,"hold_ms":4000}"#
        );
        assert_eq!(
            stored(ChallengeSetup::Reaction {
                targets: 12,
                secs: 3,
                rounds: 2
            }),
            r#"{"count":12,"rounds":2,"time_ms":3000}"#
        );
    }

    #[test]
    fn a_scan_code_is_sanitised_and_empty_means_any_code() {
        let stored = |code: &str| {
            ChallengeSetup::Scan {
                code: code.to_owned(),
            }
            .to_stored()
            .1
        };
        assert_eq!(stored(""), r#"{"value":""}"#);
        // Quotes, backslashes and braces must survive exactly.
        let tricky = r#"https://x.test/?q="a"\b{c}"#;
        let parsed: serde_json::Value = serde_json::from_str(&stored(tricky)).unwrap();
        assert_eq!(parsed["value"], tricky);
        let parsed: serde_json::Value = serde_json::from_str(&stored("ab\u{0007}cd")).unwrap();
        assert_eq!(parsed["value"], "abcd", "control characters are stripped");
    }

    #[test]
    fn every_type_survives_the_stored_form() {
        for key in CHALLENGE_TYPES {
            let setup = ChallengeSetup::new(key).unwrap();
            let (stored_key, settings) = setup.to_stored();
            assert_eq!(stored_key, key);
            assert_eq!(ChallengeSetup::from_stored(key, &settings), Some(setup));
        }
    }

    #[test]
    fn older_or_broken_settings_fall_back_to_new_challenge_values() {
        assert_eq!(
            ChallengeSetup::from_stored("challenge.hold", "not json"),
            Some(ChallengeSetup::Hold { count: 10, secs: 3 })
        );
        assert_eq!(
            ChallengeSetup::from_stored("challenge.reaction", r#"{"rounds":4}"#),
            Some(ChallengeSetup::Reaction {
                targets: 10,
                secs: 3,
                rounds: 4
            })
        );
        assert_eq!(ChallengeSetup::from_stored("challenge.gone", "{}"), None);
    }

    #[test]
    fn the_presets_exist_and_get_harder() {
        assert_eq!(preset("gentle").len(), 1);
        assert_eq!(preset("normal").len(), 2);
        assert_eq!(preset("hard").len(), 4);
        assert!(preset("other").is_empty());
    }
}
