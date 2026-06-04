#!/usr/bin/env bash
# Cold-start A/B experiment: ingest the full corpus twice from a cold system,
# once with the C SQLite ledger and once with the pure-Rust SQLite (turso)
# ledger. Captures the warm-up curve (per-batch) and bulk aggregate for each,
# into separate results files for the comparison report.
#
# "Cold" = qdrant recreated on a fresh volume, ollama restarted (model unloaded
# from RAM), and all local state files cleared, before each run.
set -uo pipefail

INPUT=/home/tom_b/oncora-input-data
RUNNER=/home/tom_b/oncora/scripts/validation-runner.sh
BATCH=${BATCH:-1000}
export THROTTLE=0
export ONCORA_OPENAI_URL=http://127.0.0.1:11434/v1
export ONCORA_QDRANT_URL=http://127.0.0.1:6334
export ONCORA_OPENAI_MODEL=qwen2.5:0.5b
export ONCORA_EMBED_MODEL=all-minilm
export BATCH

cold_reset() {
  echo "[$(date -u +%FT%TZ)] cold reset: fresh qdrant volume + ollama restart"
  docker rm -f oncora-qdrant >/dev/null 2>&1
  docker volume rm "oncora_qdrant_$1" >/dev/null 2>&1 || true
  docker run -d --name oncora-qdrant -p 6334:6334 -p 6333:6333 \
    --ulimit nofile=1048576:1048576 -v "oncora_qdrant_$1:/qdrant/storage" \
    qdrant/qdrant:v1.12.4 >/dev/null 2>&1
  docker restart oncora-ollama >/dev/null 2>&1
  # wait for both services
  for i in $(seq 1 60); do
    curl -s -m 2 http://127.0.0.1:6333/healthz >/dev/null 2>&1 \
      && curl -s -m 2 http://127.0.0.1:11434/api/tags >/dev/null 2>&1 && break
    sleep 1
  done
  sleep 2
}

run_one() {  # $1 = ledger backend tag
  local led=$1
  cold_reset "$led"
  rm -f "$INPUT/cold-${led}-mem.redb" "$INPUT/cold-${led}-ledger.db"* 2>/dev/null
  echo "[$(date -u +%FT%TZ)] === COLD RUN: ledger=$led batch=$BATCH ==="
  ONCORA_COLLECTION="oncora_cold_${led//-/_}" \
  ONCORA_REDB="$INPUT/cold-${led}-mem.redb" \
  ONCORA_LEDGER="$led" \
  ONCORA_LEDGER_PATH="$INPUT/cold-${led}-ledger.db" \
    bash "$RUNNER"
  cp "$INPUT/validation-results.jsonl" "$INPUT/results-${led}.jsonl"
  echo "[$(date -u +%FT%TZ)] === DONE $led -> results-${led}.jsonl ==="
}

run_one sqlite-c
run_one sqlite-rust
echo "[$(date -u +%FT%TZ)] COLD-COMPARE COMPLETE"
