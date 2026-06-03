//! Memory data model shared by the [`crate::MemoryStore`] trait boundary.
//!
//! The full architecture (write/consolidation path, hybrid read path,
//! storage-engine mapping) is documented in `docs/02-memory.md`; the concrete
//! engines live in the `oncora-memory` crate.

use serde::{Deserialize, Serialize};

use crate::evidence::Evidence;
use crate::ids::{ContentHash, MemoryId, ModelPin, ProjectId, ScientistId, SnapshotId, WorkflowId};

/// Which of the five memory types an entry belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryKind {
    /// Current-run scratchpad / plan state.
    Working,
    /// Append-only run history (steps, tool calls, decisions).
    Episodic,
    /// Durable facts / relationships (the knowledge graph).
    Semantic,
    /// Learned tool-use patterns / successful plans.
    Procedural,
    /// Links every item to sources, tool calls, model pin, snapshot.
    Provenance,
}

/// Cross-session scope key: a workflow belongs to a project belongs to a
/// scientist. Memory is retrieved within this scope.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryKey {
    pub scientist: ScientistId,
    pub project: ProjectId,
    pub workflow: WorkflowId,
}

/// One versioned, attributed, content-addressed memory entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub key: MemoryKey,
    pub kind: MemoryKind,
    /// The substance: claim + support/contradiction + confidence + provenance.
    pub evidence: Evidence,
    /// Data snapshot pin (for replay).
    pub snapshot: SnapshotId,
    /// Generating model pin (for replay).
    pub model: ModelPin,
    /// Content-addressed payload handle (BLAKE3).
    pub payload: Option<ContentHash>,
    /// Decay score from time + usage; below a floor the entry is tombstoned.
    pub decay_score: f64,
    /// Soft-deleted (provenance retained even when tombstoned).
    pub tombstoned: bool,
}

impl MemoryEntry {
    pub fn new(key: MemoryKey, kind: MemoryKind, evidence: Evidence) -> Self {
        let snapshot = evidence.provenance.snapshot.clone();
        let model = evidence.provenance.model.clone();
        Self {
            id: MemoryId::random(),
            key,
            kind,
            evidence,
            snapshot,
            model,
            payload: None,
            decay_score: 1.0,
            tombstoned: false,
        }
    }
}

/// A read request resolved under a memory key and token budget.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadQuery {
    pub key: MemoryKey,
    /// Free-text query for the vector arm of hybrid retrieval.
    pub text: Option<String>,
    /// Point-in-time read (cozo time-travel) — `None` means "latest".
    pub as_of: Option<SnapshotId>,
    /// Maximum number of entries to return.
    pub limit: usize,
}

impl ReadQuery {
    pub fn new(key: MemoryKey) -> Self {
        Self {
            key,
            text: None,
            as_of: None,
            limit: 8,
        }
    }
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}
