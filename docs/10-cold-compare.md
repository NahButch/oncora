# Oncora — Cold-Start Performance & C-vs-Rust SQLite Comparison

> Two cold-start bulk runs over the full PubMed corpus (fresh qdrant volume + ollama restart before each), identical except for the **`LedgerStore` SQLite backend**. Every component call is real. **Source: PubMed.**

**Corpus:** 14469 documents · **batches:** 15 · embed `all-minilm` (384d) · chat `qwen2.5:0.5b`

## Warm-up from cold (per batch)

Reference run: **C SQLite (rusqlite)**. The system starts cold (model unloaded, empty index); the first batch pays model-load + cache warm-up.

| Batch @offset | docs | embed mean ms | q e2e mean ms | docs/s |
|---|---|---|---|---|
| 0 | 1000 | 63.159 | 10683.398 | 14.3 |
| 1000 | 1000 | 62.483 | 9154.862 | 14.48 |
| 2000 | 1000 | 63.832 | 6859.586 | 14.19 |
| 3000 | 1000 | 61.778 | 4465.899 | 14.61 |
| 4000 | 1000 | 64.105 | 3957.234 | 14.14 |
| 5000 | 1000 | 62.931 | 3923.809 | 14.37 |
| 6000 | 1000 | 63.652 | 5145.42 | 14.19 |
| 7000 | 1000 | 62.738 | 4023.361 | 14.36 |
| 8000 | 1000 | 62.762 | 7196.544 | 14.37 |
| 9000 | 1000 | 60.592 | 6606.223 | 14.85 |
| 10000 | 1000 | 62.678 | 3969.095 | 14.43 |
| 11000 | 1000 | 62.217 | 7421.665 | 14.49 |
| 12000 | 1000 | 63.322 | 7942.319 | 14.29 |
| 13000 | 1000 | 61.743 | 5651.307 | 14.61 |
| 14000 | 469 | 62.52 | 4050.244 | 14.42 |

**Query latency warm-up:** 10683.398 ms (cold) → 4050.244 ms (warm).

## Bulk per-component (aggregate over all batches)

### C SQLite (rusqlite)

Ingest wall **1004.458 s** · throughput **14.405 docs/s**.

| Component | calls | mean ms | min | max |
|---|---|---|---|---|
| Embedding (Ollama) | 14469 | 62.707 | 20.854 | 118.822 |
| Vector upsert (qdrant) | 14469 | 2.601 | 1.716 | 11.805 |
| Graph assert (oxigraph) | 14469 | 0.073 | 0.064 | 0.616 |
| Memory write (redb) | 14469 | 1.372 | 0.927 | 29.246 |
| Ledger write (SQLite) | 14469 | 2.663 | 2.076 | 8.919 |

### Rust SQLite (turso)

Ingest wall **990.443 s** · throughput **14.609 docs/s**.

| Component | calls | mean ms | min | max |
|---|---|---|---|---|
| Embedding (Ollama) | 14469 | 62.433 | 21.503 | 119.827 |
| Vector upsert (qdrant) | 14469 | 2.548 | 1.709 | 10.866 |
| Graph assert (oxigraph) | 14469 | 0.073 | 0.064 | 0.261 |
| Memory write (redb) | 14469 | 1.353 | 0.925 | 31.92 |
| Ledger write (SQLite) | 14469 | 2.041 | 1.751 | 10.035 |

## Head-to-head: SQLite ledger write

The only component that differs between the two runs — same schema, same per-document `INSERT`, identical workload.

| Backend | writes | mean ms | min | max |
|---|---|---|---|---|
| C SQLite (rusqlite) | 14469 | 2.663 | 2.076 | 8.919 |
| Rust SQLite (turso) | 14469 | 2.041 | 1.751 | 10.035 |

**Result:** at 14469 sequential per-document writes, **Rust SQLite (turso)** had the lower mean latency (1.30× difference). Both completed the full bulk load without error — pure-Rust SQLite (turso) is viable as the dev relational/ledger backend at this scale, behind the same `LedgerStore` trait as C SQLite.


*Generated 2026-06-04 04:41 UTC.*
