//! Typed uncertainty signals, separated by source so they can be handled
//! differently (e.g. retrieval gaps trigger more retrieval; tool disagreement
//! triggers escalation).

use serde::{Deserialize, Serialize};

/// The origin of an uncertainty contribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UncertaintyKind {
    /// Irreducible noise in the underlying data.
    Aleatoric,
    /// Model / knowledge gaps (reducible with better evidence).
    Epistemic,
    /// Retrieval coverage gaps.
    Retrieval,
    /// Disagreement among deterministic oracle tools.
    Tool,
}

/// One contribution to the overall uncertainty of a conclusion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UncertaintySignal {
    pub kind: UncertaintyKind,
    /// Magnitude in `[0, 1]`; higher means more uncertain.
    pub magnitude: f64,
    /// Human-readable explanation, surfaced in audit trails.
    pub note: String,
}

impl UncertaintySignal {
    pub fn new(kind: UncertaintyKind, magnitude: f64, note: impl Into<String>) -> Self {
        Self {
            kind,
            magnitude: magnitude.clamp(0.0, 1.0),
            note: note.into(),
        }
    }
}
