# Oncora Constitution

> **Non-negotiable principles** governing every spec, plan, contract, and task. A design that violates a principle is rejected regardless of other merits. Derived from the "Cross-cutting non-negotiables" and trust-boundary rules of the Oncora canon.

Each principle: stable ID (`P-*`), statement, rationale, enforcement point.

---

## P-1 — Rust-native end to end

**Statement.** Whole stack is Rust. Non-Rust deps are isolated behind a named Rust trait and individually justified. Exactly **three** sanctioned non-Rust surfaces; no more without an ADR:

1. **Model endpoints** (HTTP services: on-prem vLLM/TGI, opt-in cloud) behind `ModelProvider`.
2. **ONNX Runtime** (`ort`, C++ bindings; `fastembed` uses it underneath; `duckdb`'s embedded engine is a similar contained native dependency) behind `EmbeddingProvider` / `Calibrator` / the query interface.
3. **Third-party ontologies** (external *data*, not code: GO, Reactome, ChEMBL, UMLS, MONDO, HGNC) behind `GraphStore` + snapshot pinning.

**Rationale.** One language/type system keeps agent loop, memory hooks, and uncertainty gating coherent; isolating native surfaces contains blast radius and supply-chain risk.

**Enforcement.** Trait boundaries in `oncora-core`; `cargo deny` gate-keeps native/duplicate deps; genomics (`noodles`) and imaging (`dicom-rs`) parsers — though pure Rust — additionally sit behind the `ToolHost` MCP boundary.

---

## P-2 — On-prem / privacy by default; PHI and IP never cross the trust boundary

**Statement.** Trust boundary is the VPC. All Oncora services, stores, model server, and internal MCP servers live inside it. PHI and IP never leave. The **only** sanctioned outbound path is an explicit, deny-by-default, allow-listed, audited **egress proxy** used solely for opt-in cloud model endpoints; nothing else may dial out. A run touching a cloud endpoint records that fact in its model pin.

**Rationale.** Outputs feed go/no-go decisions, manuscripts, regulatory submissions; data is governed by privacy/IP boundaries forbidding it leaving the institution.

**Enforcement.** Network policy (data tier not internet-routable); `oncora-api` auth; egress proxy with PHI stripping; span-field hygiene in `oncora-telemetry`.

---

## P-3 — No writes to source data

**Statement.** Source systems and snapshots are **read-only** inputs. No code path writes back to a source system. All derived artifacts live in Oncora-owned, content-addressed stores.

**Rationale.** Source data integrity and reproducibility; Oncora is a reasoning layer, not a system of record.

**Enforcement.** `oncora-ingest` has no write path to sources; snapshots are immutable.

---

## P-4 — Provenance on every claim

**Statement.** Every assertion carries a typed `Provenance { sources, tool_calls, model, snapshot }`. A bare claim with no provenance is a **type error in the pipeline**, not a stylistic lapse. Ungrounded prose never reaches the responder.

**Rationale.** A reviewer must trace any returned sentence back to its sources and the exact tool calls that produced it.

**Enforcement.** `oncora-core::Provenance` required on `Evidence` and on every memory write; recorded to the Postgres provenance ledger + episodic memory.

---

## P-5 — Deterministic, audited tool calls

**Statement.** Every domain capability is an MCP tool. Every MCP call is executed **deterministically** through the single `oncora-mcp-host` chokepoint and recorded — inputs, outputs, model pin, snapshot, `ToolCallId`, latency, verdict — to **both** episodic memory and the provenance ledger. Both **allow and deny** are audit events. No path calls a domain tool bypassing this recorder; the agent receives a result only **after** the call is written (record-before-return invariant).

**Rationale.** Auditability and replay; calculators and the KG act as deterministic oracles.

**Enforcement.** `oncora-mcp-host` policy gate + `tower` timeout + audit sinks.

---

## P-6 — Reproducibility: pinned models, pinned snapshots, content-addressed artifacts, deterministic replay

**Statement.** Every run pins its `ModelPin` (weights + decode config) and `SnapshotId` (data snapshot), content-addresses every artifact with **BLAKE3**, and routes all randomness through recorded seeds. Given `(snapshot, model pin, content hashes, seeds)` the same inputs reproduce the same outputs **bit-for-bit**. Any run is replayable via `oncora-cli`.

**Rationale.** Reproducible enough to publish and to re-derive any benchmark result.

**Enforcement.** `oncora-artifacts` CAS; pinned toolchain + committed `Cargo.lock`; `insta` snapshot tests; replay tests in CI.

---

## P-7 — Typed, calibrated uncertainty; the system may decline

**Statement.** Confidence is a typed, calibrated, first-class value (`Confidence(f64)` with a `CalibrationMethod` tag) flowing end to end. A reported `Confidence(0.9)` must mean ~90% empirical accuracy on held-out data (measured by ECE, enforced as a gate). The terminal act of reasoning is a `Verdict`; **`Abstain` and `Escalate` are first-class outcomes ranked above a low-confidence `Accept`**. Raw, uncalibrated scores are rejected by the scorer. The system would rather say "I do not know — escalating" than confabulate.

**Rationale.** A confident wrong answer is worse than no answer in a clinical-adjacent system.

**Enforcement.** `oncora-uncertainty` scorer is the only component allowed to emit `Accept`; ECE-gated in `oncora-eval` CI.

---

## P-8 — Reliability is a tested, gated property — not a claim

**Statement.** Any claim that Oncora is "faster / more accurate / more reliable than experts and baselines" must be (a) computed on a versioned, expert-labeled golden set, (b) compared against ≥1 named computational baseline **and** a human-expert reference on a fair footing, (c) measured for calibration and abstention quality, not just accuracy, and (d) reproducible bit-for-bit by a third party from a manifest. Regressions block merges.

**Rationale.** Publish-grade credibility; agent-vs-self is not a benchmark.

**Enforcement.** `oncora-eval` CI gate on accuracy, ECE, abstention (AURC), latency, baseline dominance, replay.

---

## P-9 — Provider swappability via trait boundaries

**Statement.** Every swappable capability is an object-safe trait in (or near) `oncora-core`. Concrete backends are the only place a third-party/non-Rust dependency may appear. Dependencies point **inward** toward `oncora-core`; the internal crate graph is a strict **DAG with no cycles**. Swapping a backend = writing one impl and rebinding a trait object at composition time — **zero changes to the agent runtime or API**.

The nine seams: `ModelProvider`, `EmbeddingProvider`, `VectorStore`, `GraphStore`, `MemoryStore`, `ToolHost`, `Calibrator`, `Verifier`, `ArtifactStore` (plus `LedgerStore` for the relational/provenance ledger).

**Rationale.** Lets the platform adopt young, fast-moving crates as primary without betting the platform on any one. "Trust the trait boundary, not the crate version."

**Enforcement.** `oncora-core` trait definitions; review gate on abstraction leaks; Cargo rejects cyclic deps.

---

## P-10 — Honest knowledge: never silently overwrite; contradictions are retained

**Statement.** Conflicting facts are **kept as competing evidence**, never destructively overwritten. The "current belief" is a computed view over edges that each carry their own confidence and provenance. Forgetting is **visibility management** (soft-delete + tombstone), never deletion; **episodic history and provenance are never decayed or deleted**.

**Rationale.** Honest uncertainty over false resolution; reproducibility of past conclusions via time-travel.

**Enforcement.** `oncora-memory` conflict-resolution policy; `cozo` time-travel + per-edge confidence.

---

## Scope guardrails (constitutional non-goals)

The platform is **not**: a general-purpose chatbot or open-domain assistant; a cloud SaaS with default external calls; a wet-lab/LIMS/ELN replacement; a source-of-truth datastore for primary research data; a system that guesses to be helpful (silence is a valid, logged outcome); or a polyglot microservice zoo.

## Amendment

Principles change only via an Architecture Decision Record (`docs/adr/NNNN-*.md`, context → decision → consequences) that explicitly supersedes the prior text. The locked technology table ([research.md](research.md)) is the rolled-up summary of such decisions.
