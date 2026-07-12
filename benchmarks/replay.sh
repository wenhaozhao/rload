#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DURATION=${DURATION:-5}
THREADS=${THREADS:-2}
CONNECTIONS=${CONNECTIONS:-100}
ENTRIES=${ENTRIES:-100000}
RUNS=${RUNS:-3}
REPLAY_ORDER=${REPLAY_ORDER:-sequential}
SEED=${SEED:-42}
ADDRESS=${ADDRESS:-127.0.0.1:18081}
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
RESULT_DIR=$(mktemp -d "$ROOT/benchmarks/results/replay-$STAMP-XXXXXX")
ACCESS_LOG="$RESULT_DIR/access.log"
mkdir -p "$RESULT_DIR"

awk -v count="$ENTRIES" 'BEGIN {
  for (i = 0; i < count; i++) {
    printf "127.0.0.1 - - [10/Oct/2000:13:55:36 -0700] \"GET /items/%d?source=replay HTTP/1.1\" 200 2 \"-\" \"rload-benchmark\"\n", i
  }
}' >"$ACCESS_LOG"

cargo build --release --manifest-path "$ROOT/Cargo.toml" --example benchmark_server
cargo build --release --manifest-path "$ROOT/Cargo.toml"

"$ROOT/target/release/examples/benchmark_server" "$ADDRESS" \
  >"$RESULT_DIR/server.log" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 100); do
  grep -q '^READY ' "$RESULT_DIR/server.log" && break
  sleep 0.05
done
grep -q '^READY ' "$RESULT_DIR/server.log"

URL="http://$ADDRESS/"
COMMON=("$ROOT/target/release/rload" --threads "$THREADS" --connections "$CONNECTIONS" --duration "${DURATION}s")
"${COMMON[@]}" "$URL" >/dev/null
if [[ $REPLAY_ORDER == sequential ]]; then
  "${COMMON[@]}" --access-log "$ACCESS_LOG" "$URL" >/dev/null
else
  "${COMMON[@]}" --access-log "$ACCESS_LOG" --replay-order "$REPLAY_ORDER" \
    --seed "$SEED" "$URL" >/dev/null
fi

measure() {
  local name=$1
  shift
  if [[ $(uname -s) == Darwin ]]; then
    /usr/bin/time -l "$@" >"$RESULT_DIR/$name.txt" 2>"$RESULT_DIR/$name.time"
  else
    /usr/bin/time -v "$@" >"$RESULT_DIR/$name.txt" 2>"$RESULT_DIR/$name.time"
  fi
}

for run in $(seq 1 "$RUNS"); do
  if (( run % 2 == 1 )); then
    modes=(static replay)
  else
    modes=(replay static)
  fi
  for mode in "${modes[@]}"; do
    if [[ $mode == static ]]; then
      measure "static-$run" "${COMMON[@]}" "$URL"
    elif [[ $REPLAY_ORDER == sequential ]]; then
      measure "replay-$run" "${COMMON[@]}" --access-log "$ACCESS_LOG" "$URL"
    else
      measure "replay-$run" "${COMMON[@]}" --access-log "$ACCESS_LOG" \
        --replay-order "$REPLAY_ORDER" --seed "$SEED" "$URL"
    fi
  done
done

printf 'threads=%s connections=%s duration=%ss entries=%s runs=%s replay_order=%s seed=%s\n' \
  "$THREADS" "$CONNECTIONS" "$DURATION" "$ENTRIES" "$RUNS" "$REPLAY_ORDER" "$SEED" \
  >"$RESULT_DIR/environment.txt"
python3 -B "$ROOT/benchmarks/replay_analysis.py" "$RESULT_DIR"
printf 'Replay benchmark results: %s\n' "$RESULT_DIR"
