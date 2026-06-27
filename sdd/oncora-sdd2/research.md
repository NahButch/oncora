# Oncora Technology Research & Decisions

> Technology survey behind the plan. Per capability: **Primary** ("build on now"), **Fallback** ("trait impl kept warm / documented escape hatch"), **Rejected** ("surveyed, declined"), with rationale + *verify-before-committing* note. Two governing principles (from [constitution.md](constitution.md)): **(1)** everything swappable sits behind a trait in `oncora-core`; **(2)** Rust-native end to end, non-Rust deps isolated and justified. *Maturity caveat:* much of the Rust AI/agent ecosystem is 0.x and moves weekly — **trust the trait boundary, not the crate version.**

Legend: ✅ Primary · 🔁 Fallback · ⛔ Rejected · ★ Chosen/decided · ◻ Optional/gated.

---

## 1. Agent / LLM orchestration

| Option | Verdict | Why / Risk to verify |
|---|---|---|
| `rig` (rig-core) | ✅ Primary | Rust-native provider-agnostic agent + tool abstractions mapping onto planner/specialist topology. **Verify:** tool-calling ergonomics (multi-tool, parallel), streaming maturity, OpenAI-compatible custom base URL. Wrap behind `oncora-agents` traits to contain 0.x breakage. |
| `swiftide` | ✅ Primary (adjacent) | Streaming ingestion/RAG indexing (a *different* job from the agent loop). We own the `oncora-ingest` contract. **Verify:** backpressure; transform-stage composition with `EmbeddingProvider`/`VectorStore`. |
| `kalosm` | ⛔ Rejected | Opinionated toward own model-loading/inference stack; competes with `candle`/`mistral.rs` + `ModelProvider` boundary. |
| `anchor-chain` | ⛔ Rejected | Statically-typed LLM DAGs attractive but small/early, thin ecosystem, uncertain maintenance. |
| `llm-chain` | ⛔ Rejected | Early "LangChain for Rust"; lower-level than rig's agent model; quiet trajectory. |

**Decision.** `rig` (agent/tool) + `swiftide` (ingestion), both wrapped behind Oncora traits. Explicit cheap fallback: in-house orchestrator driving the raw LLM clients of §2 directly through `ModelProvider`, exercised only if rig's tool-calling/streaming proves insufficient.

---

## 2. LLM clients (behind `ModelProvider`)

| Option | Verdict | Why / Risk |
|---|---|---|
| `async-openai` | ✅ Primary | OpenAI-compatible; covers on-prem vLLM/TGI via custom base URL; mature; chat/tools/streaming. **Verify:** custom base URL + on-prem auth against exact vLLM/TGI build; tool-call/streaming response shapes match (server-side drift is the real risk). |
| `async-anthropic` / Anthropic SDK | ✅ Primary (cloud opt-in) | First-class Anthropic for opt-in, egress-proxied path; identical `ModelProvider` contract. **Off by default**, proxy-only. **Verify:** SDK currency vs latest Messages API; PHI redaction in front. |
| direct `reqwest` client | 🔁 Fallback | Hand-rolled `ModelProvider` for endpoints neither client handles cleanly. Low risk, high effort. |

**Implemented + verified live.** `oncora-agents` ships `OpenAiModel` (`async-openai` 0.40) behind `ModelProvider` under `--features openai`, `temperature = 0` for reproducibility. Verified end-to-end against Ollama serving `qwen2.5:0.5b`: real output ("Yes, EGFR is indeed considered a driver gene in NSCLC …"), confidence 0.855, verdict accept, citations [PMID:0001, PMID:0002]. Note: async-openai 0.40 is heavily feature-gated — `chat-completion` (pulls `_api`) + a TLS feature required; chat types under `async_openai::types::chat`.

---

## 3. Local / edge inference

