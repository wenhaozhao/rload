# Three-way validation — 2026-07-12

Compared implementations: wrk 4.2.0, published rload 0.1.2, and development
commit `6b1fa29`. Host: macOS arm64. The controlled accuracy matrix used two
threads, 100 connections, a three-second duration, five alternating paired
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
| wrk 4.2.0 | 40,415.50 | 3,617,587 B |
| rload 0.1.2 | 40,332.02 | 3,742,106 B |
| rload development | 40,557.25 | 3,738,829 B |

Development versus release: throughput +0.56%, peak RSS -0.09%. Development
versus wrk: throughput +0.35%, peak RSS +3.35%. No throughput regression was
observed and rload development memory remained effectively equal to 0.1.2.

## Statistical accuracy versus wrk

| Candidate | RPS MAE | Avg MAE | P50 MAE | P75 MAE | P90 MAE | P99 median absolute error | Result |
|---|---:|---:|---:|---:|---:|---:|:---:|
| rload 0.1.2 | 0.416% | 1.146% | 1.749% | 1.046% | 1.396% | 1.520% | PASS |
| rload development | 0.952% | 0.737% | 1.033% | 0.972% | 0.979% | 1.212% | PASS |

Every metric passed the established gates: 3% MAE for RPS, average, P50, and
P75; 5% MAE for P90; and 5% median absolute paired error for P99.

Raw local results: `/tmp/rload-threeway-final-En0Ji2`. This path is retained in
the report for local audit; benchmark automation should archive future raw
results in `benchmarks/results/` when preparing a release artifact.

## Verdict

All three required dimensions pass for the current development commit:
functional completeness, performance regression, and statistical accuracy.
Future benchmark sign-offs must preserve the same three-way comparison against
wrk, the latest published rload release, and the current development build.
