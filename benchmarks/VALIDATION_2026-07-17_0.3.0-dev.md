# rload 0.3.0 development validation — 2026-07-17

Validated package version: `0.3.0-rc.1`.

## Local release gate

The required local release gate completed successfully on macOS arm64:

- `cargo fmt --check`
- Clippy for all targets with warnings denied
- all library, binary, CLI, and HTTP integration tests
- release build
- crate packaging and packaged-crate verification
- license and third-party-notice checks

The test suite covered 43 library tests, one binary test, 50 CLI tests, and
32 HTTP integration tests.

## Runtime-failure coverage

The current development branch validates the following fixed-request recovery
behaviour:

- an unavailable target is retried only within a bounded initial connection
  budget, then the remaining requests are reported as abandoned;
- initial, asynchronous, read, write, and timeout recovery share a three-attempt
  fixed-request budget;
- TLS certificate failures remain terminal configuration/runtime TLS failures;
- TLS-handshake I/O failures may advance to a subsequent resolved address.

`abandoned_requests` and `recovery_attempts` are exposed in the text, beauty,
JSON, HTML, and assertion outputs, including profile assertions.

## Three-way benchmark

Raw results: `benchmarks/results/threeway-20260717T142325Z`

Parameters: wrk 4.2.0, installed rload 0.2.4, current v0.3.0 development
build; 2 threads, 100 connections, 10 seconds, 5 alternating runs, 1 ms
deterministic delay plus up to 1 ms jitter.

| Client | Mean RPS | Mean peak RSS |
|---|---:|---:|
| wrk | 39,974.26 | 3,597,926 B |
| rload 0.2.4 release | 39,830.91 | 3,896,115 B |
| rload 0.3.0 development | 39,819.29 | 4,102,554 B |

Development RPS MAE versus wrk was 0.497%, P90 MAE was 1.525%, and P99
median absolute error was 3.476%. Mean development throughput was 0.03% below
the installed release. The `read_bytes` per-request MAE was 0.0128%. All
three-way gates passed.

## Replay RSS validation

Raw results:

- `benchmarks/results/replay-20260717T142026Z-DjnUNl` (100,000 entries)
- `benchmarks/results/replay-20260717T142121Z-TOGppo` (500,000 entries)

The 100,000-entry run used 255.3 B/entry with a -0.89% median throughput loss.
The 500,000-entry run used 244.6 B/entry with a -6.09% median throughput loss.
The cross-size RSS scaling slope was 242.1 B/entry. Both sizes and the scaling
slope passed the 256 B/entry memory gate; throughput remained within the 10%
gate.

## Result and remaining gates

PASS for the local release, package, three-way benchmark, replay RSS, and
cross-platform CI gates. GitHub Actions run `29594622880` passed on Linux,
macOS, and Windows; Windows also passed the path, socket-recovery, and
PowerShell release-binary checks.

`v0.3.0-rc.1` was published on 2026-07-19. Release workflow `29672618267`
published the crate, created the GitHub Release, uploaded Linux, macOS, and
Windows artifacts, updated the Homebrew tap, and deployed the website and
versioned user-guide references. The independent-server accuracy matrix remains
a post-release follow-up.
