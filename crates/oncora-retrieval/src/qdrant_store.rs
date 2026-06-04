//! Real vector database backend via the official `qdrant-client` (gRPC).
//!
//! Implements the same [`VectorStore`] trait as [`crate::InMemoryVectorStore`],
//! so the agent runtime is unchanged whether retrieval is served from the
//! in-process reference store or a qdrant cluster (see
//! `docs/05-tech-decisions.md`).
//!
//! qdrant point ids must be a `u64` or a UUID, so a stable hash of the caller's
//! string id is used as the point id and the original id is preserved in the
//! payload alongside the text and source.

use std::collections::HashMap;

use async_trait::async_trait;
use oncora_core::{OncoraError, Result, ScoredDoc, SourceRef, VectorStore};
use qdrant_client::qdrant::{
    CountPointsBuilder, CreateCollectionBuilder, Distance, PointStruct, QueryPointsBuilder,
    UpsertPointsBuilder, Value as QValue, VectorParamsBuilder,
};
use qdrant_client::{Payload, Qdrant};

/// A [`VectorStore`] backed by a qdrant server.
pub struct QdrantVectorStore {
    client: Qdrant,
    collection: String,
}

fn storage(e: impl std::fmt::Display) -> OncoraError {
    OncoraError::Storage(format!("qdrant: {e}"))
}

/// Stable 64-bit id for a caller-supplied string id (FNV-1a).
fn point_id(id: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn payload_str(payload: &HashMap<String, QValue>, key: &str) -> Option<String> {
    payload
        .get(key)
        .map(|v| v.clone().into_json())
        .and_then(|j| j.as_str().map(str::to_string))
}

impl QdrantVectorStore {
    /// Connect to qdrant at `url` (e.g. `http://127.0.0.1:6334`) and ensure the
    /// `collection` exists with `dim`-dimensional cosine vectors.
    pub async fn connect(url: &str, collection: &str, dim: u64) -> Result<Self> {
        let client = Qdrant::from_url(url).build().map_err(storage)?;
        if !client
            .collection_exists(collection)
            .await
            .map_err(storage)?
        {
            client
                .create_collection(
                    CreateCollectionBuilder::new(collection)
                        .vectors_config(VectorParamsBuilder::new(dim, Distance::Cosine)),
                )
                .await
                .map_err(storage)?;
        }
        Ok(Self {
            client,
            collection: collection.to_string(),
        })
    }
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn upsert(
        &self,
        id: &str,
        vector: Vec<f32>,
        text: String,
        source: Option<SourceRef>,
    ) -> Result<()> {
        let mut map = serde_json::Map::new();
        map.insert("doc_id".into(), id.into());
        map.insert("text".into(), text.into());
        if let Some(s) = source {
            map.insert("source_id".into(), s.id.into());
            if let Some(t) = s.title {
                map.insert("source_title".into(), t.into());
            }
        }
        let payload: Payload = serde_json::Value::Object(map).try_into().map_err(storage)?;
        let point = PointStruct::new(point_id(id), vector, payload);
        self.client
            .upsert_points(UpsertPointsBuilder::new(&self.collection, vec![point]).wait(true))
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn query(&self, vector: &[f32], k: usize) -> Result<Vec<ScoredDoc>> {
        let res = self
            .client
            .query(
                QueryPointsBuilder::new(&self.collection)
                    .query(vector.to_vec())
                    .limit(k as u64)
                    .with_payload(true),
            )
            .await
            .map_err(storage)?;

        Ok(res
            .result
            .into_iter()
            .map(|p| {
                let payload = p.payload;
                let source = payload_str(&payload, "source_id").map(|id| SourceRef {
                    id,
                    uri: None,
                    title: payload_str(&payload, "source_title"),
                });
                ScoredDoc {
                    id: payload_str(&payload, "doc_id").unwrap_or_default(),
                    score: p.score,
                    text: payload_str(&payload, "text").unwrap_or_default(),
                    source,
                }
            })
            .collect())
    }

    async fn len(&self) -> Result<usize> {
        let res = self
            .client
            .count(CountPointsBuilder::new(&self.collection))
            .await
            .map_err(storage)?;
        Ok(res.result.map(|r| r.count as usize).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HashEmbedder;
    use oncora_core::EmbeddingProvider;
    use testcontainers::GenericImage;
    use testcontainers::core::{IntoContainerPort, WaitFor};
    use testcontainers::runners::AsyncRunner;

    /// Live round-trip against a real qdrant in Docker (testcontainers).
    /// Runs only with `--features qdrant` AND a reachable Docker daemon.
    #[tokio::test]
    async fn qdrant_roundtrip() {
        let container = GenericImage::new("qdrant/qdrant", "v1.12.4")
            .with_exposed_port(6334.tcp())
            .with_wait_for(WaitFor::message_on_stdout("gRPC listening"))
            .start()
            .await
            .expect("start qdrant container (needs Docker)");
        let port = container.get_host_port_ipv4(6334.tcp()).await.unwrap();
        let url = format!("http://127.0.0.1:{port}");

        let emb = HashEmbedder::new(64);
        let store = QdrantVectorStore::connect(&url, "oncora_test", emb.dims() as u64)
            .await
            .unwrap();

        for (i, text) in ["EGFR drives NSCLC growth", "BRAF V600E in melanoma"]
            .iter()
            .enumerate()
        {
            let v = emb.embed(&[text.to_string()]).await.unwrap();
            store
                .upsert(&format!("d{i}"), v[0].clone(), text.to_string(), None)
                .await
                .unwrap();
        }

        let q = emb.embed(&["EGFR NSCLC".to_string()]).await.unwrap();
        let hits = store.query(&q[0], 2).await.unwrap();
        assert_eq!(hits[0].id, "d0", "nearest neighbour should be the EGFR doc");
        assert!(store.len().await.unwrap() >= 2);
    }
}
