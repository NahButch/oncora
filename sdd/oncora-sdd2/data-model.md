# Oncora Data Model

> Entities, types, relationships. Canonical types in `oncora-core` (one language for all crates); concrete stores mapped per type. Rust sketches illustrative but normative in shape + field set. See [contracts/core-traits.md](contracts/core-traits.md) for trait surface; [spec.md](spec.md) for requirements.

---

## 1. Core uncertainty & provenance types (`oncora-core`)

```rust
// oncora-core::uncertainty

/// Calibrated probability in [0,1]. Construction fallible so out-of-range never silently propagates. No `From<f64>`.
pub struct Confidence(f64);
impl Confidence {
    pub fn new(p: f64) -> Result<Self, UncertaintyError> {       // Err(OutOfRange) if !(0.0..=1.0)
        if (0.0..=1.0).contains(&p) { Ok(Self(p)) } else { Err(UncertaintyError::OutOfRange(p)) }
    }
    pub fn get(self) -> f64 { self.0 }
}

/// How a Confidence was post-hoc calibrated; travels with the value.
pub enum CalibrationMethod {
    Raw,               // uncalibrated model score — REJECTED by the scorer; never past it
    TemperatureScaled,
    Isotonic,
    Conformal,         // confidence implied by a calibrated prediction set
}

/// Where a claim's support came from. Every field is enough to replay the decision.
pub struct Provenance {
    pub sources: Vec<SourceRef>,     // retrieved docs / KG nodes / datasets
    pub tool_calls: Vec<ToolCallId>, // deterministic, audited MCP calls
    pub model: ModelPin,             // pinned weights + decoding params
    pub snapshot: SnapshotId,        // content-addressed data snapshot (BLAKE3)
}

/// Directional evidence for or against a claim.
pub struct Evidence {
    pub claim: ClaimId,
    pub support: Vec<EvidenceItem>,        // entailing sources / agreeing oracles
    pub contradiction: Vec<EvidenceItem>,  // refuting sources / disagreeing oracles
    pub confidence: Confidence,
    pub method: CalibrationMethod,
    pub provenance: Provenance,
}

pub struct EvidenceItem {
    pub source: SourceRef,
    pub strength: f64,        // NLI entailment prob, or oracle agreement
    pub relation: Relation,   // Entails | Contradicts | Neutral
}

/// Decomposed uncertainty so the policy can act on *why* we are unsure.
pub struct UncertaintySources {
    pub aleatoric: f64,  // irreducible data noise (assay variance, label noise)
    pub epistemic: f64,  // model / knowledge gaps (reducible with more evidence)
    pub retrieval: f64,  // corpus / KG coverage gaps
    pub tool: f64,       // oracle disagreement / calculator domain errors
}

/// Terminal outcome of reasoning. Abstain/Escalate are first-class, not failures.
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

**Design invariants.**
- `Confidence::new` fallible by construction — out-of-range probability is a bug to surface, not absorb.
- `CalibrationMethod::Raw` representable but **rejected by the scorer in code** — no `Accept` may carry `Raw` confidence.
- `UncertaintySources` is a decomposition, not a scalar — **policy routes differently** on dominant source (high epistemic → escalate to specialist; high aleatoric → abstain).
- `Provenance` is the single struct shared by uncertainty **and** every memory write — one provenance type system-wide.

### 1.1 Identifier types (`oncora-core::ids`)

`SourceRef`, `ToolCallId`, `ModelPin` (model + version + decode config / weights digest + serving backend), `SnapshotId` (BLAKE3 content-addressed data snapshot), `ClaimId`, plus `ScientistId`, `ProjectId`, `WorkflowId`, `MemoryId`, `CaseId`, `ArtifactId`, `ExperimentId`, `ContentHash([u8;32])` (BLAKE3).

---

## 2. The five memory types

| Type | Purpose | Contents | Lifetime | Primary engine | Secondary |
|---|---|---|---|---|---|
| **Working** | Run scratchpad — loop's volatile state | plan state, intermediate tool results, partial reasoning, pending-verification queue | run-scoped; discarded or selectively consolidated at run end | in-memory | `redb` spill |
| **Episodic** | Append-only flight recorder | every step, MCP tool call, observation, decision, `Verdict` in temporal order | durable, immutable (retention-bounded), never mutated | `redb` log | CAS payloads (BLAKE3) |
| **Semantic** | Curated belief state / KG | canonical entities + edges per KG schema; Claim/Evidence with per-edge confidence | long-lived; updated via consolidation + conflict resolution, never silently overwritten | `oxigraph` (RDF/SPARQL) + `cozo` (Datalog, time-travel) | — |
| **Procedural** | Learned how-to | successful plan templates, effective tool-use sequences, skill recipes, tuned retrieval strategies | long-lived; reinforced by success, decayed when superseded | `cozo` | CAS manifests |
| **Provenance/evidence** | Audit + reproducibility backbone | `Provenance` (sources, tool_calls, model, snapshot) + support/contradiction structure of `Evidence` | **permanent, immutable** — decay/tombstones hide a fact but provenance never lost | Postgres ledger (`sqlx`) | CAS |

### 2.1 Memory entry & store boundary

```rust
use oncora_core::{Confidence, Evidence, Provenance, SnapshotId, ModelPin};

