# Rload release plan

## Current status

- `rload` 0.2.4 is the current release line; the main branch contains the
  v0.2.4 promotion materials and the next planned release is v0.3.0.
- The package includes the standard license files and third-party notice.
- The package metadata points to the public repository, homepage, and docs.rs.
- `./scripts/release-check.sh` is the required local gate.
- The 0.2.0 development branch adds CI gates for macOS, Linux, and Windows.

## Post-0.1.1 priorities

1. Add CI on macOS, Linux, and Windows for formatting, Clippy, tests, package
   verification, and a smoke HTTP run. Windows must also verify path handling,
   socket error recovery, and the release binary invocation from PowerShell.
2. Publish a signed changelog and migration notes from the old internal
   `r-wrk` name to `rload`.
3. Fixed global replay rate control is implemented on the 0.2.0 development
   branch with validation and configured/measured-rate reporting. It remains
   independent from request selection order and burst profiles.
4. Access-log timestamp pacing is implemented on the 0.2.0 development branch:
   it preserves inter-record gaps, supports a playback multiplier, accepts
   second or fractional-second timestamps, rejects missing/decreasing values,
   and explicitly treats the unknowable loop-boundary gap as zero. It is
   independent from request selection and mutually exclusive with fixed rate.
5. Keep Lua/LuaJIT out of the first release line unless a separate compatibility
   design and licensing review is approved.
6. Tolerant access-log replay is implemented on the 0.2.0 development branch:
   unsupported methods are skipped, counted by method and total, and printed in
   the final summary. Machine-readable result output remains to be designed.
7. Burst/stage traffic models are implemented on the 0.2.0 development branch.
   Timed rate stages support baseline, spike, and recovery profiles, hold the
   final rate after the profile ends, and remain independent from sequential,
   shuffle, or random request selection.
8. Versioned machine-readable output is implemented with
   `--output-format json`. Schema version 1 covers aggregate, latency, HTTP,
   method, URI, socket-error, replay filtering/skipping, and pacing fields while
   preserving the existing text output as the default.

## Future candidate features

- Automatic target inference from access-log fields that explicitly provide
  scheme, host, and port. This is intentionally not scheduled for 0.2.0;
  standard origin-form logs do not contain enough information to infer a target
  safely.
- Optional GUI configuration interface built on top of the rload engine. The
  GUI should configure and validate workloads, launch or attach to an rload
  execution, and present live/final statistics without moving load-generation
  logic into the UI. The CLI and engine must remain usable without GUI
  dependencies.
- Scripted request/response hooks for request preparation and response result
  processing. The intended pipeline is `request -> pre-script filter -> core ->
  response -> post-script filter`. This remains a future candidate until a
  runtime, sandbox, failure model, and isolation design can guarantee a
  zero-cost disabled path and no performance regression for unscripted loads.
  It is not planned for v0.2.2 or v0.3.0.

## v0.2.1 planned work

Implementation and the local three-way validation gate were completed on
2026-07-13. The raw benchmark results are archived under
`benchmarks/results/threeway-20260713T102319Z`, with the sign-off report in
`benchmarks/VALIDATION_2026-07-13_0.2.1.md`.

- Add `RunSummary::read_bytes` permanently alongside the existing
  `response_body_bytes` field. `read_bytes` will count bytes successfully read
  from the response socket and handed to the HTTP parser, including response
  headers, body bytes, and chunk framing, so it can be compared directly with
  wrk's `read` metric. `response_body_bytes` will continue to represent only
  decoded response payload bytes for application-level traffic analysis.
- Report both counters in text output and JSON schema v1 without removing or
  renaming `response_body_bytes`; add regression coverage for content-length,
  chunked, connection-close, and partial-read/error cases.
- Define and document the rule that bytes successfully read before a later
  socket error remain counted, matching wrk's incremental read accounting.
- Extend the three-way benchmark report to compare `read_bytes` with wrk's
  `read` value while retaining `response_body_bytes` for payload comparisons.
