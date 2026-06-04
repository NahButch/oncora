//! # oncora-agents
//!
//! The agent runtime. This crate owns the **walking-skeleton** end-to-end slice
//! described in `docs/08-roadmap.md` Phase 0: a target-discovery query that
//! reads memory, retrieves (hybrid vector + graph), grounds against a
//! deterministic MCP tool, reasons, verifies, scores calibrated uncertainty,
//! and returns a confidence-scored, cited answer — writing the outcome back to
//! memory.
//!
//! The full multi-agent topology (planner -> domain specialists -> verifier ->
//! uncertainty scorer -> responder) layers on top of this loop; here the loop
//! is a single function so the whole architecture compiles and runs.

pub mod model;

#[cfg(feature = "openai")]
pub use model::{OpenAiEmbedder, OpenAiModel};

use std::sync::Arc;

use oncora_artifacts::InMemoryArtifactStore;
use oncora_core::{
    ArtifactStore, Calibrator, Claim, Confidence, EmbeddingProvider, Evidence, GraphStore,
    LedgerRecord, LedgerStore, MemoryEntry, MemoryKey, MemoryKind, MemoryStore, ModelProvider,
    Provenance, ReadQuery, Result, RunId, SnapshotId, SourceRef, ToolHost, VectorStore, Verdict,
    Verifier,
};
use oncora_kg::InMemoryGraphStore;
use oncora_ledger::InMemoryLedger;
use oncora_mcp_host::{BsaCalculator, EchoTool, McpHost};
use oncora_memory::InMemoryMemoryStore;
use oncora_retrieval::{HashEmbedder, HybridRetriever, InMemoryVectorStore};
use oncora_uncertainty::{
    AbstentionPolicy, GroundedVerifier, TemperatureCalibrator, self_consistency,
};
use serde::{Deserialize, Serialize};

use crate::model::TemplateModel;

/// A confidence-scored, cited answer — the unit returned to the scientist.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Answer {
    pub run_id: RunId,
    pub question: String,
    pub text: String,
    pub confidence: Confidence,
    pub verdict: Verdict,
    pub citations: Vec<SourceRef>,
    pub provenance: Provenance,
    /// Tools invoked during the run (names), for the audit summary.
    pub tools_used: Vec<String>,
}

/// The assembled platform: every provider behind its trait object, so any
/// backend is swappable. [`Platform::demo`] wires the in-memory reference
/// implementations for the walking skeleton.
#[derive(Clone)]
pub struct Platform {
    pub embedder: Arc<dyn EmbeddingProvider>,
    pub vectors: Arc<dyn VectorStore>,
    pub graph: Arc<dyn GraphStore>,
    pub memory: Arc<dyn MemoryStore>,
    pub tools: Arc<dyn ToolHost>,
    pub model: Arc<dyn ModelProvider>,
    pub calibrator: Arc<dyn Calibrator>,
    pub artifacts: Arc<InMemoryArtifactStore>,
    /// Provenance/audit ledger — every run appends an outcome record. The
    /// demo uses an in-memory ledger; the chosen real backend is pure-Rust
    /// SQLite (`turso`), swapped in behind this trait (see docs/05).
    pub ledger: Arc<dyn LedgerStore>,
    pub snapshot: SnapshotId,
    pub policy: AbstentionPolicy,
}

impl Platform {
    /// Wire the in-memory reference backends. The returned platform shares its
    /// stores via `Arc`, so the caller can ingest into the same vector/graph
    /// stores the agent later reads from.
    pub fn demo() -> Self {
        let vectors: Arc<InMemoryVectorStore> = Arc::new(InMemoryVectorStore::new());
        let graph: Arc<InMemoryGraphStore> = Arc::new(InMemoryGraphStore::new());
        let tools = McpHost::new()
            .with_tool(Box::new(BsaCalculator))
            .with_tool(Box::new(EchoTool));
        Self {
            embedder: Arc::new(HashEmbedder::new(128)),
            vectors,
            graph,
            memory: Arc::new(InMemoryMemoryStore::new()),
            tools: Arc::new(tools),
            model: Arc::new(TemplateModel::default()),
            calibrator: Arc::new(TemperatureCalibrator::new(1.5)),
            artifacts: Arc::new(InMemoryArtifactStore::new()),
            ledger: Arc::new(InMemoryLedger::new()),
            snapshot: SnapshotId::new("snapshot-dev-0001"),
            policy: AbstentionPolicy::default(),
        }
    }
}

