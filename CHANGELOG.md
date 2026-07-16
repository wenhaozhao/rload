# Changelog

All notable changes to rload are documented here.

## [Unreleased]

### Planned for 0.3.0

- Add minimum, maximum, average, and median latency statistics to aggregate and
  per-method reports while preserving the existing P50 JSON field.
- Record runtime failures and continue valid load runs, with categorized
  recovery and bounded retry behavior for fixed-request workloads.

## [0.2.4] - 2026-07-15

### Changed

- JSONL replay can skip malformed or invalid records with the opt-in
  `--skip-invalid-records` flag and reports skipped-record totals grouped by
  reason in text, beauty, and JSON output. Strict loading remains the default.

## [0.2.3] - 2026-07-15

### Added

- Generic `--stages` rate profiles for ordinary requests, Nginx access-log
  replay, and JSONL replay.
- `--version` output for identifying the installed rload build.

### Compatibility

- `--replay-stages` remains supported for replay inputs. Supplying it together
  with `--stages` is rejected before file or network I/O.
- JSON schema v1 retains `replay.stages` and also exposes the same profile as
  the additive, input-neutral `pacing.stages` field.

## [0.2.2] - 2026-07-14

### Added

- Finite replay cycles with `--replay-rounds` for filtered sequential and
  shuffle replay.
- Optional YAML request schemas for nested JSONL field extraction with
  per-field fallback to the existing top-level extraction rules.
- JSONL timestamp pacing with schema-defined chrono formats and load-time
  materialization to microseconds.
- Schema-free JSONL timestamp pacing through the default top-level timestamp
  aliases and formats.
- Default JSONL timestamp parsing accepts both Nginx and RFC3339 values when
  no explicit format is configured.
- Configured/completed replay-round fields in text, beauty, and JSON output.

### Fixed

- Timestamp-paced connections closed by the server before their scheduled
  request now reconnect at the pacing deadline instead of failing fixed-count
  runs or repeatedly reconnecting while idle.

## [0.2.1] - 2026-07-13

### Added

- A wrk-compatible `read_bytes` response counter alongside the existing
  decoded `response_body_bytes` metric in text and JSON output.
- Opt-in sectioned CLI output with `--output-beauty`.
- JSONL `args` query-string extraction and representative exported-log
  regression fixtures.

### Changed

- JSONL request replay now ignores unknown top-level fields and defaults a
  missing or null `method` to `GET`.
- Successfully read response bytes remain counted when a later socket error
  interrupts the request.

### Compatibility

- Existing default text parser anchors and `--output-format json` schema
  version 1 are retained. `response_body_bytes` remains available without
  renaming; the default text report adds one `read_bytes` line.

## [0.2.0] - 2026-07-12

### Added

- Global non-blocking replay rate control with `--replay-rate`.
- Nginx access-log timestamp pacing with `--replay-timestamps` and
  `--replay-speed`.
- Timed rate stages with `--replay-stages`, including baseline, spike, and
  recovery profiles.
- Versioned machine-readable results with `--output-format json`.
- Cross-platform CI coverage for Linux, macOS, and Windows.
- Tolerant access-log replay that skips and reports unsupported methods.

### Compatibility

- Core wrk-compatible request, duration, connection, thread, timeout, latency,
  and HTTP method options remain supported.
- Lua/LuaJIT scripting remains intentionally unsupported in the initial Rust
  release line.
- Fixed-rate, timestamp, and stage pacing are mutually exclusive.

### Validation

- Functional regression, performance regression, and statistical accuracy were
  validated against wrk 4.2.0, published rload 0.1.2, and the 0.2.0 development
  build using five-run alternating matrices.
