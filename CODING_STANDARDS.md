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
  order does not change the result, unless order-dependent semantics are an
  explicit, documented part of the option design.
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
- Preserve existing option semantics, human-readable parser anchors, structured
  output schemas, exit codes, and stdout/stderr routing unless a compatibility
  change is explicitly planned and documented.

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

Related or adjacent options are those that share an input mode, resource,
termination condition, ordering rule, pacing mechanism, or output mode. For
each CLI change, the review must list the applicable categories and include:

- one acceptance test for the option's standalone/default behavior;
- acceptance tests for supported combinations with adjacent options;
- rejection tests for each mutual-exclusion or prerequisite rule;
- coverage for every input mode declared applicable during the composability
  review;
- assertions that help text and errors describe the actual behavior; and
- API-level validation tests when the behavior is available outside the CLI.

The reviewer must explicitly report whether composability, consistency, and
mutual exclusion were checked, and identify any untested combination accepted
for follow-up.

## Metric change archive

Starting with v0.3.0, every change that targets or materially affects a
measured metric must have a committed archive entry under
`benchmarks/metric_changes/`. This includes throughput, latency percentiles,
memory, CPU, byte accuracy, error rates, recovery counters, load accuracy, and
other release or operational metrics.

The archive entry is required whether the metric improves, regresses, or is
compensated elsewhere. It must record:

- the optimization proposal and expected mechanism;
- the workload, environment, baseline, target, and raw-result locations;
- observed regression data, including distributions rather than averages when
  tail latency is involved;
- the demonstrated or best-supported regression cause, with uncertainty
  stated explicitly;
- alternatives considered and the selected optimization, compensation, or
  acceptance decision;
- post-change measurements against the same gate;
- correctness guardrails, rollback instructions, and revisit triggers; and
- the commits and validation report associated with the decision.

A failed metric run must not be overwritten or omitted after a later passing
run. Both results and the reason for accepting, compensating, or fixing the
regression remain part of the archive. Raw benchmark directories may remain
outside version control when they are too large, but their paths and summarized
measurements must be committed. A metric-affecting change is not review-complete
and must not be released without its archive entry.