/// Number of model samples used for the self-consistency estimate.
const SELF_CONSISTENCY_SAMPLES: usize = 3;

/// Run the target-discovery walking skeleton.
///
/// `focus_entity` anchors the graph arm of hybrid retrieval (e.g. a candidate
/// target symbol). Returns an [`Answer`] whose [`Verdict`] may be `Accept`,
/// `Abstain`, or `Escalate`.
#[tracing::instrument(skip(p), fields(run_id))]
pub async fn run_target_discovery(
    p: &Platform,
    key: &MemoryKey,
    question: &str,
    focus_entity: Option<&str>,
) -> Result<Answer> {
    let run_id = RunId::random();
    tracing::Span::current().record("run_id", tracing::field::display(&run_id));

    // 1. Perceive: read prior memory in scope (context-aware decisions).
    let _prior = p
        .memory
        .read(ReadQuery::new(key.clone()).with_text(question))
        .await?;

    // 2. Retrieve: hybrid vector + graph.
    let retriever = HybridRetriever::new(&*p.embedder, &*p.vectors, &*p.graph);
    let hits = retriever.retrieve(question, focus_entity, 5).await?;

    let mut provenance = Provenance::new(p.model.model_pin(), p.snapshot.clone());
    let mut tools_used = Vec::new();

    // No grounding evidence -> abstain rather than confabulate.
    if hits.is_empty() {
        let claim = Claim::new(format!("Answer to: {question}"));
        let evidence = Evidence::new(claim, Confidence::ZERO, provenance.clone());
        let answer = Answer {
            run_id,
            question: question.to_string(),
            text: "No grounding evidence was retrieved.".into(),
            confidence: Confidence::ZERO,
            verdict: Verdict::Abstain {
                reason: "empty retrieval set".into(),
            },
            citations: Vec::new(),
            provenance,
            tools_used,
        };
        persist(p, key, &answer, &evidence).await?;
        return Ok(answer);
    }

    // 3. Act: a deterministic oracle-grounding tool call (audited via the host).
    //    A real planner selects tools; here we record one grounding call as a
    //    placeholder for KG-consistency / clinical-calculator grounding.
    if let Ok(names) = p.tools.list_tools().await {
        if names.iter().any(|n| n == "echo") {
            let res = p
                .tools
                .call_tool(
                    "echo",
                    serde_json::json!({ "grounding_check": focus_entity.unwrap_or(question) }),
                )
                .await?;
            provenance = provenance.record_tool(res.call_id);
            tools_used.push(res.tool);
        }
    }

    // 4. Reason + self-consistency (calls the model N times).
    let context = hits
        .iter()
        .map(|h| h.text.as_str())
        .collect::<Vec<_>>()
        .join(" \n");
    let prompt = format!("CONTEXT:\n{context}\n\nQUESTION: {question}\nANSWER:");
    let mut samples = Vec::with_capacity(SELF_CONSISTENCY_SAMPLES);
    for _ in 0..SELF_CONSISTENCY_SAMPLES {
        samples.push(p.model.complete(&prompt).await?);
    }
    let (modal, agreement) = self_consistency(&samples).unwrap_or(("uncertain".into(), 0.0));

    // 5. Assemble evidence + provenance.
    let mut support = Vec::new();
    for h in &hits {
        support.push(h.text.clone());
        if let Some(src) = &h.source {
            provenance = provenance.cite(src.clone());
        }
    }
    let claim = Claim::new(format!("{question} -> {modal}"));
    let raw = (support.len().min(3) as f64) + 4.0 * (agreement - 0.5);
    let confidence = p.calibrator.calibrate(raw);
    let mut evidence = Evidence::new(claim.clone(), confidence, provenance.clone());
    evidence.support = support;

    // 6. Verify + score uncertainty -> verdict.
    let verifier = GroundedVerifier::new(p.policy.clone());
    let verdict = verifier.verify(&claim, &evidence).await?;

    // The model's answer is the body; the grounding metric is a separate line
    // (so it reads naturally whether the model returns a token or full prose).
    let grounding = format!(
        "[grounding: {} sources · self-consistency {:.0}%]",
        evidence.support.len(),
        agreement * 100.0
    );
    let text = match &verdict {
        Verdict::Accept => {
            let body = modal.trim();
            if body.is_empty() {
                grounding.clone()
            } else {
                format!("{body}\n{grounding}")
            }
        }
        Verdict::Abstain { reason } => format!("Abstaining: {reason}."),
        Verdict::Escalate { reason, .. } => {
            format!("Escalating for review: {reason}. {grounding}")
        }
    };

    let answer = Answer {
        run_id,
        question: question.to_string(),
        text,
        confidence,
        verdict,
        citations: evidence.provenance.sources.clone(),
        provenance: evidence.provenance.clone(),
        tools_used,
    };

    // 7. Consolidate: write the outcome to episodic + semantic memory.
    persist(p, key, &answer, &evidence).await?;
    Ok(answer)
}

