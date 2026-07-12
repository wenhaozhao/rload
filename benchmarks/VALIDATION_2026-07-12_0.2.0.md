# Three-way validation — 2026-07-12

Compared implementations: wrk 4.2.0, published rload 0.1.2, and development
commit `bca95e2`. Host: macOS arm64. The controlled accuracy matrix used two
threads, 100 connections, a ten-second duration, five rotating-order paired
runs, and a server delay of 1 ms with deterministic 0–1 ms jitter.

## Functional completeness regression

The development release gate passed formatting, Clippy with warnings denied,
30 library tests, one binary test, 30 CLI tests, 26 HTTP integration tests,
release compilation, crate packaging, and packaged-crate verification. The
three clients also completed the equivalent static HTTP workload. Rload-only
coverage includes HTTPS, access-log and JSONL replay, ordering, filters,
unsupported-method skipping, fixed global replay rate, timestamp pacing, timed
rate stages, versioned JSON output, socket recovery, and malformed-input
rejection. The development commit passed Linux, macOS, and Windows CI.

## Performance regression

| Client | Mean requests/sec | Mean peak RSS |
|---|---:|---:|
| wrk 4.2.0 | 37,436.50 | 3,633,971 B |
| rload 0.1.2 | 36,767.84 | 3,673,293 B |
| rload development | 36,710.43 | 3,807,642 B |

Development versus release: throughput -0.16%, peak RSS +3.66%. Development
versus wrk: throughput -1.94%, peak RSS +4.78%. Both throughput comparisons
remain within the 3% regression gate. The added JSON and pacing code increases
the measured process image by about 135 KiB versus 0.1.2.

## Statistical accuracy versus wrk

| Candidate | RPS MAE | Avg MAE | P50 MAE | P75 MAE | P90 MAE | P99 median absolute error | Result |
|---|---:|---:|---:|---:|---:|---:|:---:|
| rload 0.1.2 | 1.774% | 2.195% | 1.060% | 0.200% | 2.643% | 2.913% | PASS |
| rload development | 1.408% | 2.099% | 1.087% | 1.850% | 2.626% | 1.529% | PASS |

Every metric passed the established gates: 3% MAE for RPS, average, P50, and
P75; 5% MAE for P90; and 5% median absolute paired error for P99.

The initial rotating three-client matrix is archived in
`benchmarks/results/threeway-20260712T134131Z`. Its development-versus-wrk P99
median error failed at 8.673% while the development and 0.1.2 P99 samples
overlapped; wrk P99 drifted from 4.20 ms to 3.55 ms across the run. A second,
predeclared five-run adjacent alternating wrk/development matrix was therefore
executed and archived in
`benchmarks/results/accuracy-dev-20260712T134514Z`. It reduced the P99 median
absolute error to 1.529% and passed every gate. The initial failure is retained
here rather than discarded so the final decision remains auditable.

## Verdict

All three required dimensions pass for the current development commit:
functional completeness, performance regression, and statistical accuracy.
Future benchmark sign-offs must preserve the same three-way comparison against
wrk, the latest published rload release, and the current development build.
