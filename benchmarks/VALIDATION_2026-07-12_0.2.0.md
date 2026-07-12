# Three-way validation — 2026-07-12

Compared implementations: wrk 4.2.0, published rload 0.1.2, and development
commit `b4b566f`. Host: macOS arm64. The controlled accuracy matrix used two
threads, 100 connections, a ten-second duration, five rotating-order paired
runs, and a server delay of 1 ms with deterministic 0–1 ms jitter.

## Functional completeness regression

The development release gate passed formatting, Clippy with warnings denied,
27 library tests, one binary test, 23 CLI tests, 26 HTTP integration tests,
release compilation, crate packaging, and packaged-crate verification. The
three clients also completed the equivalent static HTTP workload. Rload-only
coverage includes HTTPS, access-log and JSONL replay, ordering, filters,
unsupported-method skipping, fixed global replay rate, socket recovery, and
malformed-input rejection.

## Performance regression

| Client | Mean requests/sec | Mean peak RSS |
|---|---:|---:|
| wrk 4.2.0 | 39,189.59 | 3,607,757 B |
| rload 0.1.2 | 39,182.02 | 3,807,642 B |
| rload development | 39,202.07 | 3,801,088 B |

Development versus release: throughput +0.05%, peak RSS -0.17%. Development
versus wrk: throughput +0.03%, peak RSS +5.36%. No throughput regression was
observed and rload development memory remained effectively equal to 0.1.2.

## Statistical accuracy versus wrk

| Candidate | RPS MAE | Avg MAE | P50 MAE | P75 MAE | P90 MAE | P99 median absolute error | Result |
|---|---:|---:|---:|---:|---:|---:|:---:|
| rload 0.1.2 | 0.387% | 0.791% | 0.485% | 0.991% | 1.160% | 0.699% | PASS |
| rload development | 0.464% | 0.712% | 0.485% | 0.763% | 0.957% | 0.943% | PASS |

Every metric passed the established gates: 3% MAE for RPS, average, P50, and
P75; 5% MAE for P90; and 5% median absolute paired error for P99.

Raw results are archived in
`benchmarks/results/threeway-20260712T124357Z`. The development commit also
passed the Linux, macOS, and Windows CI matrix before this report was updated.

## Verdict

All three required dimensions pass for the current development commit:
functional completeness, performance regression, and statistical accuracy.
Future benchmark sign-offs must preserve the same three-way comparison against
wrk, the latest published rload release, and the current development build.
