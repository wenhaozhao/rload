# rload 0.2.2 development validation — 2026-07-14

## Scope

This checkpoint covers the first v0.2.2 implementation of finite replay
rounds, schema-driven JSONL extraction, and JSONL timestamp pacing. The
comparison follows the project policy: wrk, the latest installed rload release,
and the current development build under equivalent parameters.

## Functional completeness

- Formatting, Clippy with warnings denied, all unit/integration/CLI tests, and
  the release package gate passed.
- CLI coverage verifies partial schema fallback, nested field extraction,
  load-time timestamp materialization, paced JSONL replay, exact finite replay
  rounds, JSON output, and invalid option combinations.
- The unchanged JSONL path retains typed deserialization; dynamic JSON and
  timestamp parsing are only used when schema or timestamp features are active.

## Performance and statistical accuracy

Raw results: `benchmarks/results/threeway-20260714T035557Z`

Parameters: 2 threads, 100 connections, 10 seconds, 5 alternating paired runs,
1 ms deterministic delay plus up to 1 ms jitter.

| Client | Mean RPS | Mean peak RSS |
|---|---:|---:|
| wrk | 37,496.62 | 3,630,694 B |
| latest rload release | 37,161.17 | 3,722,445 B |
| v0.2.2 development | 37,500.51 | 3,941,990 B |

| Metric | v0.2.2 error versus wrk |
|---|---:|
| RPS MAE | 0.432% |
| Average latency MAE | 0.530% |
| P50 MAE | 0.498% |
| P75 MAE | 0.802% |
| P90 MAE | 0.445% |
| P99 median absolute error | 0.800% |
| `read_bytes` per-request MAE | 0.0111% |

Development throughput was 0.91% above the installed release in this run.
Mean peak RSS was 5.90% above the release and remains within the 10% checkpoint
guardrail. The standard three-way gate passed.

## Result

PASS for this development checkpoint. Final release sign-off must repeat the
same three dimensions after all v0.2.2 changes and cross-platform CI complete.
