# Oncora Specification

> The **what** and **why**. This specification is deliberately implementation-agnostic:
> it states the problem, the actors, the user-visible behaviours, and the testable
> requirements. The **how** lives in [plan.md](plan.md), [research.md](research.md),
> [data-model.md](data-model.md), and [contracts/](contracts/). Principles referenced as
> `P-*` are defined in [constitution.md](constitution.md).

---

## 1. Problem statement

Oncology drug discovery is a **multimodal evidence-integration problem under deep
uncertainty**. A single decision — *is this target druggable in this indication; does
this variant confer resistance to this compound; is this trial design supported by the
mechanistic evidence* — depends on fusing peer-reviewed and preprint **literature**,
structured **multi-omics** measurements, curated **bio-ontologies / pathway graphs**, and
**DICOM imaging**, each with its own schema, vocabulary, noise model, and access
constraints. The data is heterogeneous, versioned, frequently contradictory, and almost
always governed by privacy and IP boundaries that forbid it leaving the institution.

A generic RAG chatbot fails on every axis that matters: it treats retrieval as top-k text
lookup and ignores structured omics, graph relationships, and imaging; it produces fluent
prose with no durable per-claim provenance; it has no calibrated uncertainty and no
principled abstention; it is non-reproducible against a drifting index and a moving model;
and it routinely leaks data to third-party endpoints. None of these are acceptable when
the output feeds a go/no-go on a multi-year, multi-million-dollar program, a manuscript, or
a regulatory submission.

**Oncora is the opinionated alternative:** a typed reasoning system where uncertainty,
provenance, and reproducibility are first-class values in the data model, not afterthoughts
in the prompt.

---

## 2. Goals

- **G-1** Fuse the four oncology modalities — literature, multi-omics, biomolecular
  knowledge graph, imaging — into a single reasoning substrate, not four silos.
- **G-2** Attach machine-checkable provenance to **every** claim (sources, tool calls,
  model pin, data snapshot).
- **G-3** Treat uncertainty as a typed, calibrated, first-class value that drives an explicit
  accept / abstain / escalate decision.
- **G-4** Be reproducible enough to publish: pinned models, pinned snapshots, content-addressed
  artifacts, deterministic replay.
- **G-5** Persist and reuse knowledge across sessions via a principled, versioned agent memory.
- **G-6** Run entirely on-prem / in-VPC by default; PHI and IP never cross the boundary.
- **G-7** Beat human experts and computational baselines on speed, accuracy, and reliability,
  and prove it with a benchmark harness that gates CI.
- **G-8** Be Rust-native end to end, with all domain tools exposed through MCP.

## 3. Non-goals

Per the constitution's scope guardrails: not a general-purpose chatbot; not a cloud SaaS
with default external calls; not a wet-lab/LIMS/ELN replacement; not a source-of-truth
datastore for primary research data; not a system that guesses to be helpful (silence is a
valid, logged outcome); not a polyglot microservice zoo.

---

## 4. Actors

| Actor | Role | Trust | Primary entry |
|---|---|---|---|
| **Scientist** | Initiates discovery workflows; consumes evidence-bearing answers; receives abstentions/escalations | Authenticated, inside boundary | `oncora-api` (HTTP/gRPC) |
| **Reviewer / Approver** | Human expert who adjudicates escalated decisions and approves promotions of semantic memory | Authenticated, inside boundary | `oncora-api` review queue |
| **Automated pipeline** | Scheduled/CI-driven runs: ingestion, re-indexing, benchmark gating, batch hypotheses | Service identity, inside boundary | `oncora-api` (gRPC) / `oncora-cli` |
| **Operator** | Deploys, configures, monitors; manages snapshots and upgrades | Service/admin identity | `oncora-cli`, ops tooling |

---

## 5. The four pillars (capability areas)

1. **Multimodal reasoning** — reason over text, structured omics, the KG, and imaging as one
   coherent evidence space; specialist agents per modality; a hybrid retrieval layer fuses
   vector, graph, and recency signals under a token budget. *Cross-modal evidence assembly,
   not four parallel chatbots.*
