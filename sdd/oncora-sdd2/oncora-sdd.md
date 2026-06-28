# Oncora — Consolidated Spec-Driven Development (compact, lossless)

> Single-document, information-equivalent rendering of the full SDD set in
> [`../oncora-sdd/`](../oncora-sdd/). Same facts, denser form: one preamble, section-number
> cross-references instead of per-file links, merged/deduplicated tables, mermaid diagrams
> rendered as lossless edge-lists. Derived from the Oncora design canon (`docs/00`–`docs/11`).
>
> **Oncora** (Oncology Reasoning Agents) is a Rust-native, on-prem, uncertainty-aware agentic
> reasoning platform for oncology drug discovery. Stable IDs: principles `P-*`, goals `G-*`,
> functional reqs `FR-*`, non-functional `NFR-*`, acceptance criteria `AC-*` — the chain
> principle → requirement → design → task → acceptance test is auditable end to end.

**Contents.** §1 Constitution · §2 Specification · §3 Plan · §4 Technology research · §5 Data model ·
§6 Contracts · §7 Tasks (roadmap) · §8 Quickstart & validation.

---

# 1. Constitution — non-negotiable principles

A design violating a principle is rejected regardless of other merits. Amendment only via an ADR
(`docs/adr/NNNN-*.md`, context→decision→consequences) that explicitly supersedes the prior text; the
locked tech table (§4) is the rolled-up summary.

