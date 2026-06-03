//! # oncora-retrieval
//!
//! Embeddings, vector search, and **hybrid retrieval** that fuses a vector arm
//! (qdrant/lancedb) with a graph arm ([`oncora_kg`]) using reciprocal-rank
//! fusion. See `docs/04-knowledge-and-data.md`.
//!
//! Reference backends here are deterministic and dependency-free so the walking
//! skeleton runs without a model server: [`HashEmbedder`] produces stable
//! vectors from token hashes, and [`InMemoryVectorStore`] does brute-force
//! cosine search. Production swaps in candle/fastembed + qdrant behind the same
//! [`EmbeddingProvider`] / [`VectorStore`] traits.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use oncora_core::{
    EmbeddingProvider, GraphStore, OncoraError, Result, ScoredDoc, SourceRef, VectorStore,
};

/// A deterministic, model-free embedder: hashes tokens into a fixed-width
/// bag-of-words vector. Good enough to exercise retrieval end-to-end and fully
/// reproducible; NOT semantically meaningful — replace with candle/fastembed.
pub struct HashEmbedder {
    dims: usize,
}

impl HashEmbedder {
    pub fn new(dims: usize) -> Self {
        Self { dims: dims.max(8) }
    }

    fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0f32; self.dims];
        for tok in text.split(|c: char| !c.is_alphanumeric()) {
            if tok.is_empty() {
                continue;
            }
            let h = fnv1a(&tok.to_ascii_lowercase());
            v[(h as usize) % self.dims] += 1.0;
        }
        l2_normalize(&mut v);
        v
    }
}

#[async_trait]
impl EmbeddingProvider for HashEmbedder {
    fn dims(&self) -> usize {
        self.dims
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }
}

struct Record {
    vector: Vec<f32>,
    text: String,
    source: Option<SourceRef>,
}

/// In-memory brute-force cosine [`VectorStore`].
#[derive(Default)]
pub struct InMemoryVectorStore {
    records: Mutex<HashMap<String, Record>>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn upsert(
        &self,
        id: &str,
        vector: Vec<f32>,
        text: String,
        source: Option<SourceRef>,
    ) -> Result<()> {
        self.records
            .lock()
            .map_err(|_| OncoraError::Storage("vector lock poisoned".into()))?
            .insert(
                id.to_string(),
                Record {
                    vector,
                    text,
                    source,
                },
            );
        Ok(())
    }

    async fn query(&self, vector: &[f32], k: usize) -> Result<Vec<ScoredDoc>> {
        let guard = self
            .records
            .lock()
            .map_err(|_| OncoraError::Storage("vector lock poisoned".into()))?;
        let mut scored: Vec<ScoredDoc> = guard
            .iter()
            .map(|(id, r)| ScoredDoc {
                id: id.clone(),
                score: cosine(vector, &r.vector),
                text: r.text.clone(),
                source: r.source.clone(),
            })
            .collect();
        scored.sort_by(|a, b| b.score.total_cmp(&a.score));
        scored.truncate(k);
        Ok(scored)
    }

    async fn len(&self) -> Result<usize> {
        Ok(self
            .records
            .lock()
            .map_err(|_| OncoraError::Storage("vector lock poisoned".into()))?
            .len())
    }
}

/// Fuses vector and graph retrieval into one ranked list. Holds references to
/// the two stores via the trait objects so either backend is swappable.
pub struct HybridRetriever<'a> {
    pub embedder: &'a dyn EmbeddingProvider,
    pub vectors: &'a dyn VectorStore,
    pub graph: &'a dyn GraphStore,
}

impl<'a> HybridRetriever<'a> {
    pub fn new(
        embedder: &'a dyn EmbeddingProvider,
        vectors: &'a dyn VectorStore,
        graph: &'a dyn GraphStore,
    ) -> Self {
        Self {
            embedder,
            vectors,
            graph,
        }
    }

    /// Retrieve the top-`k` documents for `query`, optionally biased toward an
    /// `entity` whose graph neighbourhood is fused in via reciprocal-rank
    /// fusion (RRF).
    pub async fn retrieve(
        &self,
        query: &str,
        entity: Option<&str>,
        k: usize,
    ) -> Result<Vec<ScoredDoc>> {
        let qv = self.embedder.embed(&[query.to_string()]).await?;
        let vec_hits = self.vectors.query(&qv[0], k * 2).await?;

        // RRF accumulation keyed by doc id.
        let mut fused: HashMap<String, (f32, ScoredDoc)> = HashMap::new();
        for (rank, doc) in vec_hits.into_iter().enumerate() {
            let rrf = 1.0 / (60.0 + rank as f32 + 1.0);
            fused.insert(doc.id.clone(), (rrf, doc));
        }

        // Graph arm: entities related to the focus entity become pseudo-docs
        // whose presence boosts matching vector hits and surfaces KG-only facts.
        if let Some(ent) = entity {
            for (rank, edge) in self.graph.related(ent).await?.into_iter().enumerate() {
                let rrf = 1.0 / (60.0 + rank as f32 + 1.0);
                let id = format!("kg:{}-{}-{}", edge.subject, edge.predicate, edge.object);
                let text = format!(
                    "{} {} {} (kg confidence {})",
                    edge.subject, edge.predicate, edge.object, edge.confidence
                );
                fused
                    .entry(id.clone())
                    .and_modify(|e| e.0 += rrf)
                    .or_insert_with(|| {
                        (
                            rrf,
                            ScoredDoc {
                                id,
                                score: 0.0,
                                text,
                                source: Some(SourceRef::new("knowledge-graph")),
                            },
                        )
                    });
            }
        }

        let mut out: Vec<ScoredDoc> = fused
            .into_values()
            .map(|(rrf, mut doc)| {
                doc.score = rrf;
                doc
            })
            .collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(k);
        Ok(out)
    }
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    a[..n].iter().zip(&b[..n]).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oncora_kg::InMemoryGraphStore;

    #[tokio::test]
    async fn retrieves_relevant_doc() {
        let emb = HashEmbedder::new(64);
        let vs = InMemoryVectorStore::new();
        let kg = InMemoryGraphStore::new();
        for (i, text) in ["EGFR drives NSCLC growth", "BRAF V600E in melanoma"]
            .iter()
            .enumerate()
        {
            let v = emb.embed(&[text.to_string()]).await.unwrap();
            vs.upsert(&format!("d{i}"), v[0].clone(), text.to_string(), None)
                .await
                .unwrap();
        }
        let r = HybridRetriever::new(&emb, &vs, &kg);
        let hits = r.retrieve("EGFR NSCLC", Some("EGFR"), 2).await.unwrap();
        assert_eq!(hits[0].id, "d0");
    }
}
