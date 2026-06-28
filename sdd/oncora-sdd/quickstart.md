# Oncora Quickstart & Validation

> How to build, run, and validate Oncora, plus the **current prototype status** and the
> real-world evidence behind it. The acceptance criteria are in [spec.md](spec.md) §10; this
> document maps them to runnable commands and recorded results.

---

## 1. Build & run the walking skeleton

```bash
cargo run --bin oncora                 # ingest a tiny corpus, ask a target-discovery
                                       # question, print a cited, confidence-scored answer
cargo run --bin oncora -- "Is BRAF actionable in melanoma?" BRAF
cargo run --bin oncora-api             # HTTP API on :8080 (GET /health, /tools; POST /query)
cargo test --workspace                 # unit tests across all crates
cargo xtask ci                         # fmt --check + clippy -D warnings + tests
```

## 2. Trait-swappability in action (each seam green against a real backend)

The same `Platform`/agent loop runs unchanged; only the concrete backend behind a core trait changes.

```bash
# LedgerStore — one trait, three interchangeable backends (incl. C vs pure-Rust SQLite)
cargo test -p oncora-ledger                                   # in-memory backend
cargo test -p oncora-ledger --features sqlite-c               # C SQLite (rusqlite, bundled)
cargo test -p oncora-ledger --features sqlite-rust            # pure-Rust SQLite (turso)
cargo test -p oncora-ledger --features "sqlite-c sqlite-rust" # all three, side by side

# ToolHost — real MCP server + client loopback
cargo test -p oncora-mcp-host --features rmcp

# GraphStore — real Oxigraph RDF quad store (per-edge confidence preserved)
cargo test -p oncora-kg --features oxigraph

# VectorStore — real qdrant cluster (starts qdrant in Docker via testcontainers; runs in CI)
cargo test -p oncora-retrieval --features qdrant

# Whole agent loop against real qdrant — same Platform, only VectorStore swapped
docker run -d -p 6334:6334 qdrant/qdrant:v1.12.4
ONCORA_QDRANT_URL=http://127.0.0.1:6334 \
  cargo test -p oncora-agents --features qdrant -- --nocapture
# answer: NSCLC is the best-supported answer (agreement 100%, 2 sources)
# confidence: 0.935 · verdict: accept · citations: [PMID:0001, PMID:0002]

# ModelProvider — real LLM inference via any OpenAI-compatible endpoint (temperature 0)
docker run -d -p 11434:11434 ollama/ollama
docker exec <id> ollama pull qwen2.5:0.5b
ONCORA_OPENAI_URL=http://127.0.0.1:11434/v1 ONCORA_OPENAI_MODEL=qwen2.5:0.5b \
  cargo test -p oncora-agents --features openai -- --nocapture
# answer: "Yes, EGFR is indeed considered a driver gene in NSCLC ..." (real model output)
# confidence: 0.855 · verdict: accept · model: openai/qwen2.5:0.5b@live · citations: [PMID:0001, PMID:0002]
```

### True end-to-end — every external seam real at once (`--features e2e`)

Real embeddings (Ollama) → qdrant (vectors) + oxigraph (graph) → rmcp (tools) → Ollama (LLM):

```bash
docker run -d -p 11434:11434 ollama/ollama
docker exec <id> ollama pull qwen2.5:0.5b && docker exec <id> ollama pull all-minilm
docker run -d -p 6334:6334 qdrant/qdrant:v1.12.4
ONCORA_OPENAI_URL=http://127.0.0.1:11434/v1 ONCORA_QDRANT_URL=http://127.0.0.1:6334 \
  cargo test -p oncora-agents --features e2e -- --nocapture
# real embedding dim: 384
# top vector hits: [(PMID:0001, 0.365), (PMID:0002, 0.225), (PMID:0003, 0.167)]  ← real semantic ranking
# answer: "Yes, EGFR is considered a driver gene in NSCLC ..." (real model)
# confidence: 0.920 · verdict: accept · tools: [echo (over MCP)] · citations: [knowledge-graph, PMID:0001, ...]
```

This is the architecture's thesis: the agent runtime never changed across any swap — only the concrete
backends behind the `oncora-core` traits did.

## 3. Browse the docs as a website (optional)

```bash
make setup    # one-time: create the build venv + markdown toolchain
make serve    # build the site and serve it at http://localhost:8137
```

Self-contained (Mermaid + highlight.js vendored), works offline once served over http.

---

## 4. Acceptance-criteria → validation mapping

| AC | Validated by |
|---|---|
| AC-1 end-to-end cited answer or abstention | `cargo run --bin oncora`; `--features openai/qdrant/e2e` runs above |
| AC-2 every tool call audited (allow+deny) before result | `oncora-mcp-host --features rmcp` loopback; MCP audit log |
| AC-3 bit-for-bit replay | `oncora-cli replay --manifest <blake3>` |
| AC-4 golden set gates CI | `cargo xtask ci`; `oncora-eval` gate (`eval.yml`) |
| AC-5 cross-modal fused answer | Phase-1 multimodal pipelines (in progress) |
| AC-6 cross-session memory reuse | `redb` persistent `MemoryStore` conformance; second-session reuse |
| AC-7 calibrated uncertainty | `oncora-uncertainty` ECE gate; conformal coverage (Phase-3 depth pending) |
| AC-8 beats baselines, replayable | `oncora-eval` three-way comparison + CI gate (needs real golden sets) |
| AC-9 scaled topology | Phase-5 deliverables (pending) |
| AC-10 governance / lineage / PHI-egress | Phase-6 deliverables (pending) |

