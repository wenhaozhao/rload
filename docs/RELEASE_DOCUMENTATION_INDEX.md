# Release documentation and communications index

This index is the required handoff for every published `rload` version. It
separates version-string automation from the content review that still needs an
owner after a release succeeds.

## How to use this index

1. Before tagging, review every **required** entry and prepare the intended
   content on the release branch.
2. The release workflow validates this file exists, publishes the crate and
   binaries, and updates the limited version strings listed below.
3. After the workflow succeeds, update the remaining required entries on
   `main`, run the normal CI matrix, and record the final release evidence.
4. Do not rewrite historical benchmark evidence, metric-change records, or
   frozen contracts merely to replace an old version label.

For prerelease tags, do not describe the version as "stable" unless the
release owner explicitly approves that positioning. GitHub Release visibility
and prerelease status must match the intended product status.

## Required post-release updates

| Document or material | Required update | Automation | Owner review |
|---|---|---|---|
| `CHANGELOG.md` | Release date, release status, and the final user-visible change summary. | None | Required |
| `RELEASE_PLAN.md` | Current-release status, completed gates, tag/release outcome, and next release work. | None | Required |
| `ROADMAP.md` | Completed release status and the next planned phase. | None | Required |
| `README.md` | Supported capability table, compatibility claims, validation line, and release references. | None | Required |
| `README.zh-cn.md` | Chinese equivalent of the README updates. | None | Required |
| `docs/user_guide.md` | Installation artifact names, new operational behavior, and user-facing examples. | Archive-version replacement only | Required |
| `docs/user_guide_zh-cn.md` | Chinese equivalent of the user-guide updates. | Archive-version replacement only | Required |
| `website/index.html` | Release label, artifact-name examples, feature highlights, and stable/prerelease wording. | Limited release-label replacement | Required |
| `website/index.zh-cn.html` | Chinese equivalent of the website updates. | Limited release-label replacement | Required |
| `MARKETING.md` | GitHub Release copy, release sequence, and campaign messages. | None | Required when release communications are used |
| `docs/POST_RELEASE_PROMOTION.md` | Authorized channel selection, message rules, and a per-release outreach record. | None | Required when release communications are used |
| `benchmarks/VALIDATION_<date>_<version>.md` | Final tag, CI/release-workflow outcome, and links or paths to release evidence. | None | Required |

## Workflow-managed material

`.github/workflows/release.yml` performs these external publication actions:

- publishes the crate;
- creates the GitHub Release and uploads Linux, macOS, and Windows artifacts;
- updates the Homebrew tap;
- updates selected version strings in the website and user guides;
- deploys the website.

Those operations do not replace the owner review in the table above. In
particular, the workflow does not verify feature descriptions, benchmark
claims, release positioning, or static archive examples.

When release communications are planned, complete
`docs/POST_RELEASE_PROMOTION.md` after this documentation handoff.

## Documents that remain historical

Do not revise these solely because a later version was released:

- `benchmarks/ACCURACY.md`, `benchmarks/BASELINE.md`, and previous validation
  reports;
- `benchmarks/metric_changes/*` records, except for a new metric-affecting
  decision that requires its own record;
- `designs/v0.3.0_contract.md`, `designs/v0.3.0_failure_recovery.md`, and
  historical design proposals;
- `CODING_STANDARDS.md`, licenses, and third-party notices unless their own
  content changed.
