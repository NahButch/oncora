# Oncora — Real-World Validation Report

> Generated from a live run of the all-real `Platform` (Ollama embeddings + chat, qdrant, oxigraph, redb, rmcp) over real PubMed-sourced oncology literature. *Source: PubMed; per-record DOIs are preserved in `~/oncora-input-data/corpus.jsonl`.*

**Run timestamp:** 2026-06-04 02:28 UTC  

**Configuration:** embed `all-minilm` (dim 384) · chat `qwen2.5:0.5b` · qdrant collection `oncora_validation` · redb `/home/tom_b/oncora-input-data/memory.redb`

## Sample size

**58 documents** ingested this run (batch offset 0).

| Topic | Documents |
|---|---|
| astrocyte-inflammation | 20 |
| glial-inflammation | 18 |
| glioblastoma | 20 |
| **total corpus** | **58** |

## Ingestion performance (per-component)

Wall time **3.603 s** · throughput **16.1 docs/s**.

| Component | n | mean ms | p50 | p95 | min | max |
|---|---|---|---|---|---|---|
| Embedding (Ollama all-minilm) | 58 | 58.054 | 61.078 | 64.481 | 35.664 | 88.159 |
| Vector upsert (qdrant) | 58 | 2.496 | 2.426 | 2.916 | 1.758 | 6.029 |
| Graph assert (oxigraph) | 58 | 0.063 | 0.06 | 0.079 | 0.057 | 0.106 |
| Memory write (redb) | 58 | 1.503 | 1.495 | 2.079 | 0.936 | 2.338 |

## Query performance (end-to-end)

3 target-discovery queries · accept rate **100%** · mean calibrated confidence **0.935**.

| Stage | n | mean ms | p50 | p95 | min | max |
|---|---|---|---|---|---|---|
| Vector search (qdrant, real query embed) | 3 | 2.813 | 1.636 | 5.299 | 1.504 | 5.299 |
| End-to-end agent (retrieve→reason→verify→score) | 3 | 8154.384 | 8003.995 | 9723.0 | 6736.158 | 9723.0 |

> The end-to-end stage is dominated by LLM inference (3 self-consistency samples per query on CPU). Vector search is sub-millisecond-to-low-ms, showing the data plane is not the bottleneck — the model is.

## Component → real backend (what was exercised)

| Trait | Real backend in this run |
|---|---|
| `EmbeddingProvider` | Ollama `all-minilm` (384-dim) |
| `VectorStore` | qdrant (gRPC) |
| `GraphStore` | oxigraph (RDF quad store) |
| `MemoryStore` | redb (persistent, on disk) |
| `ToolHost` | rmcp (real MCP) |
| `ModelProvider` | Ollama `qwen2.5:0.5b` |

All numbers above are from genuine component calls — no stubs, no mocks. The agent runtime was unchanged; only the concrete backends behind the `oncora-core` traits differ from the in-memory reference.
