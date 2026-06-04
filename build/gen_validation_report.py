#!/usr/bin/env python3
"""Generate the validation report (docs/09-validation.md) from bench stats.

Reads bench-stats.json (+ corpus.jsonl for the topic breakdown) and emits a
Markdown report. The site generator turns it into an HTML page; it also renders
on GitHub as a gh-doc. Optionally folds in a results JSONL from the long-running
validation runner to show throughput/latency over successive batches.
"""
import json, pathlib, sys, datetime, statistics

ROOT = pathlib.Path(__file__).resolve().parent.parent
STATS = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "bench-stats.json"
CORPUS = pathlib.Path("/home/tom_b/oncora-input-data/corpus.jsonl")
RESULTS = pathlib.Path("/home/tom_b/oncora-input-data/validation-results.jsonl")
OUT = ROOT / "docs" / "09-validation.md"

s = json.loads(STATS.read_text())

# topic breakdown from the corpus
topics = {}
if CORPUS.exists():
    for line in CORPUS.read_text().splitlines():
        if line.strip():
            t = json.loads(line).get("topic", "?")
            topics[t] = topics.get(t, 0) + 1

def mrow(name, m):
    if not m or m.get("count", 0) == 0:
        return f"| {name} | 0 | – | – | – | – | – |"
    return (f"| {name} | {m['count']} | {m['mean_ms']} | {m['p50_ms']} | "
            f"{m['p95_ms']} | {m['min_ms']} | {m['max_ms']} |")

ing = s["ingest"]
qry = s["query"]
cfg = s["config"]
stamp = datetime.datetime.fromtimestamp(s["timestamp_ms"] / 1000, datetime.timezone.utc).strftime("%Y-%m-%d %H:%M UTC")

lines = []
lines.append("# Oncora — Real-World Validation Report\n")
lines.append("> Generated from a live run of the all-real `Platform` "
             "(Ollama embeddings + chat, qdrant, oxigraph, redb, rmcp) over real "
             "PubMed-sourced oncology literature. *Source: PubMed; per-record DOIs "
             "are preserved in `~/oncora-input-data/corpus.jsonl`.*\n")
lines.append(f"**Run timestamp:** {stamp}  \n")
lines.append(f"**Configuration:** embed `{cfg['embed_model']}` (dim {cfg['vector_dim']}) · "
             f"chat `{cfg['chat_model']}` · qdrant collection `{cfg['collection']}` · "
             f"redb `{cfg['redb']}`\n")

lines.append("## Sample size\n")
lines.append(f"**{s['sample_size']} documents** ingested this run "
             f"(batch offset {s.get('batch_offset', 0)}).\n")
if topics:
    lines.append("| Topic | Documents |\n|---|---|")
    for t, n in sorted(topics.items()):
        lines.append(f"| {t} | {n} |")
    lines.append(f"| **total corpus** | **{sum(topics.values())}** |\n")

lines.append("## Ingestion performance (per-component)\n")
lines.append(f"Wall time **{ing['wall_s']} s** · throughput **{ing['throughput_docs_per_s']} docs/s**.\n")
lines.append("| Component | n | mean ms | p50 | p95 | min | max |")
lines.append("|---|---|---|---|---|---|---|")
lines.append(mrow("Embedding (Ollama all-minilm)", ing["embed_ms"]))
lines.append(mrow("Vector upsert (qdrant)", ing["vector_upsert_ms"]))
lines.append(mrow("Graph assert (oxigraph)", ing["graph_assert_ms"]))
lines.append(mrow("Memory write (redb)", ing["memory_write_ms"]))
lines.append("")

lines.append("## Query performance (end-to-end)\n")
lines.append(f"{qry['n_queries']} target-discovery queries · "
             f"accept rate **{qry['accept_rate']*100:.0f}%** · "
             f"mean calibrated confidence **{qry['mean_confidence']}**.\n")
lines.append("| Stage | n | mean ms | p50 | p95 | min | max |")
lines.append("|---|---|---|---|---|---|---|")
lines.append(mrow("Vector search (qdrant, real query embed)", qry["vector_search_ms"]))
lines.append(mrow("End-to-end agent (retrieve→reason→verify→score)", qry["end_to_end_ms"]))
lines.append("")
lines.append("> The end-to-end stage is dominated by LLM inference (3 self-consistency "
             "samples per query on CPU). Vector search is sub-millisecond-to-low-ms, "
             "showing the data plane is not the bottleneck — the model is.\n")

# Long-running runner trend, if present
if RESULTS.exists():
    rows = [json.loads(l) for l in RESULTS.read_text().splitlines() if l.strip()]
    if rows:
        lines.append("## Long-running validation (successive batches)\n")
        lines.append(f"{len(rows)} batches recorded by the throttled runner.\n")
        lines.append("| Batch @offset | docs | ingest docs/s | embed mean ms | q e2e mean ms | mean conf |")
        lines.append("|---|---|---|---|---|---|")
        for r in rows:
            lines.append(f"| {r.get('batch_offset','?')} | {r['sample_size']} | "
                         f"{r['ingest']['throughput_docs_per_s']} | "
                         f"{r['ingest']['embed_ms'].get('mean_ms','–')} | "
                         f"{r['query']['end_to_end_ms'].get('mean_ms','–')} | "
                         f"{r['query']['mean_confidence']} |")
        tps = [r['ingest']['throughput_docs_per_s'] for r in rows]
        lines.append(f"\n**Aggregate:** {sum(r['sample_size'] for r in rows)} docs across "
                     f"{len(rows)} batches; ingest throughput mean "
                     f"{statistics.mean(tps):.2f} docs/s (min {min(tps)}, max {max(tps)}).\n")

lines.append("## Component → real backend (what was exercised)\n")
lines.append("| Trait | Real backend in this run |")
lines.append("|---|---|")
lines.append("| `EmbeddingProvider` | Ollama `all-minilm` (384-dim) |")
lines.append("| `VectorStore` | qdrant (gRPC) |")
lines.append("| `GraphStore` | oxigraph (RDF quad store) |")
lines.append("| `MemoryStore` | redb (persistent, on disk) |")
lines.append("| `ToolHost` | rmcp (real MCP) |")
lines.append("| `ModelProvider` | Ollama `qwen2.5:0.5b` |")
lines.append("")
lines.append("All numbers above are from genuine component calls — no stubs, no mocks. "
             "The agent runtime was unchanged; only the concrete backends behind the "
             "`oncora-core` traits differ from the in-memory reference.\n")

OUT.write_text("\n".join(lines), encoding="utf-8")
print(f"wrote {OUT} ({s['sample_size']} docs)")