/// Write the run outcome into episodic and (if accepted) semantic memory.
async fn persist(
    p: &Platform,
    key: &MemoryKey,
    answer: &Answer,
    evidence: &Evidence,
) -> Result<()> {
    // CAS the rendered answer for reproducible replay.
    let bytes = serde_json::to_vec(answer)?;
    let hash = p.artifacts.put(bytes.as_slice()).await?;

    // Provenance/audit ledger: one append-only record per run (verdict + hash).
    p.ledger
        .append(LedgerRecord::new(
            answer.run_id.clone(),
            0,
            format!("run:{}", answer.verdict.label()),
            serde_json::to_string(answer)?,
            hash,
        ))
        .await?;

    p.memory
        .write(MemoryEntry::new(
            key.clone(),
            MemoryKind::Episodic,
            evidence.clone(),
        ))
        .await?;

    if answer.verdict.is_accept() {
        p.memory
            .write(MemoryEntry::new(
                key.clone(),
                MemoryKind::Semantic,
                evidence.clone(),
            ))
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oncora_core::{ProjectId, ScientistId, WorkflowId};

    fn key() -> MemoryKey {
        MemoryKey {
            scientist: ScientistId::new("s1"),
            project: ProjectId::new("p1"),
            workflow: WorkflowId::new("w1"),
        }
    }

    #[tokio::test]
    async fn abstains_with_no_evidence() {
        let p = Platform::demo();
        let a = run_target_discovery(&p, &key(), "What targets EGFR?", Some("EGFR"))
            .await
            .unwrap();
        assert!(matches!(a.verdict, Verdict::Abstain { .. }));
    }

    #[tokio::test]
    async fn answers_with_evidence() {
        let p = Platform::demo();
        // Seed the shared vector store directly (ingestion is covered elsewhere).
        let v = p
            .embedder
            .embed(&["EGFR drives NSCLC".to_string()])
            .await
            .unwrap();
        p.vectors
            .upsert(
                "d0",
                v[0].clone(),
                "EGFR drives NSCLC growth".into(),
                Some(SourceRef::new("PMID:1").with_title("EGFR in NSCLC")),
            )
            .await
            .unwrap();
        let a = run_target_discovery(&p, &key(), "Is EGFR a driver in NSCLC?", Some("EGFR"))
            .await
            .unwrap();
        assert!(!a.citations.is_empty());
        assert!(a.confidence.get() > 0.0);
        assert!(!a.tools_used.is_empty(), "an oracle tool call is audited");
    }
}