| Option | Verdict | Why / Risk |
|---|---|---|
| `candle` | ✅ Primary (tensors + small models) | Pure-Rust tensor framework (HF); embedders/calibrators/classifiers; CPU + CUDA. **Verify:** kernel coverage for our architectures; reproducible CUDA builds. |
| `mistral.rs` | ✅ Primary (local LLM) | Pure-Rust local LLM (quantized GGUF) for dev + air-gapped nodes. **Verify:** supported families/quantizations; throughput; OpenAI-compatible surface for `ModelProvider`. |
| `ort` (ONNX Runtime) | ✅ Primary (ONNX only, isolated) | Mature prebuilt-ONNX path (what `fastembed` uses). **C++ bindings — the one tolerated non-Rust runtime**, isolated behind `EmbeddingProvider`/`Calibrator`; ONNX files pinned + content-addressed. **Verify:** pinned ORT version; reproducible builds; no graph reaches past the trait. |
| `burn` | ⛔ Rejected | Aimed at training/research; heavier than inference-only need; duplicates candle. Re-survey only if we train in-house. |

---

## 4. MCP — Model Context Protocol (behind `ToolHost`)

| Option | Verdict | Why / Risk |
|---|---|---|
| `rmcp` (official Rust MCP SDK) | ✅ Primary | Official SDK; covers serving tools **and** acting as client/host; tokio-native. **Verify:** transports we deploy (stdio + streamable HTTP/SSE); client host capabilities (discovery, calling, cancellation); pin protocol version + crate. |
| in-house MCP impl | 🔁 Fallback | If `rmcp` lags a transport/capability, we own the surface behind `ToolHost`. High effort; capability-specific. |

Audit, determinism, and timeout enforcement (`tower`) live in **our** code, not the SDK's. Calls recorded to episodic memory + provenance ledger.

---

## 5. Vector search & embeddings

| Option | Verdict | Why / Risk |
|---|---|---|
| `qdrant` (server + Rust client) | ✅ Primary (text/RAG) | Rust-native server; payload filtering; HNSW; scalar/product quantization; named vectors; clusters. **Verify:** client tracks deployed server; filtering + hybrid search cover needs. |
| `lancedb` | ✅ Primary (multimodal/imaging) | Rust-native columnar; Lance format built for multimodal embeddings + metadata with versioning. **Verify:** ANN index types/recall at scale; versioning semantics for reproducibility. |
| `hnsw_rs` / `instant-distance` | 🔁 Fallback (embedded/dev) | Pure-Rust in-process ANN for dev + unit tests; no server. Never the production text path. |
| `fastembed` | ✅ Primary (embeddings) | Fast batched on-prem text embeddings via `ort`. **Pin + content-address** ONNX files; inherits the isolated `ort` surface. |
| `candle`-hosted embedding models | 🔁 Fallback (embeddings) | Pure-Rust path to avoid the ONNX surface or for an architecture `fastembed` lacks. **Verify** the specific model exists in candle. |

**Implemented.** `oncora-retrieval` ships `QdrantVectorStore` (official `qdrant-client` gRPC) behind `VectorStore` under `--features qdrant`, interchangeable with `InMemoryVectorStore`; live round-trip via `testcontainers` Docker in a dedicated `qdrant-it` CI job (not part of default `cargo test`). [Embeddings: all-minilm, 384-d; qdrant v1.12.4.]

---

## 6. Biomolecular knowledge graph — the dual-store decision (behind `GraphStore`)

The KG has two fundamentally different jobs that no single store does well:
- **Ontology layer** — canonical, standards-grounded entities (GO/Reactome/ChEMBL/UMLS/MONDO/HGNC) as RDF, URI-addressed, queried with **SPARQL**; reference material, largely read-only, snapshot-versioned.
- **Evidence/assertion graph** — Oncora's own output: claims, evidence, support/contradiction edges, per-edge confidence + provenance, accumulated across runs; needs **time-travel**, **Datalog** recursion, co-located vector search.

The dual store is the *simpler* design once you accept the jobs differ; both sit behind one `GraphStore` trait so `oncora-kg` exposes one query surface.

