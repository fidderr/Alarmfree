//! Alarm-related model types.

use serde::{Deserialize, Serialize};

/// Stable opaque alarm identifier (UUID v4 string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlarmId(pub String);
