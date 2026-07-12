# Rload release plan

## Current status

- `rload` 0.1.0 and 0.1.1 are published to crates.io; the latest published
  version is 0.1.1.
- The 0.1.1 package includes the standard license files and third-party notice.
- The package metadata points to the public repository, homepage, and docs.rs.
- `./scripts/release-check.sh` is the required local gate.
- The current release baseline is macOS arm64; Linux and other targets remain
  candidates until their CI gates run.

## Post-0.1.1 priorities

1. Add CI on macOS and Linux for formatting, Clippy, tests, package verification,
   and a smoke HTTP run.
2. Publish a signed changelog and migration notes from the old internal
   `r-wrk` name to `rload`.
3. Add replay rate control in 0.2.0: a fixed global request rate with explicit
   pacing semantics, validation, and measured-rate reporting. This remains
   independent from request selection order and burst profiles.
4. Add access-log timestamp pacing in 0.2.0: preserve inter-record timestamp
   gaps, support a playback multiplier, define behavior for second-only versus
   sub-second timestamps, and report timestamp gaps that cannot be reproduced.
   This remains independent from fixed-rate and burst modes.
5. Keep Lua/LuaJIT out of the first release line unless a separate compatibility
   design and licensing review is approved.
6. For 0.2.0, add tolerant access-log replay: skip unsupported methods instead of
   aborting the input, track skipped records by method and total, and include
   those counts in the final load summary and machine-readable result contract.
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

## Release checklist

- [x] Confirm crates.io metadata and README links resolve.
- [x] Run `./scripts/release-check.sh` on the release commit.
- [x] Run the local wrk accuracy and access-log replay matrices and archive
      results.
- [ ] Run the deferred independent-server accuracy matrix (post-release task).
- [x] Review `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD_PARTY_NOTICES.md`.
- [ ] Tag the release and publish the changelog.
