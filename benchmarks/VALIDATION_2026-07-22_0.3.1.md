# rload 0.3.1 release validation — 2026-07-22

Validated package version: `0.3.1`.

## Scope

This patch release fixes CLI-over-profile precedence for timestamp replay and
the composition of a CLI replay seed with a profile-provided replay order. It
does not alter the request engine's throughput, latency, memory, or metric
calculation paths; no metric-change archive is required.

## Local release gate

The release workflow reran `./scripts/release-check.sh` with formatting,
warnings-denied Clippy, all test targets, a release build, package creation,
and packaged-crate verification. The validated suite includes 43 library
tests, one binary test, 57 CLI tests, and 32 HTTP integration tests.

## Stage-pacing gate

The CLI suite measures request-arrival timestamps at a local target for three
10-second stages at 20, 60, and 30 RPS. Each stage permits at most 5% request
count deviation. The gate covers ordinary requests, Nginx access-log replay,
and JSONL replay. Transition behavior is assessed through the independent
stage-window counts. The automated
gate intentionally does not impose a microsecond-level inter-arrival threshold,
because OS event-loop scheduling makes that threshold non-portable.

## Documentation handoff

The release documentation index was reviewed before tagging. `CHANGELOG.md`,
`RELEASE_PLAN.md`, and this validation record contain the prepared 0.3.1
content. `docs/QUALITY_ACCEPTANCE.md` already contains the newly added
stage-pacing gate and needs no release-specific rewrite.

`README.md`, `README.zh-cn.md`, `ROADMAP.md`, the user guides, and the website
were reviewed as patch-release materials. Their 0.3.0 validation paragraphs
remain historical evidence; current release labels and installation references
were reviewed against the published artifacts. `MARKETING.md` and
`docs/POST_RELEASE_PROMOTION.md` require no 0.3.1 content until release
communications are authorized.

## Publication evidence

The `v0.3.1` tag was pushed and the release workflow completed successfully:
<https://github.com/wenhaozhao/rload/actions/runs/29908475933>. It published
the crate, GitHub Release, and eight Linux, macOS, and Windows archive/checksum
artifacts: <https://github.com/wenhaozhao/rload/releases/tag/v0.3.1>.

The follow-up CI run after making the stage gate portable passed on Ubuntu,
macOS, and Windows: <https://github.com/wenhaozhao/rload/actions/runs/29911120644>.
