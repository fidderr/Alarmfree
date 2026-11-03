//! The kinds of challenge, and [`ChallengeEngine`], which makes the sums, sequences and
//! passages of those that differ each time. Each challenge component evaluates its own
//! correctness; this module only generates.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

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

/// Makes the challenges that differ each time.
pub struct ChallengeEngine {
    rng: StdRng,
    locale: crate::models::Locale,
}

/// Word pool for typing challenges in the user's language. Short everyday words
/// without accents: this should wake the user up, not test their keyboard.
fn word_pool(locale: crate::models::Locale) -> &'static [&'static str] {
    use crate::models::Locale;
    match locale {
        Locale::English => WORD_POOL,
        Locale::Dutch => &[
            "de", "het", "een", "goede", "morgen", "wekker", "tijd", "slaap", "droom", "nacht",
            "vroeg", "opstaan", "koffie", "ontbijt", "energie", "rustig", "ademen", "zon", "licht",
            "dag", "werk", "huis", "fiets", "water", "brood", "kaas", "appel", "tafel", "stoel",
            "raam", "deur", "boek", "vogel", "kat", "hond", "bloem", "boom", "regen", "wind",
            "wolk", "straat", "stad", "vriend", "lachen", "lopen", "zingen", "denken", "wakker",
        ],
        Locale::German => &[
            "der",
            "die",
            "das",
            "guten",
            "morgen",
            "wecker",
            "zeit",
            "schlaf",
            "traum",
            "nacht",
            "aufstehen",
            "kaffee",
            "brot",
            "energie",
            "ruhig",
            "atmen",
            "sonne",
            "licht",
            "tag",
            "arbeit",
            "haus",
            "fahrrad",
            "wasser",
            "apfel",
            "tisch",
            "stuhl",
            "fenster",
            "buch",
            "vogel",
            "katze",
            "hund",
            "blume",
            "baum",
            "regen",
            "wind",
            "wolke",
            "strasse",
            "stadt",
            "freund",
            "lachen",
            "laufen",
            "singen",
            "denken",
            "wach",
            "garten",
            "milch",
            "schule",
        ],
        Locale::French => &[
            "le", "la", "un", "une", "bon", "matin", "temps", "sommeil", "nuit", "lever", "cafe",
            "pain", "calme", "soleil", "jour", "travail", "maison", "velo", "eau", "pomme",
            "table", "chaise", "porte", "livre", "oiseau", "chat", "chien", "fleur", "arbre",
            "pluie", "vent", "nuage", "rue", "ville", "ami", "rire", "marcher", "chanter",
            "penser", "jardin", "lait", "ecole", "lune", "mer", "main", "tasse", "route", "fruit",
        ],
        Locale::Spanish => &[
            "el", "la", "un", "una", "buenos", "dias", "tiempo", "sueno", "noche", "cafe", "pan",
            "calma", "sol", "luz", "dia", "trabajo", "casa", "agua", "mesa", "silla", "puerta",
            "libro", "gato", "perro", "flor", "lluvia", "viento", "nube", "calle", "ciudad",
            "amigo", "reir", "andar", "cantar", "pensar", "jardin", "leche", "escuela", "luna",
            "mar", "mano", "taza", "camino", "fruta", "manzana", "ventana", "pajaro", "arbol",
        ],
    }
}

/// How closely `typed` matches `passage`, from 0 to 1: one minus the edit distance
/// over the length. Capitals do not count, because a phone keyboard adds them by
/// itself, and neither does a space at the end. A skipped letter costs one letter,
/// not everything after it. The one measure for both the screen and the gate.
pub fn typing_accuracy(passage: &str, typed: &str) -> f32 {
    let normalize =
        |s: &str| -> Vec<char> { s.trim_end().chars().flat_map(char::to_lowercase).collect() };
    let passage = normalize(passage);
    let typed = normalize(typed);
    let longest = passage.len().max(typed.len());
    if longest == 0 {
        return 1.0;
    }
    1.0 - edit_distance(&passage, &typed) as f32 / longest as f32
}

