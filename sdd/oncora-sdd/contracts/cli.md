# Contract — `oncora-cli`

> The operator + developer CLI. Drives the single-node embedded substrate for development and is
> the home of **deterministic replay** (P-6). Depends on `oncora-api` / `oncora-agents`. Uses
> `anyhow` for ergonomic top-level error handling (binary crate convention).

## Subcommands

| Command | Purpose | Maps to |
|---|---|---|
| `oncora-cli run <query> [target]` | Submit a workflow query; print a cited, confidence-scored answer or a logged abstention | FR-AGT-*, FR-UNC-* |
| `oncora-cli replay --manifest <blake3>` | Reconstruct the pinned stack and reproduce a run **bit-for-bit** from CAS (pinned model + snapshot + content hashes + seeds) | AC-3, P-6 |
| `oncora-cli ingest <snapshot>` | Run `swiftide` ingestion of a pinned snapshot into KG + indexes + CAS (never writes to source) | FR-ING-* |
| `oncora-cli eval replay --manifest <blake3>` | Resolve a `RunManifest` from CAS; replay from fixtures (or `--live`); recompute metrics; regenerate figures; assert recomputed `Score` matches recorded within tolerance | FR-EVAL-6, AC-8 |
| `oncora-cli eval figure --report <blake3> --id <fig-id>` | Regenerate a specific figure from a recorded report | FR-EVAL-6 |

## Prototype binaries (walking skeleton)

The prototype ships runnable entry points demonstrating the spine end-to-end:

```bash
cargo run --bin oncora                       # ingest a tiny corpus, ask a target-discovery question,
                                             # print a cited, confidence-scored answer
cargo run --bin oncora -- "Is BRAF actionable in melanoma?" BRAF
cargo run --bin oncora-api                   # HTTP API on :8080 (GET /health, /tools; POST /query)
```

## Guarantees

- **Deterministic replay.** Given `(snapshot, model pin, content hashes, seeds)`, `replay` reproduces the same
  memory state, retrieval, and answer — the bit-for-bit guarantee of AC-3.
- **Refuses unpinned inputs.** The eval path refuses to run against an unpinned model endpoint or a
  non-content-addressed dataset (FR-EVAL-4).
- **One-command reproducibility.** A reviewer with the repo at a commit + the pinned snapshots reproduces any
  figure with one command and traces any number to its source artifact (P-6, P-8).