| ID | Principle | Rationale | Enforcement |
|---|---|---|---|
| **P-1** | **Rust-native end to end.** Non-Rust deps isolated behind a named Rust trait + justified. Exactly **three** sanctioned non-Rust surfaces (no more without an ADR): (1) model endpoints (HTTP: on-prem vLLM/TGI, opt-in cloud) behind `ModelProvider`; (2) ONNX Runtime (`ort`, C++; `fastembed` uses it; `duckdb`'s embedded engine is a similar contained native dep) behind `EmbeddingProvider`/`Calibrator`/query interface; (3) third-party ontologies (external *data*: GO, Reactome, ChEMBL, UMLS, MONDO, HGNC) behind `GraphStore` + snapshot pinning. | One language/type system keeps agent loop, memory, uncertainty coherent; isolating native surfaces contains blast radius + supply-chain risk. | Trait boundaries in `oncora-core`; `cargo deny`; `noodles`/`dicom-rs` (though pure Rust) additionally behind the `ToolHost` MCP boundary. |
| **P-2** | **On-prem/privacy by default; PHI & IP never cross the trust boundary (the VPC).** Only sanctioned outbound path = explicit, deny-by-default, allow-listed, audited **egress proxy** for opt-in cloud models; nothing else dials out; a cloud touch is recorded in the model pin. | Outputs feed go/no-go, manuscripts, submissions; data governed by privacy/IP boundaries. | Network policy (data tier not internet-routable); `oncora-api` auth; egress proxy PHI-stripping; span hygiene. |
| **P-3** | **No writes to source data.** Sources/snapshots are read-only; all derived artifacts live in Oncora-owned content-addressed stores. | Source integrity + reproducibility; Oncora is a reasoning layer, not a system of record. | `oncora-ingest` has no source write path; snapshots immutable. |
| **P-4** | **Provenance on every claim.** Every assertion carries `Provenance { sources, tool_calls, model, snapshot }`. A claim without provenance is a **type error**, not a style lapse; ungrounded prose never reaches the responder. | Reviewer must trace any sentence to sources + exact tool calls. | `Provenance` required on `Evidence` + every memory write; recorded to Postgres ledger + episodic memory. |
| **P-5** | **Deterministic, audited tool calls.** Every domain capability is an MCP tool, executed deterministically through the single `oncora-mcp-host` chokepoint, recorded (inputs, outputs, model pin, snapshot, `ToolCallId`, latency, verdict) to **both** episodic memory + ledger. Allow **and deny** are audit events. No bypass path; agent receives a result only **after** the write (record-before-return). | Auditability + replay; calculators/KG are deterministic oracles. | `oncora-mcp-host` policy gate + `tower` timeout + audit sinks. |
| **P-6** | **Reproducibility.** Every run pins `ModelPin` (weights+decode config) + `SnapshotId`; content-addresses every artifact (**BLAKE3**); routes all randomness through recorded seeds. Given `(snapshot, model pin, content hashes, seeds)` → same outputs **bit-for-bit**; any run replayable via `oncora-cli`. | Reproducible enough to publish + re-derive any benchmark. | `oncora-artifacts` CAS; pinned toolchain + committed `Cargo.lock`; `insta`; replay tests in CI. |
| **P-7** | **Typed, calibrated uncertainty; the system may decline.** `Confidence(f64)` + `CalibrationMethod` tag flows end to end; `Confidence(0.9)` must mean ~90% empirical accuracy (ECE-measured, gate-enforced). Terminal act = `Verdict`; **`Abstain`/`Escalate` are first-class, ranked above a low-confidence `Accept`**. Raw scores rejected by the scorer. | A confident wrong answer is worse than no answer in a clinical-adjacent system. | `oncora-uncertainty` scorer is the only `Accept` emitter; ECE-gated in CI. |
| **P-8** | **Reliability is a tested, gated property.** Any "faster/more-accurate/more-reliable than experts & baselines" claim must be (a) on a versioned expert-labeled golden set, (b) vs ≥1 named computational baseline **and** a human-expert reference on a fair footing, (c) measured for calibration + abstention quality (not just accuracy), (d) reproducible bit-for-bit by a third party from a manifest. Regressions block merges. | Publish-grade credibility; agent-vs-self is not a benchmark. | `oncora-eval` CI gate (accuracy, ECE, AURC, latency, baseline dominance, replay). |
| **P-9** | **Provider swappability via trait boundaries.** Every swappable capability is an object-safe trait in/near `oncora-core`; concrete backends are the only place a 3rd-party/non-Rust dep appears; deps point **inward**; the internal crate graph is a strict **DAG, no cycles**. Swapping a backend = one impl + rebind a trait object at composition time — **zero change to agent runtime or API**. Nine seams: `ModelProvider`, `EmbeddingProvider`, `VectorStore`, `GraphStore`, `MemoryStore`, `ToolHost`, `Calibrator`, `Verifier`, `ArtifactStore` (+ `LedgerStore`). | Adopt young, fast-moving crates as primary without betting the platform. "Trust the trait boundary, not the crate version." | `oncora-core` trait defs; review gate on abstraction leaks; Cargo rejects cyclic deps. |
| **P-10** | **Honest knowledge: never silently overwrite; contradictions retained.** Conflicting facts kept as competing evidence; "current belief" is a computed view over edges each carrying own confidence + provenance. Forgetting = visibility mgmt (soft-delete + tombstone), never deletion; **episodic history + provenance never decayed/deleted**. | Honest uncertainty over false resolution; reproducibility of past conclusions via time-travel. | `oncora-memory` conflict policy; `cozo` time-travel + per-edge confidence. |

**Scope guardrails (constitutional non-goals).** NOT: a general-purpose chatbot/open-domain assistant; a
cloud SaaS with default external calls; a wet-lab/LIMS/ELN replacement; a source-of-truth datastore for
primary research data (reads snapshots, never writes back); a system that guesses to be helpful (silence is a
valid, logged outcome); a polyglot microservice zoo.

---

# 2. Specification — what & why

## 2.1 Problem

Oncology drug discovery is a **multimodal evidence-integration problem under deep uncertainty**. A single
decision (is this target druggable in this indication; does this variant confer resistance to this compound;
is this trial design supported by mechanistic evidence) depends on fusing peer-reviewed + preprint
**literature**, structured **multi-omics**, curated **bio-ontologies/pathway graphs**, and **DICOM imaging** —
each with its own schema, vocabulary, noise model, access constraints. Data is heterogeneous, versioned,
frequently contradictory, privacy/IP-bounded. A generic RAG chatbot fails on every axis: top-k text lookup
ignoring structured omics/graph/imaging; fluent prose with no per-claim provenance; no calibrated uncertainty
or principled abstention; non-reproducible against a drifting index + moving model; leaks data to third
parties. Unacceptable when output feeds a go/no-go on a multi-year, multi-million-dollar program, a manuscript,
or a regulatory submission. **Oncora is the opinionated alternative: a typed reasoning system where
uncertainty, provenance, and reproducibility are first-class values in the data model.**

## 2.2 Goals

- **G-1** Fuse the four modalities into one reasoning substrate, not four silos.
- **G-2** Machine-checkable provenance on **every** claim (sources, tool calls, model pin, snapshot).
- **G-3** Uncertainty as a typed, calibrated, first-class value driving accept/abstain/escalate.
- **G-4** Reproducible enough to publish (pinned models/snapshots, content-addressed artifacts, deterministic replay).
- **G-5** Persist + reuse knowledge across sessions via versioned agent memory.
- **G-6** Entirely on-prem/in-VPC by default; PHI & IP never cross the boundary.
- **G-7** Beat human experts + computational baselines on speed/accuracy/reliability, proven by a CI-gating harness.
- **G-8** Rust-native end to end, all domain tools via MCP.

## 2.3 Non-goals — per the §1 scope guardrails.

## 2.4 Actors

| Actor | Role | Trust | Entry |
|---|---|---|---|
| **Scientist** | Initiates workflows; consumes evidence-bearing answers; receives abstentions/escalations | Authenticated, inside boundary | `oncora-api` (HTTP/gRPC) |
| **Reviewer/Approver** | Adjudicates escalated decisions; approves semantic-memory promotions | Authenticated, inside boundary | `oncora-api` review queue |
| **Automated pipeline** | Scheduled/CI runs: ingestion, re-indexing, benchmark gating, batch hypotheses | Service identity, inside boundary | `oncora-api` (gRPC) / `oncora-cli` |
| **Operator** | Deploys/configures/monitors; manages snapshots + upgrades | Service/admin identity | `oncora-cli`, ops tooling |

## 2.5 Four pillars

1. **Multimodal reasoning** — text/omics/KG/imaging as one evidence space; per-modality specialist agents; hybrid retrieval fusing vector+graph+recency under a token budget. Cross-modal evidence assembly, not four chatbots.
2. **Agent memory architecture** — five memory types, disciplined write/read paths, keyed by `(scientist, project, workflow)`, across multi-session workflows.
3. **Robustness & uncertainty** — typed calibrated confidence; decomposed uncertainty; grounding vs deterministic oracles + citation entailment; conformal set-valued outputs with abstention guarantee; explicit accept/abstain/escalate.
4. **Reliability & benchmarking** — faster/more-accurate/more-reliable than experts + baselines, proven reproducibly by a CI-gating harness.

## 2.6 Workflows & user stories

| Workflow | Case unit | Expected output |
|---|---|---|
| **Target discovery** | `(target, disease)` druggability/association question | `Evidence`-bearing answer + calibrated `Confidence` + citations resolvable to snapshot + audit trail + `Verdict` |
| **Translational research** | `(variant, compound, context)` sensitivity/resistance or mechanism question | mechanism-aware interpretation grounded in guideline/curation tiers |
| **Trial design / matching** | `(cohort/patient profile, trial)` eligibility/match, or design-soundness question | eligibility adjudication / design review with rationale |

Stories: *Scientist* asks each workflow and gets a cited, confidence-scored, defensible answer. *Reviewer*
traces any returned sentence → `Evidence` → `ToolCallId`s → exact snapshot lines to adjudicate an escalation.
*Scientist returning weeks later* sees accumulated, decayed-and-consolidated memory, not a cold start.
*Operator/auditor* replays any historical run from its manifest bit-for-bit. *External reviewer* re-derives any
paper figure with one command from the manifest + pinned snapshots.

## 2.7 Functional requirements

**Ingestion & knowledge (FR-ING).**
- **FR-ING-1** Ingest 4 modalities via shared 5-stage contract **source→normalize→embed→index→graph**:
  literature (PubMed abstracts/open full-text, bioRxiv/medRxiv preprints, internal reports), omics (VCF/BCF +
  tabular Parquet: expression, mutation, CNV), imaging (DICOM), ontologies (GO, Reactome, ChEMBL, UMLS, MONDO, HGNC).
- **FR-ING-2** Pull every source into an **immutable content-addressed snapshot** (`SnapshotId`, BLAKE3); a new pull = new id (P-3,P-6).
- **FR-ING-3** Attach `SourceRef`+`SnapshotId`+producing `ModelPin` to every derived record (chunk, vector, triple, evidence edge); vectors never travel without metadata (P-4).
- **FR-ING-4** Literature: strip markup; section-aware semantic chunking + overlap; extract sentence-level claim candidates; resolve recognized entities to canonical URIs.
- **FR-ING-5** Omics: parse VCF → typed `Variant` (locus, ref/alt, consequence, gene) behind VCF MCP server; validate tabular omics vs schema; normalize to HGNC symbols + canonical loci; query analytically in place — agents never touch raw files.
- **FR-ING-6** Imaging: parse DICOM behind imaging MCP server; **de-identify at the MCP boundary**; pixel data + PHI stay on-prem; only de-identified features/embeddings indexed.
- **FR-ING-7** Ontologies: load as triples queryable by SPARQL; the **identity authority** — every entity elsewhere resolves to a URI here; pin each release as a snapshot; a version bump = re-index event.
- **FR-ING-8** Embeddings **on-prem, pinned, versioned**; dimension fixed at collection creation; mixing models/dims forbidden; a model change = new pin forcing full re-index, never a silent mix.

**Knowledge graph & retrieval (FR-KG, FR-RET).**
- **FR-KG-1** Dual KG: ontology/canonical-entity layer (RDF/SPARQL, stable URIs) + evidence/assertion graph (Datalog, time-travel, **per-edge confidence + provenance**); one query surface — callers express **intent, not store choice**.
- **FR-KG-2** Support the canonical schema (entities/edges in §5.3).
- **FR-KG-3** Point-in-time queries: "what did we believe as of snapshot X" is first-class (P-10,P-6).
- **FR-RET-1** Hybrid retrieval: plan query into needed substores; fan out in parallel (text vectors, multimodal vectors, SPARQL ontology, Datalog evidence); fuse heterogeneous scores (reciprocal-rank-fusion default; weighted optional per task class); re-rank with a pinned cross-encoder; carry provenance through to the answer.
- **FR-RET-2** Degrade gracefully: per-store timeouts; a slow/empty store degrades, never blocks.
- **FR-RET-3** Assemble context under the model **token budget**: dedup, order by fused score, truncate/summarize to fit, keep provenance handles, **surface competing evidence together**.
- **FR-RET-4** The semantic-memory read path **is** this hybrid layer with a memory-scoped filter — one retrieval layer, not one per subsystem.

**Agent runtime (FR-AGT).**
- **FR-AGT-1** Fixed pipeline **Planner → Domain Specialists (genomics, literature, imaging, clinical) → Verifier → Uncertainty Scorer → Responder**; memory read at *perceive*, written at *consolidate*.
- **FR-AGT-2** Agent loop *perceive→plan→act(MCP tool calls)→observe→reflect→score→decide→consolidate*; terminates in exactly one `Verdict`.
- **FR-AGT-3** Route every domain tool call through the single MCP host chokepoint; execute deterministically; record to episodic memory + ledger (allow + deny) (P-5).
- **FR-AGT-4** Specialists run concurrently under a shared concurrency limit; a cancelled/timed-out run drops in-flight tool calls + records them as such.
- **FR-AGT-5** Bound any `Escalate → re-reason` loop by a per-run cancellation token + max-iteration cap; never an open loop.

**Memory (FR-MEM).**
- **FR-MEM-1** Five memory types — working, episodic, semantic, procedural, provenance/evidence (§5.2).
- **FR-MEM-2** Key cross-session by `(scientist, project, workflow)` — the retrieval scope + provenance attribution subject; cross-tenant reads denied.
- **FR-MEM-3** Write path **extract→dedup→conflict-resolve→consolidate→decay**: *extract* typed `(claim, support, contradiction, confidence, provenance)` candidates (un-typeable → episodic-only); *dedup* two-stage — embedding near-dup proposes, **KG entity resolution is the authoritative tie-breaker** (same resolved entities+predicate ⇒ same assertion); *conflict-resolve* never overwrite (inputs: recency, source authority, calibrated confidence; contradictions retained; current belief is a computed view); *consolidate* staged working→episodic (unconditional, immutable)→semantic (only if resolves to a KG entity, survives dedup, clears confidence/verdict bar); procedural promotion for plans/tool-sequences from high-confidence `Accept` runs; *decay* score (time-since-use + frequency); below floor ⇒ soft-delete + tombstone, excluded from default retrieval, revivable (P-10).
- **FR-MEM-4** **Episodic history + provenance never decayed/deleted** — the replay substrate.
- **FR-MEM-5** Every write stamps `SnapshotId`+`ModelPin`+BLAKE3 content hash of the CAS payload (P-6).
- **FR-MEM-6** Read path: hybrid retrieve → fuse (incl. calibrated-confidence weighting; low confidence down-weighted not hidden) → filter tombstoned (unless point-in-time) → assemble under token budget with provenance handles.

**Uncertainty & verification (FR-UNC).**
- **FR-UNC-1** Every claim carries `Confidence` + `Provenance`; missing either ⇒ cannot reach the responder (P-4,P-7).
- **FR-UNC-2** Decompose uncertainty into **aleatoric/epistemic/retrieval/tool**, each estimated distinctly; policy routes on the dominant source.
- **FR-UNC-3** Estimator suite, cheap-first: self-consistency (N samples per task class: trivial=1, high-stakes=5–9), multi-model/multi-prompt ensembles, **deterministic oracle grounding** (calculators + KG; oracle mismatch can force abstain regardless of model confidence), conformal prediction (set-valued, distribution-free coverage ≥ 1−α), ECE measurement, citation-grounding/NLI entailment (every asserted fact must be entailed by a retrieved pinned source; all-neutral ⇒ ungrounded ⇒ abstain).
- **FR-UNC-4** Verifier set per claim: **citation, oracle, consistency, contradiction, schema/unit**. Verifiers adjudicate, never generate.
- **FR-UNC-5** The **uncertainty scorer is the single point** fusing verifier outputs; applies the per-task-class calibrator → final `Confidence`+`CalibrationMethod`; **only component allowed to emit `Accept`**; refuses to `Accept` a `Raw` confidence (P-7).
- **FR-UNC-6** Post-hoc calibration behind a trait (temperature scaling, isotonic regression, conformal); fit per task class on held-out `(raw_score, correct?)` pairs from the eval harness; calibrators content-addressed + tied to `ModelPin`+`SnapshotId`.
- **FR-UNC-7** Recalibrate on any `ModelPin` change (mandatory before promotion), any `SnapshotId` change affecting a task class, on measured-ECE drift over rolling shadow evals, and **quarterly minimum**.
- **FR-UNC-8** Decide a `Verdict` by a **pure function of typed inputs**, configurable per task class (defaults conservative; high-stakes bias toward escalation); decision table in §5.6.
- **FR-UNC-9** Aggregate conservatively: a response is only as confident as its **weakest load-bearing claim** (min-confidence over planner-identified load-bearing claims). Accepted claims render with `Confidence` + citation trail; abstained/escalated surfaced explicitly with reason.

**Evaluation & benchmarking (FR-EVAL).**
- **FR-EVAL-1** Score against **two references always**: a human-expert reference + ≥1 named computational baseline; baselines run through the **same harness, snapshots, scorer** as the agent (P-8).
- **FR-EVAL-2** Versioned golden set per workflow: candidate cases only from pinned snapshots; ≥2 independent expert labels `{decision, rationale, cited evidence, confidence}`; senior adjudication; recorded inter-annotator agreement (κ); strata **normal/hard-ambiguous/abstain-expected** + wrong-but-plausible distractors; BLAKE3-hashed CAS artifact; **dev/test/sealed-holdout** splits with membership baked into the content-addressed snapshot.
- **FR-EVAL-3** Fixed metric panel per workflow: accuracy/F1, task-specific score, **ECE + reliability diagram**, **coverage-vs-risk + selective accuracy + AURC**, abstention precision/recall, latency p50/p95, cost (tokens, compute). Calibration scored on **calibrated** confidence, never raw logits. Abstention quality is a metric, not an escape hatch.
- **FR-EVAL-4** Deterministic replay from a `RunManifest` (§5.5); refuse unpinned model / non-content-addressed dataset; record every run (inputs, raw model-I/O fixtures, per-case results, scores, report) to CAS + ledger.
- **FR-EVAL-5** Gate CI (block merge) on: accuracy floor, calibration floor/ceiling, abstention floor, baseline dominance, latency, reproducibility (fixture-replay reproduces prior score within tolerance). Default CI replays from **recorded content-addressed fixtures** (deterministic, no GPU); live-model runs scheduled/nightly with tolerance bands.
- **FR-EVAL-6** One-command replay (`oncora-cli eval replay --manifest <blake3>`) + figure regen; every number back-points `report → ExperimentId → RunManifest digest → per-case RunResult digests → recorded fixtures`.

**API, CLI & governance (FR-API, FR-CLI, FR-GOV).**
- **FR-API-1** `oncora-api` is the **sole entry point** across the boundary: HTTP + gRPC, auth, RBAC, request orchestration; query endpoint, review queue, ingestion/eval triggers, health/readiness.
- **FR-CLI-1** `oncora-cli`: submit query, **deterministic replay**, ingest, eval (replay/figure) (§6.4).
- **FR-GOV-1** RBAC (Scientist·Reviewer·Pipeline·Operator) at `oncora-api` **re-checked at the MCP host policy gate**; scoped by `(project, workflow)`.
- **FR-GOV-2** Route every `Verdict::Escalate` to a human **reviewer adjudication queue**; semantic-memory promotions + escalations require reviewer approval.
- **FR-GOV-3** Append-only, content-addressed **provenance ledger** supporting a lineage walk from any returned claim → sources, tool calls, model pin, snapshot.
- **FR-GOV-4** Tag PHI/PII at ingest; redact in logs/traces; **block PHI at the egress proxy** (P-2).

## 2.8 Non-functional requirements

- **NFR-RUST-1** Rust-native; exactly three sanctioned non-Rust surfaces each behind a named trait (P-1,P-9).
- **NFR-PRIV-1** On-prem/VPC-only by default; egress proxy is the only outbound path — deny-by-default, allow-listed, PHI-stripping, fully logged; cloud use stamped into the model pin (P-2).
- **NFR-REPRO-1** Deterministic replay: pinned toolchain, committed `Cargo.lock`, single resolved version per shared dep, BLAKE3 content-addressing, recorded seeds, stable recordable model identity (build + weights digest, not a friendly name), `insta` locks (P-6).
- **NFR-PROV-1** Provenance on every claim, memory item, and benchmark number — no exemptions (P-4).
- **NFR-AUDIT-1** Record-before-return at the MCP host; allow + deny recorded; audit trail itself replayable bit-for-bit (P-5).
- **NFR-CALIB-1** Post-calibration ECE ≤ **0.05** absolute; no ECE regression > **0.01** vs last green main; conformal empirical coverage ≥ **1−α** (P-7,P-8).
- **NFR-ACC-1** Task accuracy/F1 must not drop > **1.0 pp** vs last green main and must beat the strongest baseline arm on `test` (P-8).
- **NFR-ABST-1** AURC must not regress > **0.01** vs main; abstain-recall on the abstain-expected stratum stays above floor (P-8).
- **NFR-PERF-1** Per-run structured concurrency: per-run cancellation token; per-tool/model timeouts; bounded channels (backpressure); semaphores capping concurrent model + per-server tool calls (protect GPUs + oracles); child tasks owned by the run. Hard ceiling = **GPU token throughput**; uncertainty-driven abstention is also a cost control.
- **NFR-PERF-2** Latency p95 within a per-task-class budget; hot paths (retrieval fusion, scorer, manifest hashing) guarded by microbenchmarks with regression detection. End-to-end is **LLM-bound**; data plane is single-digit-to-low-tens of ms.
- **NFR-SCALE-1** One codebase, two deployment shapes (single-node embedded; multi-node service-backed) — the difference is **configuration, not code**; agent + retrieval tiers stateless, scale by replicas; durable state in the data tier.
- **NFR-ROBUST-1** Property tests on invariants (memory, uncertainty/confidence math, parsers); fuzzing of every external-input parser (VCF, DICOM, JSON tool args); snapshot tests on prompts/traces/manifests; microbenchmarks; differential testing vs oracles/baselines.
- **NFR-OBS-1** Structured spans + OpenTelemetry, on-prem collector only; **no PHI in span fields**; runs correlated end-to-end by run id.
- **NFR-GXP-1** Built to the spirit of **ALCOA+** (Attributable, Legible, Contemporaneous, Original, Accurate, + Reproducible + Auditable); a concrete trail for any answer influencing a go/no-go or submission. Not a validated GxP system out of the box.
- **NFR-QUAL-1** PR gates: `cargo fmt --check`; `cargo clippy --all-targets --all-features -D warnings`; `cargo deny check`; `cargo test --workspace` + `cargo llvm-cov` floor on core/uncertainty/memory; property tests + scheduled fuzzing; `criterion` regression tracking; the `oncora-eval` golden-set gate.
- **NFR-DOC-1** Rustdoc on every public item (`#![deny(missing_docs)]` on libs); runnable doc-tests for provider seams; ADRs for significant decisions.

## 2.9 Edge cases (required behaviours)

| Situation | Required behaviour |
|---|---|
| No entailing source (NLI all-neutral) | `Abstain { UngroundedClaim }` — never assert ungrounded |
| Oracle (calculator/KG) disagrees on a verifiable sub-claim | `Abstain`/`Escalate`; **never override a calculator/KG with prose** |
| Conformal set > task-class max | `Abstain { ConformalSetTooLarge }`; return the set, do not pick |
| Confidence below abstain-floor, aleatoric-dominant | `Abstain { LowCalibratedConfidence }` — more work won't help |
| Confidence in escalate-band, epistemic-dominant | `Escalate { SpecialistAgent }` — more targeted evidence may resolve it |
| Confidence below abstain-floor, high-stakes class | `Escalate { HumanReviewer }`; log full evidence bundle |
| Retrieval coverage below floor | `Abstain { InsufficientRetrieval }`; flag for ingestion backfill |
| New claim contradicts existing | Retain both edges; current-belief view per recency+authority+confidence; weak evidence cannot silently flip a belief |
| Entity resolution ambiguous | Do not merge; keep separate; queue for resolution; escalate if needed |
| Existing fact superseded by trial readout | Add new time-stamped edge; old remains queryable via time-travel |
| Entry below decay floor | Tombstone; exclude from default read; **keep provenance** |
| Run cancelled/times out | Propagate cancellation token; drop in-flight tool calls; record them as dropped in episodic memory |
| Numeric claim without units / out of physiological range | Schema/unit verifier hard-rejects before scoring |
| Ontology version bump | Re-index event; old evidence stays interpretable against the ontology it was asserted under (time-travel) |
| Embedding model change | New pin; full re-index; never a silent mix of dims/models |

## 2.10 Acceptance criteria (normative)

- **AC-1 (end-to-end)** A target-discovery query returns an `Evidence`-bearing answer + calibrated `Confidence` + citations resolvable to snapshot + full audit trail, **or** a logged abstention with reason (FR-AGT-*, FR-UNC-*).
- **AC-2 (audit)** Every MCP tool call (allow + deny) recorded to **both** ledger + episodic memory before its result reaches the agent (P-5, FR-AGT-3, NFR-AUDIT-1).
- **AC-3 (replay)** `oncora-cli replay` reproduces a run **bit-for-bit** from pinned model + snapshot + CAS (P-6, FR-CLI-1).
- **AC-4 (gate)** The golden set runs in CI and **blocks merge** on any tripped gate (accuracy, calibration, abstention, baseline dominance, latency, reproducibility) (P-8, FR-EVAL-5).
- **AC-5 (cross-modal)** A query requiring literature **and** omics **and** imaging returns one fused, cited answer; each modality content-addressed + snapshot-tagged; no source written to (FR-ING-*, FR-RET-1).
- **AC-6 (cross-session memory)** A second session on the same `(scientist, project, workflow)` demonstrably reuses prior semantic + procedural memory; conflicting facts retained as competing evidence; every entry carries snapshot + model pin and is replayable (FR-MEM-*).
- **AC-7 (calibrated uncertainty)** Confidence calibrated (ECE below target on held-out); conformal sets carry guaranteed coverage; oracle disagreement reliably triggers `Escalate`; every abstention logged with reason (FR-UNC-*, NFR-CALIB-1).
- **AC-8 (beats baselines)** Benchmarks beat named baselines on the agreed metrics simultaneously (accuracy, calibration, latency); results deterministically replayable; a regression cannot merge (P-8).
- **AC-9 (scaled)** Horizontal scale under load; Postgres + qdrant failover verified; upgrade validated by golden-set replay diff; rollback confirmed (NFR-SCALE-1).
- **AC-10 (governance)** An external reviewer can trace any returned claim to its sources/tool-calls/snapshot; PHI demonstrably cannot leave the boundary; a validated replay reproduces any historical run (FR-GOV-*, NFR-GXP-1).

## 2.11 Assumptions & dependencies

Foundation models are the one unavoidable non-Rust dep (behind `ModelProvider`, on-prem default, cloud opt-in
via egress proxy). In-house MCP servers for genomics (VCF), DICOM imaging, and clinical calculators are assumed
to exist; Oncora composes/orchestrates them + hosts its own (KG, retrieval, memory). Data sources are
snapshotted into the boundary; Oncora reads snapshots, never writes back. Expert labelers available for golden
sets; benchmark validity (not the harness) is the principal eval risk.

---

# 3. Plan — how

## 3.1 Architecture

Single Cargo **workspace** (`oncora`), crates `oncora-*`, clean-architecture DAG, deps **inward toward
`oncora-core`** (P-9). `oncora-core` holds the typed vocabulary (`Confidence`, `Provenance`, `Evidence`,
`Verdict`, ids, errors) + nine provider trait defs; every concrete backend (thus every 3rd-party/non-Rust dep)
is an isolated impl. **One codebase, two deployment shapes** (NFR-SCALE-1) — configuration, not code.

**Trust boundary** = the VPC (P-2): all services, stores, on-prem model server, internal MCP servers inside;
only sanctioned outbound = the deny-by-default audited egress proxy. Hard rules: P-3, P-4, P-5, P-6.

**C4 container edges** (node → its dependencies):
`Scientist/Reviewer/Pipeline → oncora-api`. Inside the VPC:
`oncora-api → oncora-agents`; `oncora-agents → oncora-memory, oncora-retrieval, oncora-kg, oncora-mcp-host,
oncora-uncertainty`; `oncora-uncertainty → oncora-mcp-host`; `oncora-eval → oncora-agents`;
`oncora-retrieval → qdrant, lancedb, oncora-kg`; `oncora-kg → oxigraph, cozo`;
`oncora-memory → redb, oncora-kg, oncora-retrieval, oncora-artifacts, Postgres`;
`oncora-ingest → oncora-kg, oncora-retrieval, oncora-artifacts`; `oncora-artifacts → object store`;
`oncora-mcp-host → VCF(noodles), DICOM(dicom-rs), calculator oracles, internal tool servers`;
`oncora-agents/oncora-retrieval → on-prem model server`; `oncora-agents/oncora-api → oncora-telemetry`;
`Snapshotted sources → oncora-ingest`; `oncora-agents -. egress proxy .-> cloud model endpoints` (only
boundary-crossing arrow). `oncora-core` is the shared type/trait substrate every crate links (omitted as a node).

## 3.2 Crates

| Crate | Responsibility | Internal deps | Implements |
|---|---|---|---|
| `oncora-core` | Typed vocabulary + nine provider trait defs; ids; `thiserror` errors. No internal deps. | — | substrate |
| `oncora-telemetry` | Structured `tracing` + OpenTelemetry; span helpers. | — | NFR-OBS-1 |
| `oncora-artifacts` | CAS: BLAKE3 hashing, `serde`/CBOR manifests, object-store/FS backend. Replay backbone. | core | FR-ING-2/3, P-6 |
| `oncora-mcp-host` | `rmcp` host/client; registry; deterministic + audited dispatch; record-before-return. | core, telemetry | FR-AGT-3, P-5 |
| `oncora-kg` | Dual KG: `oxigraph` ontology + `cozo` evidence graph; schema; SPARQL/Datalog. | core | FR-KG-* |
| `oncora-retrieval` | Hybrid retrieval (vector+graph+recency fusion); embeddings behind `EmbeddingProvider`. | core, kg | FR-RET-* |
| `oncora-ingest` | `swiftide` pipelines (literature/omics/imaging/KG); snapshotting into CAS; never writes to source. | core, kg, retrieval, artifacts | FR-ING-* |
| `oncora-memory` | Five memory types + write/read paths; cross-session persistence; `LedgerStore`. | core, kg, retrieval, artifacts | FR-MEM-* |
| `oncora-uncertainty` | Calibration (ECE), conformal, verifiers, oracle grounding, abstention/escalation policy. | core, mcp-host | FR-UNC-* |
| `oncora-agents` | `rig`-backed planner/specialists/verifier/scorer/responder; the loop; orchestration concurrency + MCP routing. | core, memory, retrieval, kg, mcp-host, uncertainty | FR-AGT-* |
| `oncora-eval` | Benchmark harness, golden sets, metrics, CI gating. Legitimately depends on "all". | core, agents + all | FR-EVAL-* |
| `oncora-api` | `axum` (HTTP) + `tonic` (gRPC); auth; RBAC; sole entry point. | agents, memory, retrieval, kg | FR-API-1, FR-GOV-1 |
| `oncora-cli` | Operator/developer CLI incl. deterministic replay. | api / agents | FR-CLI-1 |

**Dependency graph edge-list** (`A → B` = A depends on B): `cli → api, agents`; `api → agents, memory,
retrieval, kg`; `eval → agents, uncertainty, memory, ingest`; `agents → memory, retrieval, kg, mcp, uncertainty`;
`uncertainty → mcp`; `memory → kg, retrieval, artifacts`; `ingest → kg, retrieval, artifacts`;
`retrieval → kg`; `mcp → telemetry`; and `{artifacts, retrieval, kg, memory, ingest, uncertainty, agents, eval,
mcp, telemetry} → core`. Acyclic; `core` is the leaf; `eval` + binaries at the top.

**Pillar/requirement → crate.** Multimodal: retrieval, kg, ingest, agents. Memory: memory, kg, artifacts.
Robustness & uncertainty: uncertainty, core types, mcp-host oracles. Reliability & benchmarking: eval,
telemetry. MCP for all tools: mcp-host. Provenance on every claim: core `Provenance`, artifacts, memory ledger.
Reproducibility/replay: artifacts, cli replay, pinned toolchain + `Cargo.lock`. Typed calibrated confidence:
core `Confidence`/`Verdict`, uncertainty. On-prem/privacy/RBAC: api auth+RBAC, on-prem provider impls. Observability:
telemetry. Provider swappability: core traits + per-backend impls.

## 3.3 Agent runtime

**Topology** (FR-AGT-1): `Planner → {Genomics, Literature, Imaging, Clinical} specialists → Verifier →
Uncertainty Scorer → Responder`; specialists → MCP tools (VCF, DICOM, calculators); Planner/Specialists/Verifier
↔ memory (read/write); Scorer/Responder → memory (write); Scorer -. abstain / escalate-to-reviewer .-> Responder.

**Agent loop** (FR-AGT-2; memory read at perceive, written at consolidate): `Perceive →(retrieve + memory read)
Plan → Act →(MCP tool calls) Observe → Reflect →(verify claims) Score →(calibrated confidence) Decide`;
`Decide → Accept (within thresholds) | Abstain (low confidence or wide conformal set) | Escalate (oracle
disagreement or policy trigger) | Plan (replan if recoverable)`; `Accept/Abstain/Escalate → Consolidate →(memory
write) end`.

**Concurrency** (`tokio`, structured per run):

| Concern | Mechanism | Rationale |
|---|---|---|
| Cancellation | per-run `CancellationToken` | one token cancels the whole run subtree on timeout/abort/disconnect |
| Tool/model timeouts | `tower` timeout layers | uniform per-tool deadlines; no unbounded waits |
| Backpressure | bounded `tokio::mpsc` | producers block rather than balloon memory |
| Concurrency limits | `tokio::sync::Semaphore` | cap concurrent model + per-server tool calls (protect GPUs + oracles) |
| Structured scope | task tracker / `JoinSet` per run | child tasks owned by the run; nothing outlives its parent |

Specialists run concurrently under a shared semaphore; planner fans out then joins; a cancelled run propagates
the token so in-flight tool calls are dropped + recorded in episodic memory.

**MCP routing** (P-5): `oncora-mcp-host` is the single chokepoint — holds the `rmcp` registry, resolves a tool to
its server (VCF/`noodles`, DICOM/`dicom-rs`, calculators, internal), enforces a `tower` timeout, executes
deterministically, records to episodic memory + ledger with a `ToolCallId`. Inputs validated + fuzz-tested
(`cargo-fuzz`). Calculators + KG are deterministic oracles the verifier checks against.

**End-to-end target-discovery sequence** (numbered, with alt branches): 1 Scientist→Planner: query. 2
Planner→Memory: read prior context → working+episodic+semantic recall. 3 Planner→Retrieval: hybrid retrieve. 4
Retrieval→KG: SPARQL+Datalog → ontology+evidence subgraph → fused evidence set to Planner. 5 Planner→Tools: VCF
variant lookup / imaging features / calculator oracle → typed deterministic records. 6 Planner→Verifier: claims +
provenance. 7 Verifier→KG: oracle grounding check; Verifier→Retrieval: citation NLI entailment → verified claims +
contradictions to Scorer. 8 Scorer: calibrate + conformal set. **alt** within thresholds → Accept → consolidate
semantic+procedural → answer with evidence+confidence; **else** low confidence/wide set → Abstain(reason) → log →
abstain with rationale; **else** oracle disagreement → Escalate to reviewer → log → escalated to human review.

## 3.4 Data-flow narrative

(1) **Knowledge in** — snapshots → `oncora-ingest` (`swiftide`) → KG (`oxigraph` canonical + `cozo` evidence) +
vector indexes (`qdrant` text, `lancedb` multimodal) + CAS payloads; never mutates source; each item
content-addressed + snapshot-tagged. (2) **Context out** — at perceive, `oncora-retrieval` fuses vector + graph +
recency/usage under a token budget; `oncora-memory` overlays cross-session recall. (3) **Reasoning + grounding** —
`oncora-agents` plans/dispatches specialists → tools via `oncora-mcp-host` → deterministic oracles + parsers
(`noodles`, `dicom-rs`); verifier grounds claims; `oncora-uncertainty` calibrates, computes conformal sets, emits
a `Verdict`. (4) **Audit + provenance** — every step traced (`oncora-telemetry`), logged to episodic memory (`redb`
+ CAS), attributed in the Postgres ledger; every returned claim carries `Provenance`; any run replayable.

## 3.5 Deployment (NFR-SCALE-1)

**Dev — single node, embedded** (in-process/embedded; no external services, GPU, or egress):

| Concern | Dev choice |
|---|---|
| Process | single all-in-one binary |
| Relational + provenance | SQLite via `sqlx` (pure-Rust `turso` chosen embedded ledger) |
| Text vectors | embedded `hnsw_rs` / `instant-distance` |
| Multimodal vectors | `lancedb` local files |
| KG | `oxigraph` + `cozo` embedded (cozo deferred — §4.6) |
| Working/episodic | `redb` file |
| CAS | local FS object store (BLAKE3) |
| Model | local `mistral.rs` / `candle` |
| Telemetry | `tracing` to stdout / local OTel |

**Scaled — multi-node, service-backed:**

| Concern | Scaled choice |
|---|---|
| Stateless agent tier | N replicas of `oncora-api` + `oncora-agents` behind LB |
| Retrieval tier | horizontally scaled `oncora-retrieval` workers |
| Text vectors | qdrant **cluster** (sharded + replicated) |
| Multimodal vectors | `lancedb` over shared object store |
| Relational + provenance | Postgres **HA** (primary + replicas) via `sqlx` |
| KG | `oxigraph` service + `cozo` service |
| Working/episodic | `redb` per-worker + Postgres rollup; FoundationDB optional at very large scale |
| CAS | object store (S3-compatible, on-prem) |
| Model serving | on-prem GPU vLLM/TGI OpenAI-compatible behind `ModelProvider` |
| Cloud model | opt-in only, via egress proxy |
| Telemetry | OTel collector → trace/metric backend |

Only boundary-crossing arrow = the audited egress proxy.

**Model-endpoint modes:** Dev local (`mistral.rs`/`candle`, in-process, workstation/CI); On-prem served
(vLLM/TGI OpenAI-compatible via `async-openai`, VPC-internal HTTP, **default production**); Cloud opt-in
(Anthropic/OpenAI-compatible, **egress proxy only**, explicit/audited/non-PHI).

**Scaling model:**

| Tier | Stateless? | Axis | Bottleneck | Mitigation |
|---|---|---|---|---|
| `oncora-api` + agents | Yes | replicas behind LB | GPU model concurrency | semaphore-bounded model calls; queue + backpressure |
| `oncora-retrieval` | Yes | replicas | embedding throughput | batched `fastembed`; cache hot embeddings |
| qdrant | No | shards + replicas | ANN over large corpora | sharding; quantization; payload pre-filter |
| Postgres | No | read replicas; vertical primary | provenance write volume | append-only ledger; partition by time |
| cozo / oxigraph | No | vertical; read replicas | time-travel query depth | snapshot pinning; query budgets |
| Model server | No | GPUs / model replicas | tokens per second | vLLM continuous batching; per-task model tiering |
| Object store CAS | No | native object-store scale | throughput | native horizontal object store |

Hard ceiling = GPU token throughput; uncertainty-driven abstention is also a cost control.

**Ops.** OCI images per binary (multi-stage); Kubernetes/Nomad in VPC; layered config validated at boot
(`figment`/`config`); secrets at runtime (Vault/KMS, short-lived); mTLS service-to-service (`tonic`+`tower`);
rolling stateless deploys; **expand-contract** migrations (`sqlx migrate`, forward-only/backward-compatible);
pinned image tags + reversible migrations for rollback. **An upgrade is validated by replaying a golden set on
the new build + diffing against the recorded baseline; rollback = re-pin + same replay + confirm empty diff** —
never a guess. Backup/DR: Postgres PITR, object-store versioning, KG snapshot export. `axum` health/readiness
probes; load-shed before OOM via `tower` concurrency-limit + `tokio` semaphores.

## 3.6 Security, privacy & governance (FR-GOV-*, NFR-PRIV-1, NFR-GXP-1)

- **Trust-boundary enforcement (layered, deny-by-default).** Stateless tier only via authenticated LB; data tier
  not internet-routable; egress proxy is the single outbound path, allow-listing specific endpoints; every caller
  authenticates at `oncora-api`; service-to-service mTLS; source snapshots read-only; model on-prem by default,
  cloud only via proxy with PHI stripped/blocked + recorded in the model pin.
- **Audit trail.** `oncora-mcp-host` validates inputs (fuzzed), resolves to the server, enforces a timeout,
  executes deterministically, records the call (inputs, outputs, model pin, snapshot, `ToolCallId`, latency,
  verdict) to **both** episodic memory + Postgres ledger; allow + deny recorded; record-before-return.
- **Data lineage.** Append-only, content-addressed provenance ledger links every memory item/claim/answer to
  sources, tool calls, model pin, snapshot; tamper-evident; full lineage walk.
- **RBAC & redaction.** RBAC at `oncora-api` re-checked at the MCP host policy gate; roles
  Scientist·Reviewer·Pipeline·Operator scoped per `(project, workflow)`; PHI tagged at ingest, redacted in
  logs/traces, blocked at egress; memory cross-tenant reads denied; reviewer gate on semantic promotions + escalations.
- **GxP-adjacent (ALCOA+).** Attributable; Legible & contemporaneous (`tracing`/OTel at action time); Original &
  accurate (content-addressed, no silent overwrite); Reproducible (pin + deterministic replay); Auditable
  (append-only ledger). Not a validated GxP system out of the box, but built to feed regulated work.

## 3.7 Engineering practices & reproducibility (NFR-QUAL-1, NFR-DOC-1, NFR-REPRO-1)

Libs use `thiserror` (typed, matchable errors so verdict routing can branch on failure class); bins
(`oncora-api`, `oncora-cli`, `xtask`) use `anyhow`; no library forces `anyhow` on callers.
`[workspace.dependencies]` pins one version per shared dep. CI gates (PR can't merge unless all pass): `cargo fmt
--all --check`; `cargo clippy --all-targets --all-features -- -D warnings`; `cargo deny check` (licenses,
advisories, banned/duplicate crates, allowed sources — where native deps are gate-kept); `cargo test --workspace`
+ `cargo llvm-cov` floor on core/uncertainty/memory; `proptest` invariants + scheduled `cargo-fuzz`; `criterion`
regression tracking; the `oncora-eval` golden-set gate. Reproducibility: pinned toolchain
(`rust-toolchain.toml`); committed `Cargo.lock`; BLAKE3-addressed models/snapshots/manifests/outputs; `oncora-cli
replay`; `insta` locks. `xtask/` hosts snapshot pinning/deny audits/codegen (`cargo xtask <task>`); `fuzz/` holds
`cargo-fuzz` targets (VCF, DICOM, tool args); ADRs in `docs/adr/NNNN-*.md`. Payoff (P-9): any backend swaps by
writing one impl + rebinding a trait object, zero change to `oncora-agents` or above.

---

# 4. Technology research & decisions

Per capability: **Primary** ("build on it now"), **Fallback** ("trait impl kept warm / documented escape hatch"),
**Rejected** ("surveyed + consciously declined"). Two governing principles: (1) everything swappable behind a
trait in `oncora-core`; (2) Rust-native, non-Rust deps isolated + justified. *Maturity caveat:* much of the Rust
AI/agent ecosystem is 0.x and moves weekly — trust the trait boundary, not the crate version.
Legend: ✅ Primary · 🔁 Fallback · ⛔ Rejected · ★ Chosen/decided · ◻ Optional/gated.

**4.1 Agent/LLM orchestration.** `rig` (rig-core) ✅ Primary — Rust-native provider-agnostic agent+tool
abstractions mapping onto planner/specialist topology; *verify* tool-calling (multi-tool, parallel) + streaming +
OpenAI-compatible custom base URL; wrap behind `oncora-agents` traits to contain 0.x breakage. `swiftide` ✅
Primary (adjacent) — streaming ingestion/RAG (different job); we own the `oncora-ingest` contract; *verify*
backpressure + transform-stage composition with our `EmbeddingProvider`/`VectorStore`. `kalosm` ⛔ Rejected —
opinionated own inference stack, competes with `candle`/`mistral.rs`+`ModelProvider`. `anchor-chain` ⛔ — small/
early, thin ecosystem. `llm-chain` ⛔ — early "LangChain for Rust", quiet trajectory. **Decision:** `rig`+`swiftide`
behind Oncora traits; fallback = in-house orchestrator on raw clients via `ModelProvider`.

**4.2 LLM clients (behind `ModelProvider`).** `async-openai` ✅ Primary — OpenAI-compatible covers on-prem
vLLM/TGI via custom base URL; mature; chat/tools/streaming; *verify* custom base URL + on-prem auth against the
exact build, response shapes match (server-side drift is the real risk). `async-anthropic`/Anthropic SDK ✅
Primary (cloud opt-in) — first-class Anthropic for the egress-proxied path; identical contract; **off by default**,
proxy-only; *verify* SDK currency vs latest Messages API, PHI redaction in front. direct `reqwest` 🔁 Fallback —
hand-rolled `ModelProvider` for endpoints neither handles; low risk, high effort. **Implemented + verified live:**
`OpenAiModel` (`async-openai` 0.40) behind `ModelProvider` under `--features openai`, `temperature = 0`; verified
end-to-end vs Ollama `qwen2.5:0.5b`: real output ("Yes, EGFR is indeed considered a driver gene in NSCLC …"),
confidence 0.855, verdict accept, citations [PMID:0001, PMID:0002]. Note: async-openai 0.40 heavily feature-gated
— `chat-completion` (pulls `_api`) + a TLS feature required; chat types under `async_openai::types::chat`.

**4.3 Local/edge inference.** `candle` ✅ Primary (tensors + small models) — pure-Rust HF tensor framework;
embedders/calibrators/classifiers; CPU+CUDA; *verify* kernel coverage + reproducible CUDA builds. `mistral.rs` ✅
Primary (local LLM) — pure-Rust quantized GGUF for dev + air-gapped; *verify* supported families/quantizations,
throughput, OpenAI-compatible surface. `ort` (ONNX Runtime) ✅ Primary (ONNX only, isolated) — mature prebuilt-ONNX
(what `fastembed` uses); **C++ bindings — the one tolerated non-Rust runtime**, behind
`EmbeddingProvider`/`Calibrator`; ONNX files pinned + content-addressed; *verify* pinned ORT version, reproducible
builds, no graph past the trait. `burn` ⛔ Rejected — training-aimed, heavier, duplicates candle.

**4.4 MCP (behind `ToolHost`).** `rmcp` (official Rust MCP SDK) ✅ Primary — covers serving tools **and**
client/host; tokio-native; *verify* transports we deploy (stdio + streamable HTTP/SSE), client host capabilities,
pin protocol version + crate. in-house MCP impl 🔁 Fallback — if `rmcp` lags a transport/capability, own the
surface behind `ToolHost`; high effort. Audit/determinism/timeout (`tower`) live in our code, not the SDK.

**4.5 Vector search & embeddings.** `qdrant` ✅ Primary (text/RAG) — Rust-native server; payload filtering; HNSW;
quantization; named vectors; clusters; *verify* client tracks deployed server. `lancedb` ✅ Primary (multimodal/
imaging) — Rust-native columnar; Lance format for multimodal embeddings + metadata + versioning; *verify* ANN
recall at scale + versioning semantics. `hnsw_rs`/`instant-distance` 🔁 Fallback (embedded/dev) — pure-Rust
in-process ANN; never the production text path. `fastembed` ✅ Primary (embeddings) — fast batched on-prem text via
`ort`; **pin + content-address** ONNX files. `candle`-hosted embedding models 🔁 Fallback — pure-Rust path to avoid
ONNX or for an architecture `fastembed` lacks. **Implemented:** `QdrantVectorStore` (official `qdrant-client`
gRPC) behind `VectorStore` under `--features qdrant`, interchangeable with `InMemoryVectorStore`; live round-trip
via `testcontainers` Docker in a dedicated `qdrant-it` CI job (not default `cargo test`); qdrant v1.12.4;
embeddings via Ollama `all-minilm` 384-d in the validation runs.

**4.6 Biomolecular KG — the dual-store decision (behind `GraphStore`).** Two different jobs no single store does
well: **ontology layer** (standards-grounded GO/Reactome/ChEMBL/UMLS/MONDO/HGNC as RDF, URI-addressed, SPARQL;
reference material, snapshot-versioned) vs **evidence/assertion graph** (Oncora's own claims/evidence/support-
contradiction edges, per-edge confidence + provenance; needs time-travel, Datalog recursion, co-located vector
search). The dual store is the *simpler* design once you accept the jobs differ; both behind one `GraphStore` trait.
`oxigraph` ✅ Primary (ontology) — mature pure-Rust RDF triplestore + SPARQL; no JVM; *verify* bulk-load perf +
SPARQL feature coverage (property paths, aggregates). `cozo` ✅ Primary (evidence) — hybrid graph+relational+vector
with **Datalog + time-travel** (the differentiator); *verify* time-travel + recursive-Datalog perf + embedded-
vector recall; keep `GraphStore` strict. `indradb` 🔁 Fallback — pure-Rust property graph; no native time-travel/
Datalog (we'd re-implement versioning). Apache Jena ⛔ Rejected — mature but **JVM** (rejected on the non-negotiable,
not capability). **Implemented (oxigraph) + cozo finding:** `OxigraphGraphStore` behind `GraphStore` under
`--features oxigraph` (pure-Rust in-memory RDF quad store; `default-features = false` keeps RocksDB out); passes
the same conformance test as `InMemoryGraphStore`, per-edge **confidence** in the quad's named-graph component
(`urn:oncora:conf:<v>`). **cozo deferred for build-integration, not capability.** Two original blockers: (1) cozo's
`minimal` feature pulls native SQLite (`storage-sqlite` → `sqlite3-src`), which `links`-clashed with the C-SQLite
ledger (`rusqlite`); (2) the only SQLite-free in-memory path (`graph-algo`) pulls `graph_builder 0.4.1`, which
doesn't compile against current `rayon`. **Choosing turso for the ledger removes (1)** (with C SQLite gated to a
benchmark-only feature, nothing else links native `sqlite3`). **(2) remains** (cozo still needs `rayon` via
`graph-algo` → `graph_builder 0.4.1`); re-evaluate when pinned/patched/fixed upstream, or run cozo in its own
process behind `GraphStore`. The seam means the swap costs nothing downstream.

**4.7 Omics & tabular analytics.** `polars` ✅ Primary (in-process dataframes) — Arrow-backed lazy/eager, pure
Rust; *verify* memory on largest matrices. `duckdb` ✅ Primary (SQL over Parquet) — embedded analytical SQL;
**embedded C++ via bindings**, self-contained behind our query interface; *verify* binding-vs-engine version +
reproducible bundled build. `arrow` ✅ Primary (interchange) — zero-copy columnar lingua franca tying polars ↔
duckdb ↔ Parquet ↔ CAS; align Arrow versions workspace-wide. `noodles` ✅ Primary (genomics IO) — pure-Rust
VCF/BCF/BAM/CRAM/FASTA/GFF/tabix; the VCF MCP server is built on it; *verify* format/codec coverage + large BAM/
CRAM perf. `rust-htslib` ⛔ Rejected — established but **C bindings**; `noodles` covers our formats in pure Rust;
reconsider only for a niche missing format behind a tool boundary. Genomics IO lives behind the VCF MCP server, so
even `noodles` is reached through `ToolHost`.

**4.8 Imaging.** `dicom-rs` ✅ Primary — pure-Rust DICOM parse/encode/IO behind the imaging MCP server (blast
radius contained); *verify* transfer-syntax/codec coverage (compressed pixel data), pixel decoding supported or
pluggable. No fallback named — a missing codec is added behind the same MCP boundary, not a C library in core.

**4.9 Agent state / memory store (behind `MemoryStore`/`LedgerStore`).** `redb` ✅ Primary (embedded KV/log) —
pure-Rust embedded ACID KV (stable 2.x); run logs + working-memory spill; *verify* episodic-append throughput +
on-disk format stability across pinned 2.x. `fjall` 🔁 Fallback (embedded KV/log) — pure-Rust LSM-tree; better
write-amplification if episodic volume outgrows redb. `sled` ⛔ Rejected — development in flux, long-promised
rewrite. `sqlx` + Postgres ✅ Primary (relational metadata + provenance ledger) — async compile-time-checked SQL;
Postgres for the multi-node ledger; *verify* migration discipline + compile-time checking in CI. **SQLite — pure
Rust (`turso`, ex-`limbo`)** ★ CHOSEN (dev relational/ledger) — Rust-native embedded ledger, no C in the build;
validated at scale: **14,469 sequential per-document writes, 0 errors, mean 2.04 ms — ~23% faster than the C
amalgamation**; default for `oncora-ledger` + bench; *verify* transactions/concurrency/SQL coverage before
depending past the dev ledger; Postgres remains the HA path. **SQLite — C library (`rusqlite`, `bundled`)** ◻
Benchmark-only (gated) — behind `--features sqlite-c` solely to reproduce the C-vs-Rust comparison; pulls native
`sqlite3`; opt-in so the default build stays pure-Rust. **C ↔ Rust SQLite** ★ Decided: Rust (turso) — both
backends in `crates/oncora-ledger` behind one `LedgerStore` trait; an identical conformance test passes for
in-memory, C SQLite, and turso; cold-start bulk comparison settled it for turso. `foundationdb` ◻ Optional
(scale-out state) — ordered transactional KV for very large multi-node memory; **ops-heavy + phase-gated**, behind
`MemoryStore`. `pgvector` ◻ Optional — vectors co-located in Postgres if ever wanted; never the primary vector path
(that's qdrant/lancedb).

**4.10 Service & transport (mature floor).** `tokio` ✅ (async runtime; pin a major version workspace-wide).
`axum` ✅ (HTTP API; `actix-web` named in canon, not needed). `tonic` ✅ (gRPC; *verify* protobuf/codegen pinned +
reproducible). `tower`+`tower-http` ✅ (middleware: timeout/retry/concurrency-limit/load-shed — where per-tool
timeouts + backpressure are enforced).

**4.11 Observability.** `tracing` ✅ (span-based; *verify* span hygiene so **PHI never lands in span fields**).
`tracing-opentelemetry` ✅ (bridge spans → OTel; *verify* version compatibility of `tracing` ↔ bridge ↔ OTel SDK —
this trio drifts). OpenTelemetry + collector ✅ (vendor-neutral export to an **on-prem** collector; *verify*
exporter config keeps telemetry in the VPC). Standing risk: pin all three together. Lives in `oncora-telemetry`.

**4.12 Reliability/testing.** `proptest` ✅ (property tests — memory write/read, uncertainty/confidence math,
parsers; `quickcheck` named, not needed). `cargo-fuzz` (libFuzzer) ✅ (fuzz VCF/DICOM/JSON tool args; scheduled, not
per-PR). `insta` ✅ (snapshot tests lock prompts/traces/manifests; review deliberately). `criterion` ✅
(microbenchmarks feeding CI gates; `divan` named). Differential testing ✅ (technique — cross-check vs oracles +
baselines; harness discipline in `oncora-eval`; pin/version golden sets + baselines).

**4.13 Reproducibility/artifacts (behind `ArtifactStore`).** `blake3` ✅ (CAS hashing; *verify* canonical stable
byte representation before hashing). custom CAS over object store/FS ✅ (BLAKE3-keyed, immutable, dedup-by-content;
*verify* GC/retention never deletes anything a provenance record references). `serde` (+ `serde_json`, `ciborium`
CBOR) manifests ✅ (typed versioned; JSON for human-auditable, CBOR for compact; *verify* schemas versioned so old
artifacts stay readable). deterministic seeds ✅ (technique; *verify* all randomness via a recorded seeded source —
no unseeded `rand` in the agent path). model/version pinning ✅ (technique; `ModelPin`+`SnapshotId` on every
`Provenance`; *verify* the model server reports a stable recordable identity — build + weights digest, not a friendly name).

## 4.14 Consolidated risk register

L/I = Low/Medium/High; owner = the crate owning the mitigation.

| Risk | L | I | Mitigation | Owner |
|---|---|---|---|---|
| `rig` 0.x breaking / insufficient tool-calling/streaming | High | Med | wrap behind `oncora-agents` traits; in-house orchestrator fallback; verify first | `oncora-agents` |
| `swiftide` API churn breaks ingestion stages | Med | Low | we own the `oncora-ingest` contract; swiftide implements stages | `oncora-ingest` |
| vLLM/TGI wire-format drift vs `async-openai` | Med | High | pin server build; contract-test deserialization; `reqwest` fallback | `oncora-agents` |
| `cozo` time-travel/Datalog immature at scale | Med | High | strict `GraphStore`; `indradb` fallback; verify perf early | `oncora-kg` |
| `ort`/ONNX C++ surface — build/supply-chain | Med | Med | isolate behind `EmbeddingProvider`/`Calibrator`; pin ORT + ONNX digests; `candle` fallback | `oncora-retrieval` |
| `rmcp` protocol/transport gaps | Med | Med | `ToolHost` owns audit/timeout/determinism; pin protocol; in-house fill-in | `oncora-mcp-host` |
| `mistral.rs` model/quantization support shifts | Med | Low | dev/air-gapped only; `ModelProvider` boundary; vLLM is production | `oncora-agents` |
| `lancedb` ANN recall/versioning insufficient | Low | Med | `VectorStore` trait; qdrant named-vectors fallback; verify recall at scale | `oncora-retrieval` |
| `noodles` missing a genomics format/codec | Low | Med | behind VCF MCP server; add support there; htslib last resort behind boundary | `oncora-mcp-host` |
| `dicom-rs` missing a transfer syntax/codec | Low | Med | behind imaging MCP server; add codec there, not a C dep in core | `oncora-mcp-host` |
| `redb` write throughput insufficient | Low | Med | `MemoryStore` trait; `fjall` LSM fallback; benchmark append | `oncora-memory` |
| `tracing`/`tracing-opentelemetry`/OTel SDK mismatch | Med | Low | pin all three; smoke-test export in CI | `oncora-telemetry` |
| PHI leaks into spans/telemetry or to cloud | Low | High | span hygiene; cloud off-by-default behind audited egress proxy; redaction in front | `oncora-telemetry`/`oncora-api` |
| Non-deterministic replay (unseeded RNG, unstable model id) | Med | High | all randomness via recorded seeds; model id = build + weights digest; replay tests in CI | `oncora-artifacts` |
| `arrow` version skew across polars/duckdb | Med | Low | align Arrow versions workspace-wide; verify in CI | `oncora-ingest` |
| Workspace-wide ecosystem churn (many 0.x) | High | Med | trait boundaries everywhere; verify-before-committing checklist; pin + review upgrades | all |

## 4.15 Non-Rust dependencies & isolation (P-1)

Exactly **three** sanctioned non-Rust surfaces, each behind a named trait: (1) **Model endpoints** (HTTP: vLLM/TGI
on-prem + opt-in cloud) behind `ModelProvider`; clients `async-openai`/`async-anthropic`/`reqwest` are concrete
impls; GPU serving is not a Rust workload; cloud off by default, proxy-only. (2) **ONNX Runtime** (`ort`, C++;
`fastembed` uses it) behind `EmbeddingProvider`+`Calibrator`; ONNX files pinned + content-addressed; mature
operator coverage pure Rust doesn't yet match; `candle` is the pure-Rust fallback; `duckdb`'s embedded engine is a
similar self-contained native dep behind the query interface. (3) **Third-party ontologies** (data, not code:
GO/Reactome/ChEMBL/UMLS/MONDO/HGNC) enter via ingestion into `oxigraph`, URI-addressed, queried through
`GraphStore`, snapshot-pinned; reference data we read, never source we write. Everything else is pure Rust or an
embedded engine behind a trait; `noodles`+`dicom-rs` (pure Rust) additionally sit behind the `ToolHost` boundary.

## 4.16 Verify-before-committing checklist

Re-run at the start of each implementation phase. A failed Primary routes to its already-registered Fallback (a
config change, not a rewrite).
- [ ] `rig` — tool-calling (multi-tool + parallel) + streaming; provider trait with custom OpenAI-compatible base URL.
- [ ] `swiftide` — transform-stage composition with our `EmbeddingProvider`/`VectorStore`; backpressure.
- [ ] `async-openai` ↔ vLLM/TGI — custom base URL + on-prem auth; tool-call/streaming shapes match deployed build.
- [ ] `async-anthropic` — SDK currency vs latest Messages API; redaction in front; path off-by-default.
- [ ] `candle` — kernel/architecture coverage; reproducible CUDA builds on target images.
- [ ] `mistral.rs` — supported families/quantizations; throughput; OpenAI-compatible surface for `ModelProvider`.
- [ ] `ort` — pinned version; reproducible build; ONNX digests pinned; no leakage past the trait.
- [ ] `rmcp` — transports (stdio + streamable HTTP/SSE); client/host capability maturity; protocol version pinned.
- [ ] `qdrant` — Rust client tracks deployed server; filtering + hybrid search cover needs.
- [ ] `lancedb` — ANN index types/recall at scale; versioning semantics for reproducibility.
- [ ] `fastembed` — embedder families available; ONNX models pinned/content-addressed.
- [ ] `oxigraph` — bulk-load perf for full snapshots; SPARQL feature coverage (property paths, aggregates).
- [ ] `cozo` — time-travel semantics + recursive-Datalog perf at our volumes; embedded-vector recall. *(Highest-novelty; verify first.)*
- [ ] `polars`/`duckdb`/`arrow` — Arrow versions aligned; memory behavior on largest omics matrices.
- [ ] `noodles` — format/version + codec coverage for our cohorts; large BAM/CRAM performance.
- [ ] `dicom-rs` — transfer-syntax/codec coverage; pixel decoding supported or pluggable.
- [ ] `redb` — episodic-append write throughput; on-disk format stability across pinned 2.x.
- [ ] `tracing`/`tracing-opentelemetry`/OTel SDK — versions pinned together; export stays on-prem; no PHI in span fields.
- [ ] `cargo-fuzz` — a fuzz target exists for every external-input parser (VCF, DICOM, JSON tool args).
- [ ] Reproducibility — all randomness seeded + recorded; model identity = build + weights digest; replay test passes in CI.

---

# 5. Data model

## 5.1 Core uncertainty & provenance types (`oncora-core`)

```rust
// oncora-core::uncertainty
pub struct Confidence(f64);                 // calibrated prob in [0,1]; fallible ctor, no From<f64>
impl Confidence {
    pub fn new(p: f64) -> Result<Self, UncertaintyError> {   // Err(OutOfRange) if !(0.0..=1.0)
        if (0.0..=1.0).contains(&p) { Ok(Self(p)) } else { Err(UncertaintyError::OutOfRange(p)) }
    }
    pub fn get(self) -> f64 { self.0 }
}
pub enum CalibrationMethod { Raw, TemperatureScaled, Isotonic, Conformal }  // Raw REJECTED by the scorer
pub struct Provenance {                     // every field is enough to replay the decision
    pub sources: Vec<SourceRef>, pub tool_calls: Vec<ToolCallId>,
    pub model: ModelPin, pub snapshot: SnapshotId,           // BLAKE3 content-addressed snapshot
}
pub struct Evidence {
    pub claim: ClaimId, pub support: Vec<EvidenceItem>, pub contradiction: Vec<EvidenceItem>,
    pub confidence: Confidence, pub method: CalibrationMethod, pub provenance: Provenance,
}
pub struct EvidenceItem { pub source: SourceRef, pub strength: f64, pub relation: Relation }  // strength = NLI prob / oracle agreement
pub enum Relation { Entails, Contradicts, Neutral }
pub struct UncertaintySources { pub aleatoric: f64, pub epistemic: f64, pub retrieval: f64, pub tool: f64 }
pub enum Verdict {
    Accept   { confidence: Confidence },
    Abstain  { reason: AbstainReason },
    Escalate { to: EscalationTarget, reason: String },
}
pub enum EscalationTarget { HumanReviewer, SpecialistAgent }
pub enum AbstainReason {
    LowCalibratedConfidence { conf: f64, threshold: f64 },
    ConformalSetTooLarge    { size: usize, max: usize },
    OracleDisagreement      { detail: String },
    UngroundedClaim,          // no entailing source for an asserted fact
    InsufficientRetrieval,    // retrieval coverage below floor
}
```

**Invariants.** `Confidence::new` fallible (out-of-range is a bug to surface). `CalibrationMethod::Raw`
representable but **rejected by the scorer in code** — no `Accept` carries `Raw`. `UncertaintySources` is a
decomposition (policy routes on the dominant source: high epistemic → escalate to specialist; high aleatoric →
abstain). `Provenance` is the single struct shared by uncertainty **and** every memory write.

**Identifier types (`oncora-core::ids`):** `SourceRef`, `ToolCallId`, `ModelPin` (model + version + decode config /
weights digest + serving backend), `SnapshotId` (BLAKE3 data snapshot), `ClaimId`, `ScientistId`, `ProjectId`,
`WorkflowId`, `MemoryId`, `CaseId`, `ArtifactId`, `ExperimentId`, `ContentHash([u8;32])` (BLAKE3).

## 5.2 Five memory types

| Type | Purpose | Contents | Lifetime | Primary engine | Secondary |
|---|---|---|---|---|---|
| **Working** | run scratchpad (loop's volatile state) | plan state, intermediate tool results, partial reasoning, pending-verification queue | run-scoped; discarded or selectively consolidated | in-memory | `redb` spill |
| **Episodic** | append-only flight recorder | every step, MCP tool call, observation, decision, `Verdict` in temporal order | durable, immutable (retention-bounded), never mutated | `redb` log | CAS payloads (BLAKE3) |
| **Semantic** | curated belief state / KG | canonical entities + edges; Claim/Evidence with per-edge confidence | long-lived; updated via consolidation + conflict resolution, never silently overwritten | `oxigraph` (RDF/SPARQL) + `cozo` (Datalog, time-travel) | — |
| **Procedural** | learned how-to | successful plan templates, effective tool-use sequences, skill recipes, tuned retrieval strategies | long-lived; reinforced by success, decayed when superseded | `cozo` | CAS manifests |
| **Provenance/evidence** | audit + reproducibility backbone | `Provenance` + support/contradiction structure of `Evidence` | **permanent, immutable** — decay/tombstones hide a fact but provenance is never lost | Postgres ledger (`sqlx`) | CAS |

```rust
pub struct MemoryKey { pub scientist: ScientistId, pub project: ProjectId, pub workflow: WorkflowId }
pub enum MemoryKind { Working, Episodic, Semantic, Procedural, Provenance }
pub struct MemoryEntry {
    pub id: MemoryId, pub key: MemoryKey, pub kind: MemoryKind,
    pub evidence: Evidence, pub provenance: Provenance,
    pub snapshot: SnapshotId, pub model: ModelPin,          // replay pins
    pub payload: ContentHash,                                // CAS handle, BLAKE3
    pub decay_score: f64, pub tombstoned: bool,              // below floor => tombstone
}
pub struct ReadQuery { pub key: MemoryKey, pub text: Option<String>, pub as_of: Option<SnapshotId>, pub token_budget: usize }
```

**Conflict-resolution policy (inputs: recency, source authority, calibrated confidence; never silently overwrite —
P-10):** new claim, no existing → insert with provenance; duplicate (same entities+predicate+value) → merge,
append source+tool call to provenance, bump usage; agrees + higher authority → keep both, mark higher-authority as
current belief; contradicts + more recent + higher confidence → retain both, current-belief view favors new;
contradicts + lower confidence/weaker source → retain both, current-belief stays old, flag for review;
comparable authority+confidence → retain both as competing evidence, surface together, abstain if asked; superseded
by trial readout → add new edge, time-stamp, old queryable via time-travel; ambiguous entity resolution → don't
merge, keep separate, queue; below decay floor → tombstone, exclude from default read, keep provenance.

## 5.3 KG schema

Physically: canonical entities (`Target`, `Disease`, `Pathway`, `Compound`, `Variant` identity) in `oxigraph` as
URIs; `Claim`, `Evidence`, `Source`, `Provenance`, confidence-bearing edges in `cozo`.

**ER edge-list** (`A —edge→ B`, `o{` = many): TARGET —involved_in→ PATHWAY; TARGET —associated_with→ DISEASE;
COMPOUND —modulates→ TARGET; VARIANT —located_in→ TARGET; VARIANT —confers_sensitivity_resistance→ DISEASE; TRIAL
—targets→ DISEASE; TRIAL —tests→ COMPOUND; COHORT —characterizes→ DISEASE; CLAIM —about→ {TARGET, DISEASE,
COMPOUND, VARIANT}; EVIDENCE —supports_contradicts→ CLAIM; EVIDENCE —derived_from→ SOURCE; COHORT —yields→
EVIDENCE; PROVENANCE —has→ {CLAIM, EVIDENCE, SOURCE} (everything `has` Provenance).

**Entity attributes (+ identity store):** Target{uri, hgnc_symbol, kind}·oxigraph; Disease{uri, mondo_id,
label}·oxigraph; Pathway{uri, reactome_id, label}·oxigraph; Compound{uri, chembl_id, label}·oxigraph;
Variant{uri, locus, ref_alt, consequence}·oxigraph(identity); Trial{id, phase, status}·cozo; Cohort{id, n,
description}·cozo; Claim{id, statement, confidence}·cozo; Evidence{id, polarity, weight}·cozo; Source{id, kind,
snapshot_id}·cozo; Provenance{id, model_pin, snapshot_id, tool_calls}·cozo + Postgres ledger.

**Which store a query hits:** resolve name/symbol→URI · ontology structure (pathways, gene→pathway, disease
hierarchy) · compound→target modulation from curated sources → **oxigraph/SPARQL**. What we *claim* with
supporting/contradicting evidence · per-edge confidence+provenance · point-in-time "as of snapshot X" → **cozo/
Datalog**. Cross-store join (canonical entity + accrued evidence) → **both**, fused in `oncora-kg`.

## 5.4 Storage-engine mapping & embedding strategy

| Store | What lives there | Query | Why |
|---|---|---|---|
| `qdrant` | literature/text chunk embeddings + payload; entity-label resolution vectors | HNSW + payload filter | Rust-native server; payload filtering; quantization; text/RAG workhorse |
| `lancedb` | imaging + multimodal + omics-signature embeddings + heavy metadata | vector search over columnar store | columnar, multimodal, versioned |
| `oxigraph` | ontology canonical entities + URIs (GO/Reactome/ChEMBL/UMLS/MONDO/HGNC) | SPARQL | standards triplestore; identity authority |
| `cozo` | evidence/assertion graph: claims, evidence, per-edge confidence + provenance | Datalog | time-travel + per-edge confidence + provenance; versioned evidence + semantic memory |
| `polars`/`duckdb` | tabular omics over Parquet snapshots | DataFrame / SQL | in-proc transforms + set-oriented SQL; analytical not retrieval |
| Parquet via MCP | raw omics matrices (read-only snapshots) | DuckDB SQL / Arrow | columnar at-rest source-of-record, behind MCP boundary |
| Postgres ledger | provenance/audit records linking artifact → sources/tools/model/snapshot | SQL (`sqlx`) | async compile-checked SQL; durable multi-node ledger |
| `turso` (pure-Rust SQLite) | embedded dev relational/ledger | SQL | Rust-native; ~23% faster than C SQLite at 14.5k writes |
| CAS over object store | content-addressed payloads + manifests (BLAKE3) | content-hash lookup | deterministic replay; immutable artifacts |

**Embeddings** (on-prem, pinned, versioned; every model = a `ModelPin` name+revision+checksum, downloaded once +
content-addressed + frozen; dimension fixed per collection; cross-model comparison only via re-embedding; every
vector carries `SourceRef`+`SnapshotId`+`ModelPin`; behind `EmbeddingProvider`):

| Modality | Engine | Model class | Dim | Stored in |
|---|---|---|---|---|
| Literature / text | `fastembed` over `ort` | biomedical/general sentence embedder | 768 | `qdrant` |
| Entity labels / synonyms | `fastembed` over `ort` | same text embedder | 768 | `qdrant` (resolution index) |
| Imaging | `candle` or `ort` | vision/feature encoder | 512–1024 | `lancedb` |
| Omics-derived signatures | `candle` or `ort` | tabular/profile encoder | task-specific | `lancedb` |

## 5.5 Evaluation types (`oncora-eval`)

```rust
pub struct BenchmarkCase {
    pub id: CaseId, pub workflow: Workflow,   // TargetDiscovery | Translational | TrialDesign
    pub split: Split,                          // Dev | Test | Holdout
    pub stratum: Stratum,                      // Normal | Hard | AbstainExpected
    pub input: serde_json::Value, pub expert: ExpertLabel, pub snapshot: SnapshotId,
}
pub struct ExpertLabel { pub decision: Decision, pub rationale: String, pub cited_evidence: Vec<SourceRef>, pub annotator_agreement: f64 }  // kappa
pub struct RunResult {
    pub case: CaseId, pub arm: Arm,            // Agent | Baseline(name) | Expert
    pub verdict: Verdict, pub answer: Option<Decision>,   // None when Abstain/Escalate
    pub confidence: Confidence, pub provenance: Provenance,
    pub latency_ms: u64, pub cost: Cost, pub fixtures: Vec<ArtifactId>,
}
pub struct Score {
    pub arm: Arm, pub split: Split, pub accuracy: f64, pub task_metric: f64, pub ece: f64, pub aurc: f64,
    pub selective_accuracy_at: Vec<(f64, f64)>, pub abstain_recall: f64,
    pub latency_p50_ms: u64, pub latency_p95_ms: u64, pub cost: Cost,
}
pub struct GatePolicy {
    pub max_accuracy_drop_pp: f64,   // 1.0
    pub max_ece_regression: f64,     // 0.01
    pub ece_ceiling: f64,            // 0.05
    pub max_aurc_regression: f64,    // 0.01
    pub min_abstain_recall: f64, pub must_beat_baseline: bool, pub latency_tolerance: ToleranceBand,
}
pub enum GateOutcome { Pass, Block { reasons: Vec<String> } }
```

**`RunManifest`** (complete content-addressed run description): `ModelPin` per role (planner, specialists,
verifier, calibrator) incl. weights digest + serving backend + decode params; `SnapshotId` for the golden set +
every underlying corpus/KG/omics snapshot; seeds (sampling, self-consistency, shuffling); config hash (BLAKE3 of
the fully-resolved `figment`-merged config); code provenance (git commit + lockfile digest); baseline arm pins;
**manifest digest** (BLAKE3 of all the above — *this id names the run*). Every number back-points `report →
ExperimentId → RunManifest digest → per-case RunResult digests → recorded model-I/O fixtures`.

## 5.6 Verdict policy (normative; consumed by the scorer)

Inputs: calibrated `Confidence`, conformal set size, oracle agreement, retrieval coverage, NLI grounding,
`UncertaintySources`. Thresholds (`accept`, `escalate-band`, `abstain-floor`, `conformal-max`, `retrieval-floor`)
`figment`/`config`-layered per task class, validated at load; defaults conservative; high-stakes bias toward
escalation. Decision rows: oracle disagreement on a verifiable sub-claim → `Abstain`/`Escalate` (never override a
calculator/KG with prose); no entailing source / NLI all-neutral → `Abstain{UngroundedClaim}`; conformal set >
task-class max → `Abstain{ConformalSetTooLarge}`; confidence ≥ accept-threshold AND grounded AND oracle-agree AND
singleton set → `Accept`; confidence in escalate-band AND epistemic-dominant → `Escalate{SpecialistAgent}`;
confidence below abstain-floor AND aleatoric-dominant → `Abstain{LowCalibratedConfidence}`; confidence below
abstain-floor AND high-stakes class → `Escalate{HumanReviewer}`; retrieval coverage below floor →
`Abstain{InsufficientRetrieval}`. **Aggregation:** a response is only as confident as its weakest **load-bearing**
claim (min-confidence over planner-identified load-bearing claims, not all atomic claims).

---

# 6. Contracts

## 6.1 Core provider traits

Small, object-safe traits in/near `oncora-core`, implemented in the crate owning the tech; higher layers program
against the trait. Library errors typed (`thiserror`) so callers branch on failure class.

| Trait | Contract | Defined in | Backends |
|---|---|---|---|
| `ModelProvider` | prompt/messages → completion or stream, with a `ModelPin` | `oncora-core` | `oncora-agents` (`async-openai`, Anthropic, `mistral.rs`/`candle`) |
| `EmbeddingProvider` | embed text/multimodal → fixed-width vectors, batched + deterministic | `oncora-core` | `oncora-retrieval` (`fastembed`/`ort`, `candle`) |
| `VectorStore` | upsert + ANN-search with payload filtering → scored hits | `oncora-core` | `oncora-retrieval` (`qdrant`, `lancedb`, dev `hnsw_rs`) |
| `GraphStore` | assert/query entities+edges; ontology (SPARQL) + time-traveled evidence (Datalog) | `oncora-core` | `oncora-kg` (`oxigraph`, `cozo`/`indradb`) |
| `MemoryStore` | read/write versioned, attributed, content-addressed memory by `(scientist,project,workflow)` | `oncora-core` | `oncora-memory` (`redb`+CAS, Postgres, `cozo`) |
| `ToolHost` | register + invoke MCP tools deterministically; record every call to the audit trail | `oncora-core` | `oncora-mcp-host` (`rmcp`) |
| `Calibrator` | map raw scores → calibrated `Confidence`, tagging `CalibrationMethod` | `oncora-core` | `oncora-uncertainty` (temperature/isotonic/conformal, `ort`/`candle`) |
| `Verifier` | check a claim vs evidence/oracles → support, contradiction, verdict signal | `oncora-core` | `oncora-uncertainty` (NLI entailment, oracle grounding) |
| `ArtifactStore` | put/get content-addressed blobs + manifests by BLAKE3; immutable, deduplicated | `oncora-core` | `oncora-artifacts` (object store / FS CAS) |
| `LedgerStore` | per-document/per-run relational provenance writes behind one conformance contract | `oncora-core` | `oncora-ledger` (in-memory, `turso`, gated `rusqlite`) |

```rust
use async_trait::async_trait;
#[async_trait] pub trait ModelProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<Completion, ModelError>;
    fn pin(&self) -> ModelPin;
}
#[async_trait] pub trait VectorStore: Send + Sync {
    async fn upsert(&self, points: Vec<VectorPoint>) -> Result<(), StoreError>;
    async fn search(&self, query: VectorQuery) -> Result<Vec<ScoredHit>, StoreError>;
}
pub trait Calibrator: Send + Sync {
    fn fit(&mut self, samples: &[CalibrationSample]) -> Result<(), UncertaintyError>;
    fn calibrate(&self, raw: f64, ctx: &CalibrationContext) -> Confidence;
    fn method(&self) -> CalibrationMethod;
}
#[async_trait] pub trait ArtifactStore: Send + Sync {
    async fn put(&self, bytes: &[u8]) -> Result<ContentHash, CasError>;
    async fn get(&self, hash: &ContentHash) -> Result<Vec<u8>, CasError>;
}
#[async_trait] pub trait MemoryStore {   // write = extract→dedup→conflict-res→consolidate→decay; read = hybrid→fuse→assemble; forget = soft-delete+tombstone
    type Error;
    async fn write(&self, entry: MemoryEntry) -> Result<MemoryId, Self::Error>;
    async fn read(&self, query: ReadQuery) -> Result<Vec<MemoryEntry>, Self::Error>;
    async fn forget(&self, id: MemoryId) -> Result<(), Self::Error>;  // provenance retained
}
```

**Per-trait invariants.** `ModelProvider`: on-prem default, cloud opt-in proxy-only stamped into the pin,
`temperature = 0` (P-2,P-6). `EmbeddingProvider`: batched+deterministic; one layer for retrieval + semantic memory;
fixed dim per collection; vectors carry SourceRef+SnapshotId+ModelPin (FR-ING-8). `VectorStore`: payload filtering;
degrades not blocks (FR-RET-2). `GraphStore`: one surface over the dual store; intent not store choice; per-edge
confidence + provenance + point-in-time (FR-KG-*). `MemoryStore`: keyed by `(scientist,project,workflow)`,
cross-tenant denied; episodic+provenance never decayed/deleted; every write stamps snapshot+model pin+BLAKE3
payload hash (FR-MEM-*). `ToolHost`: single chokepoint; **record-before-return**; record allow+deny; deterministic;
`tower` timeout; `ToolCallId` (P-5). `Calibrator`: fit on held-out `(raw_score, correct?)`; per task class;
content-addressed + tied to ModelPin+SnapshotId; scorer rejects `Raw` (FR-UNC-5/6). `Verifier`: adjudicates never
generates; citation/oracle/consistency/contradiction/schema-unit; populates `EvidenceItem.strength`; can force
abstain (FR-UNC-4). `ArtifactStore`: immutable, dedup-by-content; GC never deletes a referenced provenance
(P-6,P-10). `LedgerStore`: identical conformance test passes for in-memory, C SQLite (`rusqlite`, gated), pure-Rust
SQLite (`turso`, chosen); a provenance record per run inside the agent loop. **Dependency rule:** deps inward toward
`oncora-core`; providers are concrete impls; no cycles; swapping a backend never touches the agent runtime or API.

## 6.2 `oncora-api` (HTTP + gRPC) — sole entry point (P-2)

`axum` (HTTP) + `tonic` (gRPC); auth + RBAC at the edge, **re-checked at the MCP host policy gate** (FR-GOV-1);
roles scoped per `(project, workflow)`; service-to-service mTLS. *(proto)* = demonstrated in the prototype.

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/health` | liveness | liveness/readiness probe *(proto)* |
| GET | `/tools` | scientist+ | list registered MCP tools available to the caller *(proto)* |
| POST | `/query` | scientist+ | submit a workflow query → cited, confidence-scored answer **or** logged abstention/escalation *(proto)* |
| GET | `/runs/{run_id}` | scientist+ | fetch a run's verdict, evidence, citations, audit handle |
| GET | `/runs/{run_id}/lineage` | reviewer+ | provenance lineage walk: claim → sources, tool calls, model pin, snapshot |
| GET | `/review/queue` | reviewer | list escalated decisions awaiting adjudication |
| POST | `/review/{run_id}/decision` | reviewer | adjudicate an escalation; approve/deny a semantic-memory promotion |
| POST | `/ingest` | pipeline/operator | trigger ingestion of a pinned snapshot |
| POST | `/eval/run` | pipeline/operator | launch a benchmark run against a golden set |

