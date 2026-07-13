#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DURATION=${DURATION:-10}
THREADS=${THREADS:-2}
CONNECTIONS=${CONNECTIONS:-100}
ADDRESS=${ADDRESS:-127.0.0.1:18080}
RUNS=${RUNS:-5}
DELAY_US=${DELAY_US:-1000}
JITTER_US=${JITTER_US:-1000}
WRK_BIN=${WRK_BIN:-$(command -v wrk)}
RELEASE_BIN=${RELEASE_BIN:-/opt/homebrew/bin/rload}
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
RESULT_DIR="$ROOT/benchmarks/results/threeway-$STAMP"
mkdir -p "$RESULT_DIR"

cargo build --release --manifest-path "$ROOT/Cargo.toml" --example benchmark_server
cargo build --release --manifest-path "$ROOT/Cargo.toml"
DEV_BIN="$ROOT/target/release/rload"

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

"$WRK_BIN" -t"$THREADS" -c"$CONNECTIONS" -d2s "$URL" >/dev/null
"$RELEASE_BIN" -t"$THREADS" -c"$CONNECTIONS" -d2s "$URL" >/dev/null
"$DEV_BIN" -t"$THREADS" -c"$CONNECTIONS" -d2s "$URL" >/dev/null

{
  uname -a
  rustc --version
  "$WRK_BIN" --version 2>&1 | head -1 || true
  brew list --versions rload 2>/dev/null || true
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
  case $((run % 3)) in
    1) clients=(wrk release dev) ;;
    2) clients=(dev release wrk) ;;
    0) clients=(release wrk dev) ;;
  esac
  for client in "${clients[@]}"; do
    case "$client" in
      wrk)
        measure "$RESULT_DIR/wrk-$run.txt" \
          "$WRK_BIN" -t"$THREADS" -c"$CONNECTIONS" -d"${DURATION}s" --latency "$URL"
        ;;
      release)
        measure "$RESULT_DIR/release-$run.txt" \
          "$RELEASE_BIN" -t"$THREADS" -c"$CONNECTIONS" -d"${DURATION}s" "$URL"
        ;;
      dev)
        measure "$RESULT_DIR/dev-$run.txt" \
          "$DEV_BIN" -t"$THREADS" -c"$CONNECTIONS" -d"${DURATION}s" "$URL"
        ;;
    esac
  done
done

python3 "$ROOT/benchmarks/threeway_analysis.py" "$RESULT_DIR" | tee "$RESULT_DIR/analysis.txt"
printf 'Three-way benchmark results: %s\n' "$RESULT_DIR"
