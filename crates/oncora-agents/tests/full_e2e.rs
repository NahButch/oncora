//! TRUE end-to-end: a `Platform` with **every external seam real at once** —
//! real embeddings + a real vector DB + a real graph store + a real MCP tool
//! host + a real LLM — running the full target-discovery loop.
//!
//! * `EmbeddingProvider` → `OpenAiEmbedder` (Ollama embeddings, e.g. all-minilm)
//! * `VectorStore`       → `QdrantVectorStore` (qdrant gRPC)
//! * `GraphStore`        → `OxigraphGraphStore` (RDF quad store)
//! * `ToolHost`          → `RmcpToolHost` (real MCP over rmcp)
//! * `ModelProvider`     → `OpenAiModel` (Ollama chat, e.g. qwen2.5:0.5b)
//!
//! Gated by `--features e2e`; skipped unless `ONCORA_OPENAI_URL` and
//! `ONCORA_QDRANT_URL` are set.
//!
//! ```text
//! docker run -d -p 11434:11434 ollama/ollama
//! docker exec <id> ollama pull qwen2.5:0.5b && docker exec <id> ollama pull all-minilm
//! docker run -d -p 6334:6334 qdrant/qdrant:v1.12.4
//! ONCORA_OPENAI_URL=http://127.0.0.1:11434/v1 ONCORA_QDRANT_URL=http://127.0.0.1:6334 \
//!   cargo test -p oncora-agents --features e2e -- --nocapture
//! ```
#![cfg(feature = "e2e")]

use std::sync::Arc;

use oncora_agents::{OpenAiEmbedder, OpenAiModel, Platform, run_target_discovery};
use oncora_core::{
    Confidence, EmbeddingProvider, MemoryKey, ProjectId, ScientistId, SourceRef, Triple, Verdict,
    WorkflowId,
};
use oncora_kg::OxigraphGraphStore;
use oncora_mcp_host::RmcpToolHost;
use oncora_retrieval::QdrantVectorStore;

#[tokio::test]
async fn fully_real_target_discovery() {
    let (Ok(llm_url), Ok(qdrant_url)) = (
        std::env::var("ONCORA_OPENAI_URL"),
        std::env::var("ONCORA_QDRANT_URL"),
    ) else {
        eprintln!("ONCORA_OPENAI_URL / ONCORA_QDRANT_URL not set — skipping full E2E");
        return;
    };
    let chat_model = std::env::var("ONCORA_OPENAI_MODEL").unwrap_or_else(|_| "qwen2.5:0.5b".into());

    // --- assemble an all-real platform ---
    let embedder = OpenAiEmbedder::from_env()
        .await
        .expect("ONCORA_OPENAI_URL present")
        .expect("connect embedder");
    let dim = embedder.dims() as u64;
    eprintln!("real embedding dimensionality: {dim}");

    let collection = format!("oncora_full_{}", std::process::id());
    let qdrant = QdrantVectorStore::connect(&qdrant_url, &collection, dim)
        .await
        .expect("connect qdrant");
    let graph = OxigraphGraphStore::new_in_memory().expect("oxigraph");
    let tools = RmcpToolHost::connect_loopback().await.expect("rmcp host");
    let model = OpenAiModel::new(&llm_url, "local", &chat_model);

    let mut p = Platform::demo();
    p.embedder = Arc::new(embedder);
    p.vectors = Arc::new(qdrant);
    p.graph = Arc::new(graph);
    p.tools = Arc::new(tools);
    p.model = Arc::new(model);

    // --- ingest: REAL embeddings -> qdrant, entity edge -> oxigraph ---
    let corpus = [
        (
            "PMID:0001",
            "EGFR signalling in NSCLC",
            "EGFR activating mutations drive non-small-cell lung cancer; EGFR \
             tyrosine kinase inhibitors produce durable responses in mutant tumours.",
        ),
        (
            "PMID:0002",
            "KRAS in colorectal cancer",
            "KRAS mutations are common oncogenic drivers in colorectal cancer.",
        ),
        (
            "PMID:0003",
            "BRAF V600E in melanoma",
            "The BRAF V600E mutation activates MAPK signalling in melanoma.",
        ),
    ];
    for (id, title, text) in corpus {
        let v = p.embedder.embed(&[text.to_string()]).await.unwrap();
        p.vectors
            .upsert(
                id,
                v[0].clone(),
                text.to_string(),
                Some(SourceRef::new(id).with_title(title)),
            )
            .await
            .unwrap();
    }
    p.graph
        .assert(Triple::new(
            "EGFR",
            "associated_with",
            "NSCLC",
            Confidence::new(0.92),
        ))
        .await
        .unwrap();

    // --- semantic retrieval check: real embeddings should rank the EGFR paper top ---
    let qv = p
        .embedder
        .embed(&["Is EGFR a driver in NSCLC?".to_string()])
        .await
        .unwrap();
    let hits = p.vectors.query(&qv[0], 3).await.unwrap();
    eprintln!(
        "top vector hits: {:?}",
        hits.iter().map(|h| (&h.id, h.score)).collect::<Vec<_>>()
    );
    assert_eq!(
        hits[0].id, "PMID:0001",
        "real semantic embeddings should rank the EGFR/NSCLC paper first"
    );

    // --- run the full agent loop over all-real backends ---
    let key = MemoryKey {
        scientist: ScientistId::new("e2e"),
        project: ProjectId::new("demo"),
        workflow: WorkflowId::new("full"),
    };
    let answer = run_target_discovery(&p, &key, "Is EGFR a driver in NSCLC?", Some("EGFR"))
        .await
        .expect("agent run");

    eprintln!("--- FULLY REAL E2E (embeddings + qdrant + oxigraph + rmcp + ollama) ---");
    eprintln!("answer    : {}", answer.text);
    eprintln!("confidence: {}", answer.confidence);
    eprintln!("verdict   : {}", answer.verdict.label());
    eprintln!("model     : {}", answer.provenance.model);
    eprintln!("tools     : {:?}", answer.tools_used);
    eprintln!(
        "citations : {:?}",
        answer
            .citations
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>()
    );

    assert!(
        !answer.citations.is_empty(),
        "answer must cite retrieved sources"
    );
    assert!(
        answer.citations.iter().any(|c| c.id == "PMID:0001"),
        "the EGFR/NSCLC paper should be among the citations"
    );
    assert!(
        !matches!(answer.verdict, Verdict::Abstain { .. }),
        "with grounding the agent should answer, got {:?}",
        answer.verdict
    );
}