`POST /query` request: `{ workflow, query, context_key:{scientist,project,workflow}, task_class?, options? }`.
Response (Accept):
```json
{ "verdict":"accept", "answer":"NSCLC is the best-supported answer", "confidence":0.935,
  "calibration_method":"temperature_scaled", "citations":["PMID:0001","PMID:0002"],
  "evidence":{"support":[...],"contradiction":[...]},
  "provenance":{"sources":[...],"tool_calls":[...],"model":"openai/qwen2.5:0.5b@live","snapshot":"<blake3>"},
  "run_id":"<id>" }
```
Response (Abstain/Escalate): `verdict ∈ {abstain, escalate}` with `reason` (an `AbstainReason` or escalation
target + reason) + the full evidence bundle (FR-UNC-9). **gRPC** mirrors query/ingest/eval as typed RPCs with
streaming where useful, same auth+RBAC+audit; protobuf/codegen pinned + reproducible. **Cross-cutting:** auth+RBAC
re-checked at MCP host; every tool call recorded (allow+deny) before its result returns (P-5); every answer carries
`Provenance`, traceable (FR-GOV-3,P-4); PHI redacted, never to cloud except non-PHI via egress proxy (P-2,FR-GOV-4);
each request a correlated `tracing` span to the on-prem OTel collector, no PHI in span fields (NFR-OBS-1).

