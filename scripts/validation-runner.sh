#!/usr/bin/env bash
# Throttled, long-running real-world validation runner.
#
# Feeds the PubMed corpus to the all-real Oncora pipeline in successive batches
# (cumulative qdrant collection + persistent redb memory), pausing between
# batches so it behaves like a steady real-world load rather than a burst. Each
# batch ingests `BATCH` documents (timing every component) and runs the fixed
# query set, appending one stats record per batch to a results JSONL that the
# report generator turns into a trend table.
#
# Env knobs: BATCH (docs/batch), THROTTLE (seconds between batches),
#            ONCORA_CORPUS (default ~/oncora-input-data/corpus.jsonl).
set -uo pipefail

CORPUS=${ONCORA_CORPUS:-/home/tom_b/oncora-input-data/corpus.jsonl}
BATCH=${BATCH:-10}
THROTTLE=${THROTTLE:-25}
INPUT_DIR=/home/tom_b/oncora-input-data
RESULTS="$INPUT_DIR/validation-results.jsonl"
LOG="$INPUT_DIR/validation-runner.log"
BIN=/home/tom_b/oncora/target/debug/examples/bench

export ONCORA_OPENAI_URL=${ONCORA_OPENAI_URL:-http://127.0.0.1:11434/v1}
export ONCORA_QDRANT_URL=${ONCORA_QDRANT_URL:-http://127.0.0.1:6334}
export ONCORA_OPENAI_MODEL=${ONCORA_OPENAI_MODEL:-qwen2.5:0.5b}
export ONCORA_EMBED_MODEL=${ONCORA_EMBED_MODEL:-all-minilm}
export ONCORA_COLLECTION=${ONCORA_COLLECTION:-oncora_longrun}
export ONCORA_REDB=${ONCORA_REDB:-"$INPUT_DIR/longrun-memory.redb"}
export ONCORA_CORPUS="$CORPUS"
export ONCORA_RESULTS_JSONL="$RESULTS"
export ONCORA_STATS_OUT="$INPUT_DIR/last-batch-stats.json"

if [ ! -x "$BIN" ]; then
  echo "bench binary not found at $BIN — run: cargo build -p oncora-agents --features e2e --example bench" >&2
  exit 1
fi

N=$(grep -c . "$CORPUS")
: > "$RESULTS"
echo "[$(date -u +%FT%TZ)] START corpus=$N batch=$BATCH throttle=${THROTTLE}s collection=$ONCORA_COLLECTION" | tee -a "$LOG"

offset=0; b=0
while [ "$offset" -lt "$N" ]; do
  b=$((b + 1))
  start=$(date +%s)
  if ONCORA_BATCH_OFFSET="$offset" ONCORA_BATCH_SIZE="$BATCH" "$BIN" >>"$LOG" 2>&1; then
    status=ok
  else
    status=ERROR
  fi
  dur=$(( $(date +%s) - start ))
  echo "[$(date -u +%FT%TZ)] batch $b offset=$offset status=$status ${dur}s" | tee -a "$LOG"
  offset=$((offset + BATCH))
  if [ "$offset" -lt "$N" ]; then sleep "$THROTTLE"; fi
done

echo "[$(date -u +%FT%TZ)] COMPLETE batches=$b docs=$N results=$RESULTS" | tee -a "$LOG"