- Make JSONL request replay tolerant of exported application-log records:
  ignore unknown top-level fields while retaining validation for known fields.
- Default an omitted JSONL `method` to `GET`; define whether explicit `null`
  has the same meaning and cover the decision with tests and documentation.
- Extract the top-level JSONL `args` field as a raw query string and append it
  to `uri`, using `?` or `&` as appropriate. Define the canonical input rule
  that `args` must not include its own separator, and test existing-query,
  empty-query, and malformed-query edge cases.
- Add a representative two-line fixture based on real exported logs and use it
  in parser regression tests, including unknown fields, missing fields, and
  repeated records.
- Add opt-in human-readable CLI formatting with `--output-beauty`. Keep the
  existing default text parser anchors compatible for benchmark scripts and keep
  `--output-format json` unchanged. Add golden and CLI tests for the beauty
  format without changing the benchmark execution path.

## v0.2.2 completed work

The next replay-focused release will make finite replay cycles explicit and
bring timestamp pacing to JSONL request files.

1. Add `--replay-rounds <N>` for finite replay inputs. A round is one complete
   traversal of the filtered sequence; require a positive integer, define its
   interaction with duration/request limits and all pacing/order modes, and
   report configured/completed rounds without changing defaults.
2. Permit `--replay-timestamps` with `--request-file` and require the JSONL
   schema to define the timestamp format. Use `timestamp_micros` as the
   canonical field, with `time` and `_time` aliases. The schema format uses
   strftime/chrono-style placeholders and defaults to the Nginx format
   `%d/%b/%Y:%H:%M:%S %z`; fractional seconds are supported. Normalize all
   parsed values to the existing microsecond representation. No separate
   timestamp-format CLI option will be added.
3. Define alias precedence and validation: canonical `timestamp_micros` wins;
   conflicting aliases are rejected, malformed or missing timestamps in
   timestamp mode are rejected, and timestamps must be non-decreasing.
4. Share existing timestamp semantics: `--replay-speed` applies, sequential
   order is required, timestamp pacing is mutually exclusive with fixed-rate
   and stage pacing, and no synthetic delay is added between cycles.
5. Preserve tolerant JSONL behavior (unknown fields ignored, missing method
   defaults to GET, existing `args` joining rules) and update README,
   README.zh-cn, CLI help, schema notes, and changelog.
6. Add parser, pacing, CLI, compatibility, and benchmark regressions covering
   both replay sources and deterministic finite-cycle counts. Run Linux,
   macOS, and Windows CI plus the three-way benchmark gate before release.
7. Add an optional schema file for nested JSONL field mappings. Every schema
   mapping is optional: absent mappings use the conventional top-level names,
   while an absent mapping falls back to the current extraction logic for that
   field. Schema configuration changes extraction paths only; it does not
   change existing record-value defaults or validation. Timestamp remains
   optional for ordinary replay but is required when timestamp pacing is
   enabled. The schema owns timestamp format configuration; do not add a
   separate timestamp-format CLI option.
8. Enforce load-time materialization: compile schema paths and parse all field
   values, including timestamps, while loading JSONL, then store the existing
   `ReplayRequest` objects. No dynamic JSON traversal, schema lookup, or
   timestamp string parsing is permitted in the request/response hot path.

The implementation was released as `v0.2.2` on 2026-07-14. The final
validation report is `benchmarks/VALIDATION_2026-07-14_0.2.2-final.md`.

## v0.2.3 completed work

This small compatibility release generalizes staged rate control without
removing the existing replay-specific option name.

1. Add `--stages <DURATION:RPS,...>` for ordinary requests, access-log replay,
   and JSONL replay, using the existing global `StagePacer` semantics.
2. Retain `--replay-stages` as a compatibility alias restricted to replay
   inputs; reject both names when supplied together.
3. Preserve existing replay text anchors and JSON schema v1 while reporting
   ordinary-request stages as rate stages.