## 6.3 MCP tools (`oncora-mcp-host` / `rmcp`)

Every domain capability is an MCP tool (P-5); Oncora is both host/client and server. **Invocation contract (every
call):** 1 resolve tool→server via `rmcp` registry; 2 policy check (RBAC scope, PHI-egress, determinism); 3 validate
inputs (fuzzed: VCF, DICOM, JSON tool args); 4 enforce `tower` timeout; 5 execute deterministically; 6
record-before-return (inputs, outputs, ModelPin, SnapshotId, ToolCallId, latency, verdict) to **both** episodic
memory (`redb`+CAS) and Postgres ledger — a **denied** call is also recorded; 7 return the typed result (only after
6). Flow edge-list: `Agent → Host`; `Host → Policy{RBAC·PHI egress·determinism}`; `Policy --allow--> Tool`; `Policy
--deny--> Deny(reject+log)`; `Tool → Host`; `Host → {Ledger, Episodic}`; `Deny → Ledger`; `Host → Agent`.

| Server | Built on | Tools (typed in/out) | Notes |
|---|---|---|---|
| VCF / genomics | `noodles` (pure Rust) | variant lookup → typed `Variant` (locus, ref/alt, consequence, gene); cohort slicing via `polars`/`duckdb` over Parquet | agents never touch raw files; PHI/IP on-prem |
| DICOM / imaging | `dicom-rs` (pure Rust) | study/series/instance parse; de-identified feature/embedding extraction | de-identification enforced at the boundary; pixel data + PHI stay on-prem; only de-identified features/embeddings indexed (`lancedb`) |
| Clinical calculators | deterministic oracles | dose adjustment, BSA, creatinine clearance, response criteria, … | **deterministic oracles** = ground truth the verifier checks against; flag out-of-domain inputs |
| Knowledge graph | `oncora-kg` (`oxigraph`+`cozo`) | SPARQL ontology queries; Datalog evidence queries; entity resolution | also a deterministic oracle for grounding |
| Retrieval / memory | `oncora-retrieval`/`oncora-memory` | hybrid retrieve; memory read/write | internal Oncora-hosted tools |