| Option | Verdict | Why / Risk |
|---|---|---|
| `oxigraph` | ✅ Primary (ontology layer) | Mature pure-Rust RDF triplestore with SPARQL; natural for URI-addressed ontology import; no JVM. **Verify:** bulk-load perf for full snapshots; SPARQL feature coverage (property paths, aggregates). |
| `cozo` (CozoDB) | ✅ Primary (evidence graph) | Hybrid graph + relational + vector with **Datalog + time-travel** — uniquely matches versioned evidence + per-edge confidence + semantic memory; time-travel is the differentiator. **Verify:** time-travel semantics + recursive-Datalog perf at our volumes; embedded-vector recall. Keep `GraphStore` strict. |
| `indradb` | 🔁 Fallback (evidence graph) | Pure-Rust property graph; escape hatch if cozo's young feature set bites. No native time-travel/Datalog — we'd re-implement versioning. |
| Apache Jena | ⛔ Rejected | Mature, standards-complete — but **JVM**, violating Rust-native (rejected on the non-negotiable, not capability). |

**Implemented (oxigraph) + cozo finding.** `oncora-kg` ships `OxigraphGraphStore` behind `GraphStore` under `--features oxigraph` (pure-Rust in-memory RDF quad store; `default-features = false` keeps RocksDB out); passes the same conformance test as `InMemoryGraphStore`, with per-edge **confidence** in the quad's named-graph component (`urn:oncora:conf:<v>`). **cozo is deferred for build-integration, not capability.** Two original blockers: **(1)** cozo's `minimal` feature pulls native SQLite (`storage-sqlite` → `sqlite3-src`), which `links`-clashed with the C-SQLite ledger (`rusqlite`); **(2)** the only SQLite-free in-memory path (`graph-algo`) pulls `graph_builder 0.4.1`, which doesn't compile against current `rayon`. **Choosing turso for the ledger removes (1)** — with C SQLite gated to a benchmark-only feature, nothing else links native `sqlite3`. **(2) remains** (cozo still needs `rayon` via `graph-algo` → `graph_builder 0.4.1`); re-evaluate when pinned/patched/fixed upstream, or run cozo in its own process behind `GraphStore`. The seam means the swap costs nothing downstream.

---

## 7. Omics & tabular analytics

| Option | Verdict | Why / Risk |
|---|---|---|
| `polars` | ✅ Primary (in-process dataframes) | Arrow-backed lazy/eager engine; pure Rust. **Verify:** memory behavior on the largest omics matrices. |
| `duckdb` | ✅ Primary (SQL over Parquet) | Embedded analytical SQL over Parquet snapshots; **embedded C++ via bindings**, self-contained behind our query interface. **Verify:** binding-vs-engine version; reproducible bundled build. |
| `arrow` | ✅ Primary (interchange) | Zero-copy columnar lingua franca tying polars ↔ duckdb ↔ Parquet ↔ CAS. Keep Arrow versions aligned workspace-wide. |
| `noodles` | ✅ Primary (genomics IO) | Pure-Rust VCF/BCF/BAM/CRAM/FASTA/GFF/tabix; the VCF MCP server is built on it. **Verify:** format/codec coverage for our cohorts; large BAM/CRAM perf. |
| `rust-htslib` | ⛔ Rejected | htslib bindings are the established path — but **C bindings**; `noodles` covers our formats in pure Rust. Reconsider only for a niche missing format, behind a tool boundary. |

Genomics IO lives behind the VCF MCP server, so even `noodles` is reached through `ToolHost`.

---

## 8. Imaging

| Option | Verdict | Why / Risk |
|---|---|---|
| `dicom-rs` | ✅ Primary | Pure-Rust DICOM parse/encode/IO; behind the imaging MCP server (blast radius contained). **Verify:** transfer-syntax/codec coverage (e.g. compressed pixel data); pixel decoding supported or pluggable. |

No fallback named — no comparable pure-Rust alternative; a missing codec is added behind the same MCP tool boundary rather than reaching for a C library in the core.

---

## 9. Agent state / memory store (behind `MemoryStore` / `LedgerStore`)