4. Add `--version`, printing `rload 0.2.3` to stdout without requiring a target.
5. Cover every input mode, custom ordinary requests, help text, option-order
   independence, conflicts, and compatibility behavior in CLI tests.
6. Run the cross-platform CI and release gate before tagging `v0.2.3`.

The local release gate and five-run three-way benchmark passed on 2026-07-15.
See `benchmarks/VALIDATION_2026-07-15_0.2.3.md`. Linux and Windows CI remain
pending until the branch is pushed.

## v0.2.4 completed work

This maintenance release makes tolerant JSONL loading explicit and observable.

1. Add opt-in `--skip-invalid-records` behavior for JSONL request files while
   preserving strict loading by default.
2. Continue loading valid records and aggregate skipped records by validation
   reason; reject files containing no valid records.
3. Report skipped JSONL totals and reasons in text, beauty, and JSON output.
4. Model replay preparation and skipped-record accounting with named structs
   and the `SkippedRecords` newtype instead of positional tuples and raw maps.
5. Cover strict and tolerant loading, CLI validation, summaries, formatting,
   Clippy, tests, release builds, and package contents before tagging.

## v0.3.0 development plan

### Product outcome

Make a v0.2.4 workload reproducible from a committed `rload.yaml`, enforce
performance budgets in CI, and produce an offline report. New interfaces must
adapt the existing validated configuration and `RunSummary`; the load engine
does not gain a second execution model.

### Scope priority

1. **P0: profile loading** — support existing static requests, access-log
   replay, JSONL replay, filters, rounds, pacing, limits, and output options.
   Define CLI > YAML > defaults precedence and reject invalid combinations
   before network I/O.
2. **P0: assertions** — add typed comparisons for `rps`, `mean`, `p50`, `p90`,
   `p95`, `p99`, `error_rate`, `status_errors`, `socket_errors`, and
   `completed`. Support latency units (`us`, `ms`, `s`) and stable diagnostics.
3. **P0: HTML report** — generate `--output-html` from JSON result data as a
   deterministic, single-file, offline artifact.
4. **P0: latency summary statistics** — expose maximum, minimum, mean, and
   median latency in aggregate and per-method reports. Median is P50; retain
   the existing `p50_us` JSON field and add an explicit `median_us` field.
   Define empty-sample behavior before changing output.
5. **P0: failure-tolerant execution** — runtime network, timeout, TLS, read,
   write, and response failures are recorded and isolated without aborting the
   valid run. Configuration, malformed replay input, and startup failures remain
   fail-fast. Fixed-request recovery must be bounded and observable.
6. **P1/stretch: Prometheus** — add only after a low-contention snapshot design
   is measured; keep it opt-in and outside the benchmark hot path.

### Delivery slices

- **S1 — Contract freeze**: profile v1, assertion grammar, metric units, error
  codes, JSON compatibility, and HTML data contract.
- **S2 — Configuration**: parser, defaults, CLI precedence, field diagnostics,
  cross-field validation, and static/replay integration tests.
- **S3 — CI assertions**: lexer/parser/evaluator, final-summary evaluation,
  pass/fail exit behavior, and stable CI-friendly stderr diagnostics.
- **S4 — Metrics**: add min/max/mean/median accessors, aggregate and method
  output, JSON compatibility fields, empty-sample handling, and regression
  fixtures. Median must share the P50 definition rather than maintain a second
  calculation path.
- **S5 — Reporting**: deterministic HTML renderer, offline usability, CLI
  integration, fixtures, and documentation examples using all four statistics.
- **S6 — Failure tolerance**: define runtime-failure taxonomy, recovery state
  machine, retry budget, summary fields, and tests for every failure class.
- **S7 — Release hardening**: cross-platform CI, package checks, three-way
  benchmark, replay RSS validation, and `v0.3.0-rc.1`.

### Release gates

- Existing v0.2.4 CLI, text output, JSON schema v1, and replay behavior remain
  compatible.
