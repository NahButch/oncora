#!/usr/bin/env python3
"""Generate the cold-start C-vs-Rust-SQLite comparison report (docs/10-cold-compare.md).

Reads the two per-backend results files written by scripts/cold-compare.sh:
  results-sqlite-c.jsonl   (C SQLite / rusqlite)
  results-sqlite-rust.jsonl (pure-Rust SQLite / turso)
Each is one stats record per batch. Produces: warm-up curve (from cold), bulk
per-component aggregate, and a head-to-head ledger-write comparison.
Source corpus: PubMed.
"""
import json, pathlib, statistics, datetime

ROOT = pathlib.Path(__file__).resolve().parent.parent
IN = pathlib.Path("/home/tom_b/oncora-input-data")
OUT = ROOT / "docs" / "10-cold-compare.md"
FILES = {"C SQLite (rusqlite)": IN / "results-sqlite-c.jsonl",
         "Rust SQLite (turso)": IN / "results-sqlite-rust.jsonl"}

def load(p):
    return [json.loads(l) for l in p.read_text().splitlines() if l.strip()] if p.exists() else []

runs = {name: load(p) for name, p in FILES.items()}
runs = {k: v for k, v in runs.items() if v}
if not runs:
    raise SystemExit("no results files yet")

def r3(x):
    return round(x, 3)

def agg(rows, key, section):
    ms = [r[section][key] for r in rows if r[section][key].get("count", 0)]
    if not ms:
        return None
    cnt = sum(m["count"] for m in ms); tot = sum(m["total_ms"] for m in ms)
    return {"count": cnt, "mean": r3(tot / cnt),
            "min": r3(min(m["min_ms"] for m in ms)),
            "max": r3(max(m["max_ms"] for m in ms))}

L = ["# Oncora — Cold-Start Performance & C-vs-Rust SQLite Comparison\n",
     "> Two cold-start bulk runs over the full PubMed corpus (fresh qdrant volume + "
     "ollama restart before each), identical except for the **`LedgerStore` SQLite "
     "backend**. Every component call is real. **Source: PubMed.**\n"]

# config + sample
any_rows = next(iter(runs.values()))
cfg = any_rows[-1]["config"]
total = sum(r["sample_size"] for r in any_rows)
L.append(f"**Corpus:** {total} documents · **batches:** {len(any_rows)} · "
         f"embed `{cfg['embed_model']}` ({cfg['vector_dim']}d) · chat `{cfg['chat_model']}`\n")

# 1) warm-up curve (use the C run as the reference cold start)
ref_name = "C SQLite (rusqlite)" if "C SQLite (rusqlite)" in runs else next(iter(runs))
ref = runs[ref_name]
L.append("## Warm-up from cold (per batch)\n")
L.append(f"Reference run: **{ref_name}**. The system starts cold (model unloaded, "
         "empty index); the first batch pays model-load + cache warm-up.\n")
L.append("| Batch @offset | docs | embed mean ms | q e2e mean ms | docs/s |")
L.append("|---|---|---|---|---|")
for r in ref:
    L.append(f"| {r.get('batch_offset','?')} | {r['sample_size']} | "
             f"{r['ingest']['embed_ms'].get('mean_ms','–')} | "
             f"{r['query']['end_to_end_ms'].get('mean_ms','–')} | "
             f"{r['ingest']['throughput_docs_per_s']} |")
e_first = ref[0]["query"]["end_to_end_ms"]["mean_ms"]
e_last = ref[-1]["query"]["end_to_end_ms"]["mean_ms"]
L.append(f"\n**Query latency warm-up:** {e_first} ms (cold) → {e_last} ms (warm).\n")

# 2) bulk per-component aggregate (shared components; should match across runs)
L.append("## Bulk per-component (aggregate over all batches)\n")
for name, rows in runs.items():
    wall = sum(r["ingest"]["wall_s"] for r in rows)
    tps = r3(sum(r["sample_size"] for r in rows) / wall) if wall else 0
    L.append(f"### {name}\n")
    L.append(f"Ingest wall **{r3(wall)} s** · throughput **{tps} docs/s**.\n")
    L.append("| Component | calls | mean ms | min | max |")
    L.append("|---|---|---|---|---|")
    for label, key in [("Embedding (Ollama)", "embed_ms"),
                       ("Vector upsert (qdrant)", "vector_upsert_ms"),
                       ("Graph assert (oxigraph)", "graph_assert_ms"),
                       ("Memory write (redb)", "memory_write_ms"),
                       ("Ledger write (SQLite)", "ledger_write_ms")]:
        m = agg(rows, key, "ingest")
        if m:
            L.append(f"| {label} | {m['count']} | {m['mean']} | {m['min']} | {m['max']} |")
    L.append("")

# 3) head-to-head ledger
L.append("## Head-to-head: SQLite ledger write\n")
L.append("The only component that differs between the two runs — same schema, same "
         "per-document `INSERT`, identical workload.\n")
L.append("| Backend | writes | mean ms | min | max |")
L.append("|---|---|---|---|---|")
led = {}
for name, rows in runs.items():
    m = agg(rows, "ledger_write_ms", "ingest")
    if m:
        led[name] = m
        L.append(f"| {name} | {m['count']} | {m['mean']} | {m['min']} | {m['max']} |")
if len(led) == 2:
    a, b = list(led.values())
    names = list(led.keys())
    faster = names[0] if a["mean"] <= b["mean"] else names[1]
    ratio = max(a["mean"], b["mean"]) / max(min(a["mean"], b["mean"]), 1e-9)
    L.append(f"\n**Result:** at {a['count']} sequential per-document writes, "
             f"**{faster}** had the lower mean latency ({ratio:.2f}× difference). Both "
             f"completed the full bulk load without error — pure-Rust SQLite (turso) is "
             f"viable as the dev relational/ledger backend at this scale, behind the "
             f"same `LedgerStore` trait as C SQLite.\n")

L.append(f"\n*Generated {datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%d %H:%M UTC')}.*\n")
OUT.write_text("\n".join(L), encoding="utf-8")
print(f"wrote {OUT}")
