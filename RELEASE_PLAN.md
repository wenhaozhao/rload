# Rload release plan

## Current status

- `rload` 0.1.2 is published to crates.io.
- The 0.1.2 package includes the standard license files and third-party notice.
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
4. Add access-log timestamp pacing in 0.2.0: preserve inter-record timestamp
   gaps, support a playback multiplier, define behavior for second-only versus
   sub-second timestamps, and report timestamp gaps that cannot be reproduced.
   This remains independent from fixed-rate and burst modes.
5. Keep Lua/LuaJIT out of the first release line unless a separate compatibility
   design and licensing review is approved.
6. Tolerant access-log replay is implemented on the 0.2.0 development branch:
   unsupported methods are skipped, counted by method and total, and printed in
   the final summary. Machine-readable result output remains to be designed.
7. For 0.2.0, add burst/stage traffic models that control send rate over time
   (for example, a baseline rate followed by a timed spike and recovery). Keep
   this independent from request selection order: sequential, shuffle, and
   random determine which request is selected, while burst profiles determine
   when requests are sent.

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

## Deferred follow-up work

- Re-run the wrk accuracy matrix on a dedicated or separate-server host. This is
  intentionally deferred and is not a blocker for the current 0.1.1 release
  preparation.
- Investigate and resolve the zero-delay P99 sensitivity before claiming
  unconditional parity across environments.
- Define the skipped-record output schema and verify that skipped access-log
   entries do not affect sent-request latency, throughput, or URI statistics.
- Define mutually exclusive/composable rules for fixed-rate, timestamp, and
  burst pacing, then add deterministic tests for rate, multiplier, timestamp
  precision, and end-of-run behavior.

## Benchmark policy

Every benchmark sign-off must cover three dimensions: functional completeness
regression, performance regression, and statistical accuracy. Each dimension
must compare the same three implementations under equivalent parameters:
`wrk`, the latest published rload release, and the current development build.
Use at least five alternating paired runs for statistical accuracy decisions;
record throughput, latency percentiles, errors, and peak RSS, and retain the
raw result directories with the report.

## Release checklist

- [x] Confirm crates.io metadata and README links resolve.
- [x] Run `./scripts/release-check.sh` on the release commit.
- [x] Run the local wrk accuracy and access-log replay matrices and archive
      results.
- [ ] Run the deferred independent-server accuracy matrix (post-release task).
- [x] Review `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD_PARTY_NOTICES.md`.
- [ ] Tag the release and publish the changelog.

## Automated release workflow

Push a tag in the form `vMAJOR.MINOR.PATCH` (for example `v0.1.2`) to run
`.github/workflows/release.yml`. The workflow validates that the tag matches
`Cargo.toml`, runs the release gate, publishes the crate, creates a GitHub
Release, updates `wenhaozhao/homebrew-rload`, and commits the new version to
the GitHub Pages homepage.

Required Actions secrets:

- `CARGO_REGISTRY_TOKEN`: crates.io API token with publish permission.
- `HOMEBREW_TAP_TOKEN`: fine-grained token with Contents read/write access to
  `wenhaozhao/homebrew-rload`.

The workflow can also be started manually with a release tag input.
