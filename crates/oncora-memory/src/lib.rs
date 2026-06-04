//! # oncora-memory
//!
//! The agent memory architecture (named pillar; full design in
//! `docs/02-memory.md`). This crate owns the write/consolidation and hybrid
//! read paths over the five memory types; the data model itself lives in
//! [`oncora_core::memory`] so it can sit behind the [`MemoryStore`] trait.
//!
//! [`InMemoryMemoryStore`] is the reference backend. Production maps the types
//! onto engines (redb for working/episodic, cozo+oxigraph for semantic, a
//! Postgres ledger for provenance) — all behind the same trait.

use std::sync::Mutex;

use async_trait::async_trait;
use oncora_core::{MemoryEntry, MemoryId, MemoryKind, MemoryStore, OncoraError, ReadQuery, Result};

#[cfg(feature = "redb")]
mod redb_store;
#[cfg(feature = "redb")]
pub use redb_store::RedbMemoryStore;

/// Floor below which an entry is considered forgotten (tombstoned).
const DECAY_FLOOR: f64 = 0.05;

/// In-memory reference [`MemoryStore`] implementing a simplified version of the
/// documented write and read paths.
#[derive(Default)]
pub struct InMemoryMemoryStore {
    entries: Mutex<Vec<MemoryEntry>>,
}

impl InMemoryMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl MemoryStore for InMemoryMemoryStore {
    /// Write path (simplified): dedup by (key, kind, claim text), keeping the
    /// higher-confidence entry; otherwise consolidate as a new entry. A real
    /// implementation adds embedding/KG entity resolution and conflict
    /// resolution that retains contradictions as competing evidence.
    async fn write(&self, entry: MemoryEntry) -> Result<MemoryId> {
        let mut guard = self
            .entries
            .lock()
            .map_err(|_| OncoraError::Storage("memory lock poisoned".into()))?;

        if let Some(existing) = guard.iter_mut().find(|e| {
            e.key == entry.key
                && e.kind == entry.kind
                && e.evidence.claim.text == entry.evidence.claim.text
                && !e.tombstoned
        }) {
            // Conflict resolution: keep the more confident assertion, but
            // reinforce (do not lose) the existing one.
            existing.decay_score = (existing.decay_score + 0.25).min(1.0);
            if entry.evidence.confidence.get() > existing.evidence.confidence.get() {
                existing.evidence = entry.evidence;
            }
            return Ok(existing.id.clone());
        }

        let id = entry.id.clone();
        guard.push(entry);
        Ok(id)
    }

    /// Read path (simplified hybrid): scope by key, optional substring match on
    /// the claim text (stand-in for the vector arm), then rank by a fused score
    /// of relevance + recency/decay and truncate to the budget.
    async fn read(&self, query: ReadQuery) -> Result<Vec<MemoryEntry>> {
        let guard = self
            .entries
            .lock()
            .map_err(|_| OncoraError::Storage("memory lock poisoned".into()))?;

        let needle = query.text.as_deref().map(str::to_lowercase);
        let mut scored: Vec<(f64, MemoryEntry)> = guard
            .iter()
            .filter(|e| e.key == query.key && !e.tombstoned)
            .map(|e| {
                let relevance = match &needle {
                    Some(n) => {
                        if e.evidence.claim.text.to_lowercase().contains(n) {
                            1.0
                        } else {
                            0.2
                        }
                    }
                    None => 0.5,
                };
                let fused =
                    0.6 * relevance + 0.3 * e.decay_score + 0.1 * e.evidence.confidence.get();
                (fused, e.clone())
            })
            .collect();

        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        Ok(scored
            .into_iter()
            .take(query.limit)
            .map(|(_, e)| e)
            .collect())
    }

    /// Forgetting: soft-delete via tombstone; provenance is retained.
    async fn forget(&self, id: &MemoryId) -> Result<()> {
        let mut guard = self
            .entries
            .lock()
            .map_err(|_| OncoraError::Storage("memory lock poisoned".into()))?;
        let e = guard
            .iter_mut()
            .find(|e| &e.id == id)
            .ok_or_else(|| OncoraError::NotFound(format!("memory {id}")))?;
        e.tombstoned = true;
        e.decay_score = DECAY_FLOOR;
        Ok(())
    }
}

/// Apply one round of time/usage decay to all entries, tombstoning any that
/// fall below [`DECAY_FLOOR`]. Episodic and provenance memory are exempt
/// (immutable by design).
pub fn apply_decay(store: &InMemoryMemoryStore, factor: f64) {
    if let Ok(mut guard) = store.entries.lock() {
        for e in guard.iter_mut() {
            if matches!(e.kind, MemoryKind::Episodic | MemoryKind::Provenance) {
                continue;
            }
            e.decay_score *= factor.clamp(0.0, 1.0);
            if e.decay_score < DECAY_FLOOR {
                e.tombstoned = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oncora_core::{
        Claim, Confidence, Evidence, MemoryEntry, MemoryKey, ModelPin, ProjectId, Provenance,
        ScientistId, SnapshotId, WorkflowId,
    };

    fn key() -> MemoryKey {
        MemoryKey {
            scientist: ScientistId::new("s1"),
            project: ProjectId::new("p1"),
            workflow: WorkflowId::new("w1"),
        }
    }

    fn entry(text: &str, conf: f64) -> MemoryEntry {
        let prov = Provenance::new(ModelPin::new("m", "r1"), SnapshotId::new("snap1"));
        let ev = Evidence::new(Claim::new(text), Confidence::new(conf), prov);
        MemoryEntry::new(key(), MemoryKind::Semantic, ev)
    }

    /// Backend-agnostic conformance: write dedups equal claims and keeps the
    /// higher confidence; scoped read returns the consolidated entry.
    pub async fn conformance(m: &dyn MemoryStore) {
        m.write(entry("EGFR associated with NSCLC", 0.7))
            .await
            .unwrap();
        let id = m
            .write(entry("EGFR associated with NSCLC", 0.9))
            .await
            .unwrap();

        let q = ReadQuery::new(key()).with_text("EGFR");
        let hits = m.read(q).await.unwrap();
        assert_eq!(hits.len(), 1, "duplicate claims consolidate");
        assert_eq!(hits[0].evidence.confidence.get(), 0.9);

        // Forgetting tombstones the entry; a scoped read no longer returns it.
        m.forget(&id).await.unwrap();
        let after = m
            .read(ReadQuery::new(key()).with_text("EGFR"))
            .await
            .unwrap();
        assert!(after.is_empty(), "forgotten entry is hidden from reads");
    }

    #[tokio::test]
    async fn in_memory_conforms() {
        conformance(&InMemoryMemoryStore::new()).await;
    }

    #[cfg(feature = "redb")]
    #[tokio::test]
    async fn redb_conforms() {
        conformance(&RedbMemoryStore::in_memory().unwrap()).await;
    }
}