---

## 5. Prototype status

**A well-tested, end-to-end functional prototype**, held here intentionally — it demonstrates the
architecture's thesis (every external dependency behind a swappable `oncora-core` trait) against *real*
backends, validated over real PubMed literature at scale. What remains is production hardening (Phases 5–6)
and depth on the research pillars — **not core correctness.** The agent runtime (`run_target_discovery`:
retrieve → reason → verify → score → accept/abstain/escalate → persist) was **never changed** as backends were
swapped from in-memory references to real implementations.

### 5.1 Seams verified against real backends

| `oncora-core` trait | Real backend | Verified |
|---|---|---|
| `ModelProvider` | Ollama (`qwen2.5:0.5b`) via `async-openai` | live inference; accept/abstain works |
| `EmbeddingProvider` | Ollama (`all-minilm`, 384-d) | semantic ranking confirmed |
| `VectorStore` | qdrant (gRPC) | round-trip + full agent E2E |
| `GraphStore` | oxigraph (RDF/SPARQL) | conformance + per-edge confidence |
| `MemoryStore` | redb (persistent, on disk) | conformance; O(1) write |
| `LedgerStore` | **pure-Rust SQLite (`turso`)** — chosen | 14,469 writes, 0 errors, ~23% faster than C SQLite |
| `ToolHost` | rmcp (real MCP server + client) | loopback round-trip |

### 5.2 Real-world validation (14,469 PubMed abstracts, all-real pipeline)

Corpus: glioblastoma / glioma / IDH-mutant / immunotherapy / temozolomide / astrocyte·microglia /
oligodendroglioma / diffuse-midline-glioma / low-grade-glioma / glioma-stem-cells / brain-tumor — **14,469
documents** across 15 throttled batches. Config: embed `all-minilm` (dim 384) · chat `qwen2.5:0.5b` · qdrant ·
redb (on disk). Source: PubMed; per-record DOIs preserved. No stubs or mocks.

**Ingestion (aggregate; total wall 681.938 s, overall 21.217 docs/s, flat as the index grew):**

| Component (real backend) | calls | mean ms | min | max |
|---|---|---|---|---|
| Embedding — Ollama all-minilm | 14,469 | 41.231 | 37.201 | 43.679 |
| Vector upsert — qdrant | 14,469 | 2.09 | 1.267 | 12.159 |
| Graph assert — oxigraph | 14,469 | 0.056 | 0.04 | 0.222 |
| Memory write — redb | 14,469 | 1.161 | 0.793 | 30.6 |

**Query (45 queries; accept rate 100%; mean calibrated confidence rose 0.935 → 0.966 as the corpus grew):**

| Stage (real backend) | calls | mean ms | min | max |
|---|---|---|---|---|
| Vector search — qdrant | 45 | 2.216 | 1.568 | 3.292 |
| End-to-end agent (retrieve→reason→verify→score) | 45 | 5,939.534 | 3,775.544 | 12,050.059 |

End-to-end latency is **LLM-bound** (3 self-consistency samples/query on CPU); it fell from 10,479 ms (cold) to
3,960 ms (warm). The data plane — embeddings, vector search, graph, memory — is single-digit-to-low-tens of
milliseconds throughout: the **model, not the infrastructure, is the cost**. The provenance ledger is written
**inside the agent loop** (a record per run).

### 5.3 Cold-start C-vs-Rust SQLite comparison (the head-to-head)

Two cold-start bulk runs over the full 14,469-doc corpus, identical except the `LedgerStore` SQLite backend:

| Backend | writes | mean ms | min | max | ingest wall | docs/s |
|---|---|---|---|---|---|---|
| C SQLite (`rusqlite`) | 14,469 | 2.663 | 2.076 | 8.919 | 1004.458 s | 14.405 |
| Rust SQLite (`turso`) | 14,469 | 2.041 | 1.751 | 10.035 | 990.443 s | 14.609 |

**Result.** At 14,469 sequential per-document writes, **pure-Rust SQLite (`turso`) had the lower mean latency
(1.30× / ~23% faster)**; both completed without error. turso is viable as the dev relational/ledger backend at
this scale, behind the same `LedgerStore` trait. **Decision: turso is the chosen embedded ledger.**

### 5.4 Bugs surfaced and fixed by real data/scale

O(N²) memory dedup → O(1) index; oxigraph IRI encoding for arbitrary tokens; qdrant fd-limit; report-generator
Unicode-line handling — exactly what a real bulk validation is for.

### 5.5 Remaining work (summary)

- **Quick wins:** inference/embedding request concurrency (batching left idle cores); memory `read` O(N)→indexed
  by scope (`write` is O(1)); qdrant client/server version bump (1.18 vs 1.12.4).
- **Research pillars:** cozo `GraphStore` (one of two blockers cleared by turso; `graph_builder`/`rayon` compile
  break remains — pin/patch or run cozo out-of-process); decide adopt-or-retire `rig`/`swiftide`; uncertainty
  depth (conformal + post-hoc calibration fitted from the eval harness — currently a temperature calibrator +
  grounded verifier); real golden datasets + baseline arms for `oncora-eval`; multi-agent topology as an actual
  runtime (currently a single end-to-end function).
- **Phase 5 / Phase 6:** see [tasks.md](tasks.md).
