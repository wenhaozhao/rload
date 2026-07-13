# Three-way validation — 2026-07-13

Compared implementations: wrk 4.2.0, published rload 0.2.0, and the rload
0.2.1 development build. The controlled matrix used two threads, 100
connections, a ten-second duration, five rotating-order runs, and a local
server delay of 1 ms with deterministic 0–1 ms jitter. Raw results are archived
in `benchmarks/results/threeway-20260713T102319Z`.

## Functional completeness regression

The release gate passed formatting, Clippy with warnings denied, 34 library
tests, one binary test, 32 CLI tests, 26 HTTP integration tests, release
compilation, crate packaging, and packaged-crate verification. New coverage
checks content-length, chunked, connection-close, and partial-read accounting;
exported JSONL logs and query arguments; default text and JSON compatibility;
and the opt-in beauty output mode.

## Performance regression

| Client | Mean requests/sec | Mean peak RSS |
|---|---:|---:|
| wrk 4.2.0 | 40,212.88 | 3,611,034 B |
| rload 0.2.0 | 40,527.23 | 3,814,195 B |
| rload 0.2.1 development | 40,311.78 | 3,925,606 B |

Development versus 0.2.0: throughput -0.53%, peak RSS +2.92%.
Development versus wrk: throughput +0.25%, peak RSS +8.71%. The throughput
change remains comfortably inside the 3% regression gate. The approximately
109 KiB increase versus 0.2.0 includes the additional reporting and JSONL
compatibility code while leaving the hot request path allocation behavior
unchanged.

## Statistical accuracy versus wrk

| Candidate | RPS MAE | Avg MAE | P50 MAE | P75 MAE | P90 MAE | P99 median absolute error |
|---|---:|---:|---:|---:|---:|---:|
| rload 0.2.0 | 2.302% | 2.352% | 1.430% | 0.300% | 1.804% | 9.140% |
| rload 0.2.1 development | 0.428% | 0.567% | 1.030% | 0.745% | 0.832% | 0.909% |

The development build passes the established 3% gates for RPS, average, P50,
and P75 and the 5% gates for P90 and P99. The published 0.2.0 P99 comparison is
retained as observed scheduler-sensitive baseline evidence; it does not affect
the 0.2.1 sign-off because the development build passes its predeclared gate.

## Byte-accounting accuracy

The development `read_bytes` value was normalized per completed request and
compared with wrk's `read` value. Mean absolute error was **0.0069%** across the
five runs. `response_body_bytes` remains independently available for decoded
payload accounting.

## Verdict

All required dimensions pass for rload 0.2.1: functional completeness,
performance regression, statistical accuracy, and the additional wrk byte
accounting check.

The reusable three-way analysis exits non-zero when the development build
exceeds any statistical limit, regresses mean throughput by more than 3%
against the published release, or exceeds 0.1% per-request `read_bytes` MAE.
Peak RSS is reported for trend review but has no automatic limit because the
project has not established a cross-platform RSS gate.