2. **Agent memory architecture** — retain learned knowledge and make context-aware decisions
   across multi-session workflows via five memory types with disciplined write/read paths,
   keyed by `(scientist, project, workflow)`.
3. **Robustness & uncertainty** — typed, calibrated confidence; decomposed uncertainty;
   grounding against deterministic oracles and citation entailment; conformal set-valued
   outputs with an abstention guarantee; explicit accept/abstain/escalate.
4. **Reliability & benchmarking** — faster/more-accurate/more-reliable than experts and
   computational baselines, proven reproducibly by a CI-gating harness.

---

## 6. Primary workflows & user stories

Oncora has three flagship workflows. Each is the unit of both product value and evaluation.

### 6.1 Target discovery

> *As a scientist, I ask Oncora to nominate and substantiate a candidate target, so that I
> get a cited, confidence-scored druggability/association call I can defend to a review board.*

- Unit of a case: a `(target, disease)` druggability/association question.
- Expected output: an `Evidence`-bearing answer with a calibrated `Confidence`, citations
  resolvable to the snapshot, a full audit trail, and a `Verdict` (Accept / Abstain / Escalate).

### 6.2 Translational research

> *As a scientist, I ask whether a variant confers sensitivity/resistance to a compound in a
> context, so that I get a mechanism-aware interpretation grounded in guideline/curation tiers.*

- Unit of a case: a `(variant, compound, context)` sensitivity/resistance or mechanism question.

### 6.3 Trial design / matching

> *As a scientist, I ask whether a cohort/patient profile matches a trial, or whether a trial
> design is sound, so that I get an eligibility adjudication or design review with rationale.*

- Unit of a case: a `(cohort or patient profile, trial)` eligibility/match question, or a
  design-soundness question.

### 6.4 Cross-cutting stories

- **Reviewer audit.** *As a reviewer, I trace any returned sentence back through its `Evidence`,
  its `ToolCallId`s, to the exact snapshot lines, so I can adjudicate an escalation.*
- **Cross-session resumption.** *As a scientist returning to the same target weeks later, I see
  my accumulated, decayed-and-consolidated memory rather than starting cold.*
- **Deterministic replay.** *As an operator/auditor, I replay any historical run from its
  manifest and reproduce its output bit-for-bit.*
- **Reproducible benchmark.** *As an external reviewer, I re-derive any figure in a paper with
  one command from the manifest and pinned snapshots.*

---

## 7. Functional requirements

IDs are stable and referenced by plan, contracts, tasks, and quickstart.

### 7.1 Ingestion & knowledge (pillar 1)

- **FR-ING-1** Ingest four modalities via a shared five-stage contract
  **source → normalize → embed → index → graph**: literature (PubMed abstracts/open
  full-text, bioRxiv/medRxiv preprints, internal reports), omics (VCF/BCF + tabular
  Parquet: expression, mutation, CNV), imaging (DICOM), and ontologies (GO, Reactome,
  ChEMBL, UMLS, MONDO, HGNC).
- **FR-ING-2** Pull every source into an **immutable, content-addressed snapshot** identified
  by a `SnapshotId` (BLAKE3); a new pull is a new id, never an edit. *(P-3, P-6)*
- **FR-ING-3** Attach `SourceRef`, `SnapshotId`, and the producing `ModelPin` to every derived
  record (chunk, vector, triple, evidence edge). Vectors never travel without metadata. *(P-4)*
- **FR-ING-4** Literature: strip markup, section-aware semantic chunking with overlap, extract
  sentence-level claim candidates, resolve recognized entities to canonical URIs.
- **FR-ING-5** Omics: parse VCF to typed `Variant` records (locus, ref/alt, consequence, gene)
  behind the VCF MCP server; validate tabular omics against schema; normalize to HGNC symbols
  and canonical loci; query analytically in place (dataframes / SQL over Parquet) — agents never
  touch raw files.
- **FR-ING-6** Imaging: parse DICOM behind the imaging MCP server; **de-identify at the MCP
  boundary**; pixel data and PHI stay on-prem; only de-identified features/embeddings are indexed.
