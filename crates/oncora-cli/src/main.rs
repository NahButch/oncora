//! `oncora` — operator/developer CLI.
//!
//! Runs the Phase-0 walking skeleton end to end against the in-memory reference
//! backends: ingest a tiny corpus, then ask a target-discovery question and
//! print the cited, confidence-scored answer and its verdict. With no argument
//! it runs the built-in demo; pass a question to ask your own.

use oncora_agents::{Platform, run_target_discovery};
use oncora_core::{MemoryKey, ProjectId, ScientistId, WorkflowId};
use oncora_ingest::{Document, ingest};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    oncora_telemetry::init();

    let question = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Is EGFR a driver in NSCLC?".to_string());
    let entity = std::env::args().nth(2);

    let p = Platform::demo();
    let report = seed(&p).await?;
    println!(
        "ingested {} documents / {} chunks / {} graph edges into snapshot {}\n",
        report.documents, report.chunks, report.edges, p.snapshot
    );

    let key = MemoryKey {
        scientist: ScientistId::new("cli"),
        project: ProjectId::new("demo"),
        workflow: WorkflowId::new("session-1"),
    };

    let answer = run_target_discovery(&p, &key, &question, entity.as_deref()).await?;

    println!("Q: {}", answer.question);
    println!("A: {}", answer.text);
    println!("   confidence : {}", answer.confidence);
    println!("   verdict    : {}", answer.verdict.label());
    println!("   model      : {}", answer.provenance.model);
    println!("   snapshot   : {}", answer.provenance.snapshot);
    if !answer.tools_used.is_empty() {
        println!("   tools used : {}", answer.tools_used.join(", "));
    }
    if answer.citations.is_empty() {
        println!("   citations  : (none)");
    } else {
        println!("   citations  :");
        for c in &answer.citations {
            let title = c.title.clone().unwrap_or_default();
            println!("     - [{}] {}", c.id, title);
        }
    }

    // Show a deterministic oracle (clinical calculator) tool call directly.
    if let Ok(res) = p
        .tools
        .call_tool(
            "bsa_mosteller",
            serde_json::json!({ "height_cm": 170, "weight_kg": 70 }),
        )
        .await
    {
        println!(
            "\noracle (bsa_mosteller): {} (deterministic={})",
            res.output, res.deterministic
        );
    }

    Ok(())
}

async fn seed(p: &Platform) -> anyhow::Result<oncora_ingest::IngestReport> {
    let docs = vec![
        Document::new(
            "PMID:0001",
            "EGFR signalling in NSCLC",
            "EGFR activating mutations drive non-small-cell lung cancer. EGFR \
             tyrosine kinase inhibitors produce durable responses in mutant tumours.",
        )
        .about("EGFR"),
        Document::new(
            "PMID:0002",
            "BRAF V600E in melanoma",
            "The BRAF V600E mutation activates MAPK signalling in melanoma and \
             confers sensitivity to BRAF inhibitors.",
        )
        .about("BRAF"),
    ];
    Ok(ingest(&docs, &*p.embedder, &*p.vectors, &*p.graph, &p.artifacts).await?)
}
