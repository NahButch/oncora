//! End-to-end: the full target-discovery agent loop running against a **real
//! qdrant** vector database, swapped in behind the `VectorStore` trait with no
//! changes to the agent runtime.
//!
//! Gated by `--features qdrant`, and skipped unless `ONCORA_QDRANT_URL` points
//! at a reachable qdrant gRPC endpoint (e.g. `http://127.0.0.1:6334`). Start one
//! with:
//!
//! ```text
//! docker run -d -p 6334:6334 qdrant/qdrant:v1.12.4
//! ONCORA_QDRANT_URL=http://127.0.0.1:6334 \
//!   cargo test -p oncora-agents --features qdrant -- --nocapture
//! ```
#![cfg(feature = "qdrant")]

use std::sync::Arc;

use oncora_agents::{Platform, run_target_discovery};
use oncora_core::{MemoryKey, ProjectId, ScientistId, SourceRef, VectorStore, Verdict, WorkflowId};
use oncora_retrieval::QdrantVectorStore;

#[tokio::test]
async fn qdrant_backed_target_discovery() {
    let Ok(url) = std::env::var("ONCORA_QDRANT_URL") else {
        eprintln!("ONCORA_QDRANT_URL not set — skipping qdrant E2E");
        return;
    };

    // Start from the in-memory demo platform, then swap ONLY the vector store
    // for a real qdrant collection. Everything else (memory, tools, model,
    // calibrator, verifier) is unchanged — the trait boundary is the seam.
    let mut p = Platform::demo();
    let dim = p.embedder.dims() as u64;
    let collection = format!("oncora_e2e_{}", std::process::id());
    let qdrant = QdrantVectorStore::connect(&url, &collection, dim)
        .await
        .expect("connect to qdrant");

    // Ingest a tiny literature corpus straight into qdrant via the platform's
    // embedder, with provenance attached to each chunk.
    let corpus = [
        (
            "PMID:0001",
            "EGFR signalling in NSCLC",
            "EGFR activating mutations drive non-small-cell lung cancer; EGFR \
             tyrosine kinase inhibitors produce durable responses in mutant tumours.",
        ),
        (
            "PMID:0002",
            "BRAF V600E in melanoma",
            "The BRAF V600E mutation activates MAPK signalling in melanoma and \
             confers sensitivity to BRAF inhibitors.",
        ),
    ];
    for (id, title, text) in corpus {
        let v = p.embedder.embed(&[text.to_string()]).await.unwrap();
        qdrant
            .upsert(
                id,
                v[0].clone(),
                text.to_string(),
                Some(SourceRef::new(id).with_title(title)),
            )
            .await
            .unwrap();
    }
    assert!(
        qdrant.len().await.unwrap() >= 2,
        "qdrant should hold the corpus"
    );

    // The swap: a real qdrant client now backs retrieval.
    p.vectors = Arc::new(qdrant);

    let key = MemoryKey {
        scientist: ScientistId::new("e2e"),
        project: ProjectId::new("demo"),
        workflow: WorkflowId::new("qdrant"),
    };
    let answer = run_target_discovery(&p, &key, "Is EGFR a driver in NSCLC?", Some("EGFR"))
        .await
        .expect("agent run");

    eprintln!("--- qdrant-backed E2E ---");
    eprintln!("answer    : {}", answer.text);
    eprintln!("confidence: {}", answer.confidence);
    eprintln!("verdict   : {}", answer.verdict.label());
    eprintln!(
        "citations : {:?}",
        answer
            .citations
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>()
    );

    assert!(
        matches!(answer.verdict, Verdict::Accept),
        "expected Accept, got {:?}",
        answer.verdict
    );
    assert!(
        !answer.citations.is_empty(),
        "answer must be grounded in qdrant-sourced citations"
    );
}