- **FR-ING-7** Ontologies: load as triples queryable by SPARQL; this layer is the **identity
  authority** — every entity mentioned elsewhere resolves to a URI here. Pin each ontology release
  as a versioned snapshot; treat a version bump as a re-index event.
- **FR-ING-8** Embeddings are **on-prem, pinned, versioned**; a collection's dimension is fixed
  at creation; mixing models/dimensions in a collection is forbidden; a model change is a new pin
  forcing a full re-index, never a silent in-place mix.

### 7.2 Knowledge graph & retrieval (pillar 1)

- **FR-KG-1** Maintain a **dual knowledge graph**: an ontology/canonical-entity layer (RDF/SPARQL,
  stable URIs) and an evidence/assertion graph (Datalog, time-travel, **per-edge confidence and
  provenance**). Expose both behind a single graph query surface so callers express **intent, not
  store choice**.
- **FR-KG-2** Support the canonical entity/edge schema: `Target`, `Disease`, `Pathway`, `Compound`,
  `Variant`, `Trial`, `Cohort`, `Claim`, `Evidence`, `Source`, `Provenance` with edges
  `involved_in`, `associated_with`, `modulates`, `located_in`, `confers_sensitivity/resistance`,
  `targets`, `tests`, `characterizes`, `about`, `supports/contradicts`, `derived_from`, `yields`,
  `has` (see [data-model.md](data-model.md)).
- **FR-KG-3** Support **point-in-time** queries: "what did we believe as of snapshot X" is a
  first-class query. *(P-10, P-6)*
- **FR-RET-1** Provide **hybrid retrieval**: plan the query into the substores it needs, fan out
  in parallel (text vectors, multimodal vectors, SPARQL ontology, Datalog evidence), fuse
  heterogeneous scores (reciprocal-rank-fusion default; weighted optional per task class),
  re-rank with a pinned cross-encoder, and carry provenance through to the answer.
- **FR-RET-2** Degrade gracefully: a slow or empty store degrades the result via per-store
  timeouts, it does not block it.
- **FR-RET-3** Assemble context under the model's **token budget**: deduplicate, order by fused
  score, truncate/summarize to fit, keep provenance handles, and **surface competing evidence
  together** so the agent can reason about contradiction.
- **FR-RET-4** The semantic-memory read path **is** this hybrid retrieval layer with a
  memory-scoped filter — one retrieval layer, not one per subsystem.

### 7.3 Agent runtime (pillars 1 & 3)

- **FR-AGT-1** Execute a fixed orchestration pipeline: **Planner → Domain Specialists
  (genomics, literature, imaging, clinical) → Verifier → Uncertainty Scorer → Responder**, with
  memory read at *perceive* and written at *consolidate*.
- **FR-AGT-2** Execute the agent loop: *perceive → plan → act (MCP tool calls) → observe →
  reflect → score → decide → consolidate*, terminating in exactly one `Verdict`.
- **FR-AGT-3** Route every domain tool call through the single MCP host chokepoint;
  execute deterministically; record to episodic memory + provenance ledger (allow and deny). *(P-5)*
- **FR-AGT-4** Run specialists concurrently under a shared concurrency limit; a cancelled/timed-out
  run drops in-flight tool calls and records them as such. *(see NFR-PERF, NFR-ROBUST)*
- **FR-AGT-5** Bound any `Escalate → re-reason` loop by a per-run cancellation token and a
  max-iteration cap; it is never an open loop.

### 7.4 Memory (pillar 2)

- **FR-MEM-1** Maintain five memory types — **working, episodic, semantic, procedural,
  provenance/evidence** — each with defined purpose, contents, lifetime, and store mapping (see
  [data-model.md](data-model.md)).
- **FR-MEM-2** Key memory cross-session by `(scientist, project, workflow)`; this key is the
  retrieval scope and the provenance attribution subject; cross-tenant reads are denied.
