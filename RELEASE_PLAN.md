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
3. Decide whether replay rate control, timestamp pacing, burst profiles, and
   target inference belong in 0.2.0; they remain optional and unimplemented.
4. Keep Lua/LuaJIT out of the first release line unless a separate compatibility
   design and licensing review is approved.
5. For 0.2.0, add tolerant access-log replay: skip unsupported methods instead of
   aborting the input, track skipped records by method and total, and include
   those counts in the final load summary and machine-readable result contract.

## Deferred follow-up work

- Re-run the wrk accuracy matrix on a dedicated or separate-server host. This is
  intentionally deferred and is not a blocker for the current 0.1.1 release
  preparation.
- Investigate and resolve the zero-delay P99 sensitivity before claiming
  unconditional parity across environments.
- Define the skipped-record output schema and verify that skipped access-log
  entries do not affect sent-request latency, throughput, or URI statistics.

## Release checklist

- [x] Confirm crates.io metadata and README links resolve.
- [x] Run `./scripts/release-check.sh` on the release commit.
- [x] Run the local wrk accuracy and access-log replay matrices and archive
      results.
- [ ] Run the deferred independent-server accuracy matrix (post-release task).
- [x] Review `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD_PARTY_NOTICES.md`.
- [ ] Tag the release and publish the changelog.
