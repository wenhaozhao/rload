# Quality acceptance standard

This document is the single entry point for deciding whether an `rload`
change is ready to release. It summarizes the required gates; the linked
documents remain the source for their detailed procedures.

## 1. Automated release gate

Run:

```sh
./scripts/release-check.sh
```

It must pass formatting, Clippy with warnings denied, all targets' tests, the
release build, crate packaging, package verification, and required license and
release-document files. See [`scripts/release-check.sh`](../scripts/release-check.sh).

## 2. Functional and compatibility acceptance

- Existing CLI behavior, text parser anchors, JSON schema v1, and replay
  behavior remain compatible unless a compatibility change is explicitly
  designed and documented.
- Invalid configuration must fail before target connection and identify the
  affected field or conflicting options.
- CLI values override YAML profile values, which override defaults.
- New or changed CLI options cover static requests, Nginx access-log replay,
  and JSONL replay where applicable. Supported combinations, prerequisites,
  mutual exclusions, argument order, help text, errors, and public API
  validation require acceptance coverage.
- Runtime transport failures are categorized and isolated; configuration,
  input, and protocol failures retain their documented terminal behavior.

See [`CODING_STANDARDS.md`](../CODING_STANDARDS.md) and the frozen release
contract for the detailed requirements.

## 3. Statistical accuracy and baseline performance

Compare `wrk`, the latest published `rload`, and the candidate with the same
server, workload, threads, connections, and duration. Use at least five
alternating paired runs for a release decision, and retain raw results.

| Metric | Acceptance gate |
|---|---:|
| Requests/sec, average latency, P50, P75 | MAE <= 3% versus `wrk` |
| P90 | MAE <= 5% versus `wrk` |
| P99 | Median absolute paired error <= 5% versus `wrk` |
| Candidate throughput | No more than 3% regression versus published `rload` |
| Replay RSS | No more than 256 B per loaded entry |

Record throughput, latency percentiles, errors, peak RSS, workload parameters,
and environment. The full methodology is in
[`benchmarks/ACCURACY.md`](../benchmarks/ACCURACY.md) and `RELEASE_PLAN.md`.

## 4. Timed rate-stage accuracy

`--stages` and replay's compatible `--replay-stages` must be validated from
request arrival timestamps at a local target, not from configured values alone.

- Use at least three stages, each at least 10 seconds long.
- Use a target rate of at least 20 RPS for relative-error checks.
- In every stable stage, the actual RPS must be within 5% of configured RPS.
- For low-rate stages, allow no more than one request of count deviation rather
  than applying a misleading relative percentage.
- Verify that transitions neither start early nor create a compensating burst;
  assess the stable window of each stage independently.
- Cover ordinary requests, Nginx access-log replay, and JSONL replay before
  release when stage pacing changes.

## 5. Metric-affecting changes

Every change that targets or materially affects throughput, latency, memory,
CPU, byte accuracy, error rates, recovery counters, or load accuracy requires
a committed record under `benchmarks/metric_changes/`. The record must retain
failed and passing measurements, cause analysis, alternatives, rollback, and
revisit triggers. See [`benchmarks/metric_changes/README.md`](../benchmarks/metric_changes/README.md).

## 6. Release consistency and manual gates

- Version, release artifacts, README, user guides, website, and CHANGELOG
  agree.
- Review license and third-party notices before distribution.
- Complete the release documentation handoff after publication and record
  target-platform validation.
- Complete the promotion record only when external communications are
  authorized.

See [`RELEASE_DOCUMENTATION_INDEX.md`](RELEASE_DOCUMENTATION_INDEX.md).