| Option | Verdict | Why / Risk |
|---|---|---|
| `redb` | ✅ Primary (embedded KV/log) | Pure-Rust embedded ACID KV (stable 2.x); run logs + working-memory spill, no external service. **Verify:** episodic-append throughput; on-disk format stability across pinned 2.x. |
| `fjall` | 🔁 Fallback (embedded KV/log) | Pure-Rust LSM-tree; better write-amplification if episodic volume outgrows redb. Held as a warm `MemoryStore` impl. |
| `sled` | ⛔ Rejected | Was the default pure-Rust embedded KV, but development in flux with a long-promised rewrite; won't anchor durable memory on it. |
| `sqlx` + Postgres | ✅ Primary (relational metadata + provenance ledger) | Async, compile-time-checked SQL; Postgres for the multi-node provenance/audit ledger. **Verify:** migration discipline; compile-time query checking wired into CI. |
| SQLite — **pure Rust** (`turso`, ex-`limbo`) | ★ CHOSEN (dev relational / ledger) | Rust-native embedded ledger — no C in the build. Validated at scale: 14,469 sequential per-document writes, **0 errors, mean 2.04 ms — ~23% faster than the C amalgamation** (see [quickstart.md](quickstart.md) / §10-cold-compare). Default for `oncora-ledger` + bench. **Verify** transactions/concurrency/SQL coverage before depending past the dev ledger; Postgres remains the HA path. |
| SQLite — **C library** (`rusqlite`, `bundled`) | ◻ Benchmark-only (gated) | Retained behind `--features sqlite-c` solely to reproduce the C-vs-Rust comparison; pulls native `sqlite3`. Opt-in so the default build stays pure-Rust. |
| **C ↔ Rust SQLite** | ★ Decided: Rust (turso) | Both backends live in `crates/oncora-ledger` behind one `LedgerStore` trait; an identical conformance test passes for in-memory, C SQLite, and turso. Cold-start bulk comparison settled it for turso. |
| `foundationdb` | ◻ Optional (scale-out state) | Ordered transactional KV for very large multi-node memory. **Ops-heavy + phase-gated**; behind `MemoryStore`; enabled only at scale that justifies the ops cost. |
| `pgvector` | ◻ Optional (not primary) | Vectors co-located in Postgres if ever wanted next to relational metadata. Never the primary vector path (that's qdrant/lancedb). |

---

## 10. Service & transport (mature, low-risk floor)

| Option | Verdict | Why |
|---|---|---|
| `tokio` | ✅ Primary (async runtime) | De-facto runtime; every crate assumes it. Pin a major version workspace-wide. |
| `axum` | ✅ Primary (HTTP API) | Tower-native, tokio-first; the `oncora-api` external surface. (`actix-web` named in canon, not needed.) |
| `tonic` | ✅ Primary (gRPC) | Standard Rust gRPC for the internal mesh. **Verify:** protobuf/codegen pinned + reproducible in CI. |
| `tower` + `tower-http` | ✅ Primary (middleware) | Uniform timeout / retry / concurrency-limit / load-shed; where per-tool timeouts + backpressure are enforced. |

---

## 11. Observability

| Option | Verdict | Why / Risk |
|---|---|---|
| `tracing` | ✅ Primary (instrumentation) | Span-based; agent steps/tool-calls/verdicts become spans. **Verify:** span hygiene so **PHI never lands in span fields**. |
| `tracing-opentelemetry` | ✅ Primary (bridge) | Bridges spans to OTel. **Verify:** version compatibility of `tracing` ↔ bridge ↔ OTel SDK (this trio drifts — common breakage). |
| OpenTelemetry + collector | ✅ Primary (export) | Vendor-neutral export to an **on-prem** collector inside the boundary. **Verify:** exporter config keeps telemetry in the VPC. |

Standing risk: pin all three together. Lives in `oncora-telemetry`.

---

## 12. Reliability / testing

| Option | Verdict | Why |
|---|---|---|
| `proptest` | ✅ Primary (property testing) | Invariants on memory write/read, uncertainty/confidence math, parsers; shrinking finds minimal counterexamples. (`quickcheck` named, not needed.) |
| `cargo-fuzz` (libFuzzer) | ✅ Primary (fuzzing) | Fuzz untrusted/parser surfaces — VCF, DICOM, JSON tool args. Wire into CI as scheduled, not per-PR. |
| `insta` | ✅ Primary (snapshot testing) | Lock prompts, traces, manifests; catch prompt/format drift in review (review deliberately). |
| `criterion` | ✅ Primary (benchmarks) | Latency/throughput microbenchmarks feeding CI gates. (`divan` named.) |
| Differential testing | ✅ Primary (technique) | Cross-check agent outputs vs deterministic oracles + computational baselines. Harness discipline in `oncora-eval`; pin/version golden sets + baselines. |

---

## 13. Reproducibility / artifacts (behind `ArtifactStore`)

| Option | Verdict | Why / Risk |
|---|---|---|
| `blake3` (CAS hashing) | ✅ Primary | Fast cryptographic hashing for content-addressing; basis of replay. **Verify:** canonical stable byte representation before hashing. |
| custom CAS over object store / FS | ✅ Primary | We own it behind `ArtifactStore` — BLAKE3-keyed, immutable, dedup-by-content. **Verify:** GC/retention never deletes anything a provenance record references. |
| `serde` (+ `serde_json`, `ciborium` CBOR) manifests | ✅ Primary | Typed versioned manifests; JSON for human-auditable surfaces, CBOR for compact payloads. **Verify:** manifest schemas versioned so old artifacts stay readable. |
| deterministic seeds | ✅ Primary (technique) | Every stochastic step records its seed. **Verify:** all randomness routes through a recorded seeded source — no unseeded `rand` in the agent path. |
| model / version pinning | ✅ Primary (technique) | `ModelPin` + `SnapshotId` on every `Provenance`. **Verify:** the model server reports a stable recordable identity (build + weights digest, not a friendly name). |

---

## Consolidated risk register

Likelihood/Impact = Low/Medium/High. Owner-area maps to the crate owning the mitigation.

| Risk | L | I | Mitigation | Owner |
|---|---|---|---|---|
| `rig` 0.x breaking / insufficient tool-calling/streaming | High | Med | Wrap behind `oncora-agents` traits; in-house orchestrator fallback; verify first | `oncora-agents` |
| `swiftide` API churn breaks ingestion stages | Med | Low | We own the `oncora-ingest` contract; swiftide implements stages | `oncora-ingest` |
| vLLM/TGI wire-format drift vs `async-openai` | Med | High | Pin server build; contract-test deserialization; `reqwest` fallback | `oncora-agents` |
| `cozo` time-travel/Datalog immature at scale | Med | High | Strict `GraphStore`; `indradb` fallback; verify perf early | `oncora-kg` |
| `ort`/ONNX C++ surface — build/supply-chain | Med | Med | Isolate behind `EmbeddingProvider`/`Calibrator`; pin ORT + ONNX digests; `candle` fallback | `oncora-retrieval` |
| `rmcp` protocol/transport gaps | Med | Med | `ToolHost` owns audit/timeout/determinism; pin protocol; in-house fill-in | `oncora-mcp-host` |
| `mistral.rs` model/quantization support shifts | Med | Low | Dev/air-gapped only; `ModelProvider` boundary; vLLM is production path | `oncora-agents` |
| `lancedb` ANN recall/versioning insufficient | Low | Med | `VectorStore` trait; qdrant named-vectors fallback; verify recall at scale | `oncora-retrieval` |
| `noodles` missing a genomics format/codec | Low | Med | Behind VCF MCP server; add support there; htslib last resort behind boundary | `oncora-mcp-host` |
| `dicom-rs` missing a transfer syntax/codec | Low | Med | Behind imaging MCP server; add codec there, not a C dep in core | `oncora-mcp-host` |
| `redb` write throughput insufficient | Low | Med | `MemoryStore` trait; `fjall` LSM fallback; benchmark append | `oncora-memory` |
| `tracing`/`tracing-opentelemetry`/OTel SDK mismatch | Med | Low | Pin all three; smoke-test export in CI | `oncora-telemetry` |
| PHI leaks into spans/telemetry or to cloud | Low | High | Span hygiene; cloud off-by-default behind audited egress proxy; redaction in front | `oncora-telemetry`/`oncora-api` |
| Non-deterministic replay (unseeded RNG, unstable model id) | Med | High | All randomness via recorded seeds; model id = build + weights digest; replay tests in CI | `oncora-artifacts` |
| `arrow` version skew across polars/duckdb | Med | Low | Align Arrow versions workspace-wide; verify in CI | `oncora-ingest` |
| Workspace-wide ecosystem churn (many 0.x) | High | Med | Trait boundaries everywhere; verify-before-committing checklist; pin + review upgrades | all |

---

## Non-Rust dependencies and how they are isolated (P-1)

Exactly **three** sanctioned non-Rust surfaces, each behind a named Rust trait:

1. **Model endpoints (HTTP, possibly remote).** vLLM/TGI on-prem and any opt-in cloud LLM. Behind **`ModelProvider`**; clients (`async-openai`, `async-anthropic`, `reqwest`) are concrete impls. *Justification:* GPU serving is not a Rust workload; consumed as a network contract; cloud off by default, proxy-only.
2. **ONNX Runtime (`ort`, C++ bindings).** Prebuilt ONNX embedders/classifiers/calibrators; `fastembed` uses it. Behind **`EmbeddingProvider`** + **`Calibrator`**; ONNX files pinned + content-addressed. *Justification:* mature operator coverage + prebuilt-model ecosystem pure Rust doesn't yet match; `candle` is the pure-Rust fallback. `duckdb`'s embedded engine is a similar self-contained native dep behind the query interface.
3. **Third-party ontologies (data, not code).** GO/Reactome/ChEMBL/UMLS/MONDO/HGNC — external data in foreign vocabularies. Enter only via ingestion into `oxigraph`, URI-addressed, queried through **`GraphStore`**, snapshot-pinned. *Justification:* reference data we read, never source we write.

Everything else (graph/vector stores, genomics/DICOM IO, KV/log, artifact store) is pure Rust or an embedded engine behind a trait. `noodles` + `dicom-rs`, though pure Rust, additionally sit behind the `ToolHost` MCP boundary, so even a parsing bug cannot reach the agent core directly.

---

## Verify-before-committing checklist

Re-run at the start of each implementation phase ([tasks.md](tasks.md)). If verification fails for any Primary, the named Fallback is already a registered impl of the same trait, and switching is a configuration change, not a rewrite.

- [ ] **`rig`** — tool-calling (multi-tool + parallel) + streaming maturity; provider trait with custom OpenAI-compatible base URL.
- [ ] **`swiftide`** — transform-stage composition with our `EmbeddingProvider`/`VectorStore`; backpressure.
- [ ] **`async-openai` ↔ vLLM/TGI** — custom base URL + on-prem auth; tool-call/streaming shapes match the deployed build.
- [ ] **`async-anthropic`** — SDK currency vs latest Messages API; redaction in front; path off-by-default.
- [ ] **`candle`** — kernel/architecture coverage; reproducible CUDA builds on target images.
- [ ] **`mistral.rs`** — supported families/quantizations; throughput; OpenAI-compatible surface for `ModelProvider`.
- [ ] **`ort`** — pinned version; reproducible build; ONNX digests pinned; no leakage past the trait.
- [ ] **`rmcp`** — transports (stdio + streamable HTTP/SSE); client/host capability maturity; protocol version pinned.
- [ ] **`qdrant`** — Rust client tracks deployed server; filtering + hybrid search cover needs.
- [ ] **`lancedb`** — ANN index types/recall at scale; versioning semantics for reproducibility.
- [ ] **`fastembed`** — embedder families available; ONNX models pinned/content-addressed.
- [ ] **`oxigraph`** — bulk-load perf for full snapshots; SPARQL feature coverage (property paths, aggregates).
- [ ] **`cozo`** — time-travel semantics + recursive-Datalog perf at our volumes; embedded-vector recall. *(Highest-novelty; verify first.)*
- [ ] **`polars`/`duckdb`/`arrow`** — Arrow versions aligned; memory behavior on largest omics matrices.
- [ ] **`noodles`** — format/version + codec coverage for our cohorts; large BAM/CRAM performance.
- [ ] **`dicom-rs`** — transfer-syntax/codec coverage; pixel decoding supported or pluggable.
- [ ] **`redb`** — episodic-append write throughput; on-disk format stability across pinned 2.x.
- [ ] **`tracing`/`tracing-opentelemetry`/OTel SDK** — versions pinned together; export stays on-prem; no PHI in span fields.
- [ ] **`cargo-fuzz`** — a fuzz target exists for every external-input parser (VCF, DICOM, JSON tool args).
- [ ] **Reproducibility** — all randomness seeded + recorded; model identity = build + weights digest; replay test passes in CI.