**Guarantees:** determinism + content-addressing ⇒ replayable audit trail; oracle mismatch drives
`UncertaintySources.tool` → 1.0 and can force `Abstain{OracleDisagreement}` regardless of model confidence (never
override a calculator/KG with prose); genomics/imaging parsers behind the boundary so a parsing bug can't reach the
agent core (P-1); a cancelled/timed-out run drops in-flight tool calls, recorded as dropped (FR-AGT-4).
**Implemented (prototype):** in-memory host by default; under `--features rmcp`, an `rmcp`-backed MCP **server**
(exposing Oncora's tools) + a **client** routing an external server's tools through the same `ToolHost` — verified
by an in-process loopback test.

## 6.4 `oncora-cli`

Operator/developer CLI; drives the single-node embedded substrate; home of deterministic replay (P-6); `anyhow` for
top-level errors.

| Command | Purpose |
|---|---|
| `oncora-cli run <query> [target]` | submit a workflow query; print a cited, confidence-scored answer or a logged abstention |
| `oncora-cli replay --manifest <blake3>` | reconstruct the pinned stack; reproduce a run **bit-for-bit** from CAS (pinned model + snapshot + content hashes + seeds) |
| `oncora-cli ingest <snapshot>` | run `swiftide` ingestion of a pinned snapshot into KG + indexes + CAS (never writes to source) |
| `oncora-cli eval replay --manifest <blake3>` | resolve a `RunManifest` from CAS; replay from fixtures (or `--live`); recompute metrics; regenerate figures; assert recomputed `Score` matches recorded within tolerance |
| `oncora-cli eval figure --report <blake3> --id <fig-id>` | regenerate a specific figure from a recorded report |

Prototype binaries: `cargo run --bin oncora` (ingest a tiny corpus, ask a target-discovery question, print a cited,
confidence-scored answer); `cargo run --bin oncora -- "Is BRAF actionable in melanoma?" BRAF`; `cargo run --bin
oncora-api` (HTTP API on :8080 — GET /health, /tools; POST /query). **Guarantees:** given `(snapshot, model pin,
content hashes, seeds)`, `replay` reproduces the same memory state, retrieval, and answer (AC-3); the eval path
refuses unpinned model / non-content-addressed dataset (FR-EVAL-4); one-command reproducibility of any figure,
tracing any number to its source artifact (P-6,P-8).

---

# 7. Tasks — phased, risk-gated roadmap

Each phase retires a risk class + carries **de-risking spikes** testing the youngest bets (`rig`, `rmcp`,
`swiftide`, `cozo` time-travel, conformal calibration, on-prem model throughput) *before* the thick layer; a failed
spike routes to the named fallback (§4). Throughline: build the spine first, thicken one pillar at a time, put the
riskiest young-tech bet on trial before depending on it.

**Phase 0 — Walking skeleton ✅ (spine proven in prototype).** One end-to-end thin slice exercising **every crate**
at minimal depth: a target-discovery query that retrieves (hybrid), reasons, calls **one** MCP tool, verifies,
returns a confidence-scored cited answer. Per-crate minimum: core (Confidence/Provenance/Evidence/Verdict/ids/
errors); telemetry (tracing spans to stdout/local OTel); artifacts (blake3 CAS over local FS; one manifest type);
mcp-host (rmcp host; one registered tool; audit to ledger+episodic); kg (oxigraph+cozo embedded; minimal
Target/Disease/Evidence schema); retrieval (fastembed; one vector index; basic vector+graph fusion); ingest
(swiftide ingest of one small literature snapshot); memory (working+episodic+provenance write/read; semantic stub);
uncertainty (one calibration method; citation-entailment verifier; threshold verdict); agents (rig planner→one
specialist→verifier→scorer→responder); eval (10–20 golden target-discovery questions; accuracy+abstention metrics);
api (one authenticated query endpoint); cli (submit query + replay). **Exit:** a query runs end-to-end → cited,
confidence-scored answer **or** logged abstention; the single MCP tool call recorded to ledger **and** episodic;
`oncora-cli replay` reproduces bit-for-bit from pinned model+snapshot+CAS; the golden set runs in CI and gates the
build. **Spikes:** (T0-S1) verify `rig` tool-calling+streaming vs an OpenAI-compatible on-prem endpoint — fallback
in-house orchestrator on raw `async-openai`; (T0-S2) verify `rmcp` (register/route/audit one round-trip) — gaps →
wrap behind `ToolHost`. **Risk:** spine leaks abstraction → trait-boundary review before Phase 1. **Prototype
divergence (recorded):** built an in-house orchestrator + simple ingest instead of `rig`/`swiftide`; runs the loop
as a single end-to-end function, not a multi-agent runtime. Decision open: adopt or formally retire `rig`/`swiftide`.

**Phase 1 — Multimodal ingestion (literature → omics → imaging).** *Deliverables:* `swiftide` pipelines for
literature, then omics (`polars`/`duckdb` over Parquet, `noodles` VCF behind an MCP server), then imaging
(`dicom-rs` behind an MCP server, `lancedb` multimodal embeddings); KG schema fully populated (Target/Disease/
Pathway/Compound/Variant/Trial/Cohort + edges); hybrid retrieval fuses vector+graph+recency across all modalities.
*Exit:* a query requiring literature **and** omics **and** imaging returns one fused, cited answer; each modality
content-addressed + snapshot-tagged; no source written to. *Spikes:* VCF/DICOM parser robustness (fuzz `noodles`/
`dicom-rs` on real malformed files); `lancedb` multimodal embedding scale; `swiftide` pipeline contract (own it;
fallback custom `tokio` pipeline). *Risk:* entity resolution across HGNC/UMLS/MONDO → oxigraph URI grounding + dedup
in the memory write path.

**Phase 2 — Memory architecture.** *Deliverables:* working/episodic/semantic/procedural/provenance wired to
canonical stores; write path extract → dedup (embedding + KG entity resolution) → conflict resolution (recency +
authority + confidence; contradictions kept) → consolidation (working→episodic→semantic) → decay/forgetting
(tombstones, never lose provenance); read path hybrid retrieval fused under token budget, keyed by `(scientist,
project, workflow)`. *Exit:* a second session on the same key demonstrably reuses prior semantic + procedural
memory; conflicting facts retained as competing evidence; every entry carries snapshot + model pin, replayable.
*Spikes:* **`cozo` time-travel at scale** (point-in-time evidence-graph queries over a realistically sized graph —
the biggest young-tech bet; fallback `indradb` + explicit versioning); conflict-resolution policy on adversarial
contradictory inputs. *Risk:* decay erases something a reviewer needs → soft-delete + tombstones; provenance never
deleted.

**Phase 3 — Uncertainty & verification.** *Deliverables:* post-hoc calibration with ECE tracking; conformal
prediction (set-valued + abstention guarantee); verifiers (citation/NLI entailment, deterministic oracle grounding
via calculators/KG); decomposed uncertainty (aleatoric/epistemic/retrieval/tool); configurable per-task-class
thresholds. *Exit:* confidence calibrated (ECE below target on a held-out set); conformal sets carry guaranteed
coverage; oracle disagreement reliably triggers `Escalate`; every abstention logged with a reason. *Spikes:*
conformal calibration (coverage holds on oncology golden sets; tune set-size thresholds); NLI entailment verifier
quality on biomedical claims (false-accept rate). *Risk:* long-tail miscalibration → ECE gating in CI; abstain when
the conformal set is wide.

**Phase 4 — Eval & benchmarking.** *Deliverables:* expanded golden sets across modalities + task classes; metrics
(accuracy, ECE, abstention quality, latency); human-expert + computational baselines; CI gate blocking regressions.
*Exit:* benchmarks beat the named baselines on the agreed metrics; results deterministically replayable; a
regression cannot merge. *Spikes:* golden-set construction with domain experts (the benchmark's validity is the
risk, not the harness). *Risk:* benchmark overfit → held-out sealed sets; periodic expert refresh.

**Phase 5 — Deployment scale-out (single-node → multi-node).** *Tasks:* (1) **config-driven `Platform` assembly**
(replace `Platform::demo()` — the unlock for everything else); (2) stateless `oncora-api`/agent workers behind a
load balancer; (3) externalize stores (qdrant cluster, **Postgres HA** provenance ledger, oxigraph/cozo as
services); (4) on-prem GPU model server (vLLM/TGI) behind `ModelProvider`, size concurrency caps; (5) object-store-
backed CAS (`ArtifactStore` → S3/MinIO); (6) OTel collector wiring (`tracing-opentelemetry`); (7) upgrade/rollback:
expand-contract migrations, golden-set replay-diff gate, confirmed rollback. *Exit:* horizontal scale under load ·
Postgres + qdrant failover · upgrade validated by replay diff · rollback confirmed. FoundationDB evaluated only if
memory scale demands it (optional, ops-heavy, phase-gated). *Spikes:* on-prem model throughput (tokens/sec under
realistic concurrency on target GPUs; size semaphore caps — the production cost/latency ceiling); qdrant cluster
sharding/replication at representative corpus size. **Spike RESOLVED — turso adopted:** the `LedgerStore` seam has
three interchangeable backends (in-memory, C SQLite `rusqlite`, pure-Rust `turso`); an identical conformance test
passes for all three; a cold-start bulk comparison over **14,469 per-document writes** settled it — **turso
completed with 0 errors at mean 2.04 ms, ~23% faster than C SQLite**. Decision: turso is the chosen embedded ledger;
the ledger is wired into the agent loop (a provenance record per run); turso is the bench default; C SQLite kept
behind a benchmark-only feature. *Still to verify before production:* turso transaction/concurrency/full-SQL
coverage; Postgres remains the HA path. Knock-on: gating C SQLite removed one of cozo's two blockers (native-
`sqlite3` `links` clash); the `graph_builder`/`rayon` one remains.

