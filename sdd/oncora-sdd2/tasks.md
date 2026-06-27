# Oncora Tasks — Phased, Risk-Gated Roadmap

Actionable work from spec/plan. Roadmap is **risk-gated**: each phase retires a risk class and carries explicit **de-risking spikes** testing the youngest/fastest-moving bets (`rig`, `rmcp`, `swiftide`, `cozo` time-travel, conformal calibration, on-prem model throughput) *before* the thick layer is built atop. A failed spike routes to the canon's named **fallback** (see [research.md](research.md)). Throughline: **build the spine first, thicken one pillar at a time, put the riskiest young-tech bet on trial before depending on it.**

Current state ([quickstart.md](quickstart.md) §Status): end-to-end functional prototype, every seam green against a real backend; what remains is production hardening (Phases 5–6) and research-pillar depth.

---

## Phase 0 — Walking skeleton ✅ (spine proven in prototype)

**Goal.** One end-to-end thin slice exercising **every crate** at minimal depth: a target-discovery query that retrieves (hybrid), reasons, calls **one** MCP tool, verifies, returns a confidence-scored, cited answer.

**Scope (minimum of every crate).**

| Crate | Phase-0 minimum |
|---|---|
| `oncora-core` | `Confidence`, `Provenance`, `Evidence`, `Verdict`, ids, errors |
| `oncora-telemetry` | `tracing` spans to stdout/local OTel |
| `oncora-artifacts` | `blake3` CAS over local FS; one manifest type |
| `oncora-mcp-host` | `rmcp` host; one registered tool; audit to ledger + episodic |
| `oncora-kg` | `oxigraph` + `cozo` embedded; minimal Target/Disease/Evidence schema |
| `oncora-retrieval` | `fastembed` embeddings; one vector index; basic vector+graph fusion |
| `oncora-ingest` | `swiftide` ingest of one small literature snapshot |
| `oncora-memory` | working + episodic + provenance write/read (semantic stub) |
| `oncora-uncertainty` | one calibration method; citation-entailment verifier; threshold verdict |
| `oncora-agents` | `rig` planner → one specialist → verifier → scorer → responder |
| `oncora-eval` | 10–20 golden target-discovery questions; accuracy + abstention metrics |
| `oncora-api` | one authenticated query endpoint |
| `oncora-cli` | submit query + replay a recorded run |

**Exit / done-criteria.**
- Target-discovery query runs end-to-end, returns cited, confidence-scored answer **or** logged abstention.
- The single MCP tool call recorded to ledger **and** episodic memory.
- `oncora-cli replay` reproduces the run bit-for-bit from pinned model + snapshot + CAS.
- The golden set runs in CI and gates the build.

**Spikes.** (T0-S1) Verify `rig` tool-calling + streaming against an OpenAI-compatible on-prem endpoint — fallback: in-house orchestrator on raw `async-openai`. (T0-S2) Verify `rmcp` tool maturity — register, route, audit one round-trip; gaps → wrap behind `ToolHost`. **Risk:** spine leaks abstraction (a store type bleeds into `oncora-agents`) → trait-boundary review before Phase 1.

> **Prototype divergence (recorded).** Prototype built an **in-house orchestrator + simple ingest** instead of adopting `rig`/`swiftide`, and runs the loop as a single end-to-end function rather than a multi-agent runtime. Decision still open: adopt `rig`/`swiftide` or formally retire them in the docs.

---

## Phase 1 — Multimodal ingestion (literature → omics → imaging)

**Goal.** Thicken pillar 1: single-modality skeleton → true cross-modal evidence assembly.

**Deliverables.** `swiftide` pipelines for literature, then omics (`polars`/`duckdb` over Parquet, `noodles` VCF behind an MCP server), then imaging (`dicom-rs` behind an MCP server, `lancedb` multimodal embeddings). KG schema fully populated (Target/Disease/Pathway/Compound/Variant/Trial/Cohort + edges). Hybrid retrieval fuses vector + graph + recency across all modalities.

**Exit.** A query requiring literature **and** omics **and** imaging returns one fused, cited answer; each modality content-addressed + snapshot-tagged; no source ever written to.

**Spikes.** VCF/DICOM parser robustness (fuzz `noodles`/`dicom-rs` on real malformed files); `lancedb` multimodal embedding scale on a representative imaging set; `swiftide` pipeline contract (own it; fallback to a custom `tokio` pipeline). **Risk:** entity resolution across HGNC/UMLS/MONDO → `oxigraph` URI grounding + dedup in the memory write path.

---

## Phase 2 — Memory architecture

**Goal.** Thicken pillar 2: full five-type memory with canonical write/read paths + cross-session persistence.