pub struct ContentHash(pub [u8; 32]);                 // BLAKE3 identity of a payload

pub struct MemoryKey {                                 // cross-session scope key
    pub scientist: ScientistId,
    pub project:   ProjectId,
    pub workflow:  WorkflowId,
}

pub enum MemoryKind { Working, Episodic, Semantic, Procedural, Provenance }

/// One versioned, attributed, content-addressed memory entry.
pub struct MemoryEntry {
    pub id: MemoryId,
    pub key: MemoryKey,
    pub kind: MemoryKind,
    pub evidence: Evidence,         // claim + support/contradiction + confidence
    pub provenance: Provenance,     // sources + tool_calls + model + snapshot
    pub snapshot: SnapshotId,       // data snapshot pin (replay)
    pub model: ModelPin,            // generating model pin (replay)
    pub payload: ContentHash,       // CAS handle, BLAKE3
    pub decay_score: f64,           // time + usage; below floor => tombstone
    pub tombstoned: bool,
}

pub struct ReadQuery {
    pub key: MemoryKey,
    pub text: Option<String>,
    pub as_of: Option<SnapshotId>,  // point-in-time via cozo time-travel
    pub token_budget: usize,
}
```

### 2.2 Conflict-resolution policy (normative decision table)

Inputs: recency, source authority, calibrated confidence. Governing principle (P-10): **never silently overwrite; contradictions kept as competing evidence.**

| Situation | Rule |
|---|---|
| New claim, no existing assertion | Insert as new semantic fact with provenance |
| New claim duplicates existing (same entities + predicate + value) | Merge; append source + tool call to provenance; bump usage |
| New claim agrees, higher-authority source | Keep both; mark higher-authority as current belief |
| New claim contradicts, more recent + higher confidence | Retain both edges; current-belief view favors new |
| New claim contradicts, lower confidence / weaker source | Retain both; current-belief view stays old; flag for review |
| Contradiction with comparable authority + confidence | Retain both as competing evidence; surface together; abstain if asked |
| Existing fact superseded by trial readout | Add new edge; time-stamp; old remains queryable via time-travel |
| Entity resolution ambiguous | Do not merge; keep separate; queue for resolution |
| Entry below decay floor | Tombstone; exclude from default read; keep provenance |

---

## 3. Knowledge-graph schema

Physically split: canonical entities (`Target`, `Disease`, `Pathway`, `Compound`, `Variant` identity) live in `oxigraph` as URIs; `Claim`, `Evidence`, `Source`, `Provenance`, and confidence-bearing edges live in `cozo`.

ER edge-list (entity —edge→ entity, cardinality `||--o{` = one-to-many, `||--||` = one-to-one):
- TARGET ||--o{ PATHWAY : involved_in
- TARGET ||--o{ DISEASE : associated_with
- COMPOUND ||--o{ TARGET : modulates
- VARIANT ||--o{ TARGET : located_in
- VARIANT ||--o{ DISEASE : confers_sensitivity_resistance
- TRIAL ||--o{ DISEASE : targets
- TRIAL ||--o{ COMPOUND : tests
- COHORT ||--o{ DISEASE : characterizes
- CLAIM ||--o{ TARGET : about
- CLAIM ||--o{ DISEASE : about
- CLAIM ||--o{ COMPOUND : about
- CLAIM ||--o{ VARIANT : about
- EVIDENCE ||--o{ CLAIM : supports_contradicts
- EVIDENCE ||--o{ SOURCE : derived_from
- COHORT ||--o{ EVIDENCE : yields
- PROVENANCE ||--|| CLAIM : has
- PROVENANCE ||--|| EVIDENCE : has
- PROVENANCE ||--|| SOURCE : has

### 3.1 Entity attributes

| Entity | Attributes | Identity store |
|---|---|---|
| `Target` | `uri`, `hgnc_symbol`, `kind` | `oxigraph` |
| `Disease` | `uri`, `mondo_id`, `label` | `oxigraph` |
| `Pathway` | `uri`, `reactome_id`, `label` | `oxigraph` |
| `Compound` | `uri`, `chembl_id`, `label` | `oxigraph` |
| `Variant` | `uri`, `locus`, `ref_alt`, `consequence` | `oxigraph` (identity) |
| `Trial` | `id`, `phase`, `status` | `cozo` |
| `Cohort` | `id`, `n`, `description` | `cozo` |
| `Claim` | `id`, `statement`, `confidence` | `cozo` |
| `Evidence` | `id`, `polarity`, `weight` | `cozo` |
| `Source` | `id`, `kind`, `snapshot_id` | `cozo` |
| `Provenance` | `id`, `model_pin`, `snapshot_id`, `tool_calls` | `cozo` + Postgres ledger |

Edge reading: `Compound` `modulates` `Target`; `Variant` `located_in` `Target` + `confers` sensitivity/resistance; `Trial` `targets` `Disease` + `tests` `Compound`; `Cohort` `characterizes` `Disease` + `yields` `Evidence`; `Claim` `about` any canonical entity; `Evidence` `supports`/`contradicts` `Claim` + `derived_from` `Source`; **everything `has` `Provenance`.**

### 3.2 Which store a query hits

| Query intent | Store | Language |
|---|---|---|
| Resolve name/symbol → canonical entity URI | `oxigraph` | SPARQL |
| Ontology structure (pathways, gene→pathway, disease hierarchy) | `oxigraph` | SPARQL |
| Compound→target modulation from curated sources | `oxigraph` | SPARQL |
| What do we *claim*, with supporting/contradicting evidence | `cozo` | Datalog |
| Per-edge confidence + provenance of an assertion | `cozo` | Datalog |
| Point-in-time / "as of snapshot X" beliefs | `cozo` | Datalog |
| Cross-store join (canonical entity + accrued evidence) | both | SPARQL + Datalog, fused in `oncora-kg` |

---

## 4. Storage-engine mapping (what lives where)

| Store | What lives there | Query | Why this store |
|---|---|---|---|
| `qdrant` | Literature/text chunk embeddings + payload; entity-label resolution vectors | HNSW + payload filter | Rust-native server; payload filtering; quantization; text/RAG workhorse |
| `lancedb` | Imaging + multimodal + omics-signature embeddings with heavy metadata | vector search over columnar store | Columnar, multimodal, versioned; vectors travel with rich metadata |
| `oxigraph` | Ontology-grounded canonical entities + URIs (GO/Reactome/ChEMBL/UMLS/MONDO/HGNC) | SPARQL | Standards-based triplestore; identity authority |
| `cozo` | Evidence/assertion graph: claims, evidence, per-edge confidence + provenance | Datalog | Time-travel + per-edge confidence + provenance; versioned evidence + semantic memory |
| `polars` / `duckdb` | Tabular omics over Parquet snapshots | DataFrame / SQL | In-proc transforms + set-oriented SQL; analytical, not retrieval |
| Parquet via MCP | Raw omics matrices (read-only snapshots) | DuckDB SQL / Arrow | Columnar at-rest source-of-record, behind MCP boundary |
| Postgres ledger | Provenance/audit records linking artifact → sources/tools/model/snapshot | SQL (`sqlx`) | Async compile-checked SQL; durable multi-node ledger |
| `turso` (pure-Rust SQLite) | Embedded dev relational / ledger | SQL | Rust-native; validated ~23% faster than C SQLite at 14.5k writes |
| CAS over object store | Content-addressed payloads + manifests (BLAKE3) | content-hash lookup | Deterministic replay; immutable artifacts |

### 4.1 Embedding strategy

| Modality | Engine | Model class | Dim (target) | Stored in |
|---|---|---|---|---|
| Literature / text | `fastembed` over `ort` | Biomedical/general sentence embedder | 768 | `qdrant` |
| Entity labels / synonyms | `fastembed` over `ort` | Same text embedder | 768 | `qdrant` (resolution index) |
| Imaging | `candle` or `ort` | Vision/feature encoder | 512–1024 | `lancedb` |
| Omics-derived signatures | `candle` or `ort` | Tabular/profile encoder | task-specific | `lancedb` |

Rules: every embedding model referenced by a `ModelPin` (name + revision + checksum), downloaded once, content-addressed, frozen; dimension fixed per collection; cross-model comparison only via re-embedding; every vector carries `SourceRef` + `SnapshotId` + `ModelPin`; all behind `EmbeddingProvider`.

---

## 5. Evaluation types (`oncora-eval`)

```rust
use oncora_core::{Confidence, ModelPin, Provenance, SnapshotId, Verdict};

pub struct BenchmarkCase {
    pub id: CaseId,
    pub workflow: Workflow,        // TargetDiscovery | Translational | TrialDesign
    pub split: Split,              // Dev | Test | Holdout
    pub stratum: Stratum,          // Normal | Hard | AbstainExpected
    pub input: serde_json::Value,  // task input, drawn from a pinned snapshot
    pub expert: ExpertLabel,       // reference decision + rationale + cited evidence
    pub snapshot: SnapshotId,      // the golden set this case belongs to
}
pub struct ExpertLabel {
    pub decision: Decision,
    pub rationale: String,
    pub cited_evidence: Vec<SourceRef>,
    pub annotator_agreement: f64,  // kappa for the case's set
}
pub struct RunResult {
    pub case: CaseId,
    pub arm: Arm,                  // Agent | Baseline(name) | Expert
    pub verdict: Verdict,          // Accept | Abstain | Escalate
    pub answer: Option<Decision>,  // None when Abstain/Escalate
    pub confidence: Confidence,    // calibrated, with CalibrationMethod tag
    pub provenance: Provenance,    // sources, tool_calls, model, snapshot
    pub latency_ms: u64,
    pub cost: Cost,                // tokens + compute seconds
    pub fixtures: Vec<ArtifactId>, // recorded model I/O in CAS
}
pub struct Score {
    pub arm: Arm, pub split: Split,
    pub accuracy: f64, pub task_metric: f64, pub ece: f64, pub aurc: f64,
    pub selective_accuracy_at: Vec<(f64, f64)>,  // (coverage, accuracy)
    pub abstain_recall: f64,
    pub latency_p50_ms: u64, pub latency_p95_ms: u64,
    pub cost: Cost,
}
pub struct GatePolicy {
    pub max_accuracy_drop_pp: f64,   // 1.0
    pub max_ece_regression: f64,     // 0.01
    pub ece_ceiling: f64,            // 0.05
    pub max_aurc_regression: f64,    // 0.01
    pub min_abstain_recall: f64,     // floor on abstain-expected stratum
    pub must_beat_baseline: bool,    // strongest baseline arm
    pub latency_tolerance: ToleranceBand,
}
pub enum GateOutcome { Pass, Block { reasons: Vec<String> } }
```

### 5.1 `RunManifest` (complete, content-addressed run description)

`ModelPin` per role (planner, specialists, verifier, calibrator) incl. weights digest + serving backend + decode params; `SnapshotId` for the golden set and every underlying corpus/KG/omics snapshot; seeds (sampling, self-consistency, shuffling); config hash (BLAKE3 of fully-resolved `figment`-merged config); code provenance (git commit + lockfile digest); baseline arm pins; **manifest digest** (BLAKE3 of all above — *this id names the run*). Every reported number back-points: `report → ExperimentId → RunManifest digest → per-case RunResult digests → recorded model-I/O fixtures`.

---

## 6. Verdict policy (normative; consumed by the scorer)

Inputs: calibrated `Confidence`, conformal set size, oracle agreement, retrieval coverage, NLI grounding, `UncertaintySources` decomposition. Thresholds (`accept`, `escalate-band`, `abstain-floor`, `conformal-max`, `retrieval-floor`) are `figment`/`config`-layered, keyed by task class, validated at load; defaults conservative; high-stakes classes bias toward escalation.

| Condition | Verdict |
|---|---|
| Oracle disagreement on a verifiable sub-claim | `Abstain` / `Escalate` (trust the oracle; never override a calculator/KG with prose) |
| No entailing source; NLI all-neutral | `Abstain { UngroundedClaim }` |
| Conformal set size > task-class max | `Abstain { ConformalSetTooLarge }` |
| Confidence ≥ accept-threshold AND grounded AND oracle-agree AND singleton set | `Accept` |
| Confidence in escalate-band AND epistemic-dominant | `Escalate { SpecialistAgent }` |
| Confidence below abstain-floor AND aleatoric-dominant | `Abstain { LowCalibratedConfidence }` |
| Confidence below abstain-floor AND high-stakes class | `Escalate { HumanReviewer }` |
| Retrieval coverage below floor | `Abstain { InsufficientRetrieval }` |

**Aggregation:** a response is only as confident as its weakest **load-bearing** claim (min-confidence over planner-identified load-bearing claims, not all atomic claims).
