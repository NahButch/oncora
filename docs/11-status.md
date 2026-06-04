# Oncora — Prototype Status & Remaining Work

> **Status: a well-tested, end-to-end functional prototype.** The platform is
> held here intentionally — it demonstrates the architecture's thesis (every
> external dependency behind a swappable `oncora-core` trait) against *real*
> backends, validated over real PubMed literature at scale. What remains is
> production hardening (deployment, governance) and depth on the research
> pillars — not core correctness.

## What the prototype demonstrates (done & verified)

The agent runtime (`run_target_discovery`: retrieve → reason → verify → score →
accept/abstain/escalate → persist) was **never changed** as backends were swapped
from in-memory references to real implementations. Every seam below was run green
against a genuine backend:

| `oncora-core` trait | Real backend | Verified |
|---|---|---|
| `ModelProvider` | Ollama (`qwen2.5:0.5b`) via `async-openai` | live inference, accept/abstain works |
| `EmbeddingProvider` | Ollama (`all-minilm`, 384-d) | semantic ranking confirmed |
| `VectorStore` | qdrant (gRPC) | round-trip + full agent E2E |
| `GraphStore` | oxigraph (RDF/SPARQL) | conformance + per-edge confidence |
| `MemoryStore` | redb (persistent, on disk) | conformance; O(1) write |
| `LedgerStore` | **pure-Rust SQLite (`turso`)** — chosen | 14,469 writes, 0 errors, ~23% faster than C SQLite |
| `ToolHost` | rmcp (real MCP server + client) | loopback round-trip |

**Real-world validation** ([09-validation.md](09-validation.md), [10-cold-compare.md](10-cold-compare.md)):
- **14,469 PubMed abstracts** (glioblastoma / glioma / IDH-mutant / immunotherapy /
  astrocyte·glial·microglia inflammation) ingested cold through the all-real pipeline.
- Ingest **~21 docs/s** (batched embedding); per-component **flat at scale** — qdrant
  upsert ~2.1 ms, redb memory ~1.2 ms, oxigraph ~0.06 ms, turso ledger ~2.0 ms.
- Query: accept 100%, calibrated confidence rose with corpus size; end-to-end is
  **LLM-bound** (the data plane is single-digit-millisecond throughout).
- Provenance ledger is written **inside the agent loop** (a record per run).

**Bugs surfaced and fixed by real data/scale:** O(N²) memory dedup → O(1) index;
oxigraph IRI encoding for arbitrary tokens; qdrant fd-limit; report-generator
Unicode-line handling. (Exactly what a real bulk validation is for.)

## Remaining work

### Quick wins
- **Inference/embedding request concurrency** — batching used some idle cores; concurrent Ollama requests would use the rest.
- **Memory `read` is O(N)** — `write` is O(1); index `read` by scope.
- **qdrant client/server version bump** (1.18 vs 1.12.4) — silence the compatibility warning.

### Deeper / research pillars
- **cozo `GraphStore`** — one of two blockers cleared by choosing turso (native-`sqlite3` `links`); the `graph_builder`/`rayon` compile break remains. Pin/patch or run cozo out-of-process.
- **`rig` / `swiftide`** — named primary in the canon but not adopted (in-house orchestrator + simple ingest built instead). Decide: adopt or formally retire in the docs.
- **Uncertainty depth** — conformal prediction + post-hoc calibration fitted from the eval harness (currently a temperature calibrator + grounded verifier).
- **Eval vs human-experts & baselines** — `oncora-eval` has the scaffolding (golden cases, ECE, CI gate) but needs real golden datasets and baseline comparison arms.
- **Multi-agent topology** — planner → domain specialists → verifier as an actual runtime (currently a single end-to-end function).

### Phase 5 — Deployment scale-out (single-node → multi-node)
1. **Config-driven `Platform` assembly** (replace `Platform::demo()` — the unlock for everything else here).
2. Stateless `oncora-api`/agent workers behind a load balancer (horizontal scale).
3. Externalize stores: qdrant cluster, **Postgres HA** provenance ledger, oxigraph/cozo as services.
4. On-prem GPU model server (vLLM/TGI) behind `ModelProvider`; size concurrency caps.
5. Object-store-backed CAS (`ArtifactStore` → S3/MinIO).
6. OTel collector wiring (`tracing-opentelemetry`).
7. Upgrade/rollback: expand-contract migrations, golden-set replay-diff gate, confirmed rollback.
8. Spikes: on-prem model throughput (tokens/s under concurrency); qdrant cluster sharding at scale.

*Exit:* horizontal scale under load · Postgres + qdrant failover · upgrade validated by replay diff · rollback confirmed.

### Phase 6 — Governance hardening (review-board grade)
1. **RBAC** — project/workflow-scoped access control on the API.
2. **Provenance lineage walk** — query any claim → sources, tool calls, model pin, snapshot (the per-run ledger record is already written — add the lineage query + API).
3. **Audit completeness** — record-before-return invariant at the MCP host; property-test it (`McpHost` already keeps an audit log).
4. **PHI/PII redaction + egress blocking** — redact logs/traces; egress proxy denies PHI leaving the boundary; adversarial leak testing.
5. **Reviewer adjudication queue** — route `Verdict::Escalate` to human sign-off.
6. **ALCOA+ reproducibility evidence** — one-command replay of any historical run from its manifest.
7. **Change-control** — replay-gated CI (an upgrade must reproduce golden-set outputs).

*Exit:* any claim traceable to provenance · PHI cannot leave · every tool call auditable · validated replay reproduces any historical run.

### Sequencing
Phase 6 depends on Phase 5 (RBAC/egress/lineage assume the multi-node services + real ledger/CAS). Both are **infrastructure/ops-heavy** — the trait seams mean each is "add a production impl behind an existing trait," not a rewrite. Phase 6 items 2–3 have head starts from the prototype (ledger-in-loop, MCP audit log).
