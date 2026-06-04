//! Persistent agent memory backed by **redb** (pure-Rust embedded ACID KV).
//!
//! Implements the same [`MemoryStore`] trait as [`crate::InMemoryMemoryStore`],
//! so the agent runtime is unchanged whether memory lives in-process or is
//! durably persisted to disk (the canon's primary embedded engine for
//! working/episodic memory — see `docs/02-memory.md`). Entries are serialized
//! with serde_json and keyed by `MemoryId`; the write/read/forget logic mirrors
//! the in-memory reference (dedup + conflict resolution on write; fused
//! relevance + recency on read; tombstone on forget).

use std::path::Path;

use async_trait::async_trait;
use oncora_core::{MemoryEntry, MemoryId, MemoryStore, OncoraError, ReadQuery, Result};
use redb::{Database, ReadableTable, TableDefinition};

const ENTRIES: TableDefinition<&str, &[u8]> = TableDefinition::new("memory_entries");
/// Dedup index: `dedup_key` -> entry id, so the write path is O(1).
const DEDUP: TableDefinition<&str, &str> = TableDefinition::new("memory_dedup");

fn stor(e: impl std::fmt::Display) -> OncoraError {
    OncoraError::Storage(format!("redb: {e}"))
}

/// A [`MemoryStore`] persisted with redb.
pub struct RedbMemoryStore {
    db: Database,
}

impl RedbMemoryStore {
    fn init(db: Database) -> Result<Self> {
        // Ensure the table exists so reads on a fresh db don't error.
        let w = db.begin_write().map_err(stor)?;
        {
            w.open_table(ENTRIES).map_err(stor)?;
            w.open_table(DEDUP).map_err(stor)?;
        }
        w.commit().map_err(stor)?;
        Ok(Self { db })
    }

    /// Open (or create) a persistent memory database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Database::create(path).map_err(stor)?)
    }

    /// An ephemeral in-memory redb database (for tests).
    pub fn in_memory() -> Result<Self> {
        let db = Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(stor)?;
        Self::init(db)
    }

    /// Decode every stored entry.
    fn all_entries(&self) -> Result<Vec<(String, MemoryEntry)>> {
        let rtx = self.db.begin_read().map_err(stor)?;
        let table = rtx.open_table(ENTRIES).map_err(stor)?;
        let mut out = Vec::new();
        for item in table.iter().map_err(stor)? {
            let (k, v) = item.map_err(stor)?;
            let entry: MemoryEntry = serde_json::from_slice(v.value())?;
            out.push((k.value().to_string(), entry));
        }
        Ok(out)
    }
}

#[async_trait]
impl MemoryStore for RedbMemoryStore {
    async fn write(&self, entry: MemoryEntry) -> Result<MemoryId> {
        let key = crate::dedup_key(&entry);
        let wtx = self.db.begin_write().map_err(stor)?;
        let id = {
            // O(1) dedup lookup via the index table.
            let existing_id = {
                let dtab = wtx.open_table(DEDUP).map_err(stor)?;
                dtab.get(key.as_str())
                    .map_err(stor)?
                    .map(|v| v.value().to_string())
            };
            let mut entries = wtx.open_table(ENTRIES).map_err(stor)?;
            let mut dedup = wtx.open_table(DEDUP).map_err(stor)?;

            let reused = match &existing_id {
                Some(eid) => {
                    // Copy out the stored bytes so the read borrow ends before insert.
                    let cur_bytes = entries
                        .get(eid.as_str())
                        .map_err(stor)?
                        .map(|v| v.value().to_vec());
                    match cur_bytes {
                        Some(bytes) => {
                            let mut e: MemoryEntry = serde_json::from_slice(&bytes)?;
                            if e.tombstoned {
                                None
                            } else {
                                e.decay_score = (e.decay_score + 0.25).min(1.0);
                                if entry.evidence.confidence.get() > e.evidence.confidence.get() {
                                    e.evidence = entry.evidence.clone();
                                }
                                let out = serde_json::to_vec(&e)?;
                                entries.insert(eid.as_str(), out.as_slice()).map_err(stor)?;
                                Some(MemoryId::new(eid.clone()))
                            }
                        }
                        None => None,
                    }
                }
                None => None,
            };

            match reused {
                Some(id) => id,
                None => {
                    let bytes = serde_json::to_vec(&entry)?;
                    entries
                        .insert(entry.id.as_str(), bytes.as_slice())
                        .map_err(stor)?;
                    dedup
                        .insert(key.as_str(), entry.id.as_str())
                        .map_err(stor)?;
                    entry.id.clone()
                }
            }
        };
        wtx.commit().map_err(stor)?;
        Ok(id)
    }

    async fn read(&self, query: ReadQuery) -> Result<Vec<MemoryEntry>> {
        let needle = query.text.as_deref().map(str::to_lowercase);
        let mut scored: Vec<(f64, MemoryEntry)> = self
            .all_entries()?
            .into_iter()
            .map(|(_, e)| e)
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
                (fused, e)
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        Ok(scored
            .into_iter()
            .take(query.limit)
            .map(|(_, e)| e)
            .collect())
    }

    async fn forget(&self, id: &MemoryId) -> Result<()> {
        let wtx = self.db.begin_write().map_err(stor)?;
        {
            let mut table = wtx.open_table(ENTRIES).map_err(stor)?;
            let current = table
                .get(id.as_str())
                .map_err(stor)?
                .ok_or_else(|| OncoraError::NotFound(format!("memory {id}")))?;
            let mut entry: MemoryEntry = serde_json::from_slice(current.value())?;
            drop(current);
            entry.tombstoned = true;
            entry.decay_score = 0.05;
            let bytes = serde_json::to_vec(&entry)?;
            table.insert(id.as_str(), bytes.as_slice()).map_err(stor)?;
        }
        wtx.commit().map_err(stor)?;
        Ok(())
    }
}
