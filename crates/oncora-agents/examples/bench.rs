//! Real-world batch-ingest + performance harness.
//!
//! Assembles an all-real [`Platform`] (Ollama embeddings + chat, qdrant,
//! oxigraph, rmcp, redb), ingests a batch of documents from a JSONL corpus
//! while timing every component, runs a fixed set of target-discovery queries,
//! and writes a stats report (JSON, plus an appended results line for the
//! long-running validation runner).
//!
//! Built only with `--features e2e`. Configuration is via environment:
//!   ONCORA_OPENAI_URL, ONCORA_QDRANT_URL            (required)
//!   ONCORA_OPENAI_MODEL  (chat,  default qwen2.5:0.5b)
//!   ONCORA_EMBED_MODEL   (embed, default all-minilm)
//!   ONCORA_CORPUS        (default ~/oncora-input-data/corpus.jsonl)
//!   ONCORA_REDB          (default /tmp/oncora-mem.redb)
//!   ONCORA_COLLECTION    (default oncora_bench)
//!   ONCORA_BATCH_OFFSET / ONCORA_BATCH_SIZE   (slice of the corpus to ingest)
//!   ONCORA_STATS_OUT     (default ./bench-stats.json)
//!   ONCORA_RESULTS_JSONL (optional: append one compact stats line)

