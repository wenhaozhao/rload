# Changelog

All notable changes to rload are documented here.

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
