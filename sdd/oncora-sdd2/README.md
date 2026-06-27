# Oncora — Spec-Driven Development (SDD) Document Set

This is the **COMPACT** (information-equivalent, denser) rendering of the Oncora SDD set; the full prose set is in [`../oncora-sdd/`](../oncora-sdd/). Same facts, terser form.

**Oncora** (Oncology Reasoning Agents): a Rust-native, on-prem, uncertainty-aware agentic reasoning platform for oncology drug discovery.

SDD separates **what/why** (spec) from **how** (plan, contracts, data model) from **do** (tasks): intent is authored, reviewed, and versioned before implementation; every line of code traces to a requirement. Derived losslessly from the Oncora design canon (`docs/00`–`docs/11`), reframed into SDD.

## How to read this set

Read top-to-bottom; each artifact answers a distinct question and links forward.

| # | Artifact | Question it answers | SDD phase |
|---|---|---|---|
| 1 | [constitution.md](constitution.md) | What principles may never be violated? | Constitution |
| 2 | [spec.md](spec.md) | What is built and why — requirements & acceptance criteria (tech-agnostic) | Specify |
| 3 | [plan.md](plan.md) | How is it built — architecture, runtime, deployment | Plan |
| 4 | [research.md](research.md) | Which technologies, and why each won/lost | Plan (research) |
| 5 | [data-model.md](data-model.md) | What are the entities, types, and the KG schema | Plan (data) |
| 6 | [contracts/](contracts/) | What are the trait / API / tool / CLI contracts | Plan (contracts) |
| 7 | [tasks.md](tasks.md) | What work, in what order, gated how | Tasks |
| 8 | [quickstart.md](quickstart.md) | How to build, run, and validate it; current status | Validate |

### Contracts sub-set

| File | Covers |
|---|---|
| [contracts/core-traits.md](contracts/core-traits.md) | The nine provider-swappable `oncora-core` traits |
| [contracts/http-grpc-api.md](contracts/http-grpc-api.md) | The `oncora-api` external HTTP/gRPC surface |
| [contracts/mcp-tools.md](contracts/mcp-tools.md) | MCP tool contracts (VCF, DICOM, calculators) |
| [contracts/cli.md](contracts/cli.md) | The `oncora-cli` operator/developer surface |

## Traceability

Every functional requirement (`FR-*`), non-functional requirement (`NFR-*`), and principle (`P-*`) carries a stable ID. Plan sections, contracts, tasks, and quickstart scenarios reference those IDs, making the chain **principle → requirement → design → task → acceptance test** auditable end to end.

## Source provenance

Authoritative source: the Oncora `docs/` canon at repo HEAD — `00-overview`, `01-architecture`, `02-memory`, `03-uncertainty-reliability`, `04-knowledge-and-data`, `05-tech-decisions`, `06-eval-benchmarking`, `07-repo-layout`, `08-roadmap`, plus prototype evidence in `09-validation`, `10-cold-compare`, `11-status`.