- **FR-MEM-3** Write path: **extract → dedup → conflict-resolve → consolidate → decay**.
  - *Extract:* lift typed `(claim, support, contradiction, confidence, provenance)` candidates;
    candidates that cannot be typed to a KG entity stay episodic-only, not promoted to semantic.
  - *Dedup:* two-stage — embedding near-duplicate detection proposes; **KG entity resolution is
    the authoritative tie-breaker** (same resolved entities + predicate ⇒ same assertion).
  - *Conflict-resolve:* never overwrite; inputs are recency, source authority, calibrated
    confidence; contradictions retained as competing evidence; "current belief" is a computed view.
  - *Consolidate:* staged promotion working→episodic (unconditional, immutable) →semantic (only if
    it resolves to a KG entity, survives dedup, and clears the confidence/verdict bar); procedural
    promotion for plans/tool-sequences whose runs ended in high-confidence `Accept`.
  - *Decay:* a decay score (time-since-use + access frequency); below a floor ⇒ soft-delete with
    tombstone, excluded from default retrieval, revivable if re-observed. *(P-10)*
- **FR-MEM-4** **Episodic history and provenance are never decayed or deleted.** They are the
  replay substrate.
- **FR-MEM-5** Every memory write stamps `SnapshotId`, `ModelPin`, and a BLAKE3 content hash of
  the CAS payload, making the run deterministically replayable. *(P-6)*
- **FR-MEM-6** Read path: hybrid retrieve → fuse (including calibrated-confidence weighting;
  low confidence is down-weighted, not hidden) → filter tombstoned (unless point-in-time) →
  assemble under token budget with provenance handles.

### 7.5 Uncertainty & verification (pillar 3)

- **FR-UNC-1** Every claim carries a `Confidence` and `Provenance`; a claim missing either cannot
  reach the responder. *(P-4, P-7)*
- **FR-UNC-2** Decompose uncertainty into **aleatoric / epistemic / retrieval / tool** sources,
  each estimated by a distinct mechanism; the policy routes differently on the dominant source.
- **FR-UNC-3** Provide the estimator suite, layered cheap-first: self-consistency (N samples,
  N per task class: trivial=1, high-stakes=5–9), multi-model/multi-prompt ensembles, **deterministic
  oracle grounding** (clinical calculators + KG; oracle mismatch can force abstain regardless of
  model confidence), conformal prediction (set-valued output with distribution-free coverage ≥ 1−α),
  ECE measurement, and citation-grounding/NLI entailment (every asserted fact must be entailed by a
  retrieved pinned source; all-neutral ⇒ ungrounded ⇒ abstain).
- **FR-UNC-4** Run the verifier set per claim: **citation**, **oracle**, **consistency**,
  **contradiction**, and **schema/unit** verifiers. Verifiers adjudicate; they do not generate answers.
- **FR-UNC-5** The **uncertainty scorer is the single point** that fuses verifier outputs, applies the
  per-task-class calibrator to produce a final `Confidence` + `CalibrationMethod`, and is the **only
  component allowed to emit `Accept`**. It refuses to `Accept` a `Raw` (uncalibrated) confidence. *(P-7)*
- **FR-UNC-6** Provide **post-hoc calibration** behind a trait (temperature scaling, isotonic
  regression, conformal), fit per task class on held-out `(raw_score, correct?)` pairs from the eval
  harness; calibrators are content-addressed and tied to a `ModelPin` + `SnapshotId`.
- **FR-UNC-7** Recalibrate on any `ModelPin` change (mandatory before promotion), any `SnapshotId`
  change affecting a task class, on measured-ECE drift over rolling shadow evals, and **quarterly minimum**.
- **FR-UNC-8** Decide a `Verdict` by a **pure function of typed inputs**, configurable per task class
  (defaults conservative; high-stakes classes bias toward escalation). The decision table is normative
  (see §10 acceptance criteria and [data-model.md](data-model.md)).
- **FR-UNC-9** Aggregate a response conservatively: **a response is only as confident as its weakest
  load-bearing claim** (min-confidence over planner-identified load-bearing claims). Every accepted
  claim renders with its `Confidence` and a citation trail; abstained/escalated claims are surfaced
  explicitly with reason.

