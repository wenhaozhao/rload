# rload Coding and Review Standards

These standards apply to implementation and code review. Automated formatting,
Clippy, tests, and packaging checks remain necessary, but they do not replace
the semantic review requirements below.

## CLI option review

Every new or changed CLI option must be reviewed as part of the complete CLI
option system, not only as an isolated parser branch.

### Composability

- Identify every mode and input to which the option applies, including static
  requests, access-log replay, and JSONL replay.
- Verify valid combinations with related options and confirm that argument
  order does not change the result.
- Avoid hidden prerequisites. If a prerequisite is inherent, validate it before
  file or network I/O and state it in `--help`, error output, and the README.
- Confirm that CLI configuration and the corresponding public API express the
  same behavior and defaults.

### Consistency

- Reuse established naming, value syntax, units, defaults, and attached/long
  option behavior where applicable.
- Keep behavior consistent across replay inputs unless a format limitation
  requires a documented difference.
- Keep `--help`, validation errors, README examples, structured output, and
  public configuration types synchronized with the implementation.
- Preserve existing option semantics and parser-stable default output unless a
  compatibility change is explicitly planned and documented.

### Mutual exclusion

- Review the option against every existing option that controls the same
  resource, termination condition, ordering, pacing, or output mode.
- Reject invalid combinations deterministically before execution. The error
  must name the conflicting options or clearly state the violated constraint.
- Apply conflicts symmetrically: the same pair must fail regardless of argument
  order or which code path constructs the configuration.
- Do not introduce unnecessary exclusions when a well-defined composition is
  already supported by the engine.

### Required tests

Each CLI option change must include, as applicable:

- one acceptance test for the option's standalone/default behavior;
- acceptance tests for supported combinations with adjacent options;
- rejection tests for each mutual-exclusion or prerequisite rule;
- coverage for every supported input mode;
- assertions that help text and errors describe the actual behavior; and
- API-level validation tests when the behavior is available outside the CLI.

The reviewer must explicitly report whether composability, consistency, and
mutual exclusion were checked, and identify any untested combination accepted
for follow-up.
