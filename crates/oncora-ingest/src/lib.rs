//! # oncora-ingest
//!
//! Multimodal ingestion: turn literature/omics/imaging/KG sources into queryable
//! vectors and graph edges. Production builds these as streaming `swiftide`
//! pipelines (see `docs/04-knowledge-and-data.md`); here we provide a minimal,
//! synchronous reference pipeline that normalizes + chunks text, embeds it into
//! a [`VectorStore`], content-addresses the raw payload for reproducibility, and
//! asserts a claim edge into the [`GraphStore`].

use oncora_artifacts::InMemoryArtifactStore;
use oncora_core::{
    ArtifactStore, Confidence, EmbeddingProvider, GraphStore, Result, SourceRef, Triple,
    VectorStore,
};
use serde::{Deserialize, Serialize};

/// One source document to ingest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub text: String,
    /// Optional subject entity (e.g. a target symbol) to anchor graph edges.
    pub entity: Option<String>,
}

impl Document {
    pub fn new(id: impl Into<String>, title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            text: text.into(),
            entity: None,
        }
    }
    pub fn about(mut self, entity: impl Into<String>) -> Self {
        self.entity = Some(entity.into());
        self
    }
}

/// Naive sentence-ish chunker (stand-in for section-aware chunking).
pub fn chunk(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    for sentence in text.split_inclusive(['.', '!', '?']) {
        if cur.len() + sentence.len() > max_chars && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur).trim().to_string());
        }
        cur.push_str(sentence);
    }
    if !cur.trim().is_empty() {
        chunks.push(cur.trim().to_string());
    }
    if chunks.is_empty() {
        chunks.push(text.trim().to_string());
    }
    chunks
}

/// Summary of an ingestion run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IngestReport {
    pub documents: usize,
    pub chunks: usize,
    pub edges: usize,
}

/// Reference ingestion pipeline: chunk -> embed -> index, CAS the payload, and
/// assert a coarse "mentions" edge per document into the evidence graph.
pub async fn ingest(
    docs: &[Document],
    embedder: &dyn EmbeddingProvider,
    vectors: &dyn VectorStore,
    graph: &dyn GraphStore,
    artifacts: &InMemoryArtifactStore,
) -> Result<IngestReport> {
    let mut report = IngestReport {
        documents: 0,
        chunks: 0,
        edges: 0,
    };

    for doc in docs {
        // Content-address the raw payload so the snapshot is reproducible.
        let _hash = artifacts.put(doc.text.as_bytes()).await?;

        let chunks = chunk(&doc.text, 240);
        let embeddings = embedder.embed(&chunks).await?;
        for (i, (chunk_text, vector)) in chunks.iter().zip(embeddings).enumerate() {
            let source = SourceRef::new(doc.id.clone()).with_title(doc.title.clone());
            vectors
                .upsert(
                    &format!("{}#{i}", doc.id),
                    vector,
                    chunk_text.clone(),
                    Some(source),
                )
                .await?;
            report.chunks += 1;
        }

        if let Some(entity) = &doc.entity {
            graph
                .assert(Triple::new(
                    entity.clone(),
                    "mentioned_in",
                    doc.id.clone(),
                    Confidence::new(0.6),
                ))
                .await?;
            report.edges += 1;
        }
        report.documents += 1;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oncora_kg::InMemoryGraphStore;
    use oncora_retrieval::{HashEmbedder, InMemoryVectorStore};

    #[tokio::test]
    async fn ingests_documents() {
        let emb = HashEmbedder::new(64);
        let vs = InMemoryVectorStore::new();
        let kg = InMemoryGraphStore::new();
        let cas = InMemoryArtifactStore::new();
        let docs = vec![
            Document::new(
                "PMID:1",
                "EGFR in NSCLC",
                "EGFR mutations drive NSCLC. Inhibitors show response.",
            )
            .about("EGFR"),
        ];
        let report = ingest(&docs, &emb, &vs, &kg, &cas).await.unwrap();
        assert_eq!(report.documents, 1);
        assert!(report.chunks >= 1);
        assert_eq!(report.edges, 1);
        assert!(vs.len().await.unwrap() >= 1);
    }
}
