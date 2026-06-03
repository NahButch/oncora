# Oncora — Technology Decisions, Justifications, and Risk Register

This document is the technology survey behind Oncora. It **expands** the LOCKED technology table in the internal design canon — it never contradicts it. Where the canon names a primary and a fallback, this document explains *why the primary won, why the alternatives lost,* and *what must be verified before we commit code to it.* Read it alongside [01-architecture.md](01-architecture.md) for the runtime topology, [02-memory.md](02-memory.md) for how the stores are used, [04-knowledge-and-data.md](04-knowledge-and-data.md) for the KG and data layer, and [07-repo-layout.md](07-repo-layout.md) for the crate boundaries that isolate every choice below.

Two principles govern every row in every table:

1. **Everything swappable sits behind a trait in `oncora-core`.** `ModelProvider`, `EmbeddingProvider`, `VectorStore`, `GraphStore`, `MemoryStore`, `ToolHost`, `Calibrator`, `Verifier`, `ArtifactStore`. A crate is an implementation detail; the trait is the contract. This is what lets us pick a young, fast-moving crate as primary without betting the platform on it.
2. **Rust-native end to end; non-Rust dependencies are isolated and justified.** The only sanctioned non-Rust surfaces are (a) the model endpoint over HTTP, (b) `ort`/ONNX Runtime via C++ bindings, and (c) third-party ontology *data*. Each is named explicitly in the [Non-Rust dependencies](#non-rust-dependencies-and-how-they-are-isolated) section.

A verdict of **Primary** means "build on it now." **Fallback** means "we keep the trait impl warm, or it is the documented escape hatch." **Rejected** means "surveyed and consciously declined" — recorded so we do not relitigate it.

> **Maturity caveat up front.** Large parts of the Rust AI/agent ecosystem are 0.x and moving weekly. Every capability claim below carries a *verify* note, and the [verify-before-committing checklist](#verify-before-committing-checklist) collects them. Trust the trait boundary, not the crate version.

---

## 1. Agent / LLM orchestration

Oncora's agent runtime (Planner → Domain Specialists → Verifier → Uncertainty Scorer → Responder, per the canon) needs provider-agnostic agent abstractions, tool-calling, and streaming. We want a Rust-native framework so the agent loop, memory hooks, and uncertainty gating stay in one language and one type system. But this is the *least mature* corner of the ecosystem, so the orchestration crate is wrapped behind our own `oncora-agents` traits and the `ToolHost`/`ModelProvider` boundaries — we can rip the framework out without touching specialist logic.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `rig` (rig-core) | **Primary** | Rust-native, provider-agnostic agent + tool abstractions; clean `Agent`/`Tool`/`completion` model that maps onto our planner/specialist topology; active development | Young and fast-moving. **Verify**: current tool-calling ergonomics (multi-tool, parallel calls), streaming-completion maturity, and whether its provider trait covers OpenAI-compatible on-prem endpoints with custom base URLs. Wrap behind our own `oncora-agents` traits so a breaking 0.x bump is contained. |
| `swiftide` | **Primary (adjacent)** | Owns a *different* job: streaming ingestion/RAG indexing pipelines, not the agent loop. Rust-native, composable transform stages fit literature/omics/imaging/KG ingest | Young. We own the pipeline *contract* (`oncora-ingest`) regardless, so swiftide is an implementation of stages, not the API. **Verify**: backpressure behavior and whether node transforms compose cleanly with our `EmbeddingProvider`/`VectorStore` traits. |
| `kalosm` | Rejected | Capable local-first inference + agent toolkit, but opinionated toward its own model-loading/inference stack; overlaps and competes with our `candle`/`mistral.rs` + `ModelProvider` boundary rather than sitting cleanly above it | Would blur the inference/orchestration split we deliberately keep separate. Re-survey if rig stalls. |
| `anchor-chain` | Rejected | Statically-typed LLM workflow DAGs are conceptually attractive, but the project is small/early with a thin ecosystem and uncertain maintenance | Maintenance and feature risk too high for the core orchestration layer. |
| `llm-chain` | Rejected | Early "LangChain for Rust" framing; chain/prompt-template abstractions are lower-level than rig's agent model and the trajectory has been quiet | Less momentum and a weaker agent/tool story than rig. Not worth the dependency. |

**Decision.** `rig` for the agent/tool layer, `swiftide` for ingestion — both Rust-native, both wrapped behind Oncora traits. The fallback is explicit and cheap: an in-house orchestrator driving the raw LLM clients of §2 directly through `ModelProvider`. We will only exercise it if rig's tool-calling or streaming proves insufficient under verification.

---

## 2. LLM clients

The canon mandates on-prem-first, cloud opt-in only. Our default deployment talks to a GPU model server (vLLM or TGI) that exposes an **OpenAI-compatible** API inside the VPC. Every client is an implementation of the `ModelProvider` trait; the agent layer never imports a client crate directly.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `async-openai` | **Primary** | OpenAI-compatible covers our on-prem vLLM/TGI endpoint via a custom base URL — no cloud dependency required; mature, well-used, supports chat, tools, and streaming | **Verify**: that setting a custom base URL + on-prem auth works against the exact vLLM/TGI build we deploy, and that tool-call and streaming response shapes from vLLM match what the crate deserializes (server-side compatibility drift is the real risk, not the crate). |
| `async-anthropic` / Anthropic SDK | **Primary (cloud opt-in)** | First-class Anthropic access for the *opt-in*, egress-proxied cloud path; behind `ModelProvider`, identical contract to the on-prem provider | Cloud path is **off by default** and only reachable through the audited egress proxy (canon). **Verify**: SDK currency vs. the latest Messages API (tool use, streaming) and that PHI-redaction sits in front of it. |
| direct `reqwest` client | **Fallback** | Zero-magic escape hatch: a hand-rolled `ModelProvider` over `reqwest` for any endpoint whose wire format neither client above handles cleanly | More code we own and must test. Used only when a target endpoint deviates from OpenAI/Anthropic shapes. Low risk, high effort. |

**Decision.** `async-openai` is the default provider pointed at on-prem vLLM/TGI; `async-anthropic` is a registered-but-disabled provider for the cloud path; `reqwest` is the documented fallback. All three are concrete impls of `ModelProvider` in `oncora-agents`. See the trust-boundary note in [01-architecture.md](01-architecture.md).

---

## 3. Local / edge inference

Used for the dev single-node profile (canon Deployment) and for small on-device models: embedders, calibrators, classifiers, and a local LLM when no GPU server is present. The hard constraint: keep pure-Rust tensor/LLM inference separate from the one C++ dependency we tolerate (`ort`).

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `candle` | **Primary (tensors + small models)** | Pure-Rust tensor framework from HF; powers embedders, calibrators, and classifiers without leaving Rust; CPU + CUDA backends | Mature-ish but APIs still evolve. **Verify**: kernel coverage for the specific model architectures we run, and CUDA build reproducibility on our target images. |
| `mistral.rs` | **Primary (local LLM)** | Pure-Rust local LLM inference (quantized GGUF, common architectures) for the dev profile and air-gapped nodes; no GPU server needed | Fast-moving. **Verify**: which model families/quantizations are currently supported, throughput on target hardware, and whether it exposes an OpenAI-compatible surface we can put behind `ModelProvider`. |
| `ort` (ONNX Runtime) | **Primary (ONNX only), isolated** | The mature path for prebuilt ONNX embedders/classifiers (and what `fastembed` uses underneath); broad operator coverage | **C++ bindings** — the one tolerated non-Rust runtime. Isolated behind `EmbeddingProvider`/`Calibrator`; ONNX model files are pinned and content-addressed. **Verify**: pinned ORT version, reproducible builds, and that no ONNX graph reaches into anything but the trait surface. |
| `burn` | Rejected | Elegant pure-Rust deep-learning framework with multiple backends, but aimed at *training*/research; heavier and broader than our inference-only need; would duplicate candle's role | Re-survey only if we ever train models in-house rather than fine-tune externally and import. |

**Decision.** `candle` for tensors and small models, `mistral.rs` for the local LLM, `ort` strictly for prebuilt ONNX behind a trait. `burn` is rejected for an inference-serving platform. The ONNX isolation is the load-bearing call here — see [Non-Rust dependencies](#non-rust-dependencies-and-how-they-are-isolated).

---

## 4. MCP — Model Context Protocol

Per the canon, **all** domain tools are MCP tools and the MCP host routes deterministic, audited tool calls. Oncora is both an MCP **host/client** (the agent runtime calling tools) and an MCP **server** (the in-house VCF/`noodles`, DICOM/`dicom-rs`, and deterministic-calculator servers). One SDK should cover both roles.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `rmcp` (official Rust MCP SDK) | **Primary** | The official Rust SDK; covers both serving tools and acting as client/host; tokio-native, fits `oncora-mcp-host` cleanly; canonical choice | MCP and its SDKs are evolving quickly. **Verify**: current transport support (stdio + streamable HTTP/SSE) we actually deploy, client-side host capabilities (tool discovery, calling, cancellation) maturity, and protocol-version compatibility across our servers. Pin the protocol version and the crate. |
| in-house MCP impl | Fallback | If `rmcp` lags on a transport or capability we need, we own the protocol surface in `oncora-mcp-host` behind the `ToolHost` trait and can fill gaps | High effort; only for a specific missing capability. The `ToolHost` trait means specialists never see the difference. |

**Decision.** `rmcp` for both directions. The `ToolHost` trait in `oncora-core` abstracts it so audit, determinism, and timeout enforcement (via `tower`) live in *our* code, not the SDK's. Tool calls are recorded to episodic memory and the provenance ledger ([02-memory.md](02-memory.md)).

---

## 5. Vector search & embeddings

Hybrid retrieval (canon Read path) fuses vector + graph + recency. The vector side has two distinct workloads — text/RAG, and multimodal/imaging — plus an embedded option for dev and tests. Embeddings are produced on-prem.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `qdrant` (server + Rust client) | **Primary (text/RAG)** | Rust-native server, rich payload filtering, HNSW, scalar/product quantization, named vectors; runs on-prem and clusters for the scaled profile | Mature; operational footprint of running the server. **Verify**: Rust client version tracks the server we deploy; filtering + hybrid-search features match what `oncora-retrieval` needs. |
| `lancedb` | **Primary (multimodal / imaging)** | Rust-native columnar store; the Lance format is built for multimodal embeddings + metadata with versioning — ideal for imaging embeddings alongside DICOM-derived features | Younger than qdrant, but the format is solid. **Verify**: ANN index types/recall at our scale, and versioning semantics we rely on for reproducibility. |
| `hnsw_rs` / `instant-distance` | **Fallback (embedded / dev)** | Pure-Rust in-process ANN for the single-node dev profile and unit tests — no server to stand up | Library-level, no persistence/filtering story to speak of. Dev and CI only; never the production text path. |
| `fastembed` | **Primary (embeddings)** | Fast batched text embeddings on-prem; runs ONNX models via `ort`; turnkey for the common embedder families | Pulls ONNX model files — **pin them** and content-address them. Inherits the `ort` C++ surface (isolated behind `EmbeddingProvider`). |
| `candle`-hosted embedding models | **Fallback (embeddings)** | Pure-Rust embedding path when we want to avoid the ONNX surface entirely or run an architecture `fastembed` does not package | Slower to set up per-model; **verify** the specific model is implemented in candle. |

**Decision.** `qdrant` for text/RAG, `lancedb` for multimodal/imaging, embedded `hnsw_rs`/`instant-distance` for dev only. Embeddings via `fastembed` (primary) or `candle` (fallback) — both behind `EmbeddingProvider`. All vector stores implement the `VectorStore` trait so the fusion layer in `oncora-retrieval` is store-agnostic.

---

## 6. Biomolecular knowledge graph — the dual-store decision

This is the most deliberate architectural call in the data layer, so it gets the most justification. The canon mandates **two** graph stores, not one. Here is why that is correct and not over-engineering.

The KG has two fundamentally different jobs:

- **Ontology layer** — canonical, standards-grounded entities and relations imported from GO, Reactome, ChEMBL, UMLS, MONDO, HGNC. These arrive as RDF, are addressed by stable URIs, and are queried with the relationship-traversal idioms the bio world already standardizes on: **SPARQL**. This data is reference material: largely read-only, versioned at snapshot granularity.
- **Evidence / assertion graph** — Oncora's *own* output: claims, evidence, support/contradiction edges, per-edge confidence, and provenance, accumulated across runs and sessions. This needs **time-travel** (point-in-time queries: "what did we believe at snapshot X"), per-edge confidence, recursive **Datalog** reasoning, and ideally co-located vector search for semantic memory.

No single store does both jobs well. A SPARQL triplestore models the ontology natively but is awkward for versioned, confidence-weighted, time-travelled assertions. A hybrid graph/vector/Datalog store models the evidence graph beautifully but is not the natural home for standards-based RDF/SPARQL ontology import. Forcing one tool to do both would mean re-encoding ontologies into a foreign model (losing URI semantics and SPARQL) or bolting versioning onto a triplestore by hand. The dual store is the *simpler* design once you accept the two jobs are genuinely different. Both sit behind the single `GraphStore` trait, so `oncora-kg` exposes one query surface and routes to the right backend.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `oxigraph` | **Primary (ontology layer)** | Mature pure-Rust RDF triplestore with SPARQL; the natural fit for URI-addressed GO/Reactome/ChEMBL/UMLS/MONDO/HGNC import and standards-based traversal; ships as a library, no JVM | **Verify**: bulk-load performance for full ontology snapshots and SPARQL feature coverage (property paths, aggregates) we actually use. |
| `cozo` (CozoDB) | **Primary (evidence / assertion graph)** | Hybrid graph + relational + vector with **Datalog** and **time-travel** — uniquely matches versioned evidence + per-edge confidence + semantic memory; the time-travel is the differentiator nothing else offers cleanly | Young but distinctive. **Verify**: time-travel query semantics at our data volumes, Datalog performance on recursive evidence queries, and embedded-vector recall. This is the highest-novelty store; keep the `GraphStore` trait strict so we can fall back. |
| `indradb` | **Fallback (evidence graph)** | Pure-Rust property graph with pluggable storage; the escape hatch if cozo's young feature set bites | No native time-travel or Datalog — we would lose the differentiator and re-implement versioning ourselves. Fallback only. |
| Apache Jena | Rejected | Mature, standards-complete triplestore — but **JVM**, violating Rust-native + clean-isolation; oxigraph gives us SPARQL without the polyglot footprint | Rejected on the Rust-native non-negotiable, not on capability. |

**Decision.** Dual store: `oxigraph` for the ontology layer (SPARQL over imported RDF), `cozo` for the evidence/assertion graph (Datalog + time-travel + per-edge confidence + vector), `indradb` as the evidence-graph fallback, Jena rejected for the JVM. Both stores are concrete `GraphStore` impls. Full schema and entity/edge model in [04-knowledge-and-data.md](04-knowledge-and-data.md).

---

## 7. Omics & tabular analytics

Multi-omics arrives as large tabular and genomics-format data, read from pinned Parquet snapshots and genomics files, never written back to source. The genomics-IO choice is also a Rust-native-vs-C-bindings decision.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `polars` | **Primary (in-process dataframes)** | Fast, Arrow-backed, lazy/eager dataframe engine for in-proc omics transforms; pure Rust, excellent ergonomics | Mature; API still moves between minor versions. **Verify**: memory behavior on the largest omics matrices we load. |
| `duckdb` | **Primary (SQL over Parquet)** | Embedded analytical SQL directly over Parquet snapshots — the right tool for ad-hoc analytical queries the agents issue against omics | Embedded C++ engine via Rust bindings, but a self-contained, well-isolated dependency behind our query interface. **Verify**: binding version vs. engine, and reproducibility of the bundled build. |
| `arrow` | **Primary (interchange)** | The zero-copy columnar lingua franca tying polars ↔ duckdb ↔ Parquet ↔ CAS together without serialization round-trips | Mature. Keep `arrow`/`polars`/`duckdb` Arrow versions aligned — **verify** at the workspace level. |
| `noodles` | **Primary (genomics IO)** | Pure-Rust VCF/BCF/BAM/CRAM/FASTA/GFF/tabix; the in-house VCF MCP server is built on it; no C dependency | **Verify**: format/version coverage for the exact files our cohorts use; performance on large BAM/CRAM. Pure-Rust is the whole point. |
| `rust-htslib` | Rejected | The htslib bindings are the established genomics-IO path everywhere else — but they are **C bindings**, and `noodles` covers our formats in pure Rust | Rejected on the Rust-native/clean-isolation non-negotiable. Reconsider only for a niche format `noodles` genuinely lacks, and then only behind a tool boundary. |

**Decision.** `polars` + `duckdb` + `arrow` over Parquet for tabular omics; `noodles` for genomics IO (pure Rust); `rust-htslib` rejected to avoid the C-bindings surface. Genomics IO lives behind the VCF MCP server, so even noodles is reached through the `ToolHost` boundary.

---

## 8. Imaging

DICOM parsing and IO for the imaging specialist, exposed through the imaging MCP server. Imaging-derived embeddings land in `lancedb` (§5).

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `dicom-rs` | **Primary** | The pure-Rust DICOM toolkit — parse/encode/IO without a C dependency; sits behind the imaging MCP server | Mature enough for our parse/IO needs and behind the MCP boundary, so blast radius is contained. **Verify**: transfer-syntax/codec coverage for the modalities in our imaging corpus (e.g., compressed pixel data), and that pixel decoding we need is supported or pluggable. |

**Decision.** `dicom-rs` behind the imaging MCP server. No fallback named because there is no comparable pure-Rust alternative; if a codec is missing, we add it behind the same MCP tool boundary rather than reaching for a C library in the core.

---

## 9. Agent state / memory store

Backs the working and episodic memory types (canon Memory architecture): a run scratchpad and an append-only run/decision/verdict log. Pure-Rust embedded ACID KV is the requirement; the relational ledger and optional scale-out are separate rows.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `redb` | **Primary (embedded KV/log)** | Pure-Rust, embedded, ACID KV; stable 2.x; ideal for run logs and working-memory spill with no external service | Mature for our use. **Verify**: write throughput for episodic append volume and on-disk format stability across the 2.x line we pin. |
| `fjall` | **Fallback (embedded KV/log)** | Pure-Rust LSM-tree store; better write-amplification profile if episodic write volume outgrows redb | Younger than redb. Held as a warm `MemoryStore` impl, swapped only if redb's B-tree write pattern becomes the bottleneck. |
| `sled` | Rejected | Once the default pure-Rust embedded KV, but development has been in flux with a long-promised rewrite; we will not anchor durable memory on it | Rejected on maintenance/flux risk, not capability. redb and fjall cover the space. |
| `sqlx` + Postgres | **Primary (relational metadata + provenance ledger)** | Async, compile-time-checked SQL; Postgres for the multi-node provenance/audit ledger linking every memory item to sources/tool-calls/model-pin/snapshot | Mature. **Verify**: migration discipline and that compile-time query checking is wired into CI. |
| SQLite — **C library** (`rusqlite`, `bundled`) | **Fallback (dev relational)** | Single-file DB for the dev profile. The upstream **C** SQLite amalgamation compiled and linked into the binary — a non-Rust dependency isolated behind the `LedgerStore` seam (see §Non-Rust isolation). | Dev/single-node only; production ledger is Postgres for HA. |
| SQLite — **pure Rust** (`turso`, ex-`limbo`) | **Fallback (dev relational, Rust-native)** | Same `LedgerStore` trait, same SQL schema, no C in the build. The Rust-native end-state for the embedded relational slot. | `turso` 0.6 is young/pre-1.0 — **verify** feature coverage before relying on it for anything beyond the dev ledger. |
| **C ↔ Rust SQLite swap** | **Implemented + tested** | Both backends live in `crates/oncora-ledger` behind one `LedgerStore` trait; an identical conformance test passes against in-memory, C SQLite, and pure-Rust SQLite (`cargo test -p oncora-ledger --features "sqlite-c sqlite-rust"`). This is the runnable form of the migration spike in [08-roadmap.md](08-roadmap.md). | Default workspace build stays pure-Rust + fast (backends are opt-in Cargo features). |
| `foundationdb` | **Optional (scale-out state)** | Ordered, transactional KV for very large multi-node memory in the biggest deployments | **Ops-heavy** and **phase-gated** — not in the default path. Behind `MemoryStore`; enabled only at a deployment scale that justifies the operational cost. |
| `pgvector` | **Optional (not primary)** | Vectors co-located in Postgres if we ever want them next to relational metadata | Not the primary vector path — that is qdrant/lancedb (§5). Optional convenience only. |

**Decision.** `redb` for embedded KV/log (fallback `fjall`), `sled` rejected for flux; `sqlx`+Postgres for the provenance ledger (SQLite for dev); `foundationdb` optional and phase-gated; `pgvector` optional, never the primary vector store. All implement `MemoryStore` / back the provenance ledger described in [02-memory.md](02-memory.md).

---

## 10. Service & transport

The service mesh and async substrate. The canon locks these as mature, low-risk infrastructure; included for completeness and because everything above assumes them.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `tokio` | **Primary (async runtime)** | The de-facto async runtime; every crate above assumes it | Mature, low risk. Pin a major version workspace-wide. |
| `axum` | **Primary (HTTP API)** | Tower-native, ergonomic, tokio-first; the `oncora-api` external surface | Mature. Fallback `actix-web` is named in the canon but not needed. |
| `tonic` | **Primary (gRPC)** | Standard Rust gRPC for the internal service mesh between agent workers and stores | Mature. **Verify**: protobuf/codegen pinned and reproducible in CI. |
| `tower` + `tower-http` | **Primary (middleware)** | Uniform timeout / retry / concurrency-limit / load-shed layers — this is where per-tool timeouts and backpressure (canon Concurrency) are enforced | Mature. The MCP host leans on this for deterministic tool-call timeouts. |

**Decision.** `tokio` / `axum` / `tonic` / `tower` as locked. These are the stable floor under the riskier layers; no controversy.

---

## 11. Observability

Structured run traces correlate agent runs end to end (canon). Tracing is the backbone of both debugging and the audit story.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `tracing` | **Primary (instrumentation)** | The standard structured, span-based instrumentation; agent steps/tool-calls/verdicts become spans | Mature. **Verify**: span hygiene so PHI never lands in span fields (privacy non-negotiable). |
| `tracing-opentelemetry` | **Primary (bridge)** | Bridges `tracing` spans to OpenTelemetry; correlates runs across services | Mature-ish; **verify** version compatibility between `tracing`, the bridge, and the OTel SDK — this trio drifts and is a common breakage point. |
| OpenTelemetry + collector | **Primary (export/transport)** | Vendor-neutral export to an on-prem collector inside the trust boundary | On-prem collector only; **verify** exporter config keeps telemetry inside the VPC. |

**Decision.** `tracing` + `tracing-opentelemetry` + OpenTelemetry collector, on-prem. Lives in `oncora-telemetry`. The main standing risk is the `tracing`/bridge/SDK version triangle — pin all three together.

---

## 12. Reliability / testing

The canon's pillar 4 (reliability & benchmarking) and the typed-uncertainty invariants demand more than unit tests: property tests on invariants, fuzzing on parsers/tool inputs, snapshots on prompts/traces, microbenchmarks, and differential testing against baselines.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `proptest` | **Primary (property testing)** | Invariants on memory write/read, uncertainty/confidence math, and parsers; shrinking finds minimal counterexamples | Mature. Fallback `quickcheck` named in canon, not needed. |
| `cargo-fuzz` (libFuzzer) | **Primary (fuzzing)** | Fuzz the untrusted/parser surfaces — VCF, DICOM, JSON tool args — exactly where malformed input is most dangerous | Mature. **Verify**: fuzz targets exist for every external-input parser; wire into CI as scheduled, not per-PR. |
| `insta` | **Primary (snapshot testing)** | Lock agent prompts, traces, and manifests so prompt/format drift is caught in review | Mature. Snapshots must be reviewed deliberately, not blanket-accepted. |
| `criterion` | **Primary (benchmarks)** | Latency/throughput microbenchmarks with regression detection feeding CI gates | Mature. Fallback `divan` named in canon. |
| Differential testing | **Primary (technique)** | Cross-check agent outputs against deterministic oracles and computational baselines — directly serves the "beat baselines, reproducibly" pillar | Not a single crate but a harness discipline in `oncora-eval`. **Verify**: golden sets and baselines are themselves pinned/versioned. |

**Decision.** `proptest` + `cargo-fuzz` + `insta` + `criterion` plus differential testing as a harness technique. The full benchmark + CI-gating story is in [06-eval-benchmarking.md](06-eval-benchmarking.md).

---

## 13. Reproducibility / artifacts

Reproducibility is a non-negotiable, not a feature. The content-addressed store and pinning machinery make every run replayable and every benchmark re-derivable bit-for-bit.

| Option | Verdict | Why | Risk / what to verify |
|---|---|---|---|
| `blake3` (CAS hashing) | **Primary** | Fast cryptographic hashing for content-addressing every artifact; the basis of deterministic replay | Mature. **Verify**: hashing is canonical (stable byte representation before hashing) so identical inputs always hash identically. |
| custom CAS over object store / FS | **Primary** | We own the content-addressed store behind the `ArtifactStore` trait — artifacts keyed by BLAKE3 digest, immutable, dedup-by-content | We own it, so we own the risk. **Verify**: GC/retention policy never deletes anything a provenance record still references. |
| `serde` (+ `serde_json`, `ciborium` CBOR) manifests | **Primary** | Typed, versioned manifests for runs, memory entries, and tool IO; JSON for human-auditable surfaces, CBOR for compact payloads | Mature. **Verify**: manifest schemas are versioned so old artifacts stay readable. |
| deterministic seeds | **Primary (technique)** | Every stochastic step (sampling, self-consistency N-sampling) records its seed so a run replays identically | **Verify**: all randomness routes through a recorded, seeded source — no unseeded `rand` calls in the agent path. |
| model / version pinning | **Primary (technique)** | `ModelPin` + `SnapshotId` on every `Provenance` value pin the exact model and data snapshot per the canon uncertainty model | **Verify**: the model server reports a stable, recordable model identity (vLLM/TGI build + weights digest), not just a friendly name. |

**Decision.** BLAKE3-keyed custom CAS behind `ArtifactStore`, `serde`/CBOR manifests, recorded deterministic seeds, and model/snapshot pinning. This is what makes Oncora publishable; it lives in `oncora-artifacts` and is referenced throughout [04-knowledge-and-data.md](04-knowledge-and-data.md).

---

## Consolidated Risk Register

Likelihood and Impact are Low / Medium / High. "Owner-area" maps to the canon crate that owns the mitigation.

| Risk | Likelihood | Impact | Mitigation | Owner-area |
|---|---|---|---|---|
| `rig` 0.x breaking change or insufficient tool-calling/streaming | High | Medium | Wrap behind `oncora-agents` traits; in-house orchestrator on raw clients as fallback; verify tool-calling + streaming before committing | `oncora-agents` |
| `swiftide` API churn breaks ingestion stages | Medium | Low | We own the `oncora-ingest` pipeline contract; swiftide implements stages, not the API | `oncora-ingest` |
| vLLM/TGI server wire-format drift vs. `async-openai` | Medium | High | Pin server build; contract-test the deserialization; `reqwest` fallback `ModelProvider` | `oncora-agents` |
| `cozo` time-travel / Datalog immature at our scale | Medium | High | Strict `GraphStore` trait; `indradb` fallback; verify time-travel + recursive-query perf early | `oncora-kg` |
| `ort` / ONNX C++ surface — build or supply-chain issue | Medium | Medium | Isolate behind `EmbeddingProvider`/`Calibrator`; pin ORT version + ONNX model digests; `candle` pure-Rust fallback | `oncora-retrieval` |
| `rmcp` protocol/transport gaps vs. what we deploy | Medium | Medium | `ToolHost` trait owns audit/timeout/determinism; pin protocol version; in-house fill-in for missing transport | `oncora-mcp-host` |
| `mistral.rs` model/quantization support shifts | Medium | Low | Dev/air-gapped profile only; `ModelProvider` boundary; on-prem vLLM is the production path | `oncora-agents` |
| `lancedb` ANN recall/versioning insufficient for imaging | Low | Medium | `VectorStore` trait; qdrant named-vectors fallback; verify recall at scale | `oncora-retrieval` |
| `noodles` missing a genomics format/codec we need | Low | Medium | Behind VCF MCP server (`ToolHost`); add support there; htslib only as last resort behind the boundary | `oncora-mcp-host` |
| `dicom-rs` missing a transfer syntax/codec | Low | Medium | Behind imaging MCP server; add codec support there rather than a C dependency in core | `oncora-mcp-host` |
| `redb` write throughput insufficient for episodic volume | Low | Medium | `MemoryStore` trait; `fjall` LSM fallback; benchmark append throughput | `oncora-memory` |
| `tracing` / `tracing-opentelemetry` / OTel SDK version mismatch | Medium | Low | Pin all three together; smoke-test export in CI | `oncora-telemetry` |
| PHI leaks into spans/telemetry or to a cloud endpoint | Low | High | Span-field hygiene; cloud providers off-by-default behind audited egress proxy; redaction in front of any external provider | `oncora-telemetry` / `oncora-api` |
| Non-deterministic replay (unseeded randomness, unstable model identity) | Medium | High | All randomness through recorded seeds; model identity recorded as build + weights digest; replay tests in CI | `oncora-artifacts` |
| `arrow` version skew across polars/duckdb breaks zero-copy | Medium | Low | Align Arrow versions workspace-wide; verify in CI | `oncora-ingest` |
| Workspace-wide ecosystem churn (many 0.x crates) | High | Medium | Trait boundaries everywhere; verify-before-committing checklist; pin and review upgrades | all |

---

## Non-Rust dependencies and how they are isolated

The platform is Rust-native end to end. There are exactly **three** sanctioned non-Rust surfaces, and each is isolated behind a named Rust trait so it never leaks into agent or domain logic.

1. **Model endpoints (HTTP, possibly remote).** The vLLM/TGI on-prem server, and any opt-in cloud LLM, are non-Rust services reached over HTTP. They are isolated behind the **`ModelProvider`** trait in `oncora-core`; clients (`async-openai`, `async-anthropic`, `reqwest`) are concrete impls. *Justification:* GPU model serving is not a Rust workload; we consume it as a network contract and keep the trust boundary explicit. Cloud endpoints are off by default and only reachable through the audited egress proxy.

2. **ONNX Runtime (`ort`, C++ bindings).** Prebuilt ONNX embedders/classifiers/calibrators run on ORT, and `fastembed` uses `ort` underneath. Isolated behind **`EmbeddingProvider`** and **`Calibrator`**; ONNX model files are pinned and content-addressed. *Justification:* ORT has the mature operator coverage and prebuilt-model ecosystem that pure-Rust inference does not yet match; the C++ surface is contained to one trait boundary, and `candle` is the pure-Rust fallback if the bindings ever become a liability. `duckdb`'s embedded engine is a similar, self-contained native dependency, reached only through our query interface.

3. **Third-party ontologies (data, not code).** GO, Reactome, ChEMBL, UMLS, MONDO, HGNC are external *data* artifacts in foreign vocabularies/formats. They enter only through ingestion into `oxigraph`, are addressed by stable URIs, queried through the **`GraphStore`** trait, and pinned at snapshot granularity. *Justification:* this is reference data we read, never source we write; isolating it behind `GraphStore` + snapshot pinning keeps the rest of the system blind to ontology format churn and preserves reproducibility.

Everything else — graph stores, vector stores, genomics/DICOM IO, KV/log, the artifact store — is pure Rust or an embedded engine reached only through a trait. The genomics (`noodles`) and imaging (`dicom-rs`) parsers, though pure Rust, additionally sit behind the **`ToolHost`** MCP boundary, so even a parsing bug cannot reach the agent core directly.

---

## Verify-before-committing checklist

Crate maturity in the Rust AI/agent ecosystem is **fast-moving** — much of the above is 0.x and changes between minor releases. The trait boundaries are what make that survivable, but before we commit production code to any *Primary* choice, confirm the capability still holds. Re-run this checklist at the start of each implementation phase ([08-roadmap.md](08-roadmap.md)).

- [ ] **`rig`** — current tool-calling (multi-tool + parallel) and streaming-completion maturity; provider trait works with a custom OpenAI-compatible base URL.
- [ ] **`swiftide`** — transform-stage composition with our `EmbeddingProvider`/`VectorStore`; backpressure behavior.
- [ ] **`async-openai` ↔ vLLM/TGI** — custom base URL + on-prem auth; tool-call and streaming response shapes match the deployed server build.
- [ ] **`async-anthropic`** — SDK currency vs. latest Messages API (tool use, streaming); redaction sits in front; path stays off-by-default.
- [ ] **`candle`** — kernel/architecture coverage for our models; reproducible CUDA builds on target images.
- [ ] **`mistral.rs`** — supported model families/quantizations; throughput on target hardware; OpenAI-compatible surface for `ModelProvider`.
- [ ] **`ort`** — pinned ORT version; reproducible build; ONNX model digests pinned; no leakage past the trait.
- [ ] **`rmcp`** — transports we deploy (stdio + streamable HTTP/SSE); client/host capability maturity; protocol version pinned.
- [ ] **`qdrant`** — Rust client tracks deployed server; filtering + hybrid search cover retrieval needs.
- [ ] **`lancedb`** — ANN index types/recall at scale; versioning semantics for reproducibility.
- [ ] **`fastembed`** — embedder families available; ONNX models pinned/content-addressed.
- [ ] **`oxigraph`** — bulk-load perf for full ontology snapshots; SPARQL feature coverage (property paths, aggregates).
- [ ] **`cozo`** — time-travel semantics + recursive-Datalog perf at our volumes; embedded-vector recall. *(Highest-novelty; verify first.)*
- [ ] **`polars` / `duckdb` / `arrow`** — Arrow versions aligned; memory behavior on largest omics matrices.
- [ ] **`noodles`** — format/version + codec coverage for our cohorts; large BAM/CRAM performance.
- [ ] **`dicom-rs`** — transfer-syntax/codec coverage for our imaging modalities; pixel decoding supported or pluggable.
- [ ] **`redb`** — episodic-append write throughput; on-disk format stability across pinned 2.x.
- [ ] **`tracing` / `tracing-opentelemetry` / OTel SDK** — versions pinned together; export stays on-prem; no PHI in span fields.
- [ ] **`cargo-fuzz`** — a fuzz target exists for every external-input parser (VCF, DICOM, JSON tool args).
- [ ] **Reproducibility** — all randomness seeded + recorded; model identity recorded as build + weights digest; replay test passes in CI.

The rule stands: **trust the trait boundary, not the crate version.** If verification fails for any Primary, the named Fallback is already a registered impl of the same trait, and switching is a configuration change, not a rewrite.