**Phase 6 — Governance hardening (review-board grade).** *Tasks:* (1) **RBAC** project/workflow-scoped on the API;
(2) **provenance lineage walk** (per-run ledger record already written in the prototype — add the lineage query +
API); (3) **audit completeness** — record-before-return invariant at the MCP host, property-test it (`McpHost`
already keeps an audit log); (4) **PHI/PII redaction + egress blocking** — redact logs/traces; egress proxy denies
PHI leaving; adversarial leak testing; (5) **reviewer adjudication queue** routing `Verdict::Escalate` to human
sign-off; (6) **ALCOA+ reproducibility evidence** — one-command replay of any historical run from its manifest; (7)
**change-control** — replay-gated CI (an upgrade must reproduce golden-set outputs). *Exit:* any claim traceable to
provenance · PHI cannot leave · every tool call (allow + deny) auditable · validated replay reproduces any
historical run. *Spikes:* redaction completeness (adversarial PHI-leak testing against logs/traces + the egress
proxy). *Risk:* audit-trail gaps under failure → record-before-return invariant; `proptest` the MCP host.
*Sequencing:* Phase 6 depends on Phase 5 (RBAC/egress/lineage assume the multi-node services + real ledger/CAS);
both infra/ops-heavy — the trait seams make each "add a production impl behind an existing trait," not a rewrite;
Phase 6 items 2–3 have head starts from the prototype (ledger-in-loop, MCP audit log).

