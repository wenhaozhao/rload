#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
cargo package --allow-dirty

printf 'Local release checks passed.\n'
printf 'Manual gate: review the modified Apache license naming clause before distribution.\n'