use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use oncora_agents::{OpenAiEmbedder, OpenAiModel, Platform, run_target_discovery};
use oncora_core::{
    Claim, Confidence, EmbeddingProvider, Evidence, MemoryEntry, MemoryKey, MemoryKind, ProjectId,
    Provenance, ScientistId, SourceRef, Triple, Verdict, WorkflowId,
};
use oncora_kg::OxigraphGraphStore;
use oncora_memory::RedbMemoryStore;
use oncora_mcp_host::RmcpToolHost;
use oncora_retrieval::QdrantVectorStore;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct Record {
    id: String,
    title: String,
    text: String,
    #[serde(default)]
    topic: String,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Summarise a vector of millisecond latencies.
fn metric(mut v: Vec<f64>) -> Value {
    if v.is_empty() {
        return json!({ "count": 0 });
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    let pct = |p: f64| v[((p * (n as f64 - 1.0)).round() as usize).min(n - 1)];
    let total: f64 = v.iter().sum();
    json!({
        "count": n,
        "mean_ms": (total / n as f64 * 1000.0).round() / 1000.0,
        "p50_ms": (pct(0.50) * 1000.0).round() / 1000.0,
        "p95_ms": (pct(0.95) * 1000.0).round() / 1000.0,
        "min_ms": (v[0] * 1000.0).round() / 1000.0,
        "max_ms": (v[n - 1] * 1000.0).round() / 1000.0,
        "total_ms": (total * 1000.0).round() / 1000.0,
    })
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    oncora_telemetry::init();

    let llm_url = std::env::var("ONCORA_OPENAI_URL").expect("ONCORA_OPENAI_URL");
    let qdrant_url = std::env::var("ONCORA_QDRANT_URL").expect("ONCORA_QDRANT_URL");
    let chat_model = env_or("ONCORA_OPENAI_MODEL", "qwen2.5:0.5b");
    let embed_model = env_or("ONCORA_EMBED_MODEL", "all-minilm");
    let corpus_path = env_or(
        "ONCORA_CORPUS",
        "/home/tom_b/oncora-input-data/corpus.jsonl",
    );
    let redb_path = env_or("ONCORA_REDB", "/tmp/oncora-mem.redb");
    let collection = env_or("ONCORA_COLLECTION", "oncora_bench");
    let offset: usize = env_or("ONCORA_BATCH_OFFSET", "0").parse().unwrap_or(0);
    let stats_out = env_or("ONCORA_STATS_OUT", "./bench-stats.json");

    // ---- load corpus (batch slice) ----
    let all: Vec<Record> = std::fs::read_to_string(&corpus_path)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l))
        .collect::<Result<_, _>>()?;
    let batch_size: usize = std::env::var("ONCORA_BATCH_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(all.len());
    let batch: Vec<&Record> = all.iter().skip(offset).take(batch_size).collect();
    eprintln!(
        "corpus {} records; ingesting batch [{}..{}) = {} docs",
        all.len(),
        offset,
        offset + batch.len(),
        batch.len()
    );

    // ---- assemble the all-real platform ----
    let embedder = OpenAiEmbedder::connect(&llm_url, "local", &embed_model).await?;
    let dim = embedder.dims() as u64;
    let qdrant = QdrantVectorStore::connect(&qdrant_url, &collection, dim).await?;
    let graph = OxigraphGraphStore::new_in_memory()?;
    let memory = RedbMemoryStore::open(&redb_path)?;
    let tools = RmcpToolHost::connect_loopback().await?;
    let model = OpenAiModel::new(&llm_url, "local", &chat_model);

    let mut p = Platform::demo();
    p.embedder = Arc::new(embedder);
    p.vectors = Arc::new(qdrant);
    p.graph = Arc::new(graph);
    p.memory = Arc::new(memory);
    p.tools = Arc::new(tools);
    p.model = Arc::new(model);

    let key = MemoryKey {
        scientist: ScientistId::new("bench"),
        project: ProjectId::new("validation"),
        workflow: WorkflowId::new("ingest"),
    };

    // ---- ingest with per-component timing ----
    let (mut t_embed, mut t_upsert, mut t_graph, mut t_mem) =
        (vec![], vec![], vec![], vec![]);
    let ingest_start = Instant::now();
    for r in &batch {
        let t = Instant::now();
        let v = p.embedder.embed(&[r.text.clone()]).await?;
        t_embed.push(ms(t));

        let t = Instant::now();
        p.vectors
            .upsert(
                &r.id,
                v[0].clone(),
                r.text.clone(),
                Some(SourceRef::new(r.id.clone()).with_title(r.title.clone())),
            )
            .await?;
        t_upsert.push(ms(t));

        let t = Instant::now();
        p.graph
            .assert(Triple::new(
                r.topic.clone(),
                "mentions",
                r.id.clone(),
                Confidence::new(0.5),
            ))
            .await?;
        t_graph.push(ms(t));

        let t = Instant::now();
        let prov = Provenance::new(p.model.model_pin(), p.snapshot.clone());
        let ev = Evidence::new(Claim::new(r.title.clone()), Confidence::new(0.6), prov);
        p.memory
            .write(MemoryEntry::new(key.clone(), MemoryKind::Episodic, ev))
            .await?;
        t_mem.push(ms(t));
    }
    let ingest_wall = ingest_start.elapsed().as_secs_f64();

    // ---- query phase with timing ----
    let queries = [
        "What molecular alterations drive glioblastoma?",
        "How does astrocyte inflammation contribute to disease?",
        "What is the role of glial inflammation in the brain?",
    ];
    let (mut q_search, mut q_e2e, mut confidences) = (vec![], vec![], vec![]);
    let mut accepts = 0usize;
    for q in queries {
        let qv = p.embedder.embed(&[q.to_string()]).await?;
        let t = Instant::now();
        let _ = p.vectors.query(&qv[0], 5).await?;
        q_search.push(ms(t));

        let t = Instant::now();
        let ans = run_target_discovery(&p, &key, q, None).await?;
        q_e2e.push(ms(t));
        confidences.push(ans.confidence.get());
        if matches!(ans.verdict, Verdict::Accept) {
            accepts += 1;
        }
    }

    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let mean_conf = confidences.iter().sum::<f64>() / confidences.len().max(1) as f64;

    let report = json!({
        "timestamp_ms": now_ms,
        "config": {
            "embed_model": embed_model, "chat_model": chat_model,
            "vector_dim": dim, "collection": collection, "redb": redb_path,
        },
        "sample_size": batch.len(),
        "batch_offset": offset,
        "ingest": {
            "wall_s": (ingest_wall * 1000.0).round() / 1000.0,
            "throughput_docs_per_s": ((batch.len() as f64 / ingest_wall) * 100.0).round() / 100.0,
            "embed_ms": metric(t_embed),
            "vector_upsert_ms": metric(t_upsert),
            "graph_assert_ms": metric(t_graph),
            "memory_write_ms": metric(t_mem),
        },
        "query": {
            "n_queries": queries.len(),
            "vector_search_ms": metric(q_search),
            "end_to_end_ms": metric(q_e2e),
            "accept_rate": (accepts as f64 / queries.len() as f64),
            "mean_confidence": (mean_conf * 1000.0).round() / 1000.0,
        },
    });

    std::fs::write(&stats_out, serde_json::to_string_pretty(&report)?)?;
    if let Ok(results) = std::env::var("ONCORA_RESULTS_JSONL") {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(results)?;
        writeln!(f, "{}", serde_json::to_string(&report)?)?;
    }

    eprintln!("--- bench summary ---");
    eprintln!("ingested {} docs in {:.2}s ({:.1} docs/s)", batch.len(), ingest_wall, batch.len() as f64 / ingest_wall);
    eprintln!("embed     mean {} ms", report["ingest"]["embed_ms"]["mean_ms"]);
    eprintln!("upsert    mean {} ms", report["ingest"]["vector_upsert_ms"]["mean_ms"]);
    eprintln!("graph     mean {} ms", report["ingest"]["graph_assert_ms"]["mean_ms"]);
    eprintln!("memory    mean {} ms", report["ingest"]["memory_write_ms"]["mean_ms"]);
    eprintln!("q search  mean {} ms", report["query"]["vector_search_ms"]["mean_ms"]);
    eprintln!("q e2e     mean {} ms (accept {:.0}%, conf {:.3})", report["query"]["end_to_end_ms"]["mean_ms"], accepts as f64 / queries.len() as f64 * 100.0, mean_conf);
    eprintln!("stats -> {stats_out}");
    Ok(())
}
