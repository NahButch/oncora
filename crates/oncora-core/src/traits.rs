//! Provider trait boundaries.
//!
//! These are the seams that keep backends swappable. Each trait is implemented
//! by a concrete backend in its owning crate (and by an in-memory reference
//! implementation used for the walking skeleton, dev, and tests). Agents and
//! services depend only on the traits, never on a concrete backend.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::confidence::{CalibrationMethod, Confidence};
use crate::error::Result;
use crate::evidence::{Claim, Evidence, SourceRef};
use crate::ids::{ContentHash, MemoryId, ModelPin, ToolCallId};
use crate::memory::{MemoryEntry, ReadQuery};
use crate::verdict::Verdict;

/// A reasoning LLM endpoint. The one unavoidable non-Rust dependency
/// (on-prem vLLM/TGI, Anthropic, …) lives behind this trait.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// The pinned model identity, recorded in provenance for replay.
    fn model_pin(&self) -> ModelPin;

    /// Complete a prompt. Implementations should be deterministic where the
    /// backend allows (seeded), to support reproducible replay.
    async fn complete(&self, prompt: &str) -> Result<String>;
}

/// Turns text into vectors for retrieval. On-prem (candle/ort/fastembed).
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embedding dimensionality.
    fn dims(&self) -> usize;

    /// Embed a batch of texts.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// A retrieved document with a relevance score.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScoredDoc {
    pub id: String,
    pub score: f32,
    pub text: String,
    pub source: Option<SourceRef>,
}

/// Vector index (qdrant / lancedb / embedded HNSW).
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Insert or replace a vector with its payload.
    async fn upsert(
        &self,
        id: &str,
        vector: Vec<f32>,
        text: String,
        source: Option<SourceRef>,
    ) -> Result<()>;

    /// Return the `k` nearest neighbours to `vector`.
    async fn query(&self, vector: &[f32], k: usize) -> Result<Vec<ScoredDoc>>;

    /// Number of indexed vectors.
    async fn len(&self) -> Result<usize>;

    /// True if the index holds no vectors.
    async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }
}

/// A confidence-weighted, provenance-bearing edge in the evidence graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: Confidence,
}

impl Triple {
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
        confidence: Confidence,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            confidence,
        }
    }
}

/// Knowledge / evidence graph (oxigraph ontology + cozo evidence graph).
#[async_trait]
pub trait GraphStore: Send + Sync {
    /// Assert an edge into the graph.
    async fn assert(&self, triple: Triple) -> Result<()>;

    /// Outgoing edges from `subject`.
    async fn neighbors(&self, subject: &str) -> Result<Vec<Triple>>;

    /// Edges whose subject OR object matches `entity` (1-hop, both directions).
    async fn related(&self, entity: &str) -> Result<Vec<Triple>>;
}

/// The result of an MCP tool call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: ToolCallId,
    pub tool: String,
    pub output: serde_json::Value,
    /// True if the tool is a deterministic oracle (calculator, KG query),
    /// usable as ground truth for uncertainty grounding.
    pub deterministic: bool,
}

/// Hosts and routes MCP tool calls (rmcp). Calls are audited and, where the
/// tool is deterministic, replayable.
#[async_trait]
pub trait ToolHost: Send + Sync {
    /// Names of available tools.
    async fn list_tools(&self) -> Result<Vec<String>>;

    /// Invoke a tool by name with JSON arguments.
    async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<ToolResult>;
}

/// Post-hoc calibration of raw model scores into a calibrated [`Confidence`].
pub trait Calibrator: Send + Sync {
    fn method(&self) -> CalibrationMethod;
    fn calibrate(&self, raw_score: f64) -> Confidence;
}

/// Checks a claim against its evidence (and, optionally, oracle tools),
/// returning a [`Verdict`].
#[async_trait]
pub trait Verifier: Send + Sync {
    async fn verify(&self, claim: &Claim, evidence: &Evidence) -> Result<Verdict>;
}

/// Content-addressed artifact store underpinning reproducibility.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
    /// Store bytes, returning their content hash (BLAKE3).
    async fn put(&self, bytes: &[u8]) -> Result<ContentHash>;

    /// Retrieve bytes by content hash.
    async fn get(&self, hash: &ContentHash) -> Result<Vec<u8>>;
}

/// Persistent agent memory (the named pillar). See `docs/02-memory.md`.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Write path: extract -> dedup -> conflict-resolve -> consolidate -> decay.
    async fn write(&self, entry: MemoryEntry) -> Result<MemoryId>;

    /// Read path: hybrid retrieve -> fuse -> assemble under budget.
    async fn read(&self, query: ReadQuery) -> Result<Vec<MemoryEntry>>;

    /// Soft-delete with a tombstone; provenance is retained.
    async fn forget(&self, id: &MemoryId) -> Result<()>;
}
