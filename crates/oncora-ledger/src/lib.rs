//! # oncora-ledger
//!
//! Backends for the [`oncora_core::LedgerStore`] provenance/audit ledger,
//! demonstrating the platform's trait-based swappability at its sharpest: the
//! **same trait, same SQL schema, same tests** run against three
//! interchangeable backends —
//!
//! * [`InMemoryLedger`] — always available; dev/tests.
//! * `SqliteCLedger` — **C SQLite** (the upstream amalgamation, compiled and
//!   linked via `rusqlite`). Enable with `--features sqlite-c`.
//! * `SqliteRustLedger` — **pure-Rust SQLite** (`turso`, formerly `limbo`).
//!   Enable with `--features sqlite-rust`.
//!
//! This is the runnable form of the C→Rust SQLite migration spike in
//! `docs/08-roadmap.md`: ship on C SQLite, then swap the pure-Rust engine in
//! behind this exact trait and re-run the suite.

use std::sync::Mutex;

use async_trait::async_trait;
use oncora_core::{LedgerRecord, LedgerStore, OncoraError, Result, RunId};

#[cfg(feature = "sqlite-c")]
mod sqlite_c;
#[cfg(feature = "sqlite-c")]
pub use sqlite_c::SqliteCLedger;

#[cfg(feature = "sqlite-rust")]
mod sqlite_rust;
#[cfg(feature = "sqlite-rust")]
pub use sqlite_rust::SqliteRustLedger;

/// The SQL schema shared by every relational backend, so the C and Rust SQLite
/// engines are exercised identically.
pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS ledger (\
    id INTEGER PRIMARY KEY AUTOINCREMENT, \
    run_id TEXT NOT NULL, \
    seq INTEGER NOT NULL, \
    kind TEXT NOT NULL, \
    payload TEXT NOT NULL, \
    content_hash TEXT NOT NULL);";

/// In-memory reference [`LedgerStore`].
#[derive(Default)]
pub struct InMemoryLedger {
    rows: Mutex<Vec<LedgerRecord>>,
}

impl InMemoryLedger {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl LedgerStore for InMemoryLedger {
    async fn append(&self, record: LedgerRecord) -> Result<i64> {
        let mut g = self
            .rows
            .lock()
            .map_err(|_| OncoraError::Storage("ledger lock poisoned".into()))?;
        g.push(record);
        Ok(g.len() as i64)
    }

    async fn records_for(&self, run_id: &RunId) -> Result<Vec<LedgerRecord>> {
        let g = self
            .rows
            .lock()
            .map_err(|_| OncoraError::Storage("ledger lock poisoned".into()))?;
        let mut out: Vec<LedgerRecord> =
            g.iter().filter(|r| &r.run_id == run_id).cloned().collect();
        out.sort_by_key(|r| r.seq);
        Ok(out)
    }

    async fn count(&self) -> Result<usize> {
        Ok(self
            .rows
            .lock()
            .map_err(|_| OncoraError::Storage("ledger lock poisoned".into()))?
            .len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oncora_core::{ContentHash, RunId};

    /// The single, backend-agnostic conformance test. Every backend must pass
    /// it — that is the whole point of the trait boundary.
    pub async fn conformance(store: &dyn LedgerStore) {
        let run = RunId::new("run-1");
        for seq in 0..3 {
            store
                .append(LedgerRecord::new(
                    run.clone(),
                    seq,
                    "tool_call",
                    format!("{{\"seq\":{seq}}}"),
                    ContentHash::new(format!("hash{seq}")),
                ))
                .await
                .unwrap();
        }
        // A record from a different run must not leak into the query.
        store
            .append(LedgerRecord::new(
                RunId::new("run-2"),
                0,
                "verdict",
                "{}",
                ContentHash::new("other"),
            ))
            .await
            .unwrap();

        assert_eq!(store.count().await.unwrap(), 4);
        let recs = store.records_for(&run).await.unwrap();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].seq, 0);
        assert_eq!(recs[2].seq, 2);
        assert_eq!(recs[1].kind, "tool_call");
    }

    #[tokio::test]
    async fn in_memory_conforms() {
        conformance(&InMemoryLedger::new()).await;
    }

    #[cfg(feature = "sqlite-c")]
    #[tokio::test]
    async fn sqlite_c_conforms() {
        let store = SqliteCLedger::open_in_memory().unwrap();
        conformance(&store).await;
    }

    #[cfg(feature = "sqlite-rust")]
    #[tokio::test]
    async fn sqlite_rust_conforms() {
        let store = SqliteRustLedger::open_in_memory().await.unwrap();
        conformance(&store).await;
    }

    /// The payoff: three different engines behind ONE `dyn LedgerStore`, each
    /// passing the identical conformance test. C SQLite and pure-Rust SQLite
    /// are drop-in interchangeable.
    #[cfg(all(feature = "sqlite-c", feature = "sqlite-rust"))]
    #[tokio::test]
    async fn all_backends_agree() {
        let backends: Vec<(&str, Box<dyn LedgerStore>)> = vec![
            ("in-memory", Box::new(InMemoryLedger::new())),
            (
                "sqlite-c",
                Box::new(SqliteCLedger::open_in_memory().unwrap()),
            ),
            (
                "sqlite-rust",
                Box::new(SqliteRustLedger::open_in_memory().await.unwrap()),
            ),
        ];
        for (name, store) in &backends {
            conformance(store.as_ref()).await;
            eprintln!("{name}: conformance ok");
        }
    }
}
