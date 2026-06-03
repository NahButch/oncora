//! # oncora-artifacts
//!
//! Content-addressed storage (CAS) and run manifests — the substrate for
//! **reproducibility**. Artifacts are addressed by BLAKE3 hash, so identical
//! content deduplicates and any run can be replayed from a manifest of hashes.
//!
//! The [`InMemoryArtifactStore`] is the reference/dev backend; a production
//! backend writes to an object store (see `docs/08-roadmap.md`) behind the same
//! [`ArtifactStore`] trait.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use oncora_core::{ArtifactStore, ContentHash, ModelPin, OncoraError, Result, RunId, SnapshotId};
use serde::{Deserialize, Serialize};

/// Hash bytes with BLAKE3, returning a hex [`ContentHash`].
pub fn hash_bytes(bytes: &[u8]) -> ContentHash {
    ContentHash::new(blake3::hash(bytes).to_hex().to_string())
}

/// A reproducible description of one agent run: pin everything needed to replay
/// it and to attribute every reported number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: RunId,
    pub model: ModelPin,
    pub snapshot: SnapshotId,
    /// Deterministic seed used for sampling / tie-breaks.
    pub seed: u64,
    /// Content hashes of inputs/outputs captured during the run.
    pub artifacts: Vec<ContentHash>,
    /// Hash of the effective configuration.
    pub config_hash: ContentHash,
}

impl RunManifest {
    pub fn new(run_id: RunId, model: ModelPin, snapshot: SnapshotId, seed: u64) -> Self {
        Self {
            run_id,
            model,
            snapshot,
            seed,
            artifacts: Vec::new(),
            config_hash: hash_bytes(b"{}"),
        }
    }
}

/// In-memory reference [`ArtifactStore`].
#[derive(Default)]
pub struct InMemoryArtifactStore {
    blobs: Mutex<HashMap<String, Vec<u8>>>,
}

impl InMemoryArtifactStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ArtifactStore for InMemoryArtifactStore {
    async fn put(&self, bytes: &[u8]) -> Result<ContentHash> {
        let hash = hash_bytes(bytes);
        self.blobs
            .lock()
            .map_err(|_| OncoraError::Storage("artifact lock poisoned".into()))?
            .insert(hash.0.clone(), bytes.to_vec());
        Ok(hash)
    }

    async fn get(&self, hash: &ContentHash) -> Result<Vec<u8>> {
        self.blobs
            .lock()
            .map_err(|_| OncoraError::Storage("artifact lock poisoned".into()))?
            .get(hash.as_str())
            .cloned()
            .ok_or_else(|| OncoraError::NotFound(format!("artifact {hash}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trips_and_dedups() {
        let store = InMemoryArtifactStore::new();
        let h1 = store.put(b"BRCA1").await.unwrap();
        let h2 = store.put(b"BRCA1").await.unwrap();
        assert_eq!(h1, h2, "identical content must hash equally");
        assert_eq!(store.get(&h1).await.unwrap(), b"BRCA1");
    }
}
