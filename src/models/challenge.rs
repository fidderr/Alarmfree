//! Challenge configuration types: ChallengeConfig, ChallengeType, Difficulty.
//!
//! Also hosts the [`ChallengeEngine`] generator — it emits new challenge
//! instances on demand. Each challenge component evaluates its own
//! correctness UI-side; this module only generates.

use std::time::Duration;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeConfig {
    pub challenge_type: ChallengeType,
    pub difficulty: Difficulty,
    pub reference_data: Option<ReferenceData>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChallengeType {
    Math,
    /// Unified barcode / QR scanner (the camera backend handles both).
    Scan,
    ShakeToWake,
    StepCount,
    MemoryGame,
    Typing,
    HoldButton,
    /// Speed/reaction: pop a set number of targets within a time limit, over
    /// several rounds. Pure count + time + rounds — no difficulty.
    Reaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    Extreme,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReferenceData {
    /// Expected code value for the scanner challenge (barcode or QR).
    Scan(String),
}

/// A generated challenge instance ready for presentation.
#[derive(Debug, Clone)]
pub struct ChallengeInstance {
    pub challenge_type: ChallengeType,
    pub difficulty: Difficulty,
    pub prompt: ChallengePrompt,
    pub expected_answer: ChallengeAnswer,
}

#[derive(Debug, Clone)]
pub enum ChallengePrompt {
    Math {
        expression: String,
    },
    Scan {
        expected_value: String,
    },
    ShakeToWake {
        required_shakes: u32,
    },
    StepCount {
        required_steps: u32,
    },
    MemoryGame {
        sequence: Vec<u8>,
        display_duration: Duration,
    },
    Typing {
        passage: String,
    },
    /// Hold a circle for `hold_duration`, repeated `required_holds` times.
    /// Pure count + duration — no difficulty.
    HoldButton {
        required_holds: u32,
        hold_duration: Duration,
    },
    /// Pop `targets_per_round` targets within `time_limit`, for `rounds`
    /// rounds. Pure count + time + rounds — no difficulty.
    Reaction {
        targets_per_round: u32,
        time_limit: Duration,
        rounds: u32,
    },
}

#[derive(Debug, Clone)]
pub enum ChallengeAnswer {
    Numeric(i64),
    ExactString(String),
    ShakeCount(u32),
    StepCount(u32),
    Sequence(Vec<u8>),
    TypedText {
        expected: String,
        min_accuracy: f32,
    },
    HoldCount(u32),
    /// Total number of target pops required across all reaction rounds.
    ReactionTargets(u32),
}

/// Difficulty parameters for challenge generation.
#[derive(Debug, Clone, PartialEq)]
pub struct DifficultyParams {
    pub shake_count: u32,
    pub step_count: u32,
    pub memory_sequence_length: u8,
    pub typing_word_count: (u8, u8),
}

// ---------------------------------------------------------------------------
// Challenge generation engine
// ---------------------------------------------------------------------------

/// Challenge generation engine. Emits new [`ChallengeInstance`]s on demand.
pub struct ChallengeEngine {
    rng: StdRng,
}

/// Word pool for typing challenges.
const WORD_POOL: &[&str] = &[
    // Pangram classics
    "the",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    // Morning / wake-up theme
    "morning",
    "alarm",
    "clock",
    "wake",
    "time",
    "sleep",
    "dream",
    "night",
    "early",
    "rise",
    "sunshine",
    "coffee",
    "breakfast",
    "routine",
    "energy",
    "focus",
    "mindful",
    "stretch",
    "breathe",
    "calm",
    "peaceful",
    "ready",
    "start",
    "fresh",
    "bright",
    "alert",
    "awake",
    "active",
    "yawn",
    "snooze",
    "pillow",
    "blanket",
    "curtain",
    "window",
    "sunrise",
    "dawn",
    "twilight",
    "groggy",
    "refreshed",
    "rested",
    "tea",
    "juice",
    "toast",
    "cereal",
    "shower",
    "bathroom",
    "mirror",
    "brush",
    "towel",
    "kitchen",
    "stove",
    "kettle",
    "spoon",
    "plate",
    "cup",
    "mug",
    "bowl",
    "fork",
    "knife",
    // Health & wellness
    "healthy",
    "habit",
    "daily",
    "practice",
    "moment",
    "present",
    "grateful",
    "positive",
    "strong",
    "clear",
    "sharp",
    "vibrant",
    "alive",
    "purpose",
    "intention",
    "balance",
    "harmony",
    "gentle",
    "steady",
    "vitality",
    "wellness",
    "fitness",
    "yoga",
    "meditate",
    "exercise",
    "workout",
    "training",
    "muscle",
    "joint",
    "spine",
    "posture",
    "core",
    "cardio",
    "pulse",
    "heart",
    "lungs",
    "breath",
    "oxygen",
    "hydrate",
    "vitamin",
    "nutrient",
    "protein",
    "fiber",
    "salad",
    "fruit",
    "vegetable",
    "berry",
    "apple",
    "orange",
    "banana",
    "lemon",
    "honey",
    "almond",
    "yogurt",
    // Growth & motivation
    "progress",
    "forward",
    "growth",
    "learn",
    "create",
    "inspire",
    "achieve",
    "believe",
    "succeed",
    "improve",
    "discover",
    "explore",
    "imagine",
    "build",
    "shape",
    "design",
    "craft",
    "master",
    "develop",
    "refine",
    "expand",
    "elevate",
    "flourish",
    "thrive",
    "advance",
    "ascend",
    "challenge",
    "overcome",
    "conquer",
    "triumph",
    "victory",
    "winner",
    "champion",
    "hero",
    "courage",
    "brave",
    "bold",
    "fearless",
    "daring",
    "passion",
    "drive",
    "ambition",
    "vision",
    "future",
    "tomorrow",
    "destiny",
    "journey",
    "adventure",
    "story",
    "chapter",
    "lesson",
    "wisdom",
    "insight",
    "knowledge",
    "skill",
    "talent",
    "ability",
    "potential",
    "promise",
    // Nature & elements
    "ocean",
    "river",
    "mountain",
    "forest",
    "garden",
    "flower",
    "tree",
    "cloud",
    "wind",
    "rain",
    "snow",
    "fire",
    "earth",
    "stone",
    "valley",
    "meadow",
    "stream",
    "summit",
    "horizon",
    "shore",
    "beach",
    "sand",
    "wave",
    "tide",
    "coast",
    "island",
    "lake",
    "pond",
    "creek",
    "waterfall",
    "canyon",
    "desert",
    "jungle",
    "savanna",
    "tundra",
    "glacier",
    "volcano",
    "boulder",
    "pebble",
    "moss",
    "grass",
    "leaf",
    "branch",
    "root",
    "petal",
    "seed",
    "blossom",
    "rose",
    "tulip",
    "daisy",
    "lily",
    "sunflower",
    "bird",
    "robin",
    "eagle",
    "owl",
    "sparrow",
    "hawk",
    "falcon",
    "swan",
    "deer",
    "wolf",
    "bear",
    "rabbit",
    "squirrel",
    "raccoon",
    "otter",
    "horse",
    "pony",
    "stallion",
    "lion",
    "tiger",
    "panther",
    "leopard",
    "elephant",
    "giraffe",
    "zebra",
    "kangaroo",
    "koala",
    "panda",
    "monkey",
    "dolphin",
    "whale",
    "shark",
    "octopus",
    "starfish",
    "coral",
    "seaweed",
    "butterfly",
    "dragonfly",
    "ladybug",
    "firefly",
    "honeybee",
    "spider",
    // Common short connectors
    "and",
    "or",
    "but",
    "with",
    "from",
    "into",
    "upon",
    "after",
    "before",
    "until",
    "while",
    "during",
    "around",
    "through",
    "between",
    "along",
    "above",
    "below",
    "behind",
    "ahead",
    "among",
    "across",
    "beyond",
    "within",
    "without",
    "against",
    "toward",
    "near",
    "next",
    "then",
    // Pronouns & articles
    "this",
    "that",
    "these",
    "those",
    "such",
    "each",
    "every",
    "any",
    "some",
    "few",
    "many",
    "more",
    "most",
    "less",
    "least",
    "much",
    "all",
    "both",
    "either",
    "neither",
    "none",
    "other",
    "another",
    // Action verbs
    "run",
    "walk",
    "move",
    "lift",
    "carry",
    "push",
    "pull",
    "open",
    "close",
    "watch",
    "listen",
    "speak",
    "write",
    "read",
    "think",
    "smile",
    "laugh",
    "jump",
    "skip",
    "hop",
    "leap",
    "bounce",
    "roll",
    "slide",
    "swim",
    "climb",
    "crawl",
    "dance",
    "sing",
    "play",
    "rest",
    "wait",
    "stay",
    "leave",
    "arrive",
    "return",
    "travel",
    "wander",
    "drift",
    "float",
    "sail",
    "fly",
    "soar",
    "glide",
    "ride",
    "cycle",
    "row",
    "paddle",
    "fix",
    "mend",
    "repair",
    "polish",
    "clean",
    "wash",
    "rinse",
    "dry",
    "fold",
    "stack",
    "store",
    "pack",
    "unpack",
    "find",
    "search",
    "seek",
    "follow",
    "lead",
    "guide",
    "show",
    "share",
    "give",
    "offer",
    "accept",
    "thank",
    "praise",
    "honor",
    "respect",
    "love",
    "care",
    "help",
    "support",
    "comfort",
    "soothe",
    "heal",
    // Descriptive adjectives
    "warm",
    "cool",
    "soft",
    "loud",
    "quiet",
    "smooth",
    "rough",
    "swift",
    "slow",
    "tiny",
    "huge",
    "wide",
    "narrow",
    "deep",
    "shallow",
    "ancient",
    "modern",
    "simple",
    "complex",
    "elegant",
    "subtle",
    "noble",
    "humble",
    "kind",
    "wise",
    "clever",
    "smart",
    "witty",
    "silly",
    "funny",
    "merry",
    "happy",
    "joyful",
    "cheerful",
    "delighted",
    "blissful",
    "serene",
    "hopeful",
    "eager",
    "curious",
    "patient",
    "still",
    "lively",
    "cozy",
    "snug",
    "tidy",
    "neat",
    "shiny",
    "glowing",
    "dim",
    "dark",
    "light",
    "golden",
    "silver",
    "crimson",
    "scarlet",
    "azure",
    "indigo",
    "violet",
    "amber",
    "emerald",
    "ruby",
    "sapphire",
    "pearl",
    "fragile",
    "sturdy",
    "solid",
    "fluid",
    "frozen",
    "melting",
    "boiling",
    "steamy",
    "misty",
    "foggy",
    "sunny",
    "cloudy",
    "stormy",
    "windy",
    "rainy",
    "snowy",
    "icy",
    "tropical",
    "arctic",
    "temperate",
    "mild",
    // Time-related
    "minute",
    "hour",
    "day",
    "week",
    "month",
    "year",
    "season",
    "decade",
    "century",
    "yesterday",
    "today",
    "tonight",
    "weekend",
    "holiday",
    "spring",
    "summer",
    "autumn",
    "winter",
    "harvest",
    "festive",
    // Places & buildings
    "home",
    "house",
    "room",
    "bedroom",
    "office",
    "garage",
    "yard",
    "porch",
    "balcony",
    "rooftop",
    "attic",
    "basement",
    "library",
    "school",
    "college",
    "studio",
    "theater",
    "museum",
    "gallery",
    "park",
    "plaza",
    "market",
    "shop",
    "cafe",
    "diner",
    "bakery",
    "deli",
    "bridge",
    "tunnel",
    "highway",
    "street",
    "avenue",
    "alley",
    "lane",
    "village",
    "town",
    "city",
    "kingdom",
    "country",
    "continent",
    "globe",
    // Things & objects
    "book",
    "page",
    "letter",
    "word",
    "poem",
    "song",
    "music",
    "melody",
    "rhythm",
    "beat",
    "tune",
    "chord",
    "note",
    "violin",
    "piano",
    "guitar",
    "drum",
    "trumpet",
    "flute",
    "harp",
    "voice",
    "echo",
    "paint",
    "canvas",
    "color",
    "shade",
    "tone",
    "shadow",
    "lamp",
    "candle",
    "torch",
    "flame",
    "spark",
    "ember",
    "ash",
    "smoke",
    "tool",
    "screw",
    "nail",
    "hammer",
    "wrench",
    "saw",
    "drill",
    "ruler",
    "pen",
    "pencil",
    "ink",
    "paper",
    "notebook",
    "diary",
    "journal",
    "key",
    "lock",
    "door",
    "gate",
    "fence",
    "wall",
    "ceiling",
    "floor",
    "rug",
    "carpet",
    "couch",
    "chair",
    "table",
    "desk",
    "shelf",
    "cabinet",
    "compass",
    "map",
    "telescope",
    "lens",
    "camera",
    "phone",
    "screen",
    "keyboard",
    "mouse",
    "speaker",
    "headphone",
    // Emotions & states
    "joy",
    "peace",
    "hope",
    "trust",
    "faith",
    "wonder",
    "awe",
    "delight",
    "warmth",
    "kindness",
    "tender",
    "fond",
    "dear",
    "ease",
    "silence",
    "stillness",
    "relief",
    "grin",
    "wink",
    "nod",
    "hug",
    "kiss",
    "cheer",
    // Abstract concepts
    "idea",
    "thought",
    "memory",
    "fantasy",
    "concept",
    "theory",
    "logic",
    "reason",
    "truth",
    "fact",
    "proof",
    "evidence",
    "method",
    "process",
    "system",
    "pattern",
    "structure",
    "framework",
    "answer",
    "question",
    "problem",
    "solution",
    "puzzle",
    "riddle",
    "clue",
    "secret",
    "mystery",
    "magic",
    "marvel",
    "miracle",
    "blessing",
];

impl ChallengeEngine {
    /// Create a new ChallengeEngine with a random seed.
    pub fn new() -> Self {
        Self {
            rng: StdRng::from_entropy(),
        }
    }

    /// Generate a challenge instance from config.
    pub fn generate(&mut self, config: &ChallengeConfig) -> ChallengeInstance {
        let params = Self::difficulty_params(config.challenge_type, config.difficulty);

        match config.challenge_type {
            ChallengeType::Math => self.generate_math(config.difficulty),
            ChallengeType::Scan => self.generate_scan(config),
            ChallengeType::ShakeToWake => self.generate_shake(&params, config.difficulty),
            ChallengeType::StepCount => self.generate_step_count(&params, config.difficulty),
            ChallengeType::MemoryGame => self.generate_memory(&params, config.difficulty),
            ChallengeType::Typing => self.generate_typing(&params, config.difficulty),
            ChallengeType::HoldButton => self.generate_hold_button(config.difficulty),
            ChallengeType::Reaction => self.generate_reaction(config.difficulty),
        }
    }

    /// Get difficulty parameters for a challenge type.
    pub fn difficulty_params(
        _challenge_type: ChallengeType,
        difficulty: Difficulty,
    ) -> DifficultyParams {
        match difficulty {
            Difficulty::Easy => DifficultyParams {
                shake_count: 10,
                step_count: 10,
                memory_sequence_length: 4,
                typing_word_count: (5, 10),
            },
            Difficulty::Medium => DifficultyParams {
                shake_count: 25,
                step_count: 30,
                memory_sequence_length: 6,
                typing_word_count: (15, 20),
            },
            Difficulty::Hard => DifficultyParams {
                shake_count: 40,
                step_count: 60,
                memory_sequence_length: 9,
                typing_word_count: (25, 35),
            },
            Difficulty::Extreme => DifficultyParams {
                shake_count: 50,
                step_count: 100,
                memory_sequence_length: 12,
                typing_word_count: (40, 60),
            },
        }
    }

    // --- Private generation methods ---

    fn generate_math(&mut self, difficulty: Difficulty) -> ChallengeInstance {
        // Generate clean, unambiguous math expressions per difficulty:
        // Easy: XX +/- XX (2-digit), Medium: XXX +/- XXX (3-digit),
        // Hard: XX × XX, Extreme: XXX × XX.
        let (expression, result) = match difficulty {
            Difficulty::Easy => {
                let a = self.rng.gen_range(10..99);
                let b = self.rng.gen_range(10..99);
                if self.rng.gen_bool(0.5) {
                    (format!("{} + {}", a, b), a + b)
                } else {
                    let (big, small) = if a >= b { (a, b) } else { (b, a) };
                    (format!("{} - {}", big, small), big - small)
                }
            }
            Difficulty::Medium => {
                let a = self.rng.gen_range(100..999);
                let b = self.rng.gen_range(100..999);
                if self.rng.gen_bool(0.5) {
                    (format!("{} + {}", a, b), a + b)
                } else {
                    let (big, small) = if a >= b { (a, b) } else { (b, a) };
                    (format!("{} - {}", big, small), big - small)
                }
            }
            Difficulty::Hard => {
                let a = self.rng.gen_range(11..49);
                let b = self.rng.gen_range(2..19);
                (format!("{} × {}", a, b), a * b)
            }
            Difficulty::Extreme => {
                let a = self.rng.gen_range(100..499);
                let b = self.rng.gen_range(2..12);
                (format!("{} × {}", a, b), a * b)
            }
        };

        ChallengeInstance {
            challenge_type: ChallengeType::Math,
            difficulty,
            prompt: ChallengePrompt::Math { expression },
            expected_answer: ChallengeAnswer::Numeric(result),
        }
    }

    fn generate_scan(&mut self, config: &ChallengeConfig) -> ChallengeInstance {
        let expected_value = config
            .reference_data
            .as_ref()
            .map(|ReferenceData::Scan(v)| v.clone())
            .unwrap_or_default();

        ChallengeInstance {
            challenge_type: ChallengeType::Scan,
            difficulty: config.difficulty,
            prompt: ChallengePrompt::Scan {
                expected_value: expected_value.clone(),
            },
            expected_answer: ChallengeAnswer::ExactString(expected_value),
        }
    }

    fn generate_shake(
        &mut self,
        params: &DifficultyParams,
        difficulty: Difficulty,
    ) -> ChallengeInstance {
        ChallengeInstance {
            challenge_type: ChallengeType::ShakeToWake,
            difficulty,
            prompt: ChallengePrompt::ShakeToWake {
                required_shakes: params.shake_count,
            },
            expected_answer: ChallengeAnswer::ShakeCount(params.shake_count),
        }
    }

    fn generate_step_count(
        &mut self,
        params: &DifficultyParams,
        difficulty: Difficulty,
    ) -> ChallengeInstance {
        ChallengeInstance {
            challenge_type: ChallengeType::StepCount,
            difficulty,
            prompt: ChallengePrompt::StepCount {
                required_steps: params.step_count,
            },
            expected_answer: ChallengeAnswer::StepCount(params.step_count),
        }
    }

    /// HoldButton is a pure count + duration challenge. The concrete
    /// values (how many holds, how long each) are supplied per-alarm via
    /// the challenge config and applied UI-side; this generator only
    /// supplies sensible fallbacks for the engine path.
    fn generate_hold_button(&mut self, difficulty: Difficulty) -> ChallengeInstance {
        const DEFAULT_HOLDS: u32 = 3;
        const DEFAULT_HOLD_MS: u64 = 3000;
        ChallengeInstance {
            challenge_type: ChallengeType::HoldButton,
            difficulty,
            prompt: ChallengePrompt::HoldButton {
                required_holds: DEFAULT_HOLDS,
                hold_duration: Duration::from_millis(DEFAULT_HOLD_MS),
            },
            expected_answer: ChallengeAnswer::HoldCount(DEFAULT_HOLDS),
        }
    }

    /// Reaction is a pure count + time + rounds challenge. The concrete values
    /// are supplied per-alarm via the challenge config and applied UI-side;
    /// this generator only supplies sensible fallbacks for the engine path.
    fn generate_reaction(&mut self, difficulty: Difficulty) -> ChallengeInstance {
        const DEFAULT_TARGETS: u32 = 10;
        const DEFAULT_TIME_MS: u64 = 3000;
        const DEFAULT_ROUNDS: u32 = 2;
        ChallengeInstance {
            challenge_type: ChallengeType::Reaction,
            difficulty,
            prompt: ChallengePrompt::Reaction {
                targets_per_round: DEFAULT_TARGETS,
                time_limit: Duration::from_millis(DEFAULT_TIME_MS),
                rounds: DEFAULT_ROUNDS,
            },
            expected_answer: ChallengeAnswer::ReactionTargets(DEFAULT_TARGETS * DEFAULT_ROUNDS),
        }
    }

    fn generate_memory(
        &mut self,
        params: &DifficultyParams,
        difficulty: Difficulty,
    ) -> ChallengeInstance {
        let len = params.memory_sequence_length as usize;
        // Grid size: 9 cells (3x3) for Easy/Medium, 12 cells (4x3) for Hard/Extreme.
        let grid_cells = match difficulty {
            Difficulty::Easy | Difficulty::Medium => 9u8,
            Difficulty::Hard | Difficulty::Extreme => 12u8,
        };
        // Sequence length cannot exceed grid size since cells must be unique.
        let len = len.min(grid_cells as usize);

        // Generate a unique-cell sequence by shuffling [0..grid_cells) and taking `len`.
        let mut pool: Vec<u8> = (0..grid_cells).collect();
        for i in (1..pool.len()).rev() {
            let j = self.rng.gen_range(0..=i);
            pool.swap(i, j);
        }
        let sequence: Vec<u8> = pool.into_iter().take(len).collect();

        // Display duration scales with sequence length.
        let display_ms = (len as u64) * 1000;

        ChallengeInstance {
            challenge_type: ChallengeType::MemoryGame,
            difficulty,
            prompt: ChallengePrompt::MemoryGame {
                sequence: sequence.clone(),
                display_duration: Duration::from_millis(display_ms),
            },
            expected_answer: ChallengeAnswer::Sequence(sequence),
        }
    }

    fn generate_typing(
        &mut self,
        params: &DifficultyParams,
        difficulty: Difficulty,
    ) -> ChallengeInstance {
        let (min_words, max_words) = params.typing_word_count;
        let word_count = self.rng.gen_range(min_words..=max_words) as usize;

        let passage: String = (0..word_count)
            .map(|_| {
                let idx = self.rng.gen_range(0..WORD_POOL.len());
                WORD_POOL[idx]
            })
            .collect::<Vec<&str>>()
            .join(" ");

        // Minimum accuracy: Easy=80%, Medium=85%, Hard=90%, Extreme=95%.
        let min_accuracy = match difficulty {
            Difficulty::Easy => 0.80,
            Difficulty::Medium => 0.85,
            Difficulty::Hard => 0.90,
            Difficulty::Extreme => 0.95,
        };

        ChallengeInstance {
            challenge_type: ChallengeType::Typing,
            difficulty,
            prompt: ChallengePrompt::Typing {
                passage: passage.clone(),
            },
            expected_answer: ChallengeAnswer::TypedText {
                expected: passage,
                min_accuracy,
            },
        }
    }
}

impl Default for ChallengeEngine {
    fn default() -> Self {
        Self::new()
    }
}