**Timeline** (section · phase · id · start · duration): Skeleton · Phase 0 walking skeleton · p0 · 2026-06-01 ·
60d. Pillars · Phase 1 multimodal ingest · p1 · after p0 · 75d; Phase 2 memory architecture · p2 · after p1 · 75d;
Phase 3 uncertainty verify · p3 · after p2 · 75d; Phase 4 eval benchmarking · p4 · after p3 · 60d. Hardening · Phase
5 deployment scale-out · p5 · after p4 · 75d; Phase 6 governance hardening · p6 · after p4 · 90d.

**Milestones:** Spine proven (0): end-to-end cited answer + replay bit-for-bit. `rig`+`rmcp` validated (0): one
tool round-trip audited; fallback decision recorded. Cross-modal answer (1): one fused answer over literature +
omics + imaging. Cross-session memory (2): session 2 reuses semantic + procedural memory. `cozo` time-travel proven
(2): point-in-time query at target graph size within budget. Calibrated uncertainty (3): ECE below target;
conformal coverage holds. Beats baselines (4): wins accuracy/calibration/latency vs human + computational
baselines. Scaled topology live (5): horizontal scale + failover + replay-validated upgrade. On-prem throughput
sized (5): tokens/sec measured; semaphore caps set. Review-board ready (6): full lineage walk + PHI-egress block
verified.

