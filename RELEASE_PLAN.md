# Rload release plan

## Current status

- `rload` 0.1.0 has been manually published to crates.io.
- The license-file and third-party-notice completion in this checkout is
  prepared for the next publish, `0.1.1`; crates.io versions are immutable.
- The package metadata points to the public repository, homepage, and docs.rs.
- `./scripts/release-check.sh` is the required local gate.
- The current release baseline is macOS arm64; Linux and other targets remain
  candidates until their CI gates run.

## Post-0.1.0 priorities

1. Add CI on macOS and Linux for formatting, Clippy, tests, package verification,
   and a smoke HTTP run.
2. Re-run the wrk accuracy matrix on a dedicated or separate-server host and
   resolve the zero-delay P99 sensitivity before claiming unconditional parity.
3. Publish a signed changelog and migration notes from the old internal
   `r-wrk` name to `rload`.
4. Decide whether replay rate control, timestamp pacing, burst profiles, and
   target inference belong in 0.2.0; they remain optional and unimplemented.
5. Keep Lua/LuaJIT out of the first release line unless a separate compatibility
   design and licensing review is approved.

## Release checklist

- [ ] Confirm crates.io metadata and README links resolve.
- [ ] Run `./scripts/release-check.sh` on the release commit.
- [ ] Run the wrk accuracy and access-log replay matrices and archive results.
- [ ] Review `LICENSE-MIT`, `LICENSE-APACHE`, and `THIRD_PARTY_NOTICES.md`.
- [ ] Tag the release and publish the changelog.
