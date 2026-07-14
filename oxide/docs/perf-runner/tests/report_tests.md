# oxide-perf-runner tests `report_tests.rs`

## Intention and purpose

This integration suite freezes performance-report schemas, filtered execution, coverage gates, persisted workload semantics, and explanatory counters.

## Relation to the rest of the code

- Tests launch the `oxide-perf-runner` binary and parse its JSON reports.
- The C18 frame-resource test exercises the production Metal renderer through architecture-matrix cases.

## Entry points list

- `metal_frame_resource_rows_freeze_visible_and_offscreen_depth_contracts()` verifies three-slot visible no-growth high water and eight-slot offscreen all-slot cold growth followed by zero warm growth/skips, including C26's 16 KiB completion-protected property buffer per physical slot.
- `metal_prepared_chunk_rows_freeze_clean_and_one_dirty_contracts()` requires exact clean and one-dirty prepared-cache work counters, including zero clean immutable upload and one 12,288-byte dynamic uniform-ring slice.
- `filtered_run_suite_supports_retained_snapshot_authoring_case()` keeps the public retained-snapshot authoring row routable.
- Other test functions cover report comparison, contract coverage, architecture rows, authoring rows, and persisted baseline requirements.

## Logic narrative

Each filtered integration test writes a process-unique temporary report, verifies the child process succeeded, isolates the requested rows, and asserts exact semantic counters before deleting the artifact. The frame-resource row asserts exact ring residency and upload bytes so a timing-only result cannot hide reduced depth, omitted stress, or unexercised growth.

## Preconditions and postconditions

- Real Metal row tests require macOS and are compile-time guarded.
- Passing C18 coverage proves every configured slot was exercised and warm submissions allocate no replacement ring buffers.
- Passing C23 coverage proves the hot retained working set is complete, reports a 100% hit rate, and remains within its hard byte budget, while the one-use path retains zero node-cache bytes and records one explicit fallback. The public authoring row must preserve its configured CPU/prepared-GPU budgets on unchanged-policy access.
- Passing C24 coverage proves clean mixed replay has 256 hits and zero uploads/copies/traversal, while alternating one dirty chunk produces exactly 255 hits, one miss, 64 traversed commands, and 3,072 uploaded bytes per frame.

## Edge cases and failure modes

- Missing rows, malformed JSON, nonzero backpressure, unexpected growth, or changed workload cardinality fail explicitly.
- Temporary report names include the process id to avoid parallel-test collisions.

## Concurrency and memory behavior

Child processes own independent renderer instances. Large draw lists are built once per row and reused across frames.

## Performance notes

The 4,096-quad visible row stays within initial 512/64/72 KiB capacity, whose uniform size also covers the existing 1,024-marker high-water workload. The 8,192-quad row deliberately exceeds VB/IB capacity in all eight offscreen slots, then verifies retained geometric growth eliminates warm allocation.

## Feature flags and cfgs

Metal-specific report tests use `#[cfg(target_os = "macos")]`.

## Testing and benchmarks

Run `cargo test --locked -p oxide-perf-runner --test report_tests`.

## Examples

Set `OXIDE_PERF_RUNNER_FILTER=gpu.architecture.frame_resources.` with `--run-suite --smoke --json-out <path>` to inspect both C18 rows.

## Changelog
- 2026-07-13: added C26 zero-geometry CPU animation and exact Metal property-ring report assertions.

- 2026-07-13: added exact C24 clean/one-dirty Metal prepared-chunk and retained-snapshot authoring report contracts.
- 2026-07-13: added C23 retained cache-pressure and public cache-policy authoring report contracts.
- 2026-07-13: added exact visible/offscreen frame-resource report contracts.
