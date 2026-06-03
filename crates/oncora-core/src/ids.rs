//! Strongly-typed identifiers and reproducibility pins.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Declare a `String`-backed newtype id with a few ergonomic constructors.
macro_rules! string_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            /// Wrap an existing string.
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }
            /// Mint a fresh random (UUID v4) id.
            pub fn random() -> Self {
                Self(Uuid::new_v4().to_string())
            }
            /// Borrow the inner string.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

string_id!(
    /// Identifies a single agent run; the unit of reproducible replay.
    RunId
);
string_id!(
    /// Identifies a memory entry.
    MemoryId
);
string_id!(
    /// Identifies a single tool call (audited, deterministic).
    ToolCallId
);
string_id!(
    /// A scientist / user principal.
    ScientistId
);
string_id!(
    /// A research project.
    ProjectId
);
string_id!(
    /// A multi-session discovery workflow.
    WorkflowId
);
string_id!(
    /// Pins a content-addressed data snapshot (for reproducible retrieval).
    SnapshotId
);

/// Content-addressed identity of an artifact payload (hex-encoded BLAKE3).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContentHash(pub String);

impl ContentHash {
    pub fn new(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Pins the exact generating model + revision so a run can be reproduced.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelPin {
    /// Model name, e.g. `on-prem/llama-3.1-70b-instruct`.
    pub name: String,
    /// Immutable revision / digest of the served weights.
    pub revision: String,
}

impl ModelPin {
    pub fn new(name: impl Into<String>, revision: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            revision: revision.into(),
        }
    }
}

impl std::fmt::Display for ModelPin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.revision)
    }
}
