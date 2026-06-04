# Oncora — Real-World Validation Report

> Generated from a live, long-running run of the all-real `Platform` (Ollama embeddings + chat, qdrant, oxigraph, redb, rmcp) over real oncology literature. **Source: PubMed**; per-record DOIs are preserved in `~/oncora-input-data/corpus.jsonl`. No stubs or mocks — every number is from a genuine component call.

**Run finished:** 2026-06-04 02:43 UTC  
**Batches:** 14  
**Configuration:** embed `all-minilm` (dim 384) · chat `qwen2.5:0.5b` · qdrant `oncora_longrun` · redb (on disk)

## Sample size

**135 documents** ingested across **14 throttled batches** into a cumulative qdrant collection + persistent redb memory.

| Topic | Documents in corpus |
|---|---|
| astrocyte-inflammation | 20 |
| glial-inflammation | 18 |
| glioblastoma | 20 |
| glioblastoma-immunotherapy | 19 |
| idh-mutant-glioma | 20 |
| microglia-neuroinflammation | 20 |
| reactive-astrocytes | 18 |
| **total corpus** | **135** |

## Ingestion performance — per component (aggregate over all batches)

Total ingest wall time **8.649 s** · overall throughput **15.609 docs/s** (held steady as the index grew).

| Component (real backend) | calls | mean ms | min | max |
|---|---|---|---|---|
| Embedding — Ollama all-minilm | 135 | 58.397 | 22.453 | 83.193 |
| Vector upsert — qdrant | 135 | 2.547 | 1.739 | 5.081 |
| Graph assert — oxigraph | 135 | 0.064 | 0.057 | 0.094 |
| Memory write — redb | 135 | 3.05 | 0.824 | 5.105 |

## Query performance — end-to-end (aggregate)

**42 queries** over the run · accept rate **100%** · mean calibrated confidence rose **0.935 → 0.966** as the corpus (and retrievable grounding) grew.

| Stage (real backend) | calls | mean ms | min | max |
|---|---|---|---|---|
| Vector search — qdrant | 42 | 1.518 | 1.265 | 1.966 |
| End-to-end agent — retrieve→reason→verify→score | 42 | 5312.77 | 3521.776 | 12388.14 |

> End-to-end latency is **LLM-bound** (3 self-consistency samples/query on CPU); it fell from **8656.72 ms** (cold) to **3744.76 ms** (warm). The data plane — embeddings, vector search, graph, memory — is single-digit-to-low-tens of milliseconds throughout, so the model, not the infrastructure, is the cost.

## Per-batch trend

| Batch @offset | docs | ingest docs/s | embed mean ms | q e2e mean ms | mean conf |
|---|---|---|---|---|---|
| 0 | 10 | 17.24 | 54.178 | 8656.72 | 0.935 |
| 10 | 10 | 15.95 | 58.379 | 9757.631 | 0.92 |
| 20 | 10 | 16.14 | 57.076 | 6896.514 | 0.935 |
| 30 | 10 | 15.32 | 60.295 | 7326.621 | 0.95 |
| 40 | 10 | 15.93 | 57.451 | 6177.467 | 0.95 |
| 50 | 10 | 15.11 | 60.468 | 5088.835 | 0.95 |
| 60 | 10 | 16.25 | 55.868 | 3672.094 | 0.966 |
| 70 | 10 | 15.15 | 60.09 | 3714.536 | 0.966 |
| 80 | 10 | 15.91 | 56.883 | 3716.839 | 0.966 |
| 90 | 10 | 15.41 | 58.692 | 3951.862 | 0.95 |
| 100 | 10 | 15.6 | 57.321 | 3651.898 | 0.966 |
| 110 | 10 | 14.22 | 63.591 | 4358.811 | 0.95 |
| 120 | 10 | 15.11 | 59.558 | 3664.198 | 0.966 |
| 130 | 5 | 15.6 | 57.033 | 3744.76 | 0.966 |

**Ingest throughput across batches:** mean 15.639 docs/s (min 14.22, max 17.24) — flat, i.e. ingestion scaled cleanly with corpus size.

## Component → real backend exercised

| `oncora-core` trait | Real backend |
|---|---|
| `EmbeddingProvider` | Ollama `all-minilm` (384-dim) |
| `VectorStore` | qdrant (gRPC) |
| `GraphStore` | oxigraph (RDF quad store) |
| `MemoryStore` | redb (persistent, on disk) |
| `ToolHost` | rmcp (real MCP) |
| `ModelProvider` | Ollama `qwen2.5:0.5b` |

The agent runtime was identical to the in-memory walking skeleton; only the concrete backends behind the traits changed. This run is the architecture's thesis under sustained real-world load.
