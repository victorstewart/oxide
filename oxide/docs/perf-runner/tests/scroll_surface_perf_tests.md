# oxide-perf-runner `tests/scroll_surface_perf_tests.rs`

## Intention and purpose

This focused integration test protects the two performance rows that admit the
public Rust-owned vertical scroll surface into the authoring and user-journey
contracts. It also keeps the older programmatic feed matrix semantically
distinct from a raw-touch fling.

## Relation to the rest of the code

The test launches the `oxide-perf-runner` binary with an exact three-case
filter, reads its temporary JSON report, and validates public `PerfReport`
fields:

```text
filtered perf-runner process
  -> vertical-scroll authoring row
  -> raw-touch settled feed journey row
  -> programmatic feed viewport-transition row
  -> public PerfReport assertions
```

## Entry points list

- `filtered_scroll_surface_cases_preserve_raw_touch_and_programmatic_contracts()`
  runs the filtered smoke suite and validates case identity, workload metrics,
  settlement, 120 Hz timestamp policy, and the programmatic-matrix note.

## Logic narrative

The subprocess filter selects only the scroll-surface authoring microcase, the
canonical raw-touch feed journey, and the existing feed offset matrix. The test
requires the authoring row to consume four raw samples plus one inertial step.
It requires the journey to encode a 2,000-row virtualized feed until actual
settlement, with every simulated 120 Hz display step accounted for. Finally, it
rejects the old false `hard fling` wording from the direct-offset matrix.

## Preconditions and postconditions

- Cargo supplies `CARGO_BIN_EXE_oxide-perf-runner` to the integration test.
- The filtered suite writes only a process-unique temporary report.
- Success proves all three IDs dispatch and their distinguishing metrics remain
  present in the serialized public report.

## Edge cases and failure modes

- A missing case, renamed metric, non-settling journey, wrong row count, or
  failed subprocess fails the test.
- Settlement must occur in no more than 1,024 simulated display steps.
- The programmatic matrix must not describe direct offset changes as a fling.

## Concurrency and memory behavior

The subprocess owns its benchmark state and report. Its unique process-ID path
avoids collisions with parallel test binaries, and the report is removed after
successful assertions.

## Performance notes

The test runs smoke sampling only. It validates workload structure and metrics,
not timing thresholds or persisted baseline values.

## Feature flags and cfgs

The test has no feature-specific behavior and exercises the native offscreen
CPU cases.

## Testing and benchmarks

Run with
`cargo test --locked -p oxide-perf-runner --test scroll_surface_perf_tests`.

## Examples

The filtered subprocess command in the test is the executable example.

## Changelog

- 2026-08-06: Added focused authoring, settled raw-touch feed, and programmatic
  feed-matrix contract coverage.
