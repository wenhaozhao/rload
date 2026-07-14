# rload 0.2.2 final release validation — 2026-07-14

## Functional completeness

- 106 unit, CLI, HTTP integration, and example-target tests passed.
- `cargo fmt --check`, Clippy with warnings denied, release packaging, and
  packaged-crate verification passed.
- Timestamp replay regression passed for schema-free JSONL, schema-defined
  JSONL, multiple connections, early idle close, deferred reconnect, and
  fixed-count completion.
- Ubuntu, macOS, and Windows CI passed on the final commit.

## Three-way benchmark

Raw results: `benchmarks/results/threeway-20260714T061658Z`

Parameters: wrk 4.2.0, rload 0.2.0 release baseline, current development
build; 2 threads, 100 connections, 20 seconds, 5 alternating paired runs,
1 ms deterministic delay plus up to 1 ms jitter.

| Client | Mean RPS | Mean peak RSS |
|---|---:|---:|
| wrk | 40,312.81 | 3,607,757 B |
| rload 0.2.0 release | 40,179.98 | 3,974,758 B |
| current development | 40,263.15 | 3,955,098 B |

| Metric | Development error versus wrk |
|---|---:|
| RPS MAE | 0.402% |
| Average latency MAE | 0.813% |
| P50 MAE | 0.877% |
| P75 MAE | 0.823% |
| P90 MAE | 1.180% |
| P99 median absolute error | 2.102% |
| `read_bytes` per-request MAE | 0.0059% |

Development throughput was 0.21% above the installed release baseline. Mean
peak RSS was 0.49% below that baseline. All statistical and regression gates
passed.

## Result

PASS. The v0.2.2 branch is ready to merge and tag for release.