### 7.6 Evaluation & benchmarking (pillar 4)

- **FR-EVAL-1** Score against **two references always**: a human-expert reference label and ≥1 named
  computational baseline; baselines run through the **same harness, same snapshots, same scorer** as
  the agent. *(P-8)*
- **FR-EVAL-2** Curate a versioned golden set per flagship workflow: candidate cases only from pinned
  snapshots; ≥2 independent expert labels `{decision, rationale, cited evidence, confidence}`; senior
  adjudication; recorded inter-annotator agreement (κ); a stratum each for **normal / hard-ambiguous /
  abstain-expected** plus wrong-but-plausible distractors; BLAKE3-hashed CAS artifact; **dev / test /
  sealed-holdout** splits with split membership baked into the content-addressed snapshot.
- **FR-EVAL-3** Report a fixed metric panel per workflow: accuracy/F1, task-specific score, **ECE +
  reliability diagram**, **coverage-vs-risk + selective accuracy + AURC**, abstention precision/recall,
  latency p50/p95, and cost (tokens, compute). Calibration is scored on **calibrated** confidence, never
  raw logits. Abstention quality is a metric, not an escape hatch.
- **FR-EVAL-4** Provide **deterministic replay from a `RunManifest`** (model pins, snapshot ids, seeds,
  config hash, code provenance, baseline arm pins, manifest digest); refuse to run against an unpinned
  model or non-content-addressed dataset; record every run (inputs, raw model-I/O fixtures, per-case
  results, scores, report) to CAS + ledger.
- **FR-EVAL-5** Gate CI (block merge) on: accuracy floor, calibration floor/ceiling, abstention floor,
  baseline dominance, latency, and reproducibility (fixture-replay reproduces prior score within
  tolerance). Default CI replays from **recorded content-addressed fixtures** (deterministic, no GPU);
  live-model runs are scheduled/nightly with tolerance bands.
- **FR-EVAL-6** Provide one-command replay (`oncora-cli eval replay --manifest <blake3>`) and
  figure regeneration; every reported number carries a back-pointer
  `report → ExperimentId → RunManifest digest → per-case RunResult digests → recorded fixtures`.

### 7.7 API, CLI & governance (cross-cutting)

- **FR-API-1** `oncora-api` is the **sole entry point** across the trust boundary: HTTP + gRPC, auth,
  RBAC, request orchestration; a query endpoint, a review queue, ingestion/eval triggers, health/readiness.
- **FR-CLI-1** `oncora-cli` provides operator/developer commands: submit query, **deterministic replay**,
  ingest, eval (replay/figure). *(see [contracts/cli.md](contracts/cli.md))*
- **FR-GOV-1** Enforce RBAC (roles Scientist · Reviewer · Pipeline · Operator) at `oncora-api` and
  **re-checked at the MCP host policy gate**; scope by `(project, workflow)`.
- **FR-GOV-2** Route every `Verdict::Escalate` to a human **reviewer adjudication queue**; promotions to
  semantic memory and escalations require reviewer approval.
- **FR-GOV-3** Maintain a **data-lineage / provenance ledger** (append-only, content-addressed) supporting
  a lineage walk from any returned claim to its sources, tool calls, model pin, and snapshot.
- **FR-GOV-4** Tag PHI/PII at ingest; redact in logs/traces; **block PHI at the egress proxy**. *(P-2)*

---

## 8. Non-functional requirements

- **NFR-RUST-1** Rust-native end to end; exactly three sanctioned non-Rust surfaces, each behind a
  named trait. *(P-1, P-9)*
- **NFR-PRIV-1** On-prem/VPC-only by default; the egress proxy is the only outbound path, deny-by-default,
  allow-listed, PHI-stripping, fully logged; cloud model use stamped into the run's model pin. *(P-2)*
- **NFR-REPRO-1** Deterministic replay: pinned toolchain, committed `Cargo.lock`, single resolved version
  per shared dep, BLAKE3 content-addressing, recorded seeds, stable recordable model identity (build +
  weights digest, not a friendly name), `insta` snapshot locks. *(P-6)*
