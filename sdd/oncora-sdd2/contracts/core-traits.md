# Contract — Core Provider Traits (`oncora-core`)

> Nine (+ `LedgerStore`) provider-swappable seams. Each: small, object-safe trait, one contract, defined in/near `oncora-core`, implemented in the crate owning the underlying tech. Higher layers program against the trait, never the concrete backend (P-9). Library errors typed (`thiserror`) so callers branch on failure class and drive `Verdict::Abstain`/`Escalate`.

## Summary

| Trait | One-line contract | Defined in | Implemented in (backends) |
|---|---|---|---|
| `ModelProvider` | Prompt/messages → completion or stream, with a `ModelPin` for reproducibility | `oncora-core` | `oncora-agents` (`async-openai`, Anthropic, `mistral.rs`/`candle`) |
| `EmbeddingProvider` | Embed text/multimodal items into fixed-width vectors, batched + deterministic | `oncora-core` | `oncora-retrieval` (`fastembed`/`ort`, `candle`) |
| `VectorStore` | Upsert + ANN-search vectors with payload filtering, returning scored hits | `oncora-core` | `oncora-retrieval` (`qdrant`, `lancedb`, dev `hnsw_rs`) |
| `GraphStore` | Assert/query entities + edges; ontology (SPARQL) + time-traveled evidence (Datalog) | `oncora-core` | `oncora-kg` (`oxigraph`, `cozo`/`indradb`) |
| `MemoryStore` | Read/write versioned, attributed, content-addressed memory by `(scientist,project,workflow)` | `oncora-core` | `oncora-memory` (`redb` + CAS, Postgres, `cozo`) |
| `ToolHost` | Register + invoke MCP tools deterministically; record every call to the audit trail | `oncora-core` | `oncora-mcp-host` (`rmcp`) |
| `Calibrator` | Map raw scores → calibrated `Confidence`, tagging the `CalibrationMethod` | `oncora-core` | `oncora-uncertainty` (temperature/isotonic/conformal, `ort`/`candle`) |
| `Verifier` | Check a claim against evidence/oracles; return support, contradiction, verdict signal | `oncora-core` | `oncora-uncertainty` (NLI entailment, oracle grounding) |
| `ArtifactStore` | Put/get content-addressed blobs + manifests by BLAKE3; immutable, deduplicated | `oncora-core` | `oncora-artifacts` (object store / FS CAS) |
| `LedgerStore` | Per-document/per-run relational provenance writes behind one conformance contract | `oncora-core` | `oncora-ledger` (in-memory, `turso`, gated `rusqlite`) |

## Trait sketches

```rust
//! Provider seams declared in `oncora-core`. Concrete backends live in the crate that
//! owns the underlying technology; higher layers depend only on these traits.

use async_trait::async_trait;

/// Pluggable text/chat model backend. Carries a `ModelPin` so any completion is
/// reproducible against a fixed model + decoding config.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest) -> Result<Completion, ModelError>;
    fn pin(&self) -> ModelPin;
}

/// Approximate-nearest-neighbour vector store with payload filtering.
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, points: Vec<VectorPoint>) -> Result<(), StoreError>;
    async fn search(&self, query: VectorQuery) -> Result<Vec<ScoredHit>, StoreError>;
}

/// Post-hoc calibration of raw scores into a typed, tagged `Confidence`.
pub trait Calibrator: Send + Sync {
    fn fit(&mut self, samples: &[CalibrationSample]) -> Result<(), UncertaintyError>;
    fn calibrate(&self, raw: f64, ctx: &CalibrationContext) -> Confidence;
    fn method(&self) -> CalibrationMethod;
}

/// Content-addressed artifact store: immutable, deduplicated, BLAKE3-keyed.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
    async fn put(&self, bytes: &[u8]) -> Result<ContentHash, CasError>;
    async fn get(&self, hash: &ContentHash) -> Result<Vec<u8>, CasError>;
}

/// Provider-swappable memory store. Write = extract→dedup→conflict-res→consolidate→decay;
/// read = hybrid retrieve→fuse→assemble under budget; forget = soft-delete + tombstone.
#[async_trait]
pub trait MemoryStore {
    type Error;
    async fn write(&self, entry: MemoryEntry) -> Result<MemoryId, Self::Error>;
    async fn read(&self, query: ReadQuery) -> Result<Vec<MemoryEntry>, Self::Error>;
    async fn forget(&self, id: MemoryId) -> Result<(), Self::Error>;  // provenance retained
}
```

## Per-trait contracts & invariants

- **`ModelProvider`** — on-prem default (vLLM/TGI OpenAI-compatible via `async-openai`); cloud opt-in (Anthropic) only through egress proxy; cloud touch stamped into `ModelPin`; `temperature = 0` for reproducible runs. *(P-2, P-6)*
- **`EmbeddingProvider`** — batched + deterministic; one embedding layer for retrieval **and** semantic memory; fixed dimension per collection; every vector carries `SourceRef`+`SnapshotId`+`ModelPin`. *(FR-ING-8)*
- **`VectorStore`** — payload filtering required; degrades (per-store timeout) rather than blocks (FR-RET-2).
- **`GraphStore`** — single surface over dual store; callers express **intent, not store choice**; supports per-edge confidence + provenance and point-in-time queries. *(FR-KG-*)*
- **`MemoryStore`** — keyed by `(scientist, project, workflow)`; cross-tenant reads denied; episodic + provenance never decayed/deleted; every write stamps snapshot + model pin + BLAKE3 payload hash. *(FR-MEM-*)*
- **`ToolHost`** — single chokepoint; **record-before-return**; record allow **and** deny; deterministic execution; `tower` timeout; `ToolCallId` on every call. *(P-5)*
- **`Calibrator`** — fit on held-out `(raw_score, correct?)` from eval harness; per task class; content-addressed + tied to `ModelPin`+`SnapshotId`; scorer rejects `Raw`. *(FR-UNC-5/6)*
- **`Verifier`** — adjudicates, never generates; citation/oracle/consistency/contradiction/schema-unit verifiers; populates `EvidenceItem.strength`; can force abstain. *(FR-UNC-4)*
- **`ArtifactStore`** — immutable, dedup-by-content; GC/retention may never delete anything a provenance record still references. *(P-6, P-10)*
- **`LedgerStore`** — identical conformance test passes for in-memory, C SQLite (`rusqlite`, gated), and pure-Rust SQLite (`turso`, chosen); a provenance record is written per run inside the agent loop.

**Dependency rule.** Dependencies point inward toward `oncora-core`; providers are concrete impls of these traits; no cycles. Swapping a backend = swapping an impl, never touching the agent runtime or API.
