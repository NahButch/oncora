//! End-to-end: the target-discovery agent loop running with **real LLM
//! inference** (an OpenAI-compatible endpoint such as Ollama), swapped in behind
//! the `ModelProvider` trait — the deterministic stub replaced by an actual
//! model, with no other change to the runtime.
//!
//! Gated by `--features openai`; skipped unless `ONCORA_OPENAI_URL` is set.
//!
//! ```text
//! docker run -d -p 11434:11434 ollama/ollama
//! docker exec <id> ollama pull qwen2.5:0.5b
//! ONCORA_OPENAI_URL=http://127.0.0.1:11434/v1 ONCORA_OPENAI_MODEL=qwen2.5:0.5b \
//!   cargo test -p oncora-agents --features openai -- --nocapture
//! ```
#![cfg(feature = "openai")]

use std::sync::Arc;

use oncora_agents::model::OpenAiModel;
use oncora_agents::{Platform, run_target_discovery};
use oncora_core::{
    MemoryKey, ModelProvider, ProjectId, ScientistId, SourceRef, Verdict, WorkflowId,
};

#[tokio::test]
async fn openai_backed_target_discovery() {
    let Some(model) = OpenAiModel::from_env() else {
        eprintln!("ONCORA_OPENAI_URL not set — skipping LLM E2E");
        return;
    };

    // 1. Direct completion sanity check against the live model.
    let direct = model
        .complete("Reply with the single token: OK")
        .await
        .expect("model completion");
    eprintln!("direct completion: {direct:?}");
    assert!(!direct.is_empty(), "model returned an empty completion");

    // 2. Full agent loop with the real model substituted for the stub.
    let mut p = Platform::demo();
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

    // The swap: real inference now drives reasoning + self-consistency.
    p.model = Arc::new(model);

    let key = MemoryKey {
        scientist: ScientistId::new("e2e"),
        project: ProjectId::new("demo"),
        workflow: WorkflowId::new("llm"),
    };
    let answer = run_target_discovery(&p, &key, "Is EGFR a driver in NSCLC?", Some("EGFR"))
        .await
        .expect("agent run");

    eprintln!("--- LLM-backed E2E ---");
    eprintln!("answer    : {}", answer.text);
    eprintln!("confidence: {}", answer.confidence);
    eprintln!("verdict   : {}", answer.verdict.label());
    eprintln!("model     : {}", answer.provenance.model);
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
        "answer must be grounded in retrieved citations"
    );
    assert!(
        !matches!(answer.verdict, Verdict::Abstain { .. }),
        "with grounding evidence the agent should answer, got {:?}",
        answer.verdict
    );
}
