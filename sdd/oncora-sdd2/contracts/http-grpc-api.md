# Contract — `oncora-api` (HTTP + gRPC)

> `oncora-api` is the **sole entry point** across the trust boundary (P-2): `axum` (HTTP) + `tonic` (gRPC), with authentication and RBAC. All callers (Scientist, Reviewer, Pipeline, Operator) enter here; service-to-service is mTLS. RBAC enforced here **and re-checked at the MCP host policy gate** (FR-GOV-1). Roles scoped per `(project, workflow)`.

Concrete prototype surface (per README walking skeleton) plus specified production surface. Endpoints marked *(proto)* are demonstrated in the prototype; others specified by plan/spec and implemented as the API thickens.

## HTTP surface

| Method | Path | Auth | Purpose | Maps to |
|---|---|---|---|---|
| GET | `/health` | none/liveness | Liveness/readiness probe *(proto)* | NFR ops; `axum` health |
| GET | `/tools` | scientist+ | List registered MCP tools available to the caller *(proto)* | `oncora-mcp-host` registry |
| POST | `/query` | scientist+ | Submit a workflow query; returns a cited, confidence-scored answer **or** a logged abstention/escalation *(proto)* | FR-AGT-*, FR-UNC-* |
| GET | `/runs/{run_id}` | scientist+ | Fetch a run's verdict, evidence, citations, and audit handle | FR-GOV-3 |
| GET | `/runs/{run_id}/lineage` | reviewer+ | Provenance lineage walk: claim → sources, tool calls, model pin, snapshot | FR-GOV-3 |
| GET | `/review/queue` | reviewer | List escalated decisions awaiting adjudication | FR-GOV-2 |
| POST | `/review/{run_id}/decision` | reviewer | Adjudicate an escalation; approve/deny a semantic-memory promotion | FR-GOV-2 |
| POST | `/ingest` | pipeline/operator | Trigger ingestion of a pinned snapshot | FR-ING-* |
| POST | `/eval/run` | pipeline/operator | Launch a benchmark run against a golden set | FR-EVAL-* |

### `POST /query` — request / response shape

Request: `{ workflow, query, context_key: { scientist, project, workflow }, task_class?, options? }`.

Response (Accept):
```json
{
  "verdict": "accept",
  "answer": "NSCLC is the best-supported answer",
  "confidence": 0.935,
  "calibration_method": "temperature_scaled",
  "citations": ["PMID:0001", "PMID:0002"],
  "evidence": { "support": [...], "contradiction": [...] },
  "provenance": { "sources": [...], "tool_calls": [...], "model": "openai/qwen2.5:0.5b@live", "snapshot": "<blake3>" },
  "run_id": "<id>"
}
```
Response (Abstain/Escalate): `verdict ∈ {abstain, escalate}` with `reason` (an `AbstainReason` or escalation target + reason) and the full evidence bundle; the user always sees what the system declined to assert and why (FR-UNC-9).

## gRPC surface (`tonic`)

Used by internal service mesh + automated pipeline (service identity). Mirrors HTTP query/ingest/eval operations as typed RPCs with streaming where useful (e.g. streamed partial reasoning / progress), under the same auth + RBAC + audit guarantees. Protobuf/codegen pinned and reproducible in CI.

## Cross-cutting guarantees

- **Auth + RBAC** at the edge; re-checked at the MCP host (defense in depth). *(FR-GOV-1)*
- **Audit.** Every tool call triggered by a request is recorded (allow and deny) before its result returns (P-5).
- **Provenance.** Every answer carries `Provenance`; every number/claim traceable (FR-GOV-3, P-4).
- **Privacy.** PHI redacted in logs/traces; never emitted to a cloud endpoint except non-PHI tasks via the audited egress proxy (P-2, FR-GOV-4).
- **Observability.** Each request is a correlated `tracing` span exported to the on-prem OTel collector; no PHI in span fields (NFR-OBS-1).
