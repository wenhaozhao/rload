#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
cargo package --allow-dirty

test -f LICENSE-MIT
test -f LICENSE-APACHE
test -f THIRD_PARTY_NOTICES.md
package_files=$(cargo package --allow-dirty --list)
grep -qx 'LICENSE-MIT' <<<"$package_files"
grep -qx 'LICENSE-APACHE' <<<"$package_files"
grep -qx 'THIRD_PARTY_NOTICES.md' <<<"$package_files"

printf 'Local release checks passed.\n'
printf 'Manual gate: review third-party notices and target-platform validation before distribution.\n'
