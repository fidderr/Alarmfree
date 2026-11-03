pub mod alarm;
pub mod challenge;
pub mod history;
pub mod settings;
pub mod setup;

// Re-export key types
pub use alarm::AlarmId;
pub use challenge::{
    typing_accuracy, ChallengeAnswer, ChallengeConfig, ChallengeEngine, ChallengeInstance,
    ChallengePrompt, ChallengeType, Difficulty, DifficultyParams, ReferenceData,
};
pub use history::{AggregateMetrics, AlarmEvent, AlarmOutcome, ChallengeSolve, HistoryFilter};
pub use settings::{
    Accent, AppSettings, Locale, Prefs, SortMode, ThemeMode, TimeFormat, WeekStart,
};
pub use setup::ChallengeSetup;