**Deliverables.** Working/episodic/semantic/procedural/provenance wired to canonical stores. Write path: extract → dedup (embedding + KG entity resolution) → conflict resolution (recency + authority + confidence; contradictions kept) → consolidation (working→episodic→semantic) → decay/forgetting (tombstones, never lose provenance). Read path: hybrid retrieval fused under token budget, keyed by `(scientist, project, workflow)`.

**Exit.** A second session on the same key demonstrably reuses prior semantic + procedural memory; conflicting facts retained as competing evidence (not overwritten); every entry carries snapshot + model pin and is replayable.

**Spikes.** **`cozo` time-travel at scale** (point-in-time evidence-graph queries over a realistically sized graph — the canon's biggest young-tech bet; fallback `indradb` + explicit versioning); conflict-resolution policy on adversarial contradictory inputs. **Risk:** decay erases something a reviewer needs → soft-delete + tombstones; provenance never deleted.

---

## Phase 3 — Uncertainty & verification

**Goal.** Thicken pillar 3: typed, calibrated uncertainty driving accept/abstain/escalate.

**Deliverables.** Post-hoc calibration with ECE tracking; conformal prediction (set-valued outputs + abstention guarantee); verifiers (citation/NLI entailment, deterministic oracle grounding via calculators/KG); decomposed uncertainty (aleatoric/epistemic/retrieval/tool); configurable per-task-class thresholds.

**Exit.** Confidence calibrated (ECE below target on a held-out set); conformal sets carry guaranteed coverage; oracle disagreement reliably triggers `Escalate`; every abstention logged with a reason.

**Spikes.** Conformal calibration (confirm coverage holds on oncology golden sets; tune set-size thresholds); NLI entailment verifier quality on biomedical claims (false-accept rate). **Risk:** long-tail miscalibration → ECE gating in CI; abstain when conformal set is wide.

---

## Phase 4 — Eval & benchmarking

**Goal.** Thicken pillar 4: prove faster/more-accurate/more-reliable vs human experts and computational baselines, reproducibly.

**Deliverables.** Expanded golden sets across modalities and task classes; metrics (accuracy, ECE, abstention quality, latency); human-expert + computational baselines; CI gate blocking regressions.

**Exit.** Benchmarks beat the named baselines on the agreed metrics; results deterministically replayable; a regression cannot merge.

**Spikes.** Golden-set construction with domain experts (the benchmark's validity is the risk, not the harness). **Risk:** benchmark overfit → held-out sealed sets; periodic expert refresh.

---

## Phase 5 — Deployment scale-out (single-node → multi-node)

**Goal.** Move from single-node embedded to the multi-node scaled topology (plan §6).

**Deliverables / task list.**
1. **Config-driven `Platform` assembly** (replace `Platform::demo()` — the unlock for everything else).
2. Stateless `oncora-api`/agent workers behind a load balancer (horizontal scale).
3. Externalize stores: qdrant cluster, **Postgres HA** provenance ledger, oxigraph/cozo as services.
4. On-prem GPU model server (vLLM/TGI) behind `ModelProvider`; size concurrency caps.
5. Object-store-backed CAS (`ArtifactStore` → S3/MinIO).
6. OTel collector wiring (`tracing-opentelemetry`).
7. Upgrade/rollback: expand-contract migrations; golden-set replay-diff gate; confirmed rollback.

**Exit.** Horizontal scale under load · Postgres + qdrant failover · upgrade validated by replay diff · rollback confirmed. FoundationDB evaluated only if memory scale demands it (optional, ops-heavy, phase-gated).

**Spikes.** On-prem model throughput (tokens/sec under realistic concurrency on target GPUs; size semaphore caps — the production cost/latency ceiling); qdrant cluster sharding/replication at representative corpus size.

> **Spike RESOLVED — turso adopted.** The `LedgerStore` seam has three interchangeable backends (in-memory, C SQLite `rusqlite`, pure-Rust `turso`); an identical conformance test passes for all three. A cold-start bulk comparison over 14,469 per-document writes settled it: **turso completed with 0 errors at mean 2.04 ms — ~23% faster than C SQLite.** Decision: turso is the chosen embedded ledger; the ledger is wired into the agent loop (a provenance record per run); turso is the bench default; C SQLite kept behind a benchmark-only feature. *Still to verify before production:* turso transaction/concurrency/full-SQL coverage; Postgres remains the HA path. Knock-on: gating C SQLite removed one of cozo's two blockers (the native-`sqlite3` `links` clash); the `graph_builder`/`rayon` one remains.

---

## Phase 6 — Governance hardening (review-board grade)

**Goal.** Lock down security/privacy/governance (plan §7) to review-board grade.

**Deliverables / task list.**
1. **RBAC** — project/workflow-scoped access control on the API.
2. **Provenance lineage walk** — query any claim → sources, tool calls, model pin, snapshot (per-run ledger record already written in the prototype — add the lineage query + API).
3. **Audit completeness** — record-before-return invariant at the MCP host; property-test it (`McpHost` already keeps an audit log).
4. **PHI/PII redaction + egress blocking** — redact logs/traces; egress proxy denies PHI leaving the boundary; adversarial leak testing.
5. **Reviewer adjudication queue** — route `Verdict::Escalate` to human sign-off.
6. **ALCOA+ reproducibility evidence** — one-command replay of any historical run from its manifest.
7. **Change-control** — replay-gated CI (an upgrade must reproduce golden-set outputs).

**Exit.** Any claim traceable to provenance · PHI cannot leave · every tool call (allow and deny) auditable · validated replay reproduces any historical run.

**Spikes.** Redaction completeness (adversarial PHI-leak testing against logs/traces and the egress proxy). **Risk:** audit-trail gaps under failure → record-before-return invariant; `proptest` the MCP host.

**Sequencing.** Phase 6 depends on Phase 5 (RBAC/egress/lineage assume the multi-node services + real ledger/CAS). Both are infrastructure/ops-heavy — the trait seams mean each is "add a production impl behind an existing trait," not a rewrite. Phase 6 items 2–3 have head starts from the prototype (ledger-in-loop, MCP audit log).

---

## Timeline & milestones

Gantt (dateFormat YYYY-MM-DD; axisFormat %b %y), as a lossless table:

| Section | Phase | Id | Start | Duration |
|---|---|---|---|---|
| Skeleton | Phase 0 walking skeleton | p0 | 2026-06-01 | 60d |
| Pillars | Phase 1 multimodal ingest | p1 | after p0 | 75d |
| Pillars | Phase 2 memory architecture | p2 | after p1 | 75d |
| Pillars | Phase 3 uncertainty verify | p3 | after p2 | 75d |
| Pillars | Phase 4 eval benchmarking | p4 | after p3 | 60d |
| Hardening | Phase 5 deployment scale-out | p5 | after p4 | 75d |
| Hardening | Phase 6 governance hardening | p6 | after p4 | 90d |

| Milestone | Phase | Success metric |
|---|---|---|
| Spine proven | 0 | End-to-end cited answer + replay bit-for-bit |
| `rig` + `rmcp` validated | 0 | One tool round-trip audited; fallback decision recorded |
| Cross-modal answer | 1 | One fused answer over literature + omics + imaging |
| Cross-session memory | 2 | Session 2 reuses semantic + procedural memory |
| `cozo` time-travel proven | 2 | Point-in-time query at target graph size within budget |
| Calibrated uncertainty | 3 | ECE below target; conformal coverage holds |
| Beats baselines | 4 | Wins accuracy/calibration/latency vs human + computational baselines |
| Scaled topology live | 5 | Horizontal scale + failover + replay-validated upgrade |
| On-prem throughput sized | 5 | Tokens/sec measured; semaphore caps set |
| Review-board ready | 6 | Full lineage walk + PHI-egress block verified |

## Biggest risks & early de-risking

| Risk | Severity | Phase exposed | De-risk early |
|---|---|---|---|
| `rig` tool-calling/streaming immature | High | 0 | Phase-0 spike; fall back to in-house orchestrator on `async-openai` |
| `rmcp` host/tool gaps | High | 0 | Phase-0 spike; wrap behind `ToolHost` |
| `cozo` time-travel does not scale | High | 2 | Phase-2 spike at target graph size; fallback `indradb` + versioning |
| Conformal coverage fails on domain data | High | 3 | Phase-3 calibration spike on oncology golden sets |
| On-prem GPU throughput too low | High | 5 | Phase-5 throughput spike; abstention as cost control |
| Parser fragility on real VCF/DICOM | Medium | 1 | `cargo-fuzz` parser fuzzing on malformed real files |
| PHI leak via logs or egress | High | 6 | Adversarial redaction testing; deny-by-default egress proxy |
| Pure-Rust SQLite not yet drop-in | Low | 5 | **Resolved** — turso adopted behind the seam, validated faster than C SQLite |
| Audit-trail gap under failure | High | 6 | Record-before-return invariant; `proptest` the MCP host |
| Benchmark validity / overfit | Medium | 4 | Expert-built held-out golden sets; periodic refresh |
| Abstraction leak across crate boundaries | Medium | 0 | Trait-boundary review gate before each pillar phase |
