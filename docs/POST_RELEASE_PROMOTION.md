# Post-release channel promotion process

Use this process after a release workflow has completed successfully. It turns
a published version into an auditable, owner-approved outreach plan without
automating posts or assuming access to third-party accounts.

## Preconditions

Before any outreach, confirm all of the following:

- the GitHub Release is public and its prerelease/draft state matches the
  intended positioning;
- the crate, release artifacts, Homebrew formula, and website installation
  instructions resolve;
- the release documentation handoff in
  `docs/RELEASE_DOCUMENTATION_INDEX.md` is complete or has named follow-up
  owners;
- every performance claim links to its validation report; and
- prerelease versions are described as prereleases, never as stable releases,
  unless the release owner explicitly approves a different label.

## Release kit

Prepare one source of truth before posting:

- release URL and tag;
- one-sentence product statement and the three most user-visible changes;
- installation command and one runnable example;
- validation-report link, with qualified performance claims only;
- known limitations, compatibility notes, and prerelease status; and
- one contact path for feedback: GitHub Issues or Discussions.

## Channel plan

Use only channels that the release owner has authorized and can maintain.

| Window | Channel | Action | Record |
|---|---|---|---|
| Release day | GitHub Release and repository | Verify notes, artifacts, installation instructions, and feedback links. | Release URL and verification owner |
| Release day | Project website and docs | Verify the published version, archive names, and release positioning. | Published page URL |
| Release day | Existing project audience | Post a concise release note with the release URL, one runnable command, and prerelease status where applicable. | Post URL and channel |
| Days 1–3 | Rust, SRE, and performance-engineering communities | Share a technical use case and reproducible example; answer questions and link to source evidence. | Post URL, audience, and discussion outcome |
| Days 1–7 | Chinese-language developer communities, when authorized | Publish the Chinese release summary and installation example; keep claims aligned with the English source of truth. | Post URL and channel |
| Days 7–14 | Maintainers and early users | Review feedback, installation failures, and issue themes; decide whether documentation, a patch release, or no action is warranted. | Short retrospective and follow-up issue links |

Examples of optional community destinations include a maintained project
discussion space, Rust user forums, SRE/performance communities, and approved
Chinese developer communities. Select channels for audience fit and moderation
capacity rather than posting everywhere.

## Message rules

- Lead with the user problem, not a feature inventory.
- Include an installation or reproduction command that has been verified for
  the released artifact.
- State measured performance as environment-specific validation evidence, not
  a universal capacity guarantee.
- Do not claim HTTP/2, gRPC, Lua/LuaJIT, distributed execution, or other
  deferred features.
- For RCs, state the intended feedback or validation goal and invite reports
  through the designated feedback path.

## Promotion record

For every release that receives outreach, add a short record under
`docs/release_promotions/<tag>.md` using this shape:

```md
# Promotion record: <tag>

- Release URL:
- Positioning: stable / prerelease
- Owner:
- Release-day channels:
- Community channels:
- Feedback and issue links:
- Follow-up decision:
```

Do not store account credentials, private audience lists, or personal contact
details in the repository.
