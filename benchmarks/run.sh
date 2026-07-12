#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DURATION=${DURATION:-10}
THREADS=${THREADS:-2}
CONNECTIONS=${CONNECTIONS:-100}
ADDRESS=${ADDRESS:-127.0.0.1:18080}
RUNS=${RUNS:-3}
DELAY_US=${DELAY_US:-0}
JITTER_US=${JITTER_US:-0}
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
RESULT_DIR="$ROOT/benchmarks/results/$STAMP"
mkdir -p "$RESULT_DIR"

cargo build --release --manifest-path "$ROOT/Cargo.toml" --example benchmark_server
cargo build --release --manifest-path "$ROOT/Cargo.toml"

"$ROOT/target/release/examples/benchmark_server" "$ADDRESS" "$DELAY_US" "$JITTER_US" \
  >"$RESULT_DIR/server.log" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 100); do
  grep -q '^READY ' "$RESULT_DIR/server.log" && break
  sleep 0.05
done
grep -q '^READY ' "$RESULT_DIR/server.log"

URL="http://$ADDRESS/"
wrk -t"$THREADS" -c"$CONNECTIONS" -d2s "$URL" >/dev/null
"$ROOT/target/release/rload" --threads "$THREADS" --connections "$CONNECTIONS" \
  --duration 2s "$URL" >/dev/null

{
  uname -a
  rustc --version
  cargo --version
  wrk --version 2>&1 | head -1 || true
  printf 'threads=%s connections=%s duration=%ss runs=%s delay_us=%s jitter_us=%s\n' \
    "$THREADS" "$CONNECTIONS" "$DURATION" "$RUNS" "$DELAY_US" "$JITTER_US"
} >"$RESULT_DIR/environment.txt"

measure() {
  local output=$1
  shift
  if [[ $(uname -s) == Darwin ]]; then
    /usr/bin/time -l "$@" >"$output" 2>"$output.time"
  else
    /usr/bin/time -v "$@" >"$output" 2>"$output.time"
  fi
}

for run in $(seq 1 "$RUNS"); do
  if (( run % 2 == 1 )); then
    clients=(wrk rload)
  else
    clients=(rload wrk)
  fi
  for client in "${clients[@]}"; do
    if [[ $client == wrk ]]; then
      measure "$RESULT_DIR/wrk-$run.txt" \
        wrk -t"$THREADS" -c"$CONNECTIONS" -d"${DURATION}s" --latency "$URL"
    else
      measure "$RESULT_DIR/rload-$run.txt" \
        "$ROOT/target/release/rload" --threads "$THREADS" \
        --connections "$CONNECTIONS" --duration "${DURATION}s" "$URL"
    fi
  done
done

printf 'Benchmark results: %s\n' "$RESULT_DIR"
