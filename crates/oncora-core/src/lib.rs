//! # oncora-core
//!
//! Foundational, dependency-light types and **provider trait boundaries** for the
//! Oncora platform. Every other crate depends on this one; dependencies point
//! inward to `oncora-core` (see `docs/07-repo-layout.md`).
//!
//! The two ideas that make Oncora more than a RAG chatbot live here as types:
//!
//! * [`Confidence`] — a typed, calibrated, first-class uncertainty value, and
//!   [`Verdict`] — the system's ability to `Accept`, `Abstain`, or `Escalate`.
//! * [`Provenance`] / [`Evidence`] — provenance attached to every claim, plus
//!   the [`ModelPin`] + [`SnapshotId`] that make a run reproducible.
//!
//! The provider traits ([`ModelProvider`], [`VectorStore`], [`GraphStore`],
//! [`MemoryStore`], [`ToolHost`], [`Calibrator`], [`Verifier`],
//! [`ArtifactStore`], [`EmbeddingProvider`]) are the swappable seams: concrete
//! backends (qdrant, oxigraph, cozo, redb, rmcp, candle, …) implement them in
//! their owning crates, so providers can be replaced without touching callers.

pub mod confidence;
pub mod error;
pub mod evidence;
pub mod ids;
pub mod ledger;
pub mod memory;
pub mod traits;
pub mod uncertainty;
pub mod verdict;

pub use confidence::{CalibrationMethod, Confidence};
pub use error::{OncoraError, Result};
pub use evidence::{Claim, Evidence, Provenance, SourceRef};
pub use ids::{
    ContentHash, MemoryId, ModelPin, ProjectId, RunId, ScientistId, SnapshotId, ToolCallId,
    WorkflowId,
};
pub use ledger::{LedgerRecord, LedgerStore};
pub use memory::{MemoryEntry, MemoryKey, MemoryKind, ReadQuery};
pub use traits::{
    ArtifactStore, Calibrator, EmbeddingProvider, GraphStore, MemoryStore, ModelProvider,
    ScoredDoc, ToolHost, ToolResult, Triple, VectorStore, Verifier,
};
pub use uncertainty::{UncertaintyKind, UncertaintySignal};
pub use verdict::{EscalationTarget, Verdict};

/// Convenience re-exports for downstream crates.
pub mod prelude {
    pub use crate::{
        ArtifactStore, CalibrationMethod, Calibrator, Claim, Confidence, EmbeddingProvider,
        EscalationTarget, Evidence, GraphStore, LedgerRecord, LedgerStore, MemoryEntry, MemoryKey,
        MemoryKind, MemoryStore, ModelPin, ModelProvider, OncoraError, Provenance, ReadQuery,
        Result, RunId, ScoredDoc, SnapshotId, SourceRef, ToolHost, ToolResult, Triple,
        UncertaintyKind, UncertaintySignal, VectorStore, Verdict, Verifier,
    };
}