**Biggest risks & early de-risking:** `rig` tool-calling/streaming immature (High, P0) → Phase-0 spike; fall back
to in-house orchestrator on `async-openai`. `rmcp` host/tool gaps (High, P0) → Phase-0 spike; wrap behind
`ToolHost`. `cozo` time-travel does not scale (High, P2) → Phase-2 spike at target graph size; fallback `indradb` +
versioning. Conformal coverage fails on domain data (High, P3) → Phase-3 calibration spike on oncology golden sets.
On-prem GPU throughput too low (High, P5) → Phase-5 throughput spike; abstention as cost control. Parser fragility
on real VCF/DICOM (Medium, P1) → `cargo-fuzz` on malformed real files. PHI leak via logs or egress (High, P6) →
adversarial redaction testing; deny-by-default egress proxy. Pure-Rust SQLite not yet drop-in (Low, P5) →
**resolved**: turso adopted behind the seam, validated faster than C SQLite. Audit-trail gap under failure (High,
P6) → record-before-return invariant; `proptest` the MCP host. Benchmark validity/overfit (Medium, P4) →
expert-built held-out golden sets; periodic refresh. Abstraction leak across crate boundaries (Medium, P0) →
trait-boundary review gate before each pillar phase.

---

# 8. Quickstart & validation

## 8.1 Build & run the walking skeleton

```bash
cargo run --bin oncora                 # ingest a tiny corpus, ask a target-discovery
                                       # question, print a cited, confidence-scored answer
cargo run --bin oncora -- "Is BRAF actionable in melanoma?" BRAF
cargo run --bin oncora-api             # HTTP API on :8080 (GET /health, /tools; POST /query)
cargo test --workspace                 # unit tests across all crates
cargo xtask ci                         # fmt --check + clippy -D warnings + tests
```

## 8.2 Trait-swappability in action (each seam green against a real backend)

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

True end-to-end — every external seam real at once (`--features e2e`): real embeddings (Ollama) → qdrant (vectors)
+ oxigraph (graph) → rmcp (tools) → Ollama (LLM):

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

The agent runtime never changed across any swap — only the concrete backends behind the `oncora-core` traits did.

Docs site (optional): `make setup` (one-time: build venv + markdown toolchain); `make serve` (build + serve at
http://localhost:8137; self-contained, offline once served).

## 8.3 Acceptance-criteria → validation mapping

| AC | Validated by |
|---|---|
| AC-1 end-to-end cited answer or abstention | `cargo run --bin oncora`; `--features openai/qdrant/e2e` runs |
| AC-2 every tool call audited (allow+deny) before result | `oncora-mcp-host --features rmcp` loopback; MCP audit log |
| AC-3 bit-for-bit replay | `oncora-cli replay --manifest <blake3>` |
| AC-4 golden set gates CI | `cargo xtask ci`; `oncora-eval` gate (`eval.yml`) |
| AC-5 cross-modal fused answer | Phase-1 multimodal pipelines (in progress) |
| AC-6 cross-session memory reuse | `redb` persistent `MemoryStore` conformance; second-session reuse |
| AC-7 calibrated uncertainty | `oncora-uncertainty` ECE gate; conformal coverage (Phase-3 depth pending) |
| AC-8 beats baselines, replayable | `oncora-eval` three-way comparison + CI gate (needs real golden sets) |
| AC-9 scaled topology | Phase-5 deliverables (pending) |
| AC-10 governance / lineage / PHI-egress | Phase-6 deliverables (pending) |

## 8.4 Prototype status

**A well-tested, end-to-end functional prototype**, held intentionally — demonstrates the thesis (every external
dependency behind a swappable `oncora-core` trait) against *real* backends, validated over real PubMed literature
at scale. What remains is production hardening (Phases 5–6) + research-pillar depth — **not core correctness.** The
runtime (`run_target_discovery`: retrieve → reason → verify → score → accept/abstain/escalate → persist) was
**never changed** as backends were swapped from in-memory references to real implementations.

**Seams verified against real backends:**

| `oncora-core` trait | Real backend | Verified |
|---|---|---|
| `ModelProvider` | Ollama (`qwen2.5:0.5b`) via `async-openai` | live inference; accept/abstain works |
| `EmbeddingProvider` | Ollama (`all-minilm`, 384-d) | semantic ranking confirmed |
| `VectorStore` | qdrant (gRPC) | round-trip + full agent E2E |
| `GraphStore` | oxigraph (RDF/SPARQL) | conformance + per-edge confidence |
| `MemoryStore` | redb (persistent, on disk) | conformance; O(1) write |
| `LedgerStore` | **pure-Rust SQLite (`turso`)** — chosen | 14,469 writes, 0 errors, ~23% faster than C SQLite |
| `ToolHost` | rmcp (real MCP server + client) | loopback round-trip |

## 8.5 Real-world validation (14,469 PubMed abstracts, all-real pipeline)

Corpus **14,469 documents** across 15 throttled batches. Config: embed `all-minilm` (dim 384) · chat `qwen2.5:0.5b`
· qdrant · redb (on disk). Source: PubMed; per-record DOIs preserved; no stubs/mocks. Corpus topics (docs): IDH
mutant glioma 1137 · astrocyte 1500 · brain tumor 1263 · diffuse midline glioma 728 · glioblastoma 1463 ·
glioblastoma immunotherapy 1303 · glioblastoma temozolomide 1215 · glioma 1456 · glioma stem cells 1326 · low grade
glioma 951 · microglia 1256 · oligodendroglioma 871 → total 14,469.

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
3,960 ms (warm). The data plane is single-digit-to-low-tens of ms throughout: **model, not infrastructure, is the
cost.** The provenance ledger is written **inside the agent loop** (a record per run).

## 8.6 Cold-start C-vs-Rust SQLite comparison

Two cold-start bulk runs over the full 14,469-doc corpus, identical except the `LedgerStore` SQLite backend:

| Backend | writes | mean ms | min | max | ingest wall | docs/s |
|---|---|---|---|---|---|---|
| C SQLite (`rusqlite`) | 14,469 | 2.663 | 2.076 | 8.919 | 1004.458 s | 14.405 |
| Rust SQLite (`turso`) | 14,469 | 2.041 | 1.751 | 10.035 | 990.443 s | 14.609 |

**Result.** At 14,469 sequential per-document writes, **pure-Rust SQLite (`turso`) had the lower mean latency
(1.30× / ~23% faster)**; both completed without error. turso is viable as the dev relational/ledger backend at this
scale, behind the same `LedgerStore` trait. **Decision: turso is the chosen embedded ledger.**

## 8.7 Bugs surfaced by real data/scale & remaining work

**Bugs fixed:** O(N²) memory dedup → O(1) index; oxigraph IRI encoding for arbitrary tokens; qdrant fd-limit;
report-generator Unicode-line handling. **Remaining work — quick wins:** inference/embedding request concurrency
(batching left idle cores); memory `read` O(N)→indexed by scope (`write` is O(1)); qdrant client/server version
bump (1.18 vs 1.12.4). **Research pillars:** cozo `GraphStore` (one of two blockers cleared by turso;
`graph_builder`/`rayon` compile break remains — pin/patch or run cozo out-of-process); decide adopt-or-retire
`rig`/`swiftide`; uncertainty depth (conformal + post-hoc calibration fitted from the eval harness — currently a
temperature calibrator + grounded verifier); real golden datasets + baseline arms for `oncora-eval`; multi-agent
topology as an actual runtime (currently a single end-to-end function). **Phase 5 / Phase 6:** see §7.
