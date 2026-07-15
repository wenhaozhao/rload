# rload 0.2.3 pre-release validation — 2026-07-15

## Functional completeness

- 115 library, binary, CLI, and HTTP integration tests passed.
- `cargo fmt --check`, Clippy with warnings denied, release build, crate
  packaging, and packaged-crate verification passed through
  `./scripts/release-check.sh`.
- Generic `--stages` coverage passed for ordinary default and custom requests,
  Nginx access-log replay, JSONL replay, option conflicts, compatibility alias,
  text output, JSON output, and the public Rust API.
- `--version` coverage passed for standalone output, option ordering, and
  values that happen to equal `--version`.
- Linux and Windows CI remain required before tagging; this local gate ran on
  macOS arm64.

## Three-way benchmark

Raw results: `benchmarks/results/threeway-20260715T033815Z`

Parameters: wrk 4.2.0, installed rload 0.2.2, current 0.2.3 development build;
2 threads, 100 connections, 10 seconds, 5 alternating paired runs, 1 ms
deterministic delay plus up to 1 ms jitter.

| Client | Mean RPS | Mean peak RSS |
|---|---:|---:|
| wrk | 40,265.80 | 3,712,614 B |
| rload 0.2.2 release | 40,228.06 | 3,876,454 B |
| rload 0.2.3 development | 40,125.05 | 3,863,347 B |

| Metric | Development error versus wrk |
|---|---:|
| RPS MAE | 0.847% |
| Average latency MAE | 2.113% |
| P50 MAE | 1.264% |
| P75 MAE | 0.896% |
| P90 MAE | 1.462% |
| P99 median absolute error | 2.141% |
| `read_bytes` per-request MAE | 0.0148% |

Development throughput was 0.26% below the installed 0.2.2 release baseline.
Mean peak RSS was 0.34% below that baseline. All statistical and regression
gates passed.

## Result

PASS. The v0.2.3 branch passes local release and benchmark gates and is ready
for cross-platform CI. After Linux, macOS, and Windows CI pass, it can be merged
and tagged for release.
