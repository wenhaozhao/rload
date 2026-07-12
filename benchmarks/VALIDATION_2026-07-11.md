# Completeness validation — 2026-07-11

Baseline: wrk 4.2.0 (kqueue). Candidate: rload 0.1.0. Host: macOS arm64,
Rust 1.96.1. All comparisons used the same local server, URL, two client
threads, connection count, duration, and alternating client order.

## Functional and release gates

The local release gate passed: formatting, Clippy with warnings denied, 73
tests, release build, crate packaging, and compilation from the packaged crate.
Coverage includes HTTP/HTTPS, response framing, fixed-count and duration runs,
curl-style ordinary requests, access-log and JSONL replay, replay ordering and
filters, statistics, socket-error recovery, CLI exits, and malformed-protocol
failure behavior.

This is complete for the declared first-release scope, not a complete drop-in
replacement for every wrk feature. Lua/LuaJIT scripting is intentionally absent.
Access-log replay is intentionally limited to GET and HEAD because common and
combined logs cannot reconstruct request bodies. Original timestamp pacing,
rate control, burst profiles, and target inference are optional future work.

## Static HTTP performance and accuracy

Five paired 10-second runs were performed at each of 10, 100, and 400
connections (15 pairs total). Result directories:

- `20260711T143304Z` — 10 connections
- `20260711T143450Z` — 100 connections
- `20260711T143636Z` — 400 connections

| Metric | Bias | MAE | Gate statistic | Result |
|---|---:|---:|---:|:---:|
| Requests/sec | +0.673% | 0.986% | MAE 0.986% <= 3% | PASS |
| Average latency | -0.530% | 1.171% | MAE 1.171% <= 3% | PASS |
| P50 | -0.274% | 1.064% | MAE 1.064% <= 3% | PASS |
| P75 | -0.159% | 0.961% | MAE 0.961% <= 3% | PASS |
| P90 | -0.350% | 0.944% | MAE 0.944% <= 5% | PASS |
| P99 | +0.927% | 5.638% | median absolute error 5.128% <= 5% | **FAIL** |

RPS performance is equivalent within the configured 3% gate. The figures above
were collected before a later lazy-allocation optimization for per-method HDR
histograms, so they are retained as historical evidence rather than the current
static-memory baseline.

After that optimization, a fresh three-pair, 100-connection, five-second
comparison (`20260711T235031Z`) reported median maximum RSS of approximately
3.47 MiB for wrk and 3.55 MiB for rload. Its RPS MAE was 2.170% and all
latency gates passed. The optimization allocates an HDR histogram only once its
HTTP method is observed, preserving the original metrics API and avoiding
unused-method allocations.

The zero-delay local P99 result is not accepted under the preset rule, despite
missing by only 0.128 percentage points. It must not be reported as a pass.

## Controlled tail-latency check

Five additional 100-connection pairs used a fixed 1 ms server delay and
deterministic 0–1 ms jitter (`20260711T144024Z`). Every metric passed. RPS MAE
was 0.516%; P90 MAE was 1.825%; P99 median absolute error was 0.567%. This
supports correctness of the histogram and percentile implementation while
showing that the zero-delay loopback P99 gate is sensitive to scheduler noise.

## Access-log replay performance

Three alternating static/replay pairs were run at each input size:

| Entries | Median throughput loss | Incremental RSS | Result |
|---:|---:|---:|:---:|
| 100,000 | +1.68% | 252.5 B/entry | PASS |
| 500,000 | -1.81% | 249.1 B/entry | PASS |

The measured RSS scaling slope was 248.7 B/entry, passing the configured
0–256 B/entry gate. Result directories are
`replay-20260711T143854Z-sgFZQy` and `replay-20260711T143935Z-Rkqi2O`.

## Verdict

rload is functionally complete for its explicitly declared first-release scope,
and its throughput, central latency, controlled tail latency, replay overhead,
and replay-memory scaling pass their gates. It is not feature-equivalent to wrk
because Lua scripting is intentionally excluded. Unqualified accuracy sign-off
is also withheld because this run's zero-delay P99 gate narrowly failed; repeat
the full matrix on a dedicated or separate-server environment before release.
