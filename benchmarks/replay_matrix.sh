#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
ENTRY_COUNTS=${ENTRY_COUNTS:-"100000 500000"}
directories=()

for entries in $ENTRY_COUNTS; do
  output=$(ENTRIES="$entries" "$ROOT/benchmarks/replay.sh")
  printf '%s\n' "$output"
  directories+=("$(printf '%s\n' "$output" | sed -n 's/^Replay benchmark results: //p')")
done

python3 -B "$ROOT/benchmarks/replay_analysis.py" "${directories[@]}"