- **NFR-PROV-1** Provenance on every claim, memory item, and benchmark number — no special exemptions. *(P-4)*
- **NFR-AUDIT-1** Record-before-return invariant at the MCP host; both allow and deny recorded; the audit
  trail is itself replayable bit-for-bit. *(P-5)*
- **NFR-CALIB-1** Post-calibration ECE ≤ **0.05** absolute on the golden set; no ECE regression > **0.01**
  vs last green main. Conformal empirical coverage ≥ **1−α**. *(P-7, P-8)*
- **NFR-ACC-1** Task accuracy/F1 must not drop > **1.0 pp** vs last green main and must beat the strongest
  baseline arm on `test`. *(P-8)*
- **NFR-ABST-1** AURC must not regress > **0.01** vs main; abstain-recall on the abstain-expected stratum
  must stay above its floor. *(P-8)*
- **NFR-PERF-1** Per-run structured concurrency on an async runtime: per-run cancellation token; per-tool/
  per-model timeouts; bounded channels for backpressure; semaphores capping concurrent model and per-server
  tool calls (protect GPUs and oracles); child tasks owned by the run. The hard ceiling is **GPU token
  throughput**; uncertainty-driven abstention is also a cost control.
- **NFR-PERF-2** Latency p95 within a per-task-class budget; latency-sensitive hot paths (retrieval fusion,
  scorer, manifest hashing) guarded by microbenchmarks with regression detection. End-to-end latency is
  expected to be **LLM-bound**; the data plane (embeddings, vector, graph, memory) is single-digit-to-low-tens
  of milliseconds.
- **NFR-SCALE-1** One codebase, two deployment shapes (single-node embedded; multi-node service-backed);
  the difference is **configuration, not code**. Agent and retrieval tiers are stateless and scale by replicas;
  all durable state lives in the data tier.
- **NFR-ROBUST-1** Property tests on invariants (memory write/read, uncertainty/confidence math, parsers);
  fuzzing of every external-input parser (VCF, DICOM, JSON tool args); snapshot tests on prompts/traces/manifests;
  microbenchmarks; differential testing against oracles/baselines.
- **NFR-OBS-1** Structured spans + OpenTelemetry, on-prem collector only; **no PHI in span fields**; runs
  correlated end-to-end by run id.
- **NFR-GXP-1** Built to the spirit of **ALCOA+**: Attributable, Legible, Contemporaneous, Original, Accurate,
  plus Reproducible and Auditable — a concrete trail for any answer influencing a go/no-go or submission. (Not a
  validated GxP system out of the box.)
- **NFR-QUAL-1** Quality gates a PR must pass: `cargo fmt --check`; `cargo clippy --all-targets --all-features
  -D warnings`; `cargo deny check`; `cargo test --workspace` with a coverage floor on core/uncertainty/memory;
  property tests + scheduled fuzzing; `criterion` regression tracking; the `oncora-eval` golden-set gate.
- **NFR-DOC-1** Rustdoc on every public item (`#![deny(missing_docs)]` on libraries); runnable doc-tests for
  provider seams; ADRs for significant decisions.

---

## 9. Edge cases & required behaviours

