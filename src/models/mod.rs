pub mod alarm;
pub mod challenge;
pub mod history;
pub mod settings;

// Re-export key types
pub use alarm::AlarmId;
pub use challenge::{
    ChallengeAnswer, ChallengeConfig, ChallengeEngine, ChallengeInstance, ChallengePrompt,
    ChallengeType, Difficulty, DifficultyParams, ReferenceData,
};
pub use history::{AggregateMetrics, AlarmEvent, ChallengeSolve, HistoryFilter};
pub use settings::{AppSettings, Locale, ThemeMode};