- Invalid configuration fails before target connection and names the field.
- YAML and CLI assertions share one typed evaluator over `RunSummary`.
- Aggregate and per-method latency summaries expose min/max/mean/median; the
  explicit median field is equivalent to existing P50 within histogram precision.
- Empty latency samples are represented consistently in text, JSON, and HTML.
- Runtime failures never abort a valid load run and are reported by category;
  startup/configuration/input errors remain fail-fast.
- Fixed-request runs cannot retry an unavailable target forever: the retry
  budget/completion rule is explicit and covered by tests.
- Same JSON input produces byte-identical HTML output.
- Fixed baseline throughput regression is at most 3%; P99 regression target is
  at most 5%; replay memory stays within the existing gate.
- Release artifacts, version output, README, website, and changelog agree.

### Deferred from v0.3.0

HTTP/2, gRPC, Lua/LuaJIT, distributed execution, target inference, scripted
hooks, TUI/GUI, and mandatory Prometheus deployment.

## Deferred follow-up work

- Re-run the wrk accuracy matrix on a dedicated or separate-server host. This is
  intentionally deferred and is not a blocker for the current 0.1.1 release
  preparation.
- Investigate and resolve the zero-delay P99 sensitivity before claiming
  unconditional parity across environments.
- Define the skipped-record output schema and verify that skipped access-log
  entries do not affect sent-request latency, throughput, or URI statistics.
- Fixed-rate, timestamp, and stage pacing are mutually exclusive and covered by
  multiplier, precision, transition, and duration-boundary tests.

## Benchmark policy

Every benchmark sign-off must cover three dimensions: functional completeness
regression, performance regression, and statistical accuracy. Each dimension
must compare the same three implementations under equivalent parameters:
`wrk`, the latest published rload release, and the current development build.
Use at least five alternating paired runs for statistical accuracy decisions;
record throughput, latency percentiles, errors, and peak RSS, and retain the
raw result directories with the report.

Starting with v0.3.0, every metric optimization, regression, compensation, or
accepted tradeoff must also have a committed entry in
`benchmarks/metric_changes/` following its template. Release sign-off must
confirm that failed measurements remain archived alongside later passing
measurements and their cause or uncertainty assessment.

## Release checklist

- [x] Confirm crates.io metadata and README links resolve.
- [x] Run `./scripts/release-check.sh` on the release commit.
- [x] Run the local wrk accuracy and access-log replay matrices and archive
      results.
- [ ] Run the deferred independent-server accuracy matrix (post-release task).
- [x] Review `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD_PARTY_NOTICES.md`.
- [x] Update the version to 0.2.0 and run the final package gate.
- [x] Tag `v0.2.2` and publish the changelog.
- [x] Complete the local v0.2.3 release and three-way benchmark gates.
- [ ] Pass Linux, macOS, and Windows CI, then tag `v0.2.3`.
- [x] Complete the local v0.2.4 release gate and package verification.
- [ ] Pass Linux, macOS, and Windows CI, then tag `v0.2.4`.
- [ ] Freeze the v0.3.0 profile/assertion/report schemas.
- [ ] Implement and validate the v0.3.0 vertical slice.
- [ ] Confirm every v0.3.0 metric-affecting change has an indexed archive entry.

## Automated release workflow

Push a tag in the form `vMAJOR.MINOR.PATCH` (for example `v0.2.0`) to run
`.github/workflows/release.yml`. The workflow validates that the tag matches
`Cargo.toml`, runs the release gate, publishes the crate, creates a GitHub
Release, updates `wenhaozhao/homebrew-rload`, and commits the new version to
the GitHub Pages homepage.

Required Actions secrets:

- `CARGO_REGISTRY_TOKEN`: crates.io API token with publish permission.
- `HOMEBREW_TAP_TOKEN`: fine-grained token with Contents read/write access to
  `wenhaozhao/homebrew-rload`.

The workflow can also be started manually with a release tag input. Each
release also attaches precompiled archives for Linux x86_64, macOS arm64, and
Windows x86_64, together with SHA-256 checksum files. The archives contain the
`rload` executable, README, and license files.