| Situation | Required behaviour |
|---|---|
| No entailing source for an asserted fact (NLI all-neutral) | `Abstain { UngroundedClaim }` — never assert ungrounded |
| Oracle (calculator/KG) disagrees with the model on a verifiable sub-claim | `Abstain` / `Escalate`; **never override a calculator/KG with prose** |
| Conformal set larger than the task-class max | `Abstain { ConformalSetTooLarge }`; return the set, do not pick |
| Confidence below abstain-floor, aleatoric-dominant | `Abstain { LowCalibratedConfidence }` — more work will not help |
| Confidence in escalate-band, epistemic-dominant | `Escalate { SpecialistAgent }` — more targeted evidence may resolve it |
| Confidence below abstain-floor, high-stakes task class | `Escalate { HumanReviewer }`; log full evidence bundle |
| Retrieval coverage below floor | `Abstain { InsufficientRetrieval }`; flag for ingestion backfill |
| New claim contradicts an existing one | Retain both edges; current-belief view per recency+authority+confidence; weak evidence cannot silently flip a belief |
| Entity resolution ambiguous | Do not merge; keep separate; queue for resolution; escalate if needed |
| Existing fact superseded by a trial readout | Add a new time-stamped edge; old remains queryable via time-travel |
| Entry below decay floor | Tombstone; exclude from default read; **keep provenance** |
| A run is cancelled or times out | Propagate the cancellation token; drop in-flight tool calls; record them as dropped in episodic memory |
| Numeric claim without units / out of physiological range | Schema/unit verifier hard-rejects before scoring |
| Ontology version bump | Treat as a re-index event; old evidence stays interpretable against the ontology it was asserted under (time-travel) |
| Embedding model change | New pin; full re-index; never a silent in-place mix of dimensions/models |

---

## 10. Acceptance criteria (normative)

A build satisfies this specification when:

- **AC-1 (end-to-end).** A target-discovery query runs end-to-end and returns an `Evidence`-bearing
  answer with a calibrated `Confidence`, citations resolvable to the snapshot, and a full audit trail,
  **or** a logged abstention with reason. *(FR-AGT-*, FR-UNC-*)*
- **AC-2 (audit).** Every MCP tool call (allow and deny) is recorded to **both** the provenance ledger
  and episodic memory before its result reaches the agent. *(P-5, FR-AGT-3, NFR-AUDIT-1)*
- **AC-3 (replay).** `oncora-cli replay` reproduces a run **bit-for-bit** from pinned model + snapshot +
  CAS. *(P-6, FR-CLI-1)*
- **AC-4 (gate).** The golden set runs in CI and **blocks merge** on any tripped gate (accuracy,
  calibration, abstention, baseline dominance, latency, reproducibility). *(P-8, FR-EVAL-5)*
- **AC-5 (cross-modal).** A query requiring literature **and** omics **and** imaging evidence returns one
  fused, cited answer; each modality is content-addressed and snapshot-tagged; no source is ever written to.
  *(FR-ING-*, FR-RET-1)*
- **AC-6 (cross-session memory).** A second session on the same `(scientist, project, workflow)` demonstrably
  reuses prior semantic + procedural memory; conflicting facts are retained as competing evidence; every entry
  carries snapshot + model pin and is replayable. *(FR-MEM-*)*
- **AC-7 (calibrated uncertainty).** Confidence is calibrated (ECE below target on a held-out set); conformal
  sets carry the guaranteed coverage; oracle disagreement reliably triggers `Escalate`; every abstention is
  logged with a reason. *(FR-UNC-*, NFR-CALIB-1)*
- **AC-8 (beats baselines).** Benchmarks beat the named baselines on the agreed metrics simultaneously
  (accuracy, calibration, latency); results are deterministically replayable; a regression cannot merge. *(P-8)*
- **AC-9 (scaled).** Horizontal scale demonstrated under load; Postgres and qdrant failover verified; an upgrade
  validated by golden-set replay diff; rollback confirmed. *(NFR-SCALE-1)*
- **AC-10 (governance).** An external reviewer can trace any returned claim to its sources/tool-calls/snapshot;
  PHI demonstrably cannot leave the boundary; a validated replay reproduces any historical run. *(FR-GOV-*, NFR-GXP-1)*

---

## 11. Assumptions & dependencies

- Foundation models are the one unavoidable non-Rust dependency; they sit behind `ModelProvider` and run
  on-prem by default; any cloud endpoint is opt-in per deployment behind the egress proxy.
- In-house MCP servers for genomics (VCF), DICOM imaging, and clinical calculators are assumed to exist;
  Oncora composes and orchestrates them and hosts its own (KG, retrieval, memory).
- Data sources are snapshotted into the trust boundary; Oncora reads snapshots and never writes back.
- Expert labelers are available to construct and refresh golden sets; benchmark validity (not the harness)
  is the principal evaluation risk.
