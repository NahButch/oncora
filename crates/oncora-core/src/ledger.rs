//! The provenance / audit **ledger** boundary.
//!
//! This is the append-only record of what an agent run did — every tool call,
//! verdict, and consolidated claim — linked to its content hash. It is the
//! natural home for a relational store (SQLite in dev, Postgres in production;
//! see `docs/08-roadmap.md`). Like every other provider, it sits behind a
//! trait so the backend is swappable — including **C SQLite vs pure-Rust
//! SQLite** (the migration spike in `docs/08-roadmap.md`).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::ids::{ContentHash, RunId};

/// One append-only ledger entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerRecord {
    /// The run this record belongs to.
    pub run_id: RunId,
    /// Monotonic sequence within the run (ordering of events).
    pub seq: i64,
    /// Record kind, e.g. `tool_call`, `verdict`, `claim`.
    pub kind: String,
    /// JSON payload (the event body).
    pub payload: String,
    /// Content hash of the payload (reproducibility / dedup).
    pub content_hash: ContentHash,
}

impl LedgerRecord {
    pub fn new(
        run_id: RunId,
        seq: i64,
        kind: impl Into<String>,
        payload: impl Into<String>,
        content_hash: ContentHash,
    ) -> Self {
        Self {
            run_id,
            seq,
            kind: kind.into(),
            payload: payload.into(),
            content_hash,
        }
    }
}

/// Append-only, queryable provenance/audit ledger.
///
/// Backends: in-memory (dev/tests), **C SQLite** (`rusqlite`), **pure-Rust
/// SQLite** (`turso`), and Postgres (production) — all interchangeable.
#[async_trait]
pub trait LedgerStore: Send + Sync {
    /// Append a record; returns the storage row id.
    async fn append(&self, record: LedgerRecord) -> Result<i64>;

    /// All records for a run, ordered by `seq`.
    async fn records_for(&self, run_id: &RunId) -> Result<Vec<LedgerRecord>>;

    /// Total number of records.
    async fn count(&self) -> Result<usize>;
}
