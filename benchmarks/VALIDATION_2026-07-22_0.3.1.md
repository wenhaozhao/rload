# rload 0.3.1 release-preparation validation — 2026-07-22

Validated package version: `0.3.1`.

## Scope

This patch release fixes CLI-over-profile precedence for timestamp replay and
the composition of a CLI replay seed with a profile-provided replay order. It
does not alter the request engine's throughput, latency, memory, or metric
calculation paths; no metric-change archive is required.

## Local release gate

Before tagging, `./scripts/release-check.sh` must pass with formatting,
warnings-denied Clippy, all test targets, a release build, package creation,
and packaged-crate verification. The validated suite includes 43 library
tests, one binary test, 57 CLI tests, and 32 HTTP integration tests.

## Stage-pacing gate

The CLI suite measures request-arrival timestamps at a local target for three
10-second stages at 20, 60, and 30 RPS. Each stage permits at most 5% request
count deviation. The gate covers ordinary requests, Nginx access-log replay,
and JSONL replay; it also rejects abnormally dense adjacent arrivals, including
at stage boundaries.

## Documentation preparation

The release documentation index was reviewed before tagging. `CHANGELOG.md`,
`RELEASE_PLAN.md`, and this validation record contain the prepared 0.3.1
content. `docs/QUALITY_ACCEPTANCE.md` already contains the newly added
stage-pacing gate and needs no release-specific rewrite.

`README.md`, `README.zh-cn.md`, `ROADMAP.md`, the user guides, and the website
were reviewed as patch-release materials. Their current 0.3.0 validation and
artifact references are historical publication evidence; do not rewrite them
before a 0.3.1 artifact exists. After the workflow publishes the tag, review
those references against the generated artifacts and update any release label
or installation example that changes. `MARKETING.md` and
`docs/POST_RELEASE_PROMOTION.md` require no 0.3.1 content until release
communications are authorized.

## Remaining external gates

This record is pre-release evidence, not publication evidence. After tag
`v0.3.1` is pushed, the release workflow must pass Linux, macOS, and Windows
artifact builds, publish the crate and GitHub Release, and provide the final
workflow URL. Update `RELEASE_PLAN.md` and the release documentation handoff
with those outcomes after publication.