/// Levenshtein distance.
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut current = Vec::with_capacity(b.len() + 1);
        current.push(i + 1);
        for (j, cb) in b.iter().enumerate() {
            let replaced = previous[j] + usize::from(ca != cb);
            current.push(replaced.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
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
    /// With a random seed, and passages in this language.
    pub fn new(locale: crate::models::Locale) -> Self {
        Self {
            rng: StdRng::from_entropy(),
            locale,
        }
    }

    /// A sum and its answer. Easy: XX +/- XX, Medium: XXX +/- XXX, Hard: XX × XX,
    /// Extreme: XXX × XX.
    pub fn math(&mut self, difficulty: Difficulty) -> (String, i64) {
        match difficulty {
            Difficulty::Easy => self.add_or_subtract(10..99),
            Difficulty::Medium => self.add_or_subtract(100..999),
            Difficulty::Hard => {
                let a = self.rng.gen_range(11..49);
                let b = self.rng.gen_range(2..19);
                (format!("{a} × {b}"), a * b)
            }
            Difficulty::Extreme => {
                let a = self.rng.gen_range(100..499);
                let b = self.rng.gen_range(2..12);
                (format!("{a} × {b}"), a * b)
            }
        }
    }

    fn add_or_subtract(&mut self, range: std::ops::Range<i64>) -> (String, i64) {
        let a = self.rng.gen_range(range.clone());
        let b = self.rng.gen_range(range);
        if self.rng.gen_bool(0.5) {
            (format!("{a} + {b}"), a + b)
        } else {
            let (big, small) = (a.max(b), a.min(b));
            (format!("{big} - {small}"), big - small)
        }
    }

    /// Cells to remember in order, and the width of the grid they are on: 3 by 3, or
    /// 4 by 3 from Hard on. No cell twice.
    pub fn memory(&mut self, difficulty: Difficulty) -> (Vec<u8>, u8) {
        let (len, width, cells) = match difficulty {
            Difficulty::Easy => (4, 3, 9u8),
            Difficulty::Medium => (6, 3, 9),
            Difficulty::Hard => (9, 4, 12),
            Difficulty::Extreme => (12, 4, 12),
        };
        let mut pool: Vec<u8> = (0..cells).collect();
        for i in (1..pool.len()).rev() {
            let j = self.rng.gen_range(0..=i);
            pool.swap(i, j);
        }
        pool.truncate(len);
        (pool, width)
    }

    /// A passage to type, and how much of it has to be right.
    pub fn typing(&mut self, difficulty: Difficulty) -> (String, f32) {
        let (min_words, max_words, min_accuracy) = match difficulty {
            Difficulty::Easy => (5, 10, 0.80),
            Difficulty::Medium => (15, 20, 0.85),
            Difficulty::Hard => (25, 35, 0.90),
            Difficulty::Extreme => (40, 60, 0.95),
        };
        let word_count = self.rng.gen_range(min_words..=max_words);
        let pool = word_pool(self.locale);
        let passage = (0..word_count)
            .map(|_| pool[self.rng.gen_range(0..pool.len())])
            .collect::<Vec<&str>>()
            .join(" ");
        (passage, min_accuracy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_difficulty_asks_for_something_doable() {
        let mut engine = ChallengeEngine::new(crate::models::Locale::English);
        for (difficulty, cells, width) in [
            (Difficulty::Easy, 4, 3),
            (Difficulty::Medium, 6, 3),
            (Difficulty::Hard, 9, 4),
            (Difficulty::Extreme, 12, 4),
        ] {
            let (sequence, grid) = engine.memory(difficulty);
            assert_eq!(sequence.len(), cells);
            assert_eq!(grid, width);
            let mut unique = sequence.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), cells, "no cell twice");

            let (sum, answer) = engine.math(difficulty);
            assert!(!sum.is_empty());
            assert!(answer >= 0, "{sum} = {answer}");

            let (passage, accuracy) = engine.typing(difficulty);
            assert!(passage.split(' ').count() >= 5);
            assert!((0.8..=0.95).contains(&accuracy));
        }
    }

    #[test]
    fn capitals_do_not_count_when_typing() {
        assert_eq!(typing_accuracy("good morning", "Good Morning"), 1.0);
        assert_eq!(typing_accuracy("good morning", "GOOD MORNING "), 1.0);
    }

    #[test]
    fn a_skipped_letter_costs_one_letter_and_not_everything_after_it() {
        let accuracy = typing_accuracy("good morning sunshine", "god morning sunshine");
        assert!(accuracy > 0.95, "{accuracy}");
    }

    #[test]
    fn typing_something_else_is_not_close() {
        assert!(typing_accuracy("good morning", "xxxx") < 0.2);
        assert!(typing_accuracy("good morning", "") < 0.01);
        assert_eq!(typing_accuracy("", ""), 1.0);
    }

    #[test]
    fn every_language_has_words_a_phone_keyboard_types_easily() {
        use crate::models::Locale;
        for locale in [
            Locale::English,
            Locale::Dutch,
            Locale::German,
            Locale::French,
            Locale::Spanish,
        ] {
            let pool = word_pool(locale);
            assert!(pool.len() >= 40, "{locale:?} has {} words", pool.len());
            for word in pool {
                assert!(
                    word.chars().all(|c| c.is_ascii_lowercase()),
                    "{locale:?}: {word}"
                );
            }
        }
    }
}
