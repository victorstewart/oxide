# oxide-perf-runner tests `report_tests.rs`

## Intention and purpose

This integration suite freezes performance-report schemas, filtered execution, coverage gates, persisted workload semantics, and explanatory counters.

## Relation to the rest of the code

- Tests launch the `oxide-perf-runner` binary and parse its JSON reports.
- The C18 frame-resource test exercises the production Metal renderer through architecture-matrix cases.

## Entry points list

- `metal_frame_resource_rows_freeze_visible_and_offscreen_depth_contracts()` verifies three-slot visible no-growth high water and eight-slot offscreen all-slot cold growth followed by zero warm growth/skips, including C26's 16 KiB completion-protected property buffer per physical slot.
- `metal_prepared_chunk_rows_freeze_clean_and_one_dirty_contracts()` requires exact clean and one-dirty prepared-cache work counters, including zero clean immutable upload and one 12,288-byte dynamic uniform-ring slice.
- `metal_prepared_layer_rows_freeze_body_free_clean_and_single_dirty_contracts()` requires body-free clean architecture/authoring replay and one bounded dirty layer refresh with no new warm texture.
- `metal_architecture_reports_reconciled_renderer_resource_families()` requires the warm static ID-mask row to hit once, skip raster/seed/JFA, encode one compositor pass, retain one cache/in-flight generation, create no target, report nonzero in-flight/total/peak bytes and zero blocked reuse, and stay within its byte budget while preserving the broader resource-family accounting contract. It also freezes C58 no-bloom, one-/three-layer, clipped-viewport, and overlay rows; source/pass/resource/alias/plan counters; indexed raw samples; and reduced conservative-region work.
- `metal_blur_sigma_sweep_freezes_quality_ladder_work()` requires sigma 2 to retain exact samples and exponential taps, sigma 8/16/32/64 to select two paired passes with zero runtime exponentials, and process-resident table bytes to grow only after paired first use.
- `retained_spatial_queries_have_engine_and_authoring_contracts()` freezes 512-instance smoke cardinality, one-entry CPU selection, zero vertex visits, metadata residency, and authoring routing.
- `metal_spatial_rows_freeze_small_and_full_damage_contracts()` freezes one selected small-damage instance/command/draw, four shaded pixels, zero vertex/copy/upload work, and full linear 512-draw static-plan replay.
- `filtered_run_suite_supports_retained_snapshot_authoring_case()` keeps the public retained-snapshot authoring row routable.
- Other test functions cover report comparison, contract coverage, architecture rows, authoring rows, and persisted baseline requirements.

## Logic narrative

Each filtered integration test writes a process-unique temporary report, verifies the child process succeeded, isolates the requested rows, and asserts exact semantic counters before deleting the artifact. The frame-resource row asserts exact ring residency and upload bytes so a timing-only result cannot hide reduced depth, omitted stress, or unexercised growth.

## Preconditions and postconditions

- Real Metal row tests require macOS and are compile-time guarded.
- Passing C18 coverage proves every configured slot was exercised and warm submissions allocate no replacement ring buffers.
- Passing C23 coverage proves the hot retained working set is complete, reports a 100% hit rate, and remains within its hard byte budget, while the one-use path retains zero node-cache bytes and records one explicit fallback. The public authoring row must preserve its configured CPU/prepared-GPU budgets on unchanged-policy access.
- Passing C24 coverage proves clean mixed replay has 256 hits and zero uploads/copies/traversal, while alternating one dirty chunk produces exactly 255 hits, one miss, 64 traversed commands, and 3,072 uploaded bytes per frame.
- Passing C27 coverage proves small damage never scans unrelated glyph/mesh vertices and full damage bypasses querying while reusing the unchanged plan.
- Passing C29 coverage proves clean layer replay performs 100 composites with zero body/copy/upload/offscreen/preparation work, while the dirty row records exactly one miss, offscreen replay from the prepared body, and additional render pass with zero warm copy/upload/preparation.
- Passing C32/C36 accounting coverage proves the warm static ID-mask row performs no chunk preparation, field-building pass, or target creation; retains one in-flight generation; reports its actual storage; and cannot silently exceed its field-cache budget or recycle a busy generation.
- Passing C52 coverage proves the production quarter-resolution sweep preserves its declared sigma/radius, exact subthreshold branch, 46–49% paired sample reduction, zero paired runtime exponential taps, unchanged pass count, and bounded lazy kernel-table residency.

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
- 2026-07-15: froze C58 Scene3D bloom graph rows, raw samples, source/pass/resource/alias/reuse counters, no-bloom guardrail, viewport work reduction, and overlay pass.
- 2026-07-14: froze C52 sigma/radius, exact/paired selection, sample reduction, exponential-tap, and lazy table-memory report counters.
- 2026-07-14: froze C36 warm ID-mask generation, creation, in-flight/total/peak byte, and blocked-reuse report counters.
- 2026-07-14: froze C32 warm ID-mask hit, stage-pass, entry, residency, and budget report counters.
- 2026-07-13: added C29 prepared-layer clean/one-dirty and public retained-snapshot authoring work-contract assertions.
- 2026-07-13: added C27 CPU/authoring spatial-query and Metal small/full damage work-contract assertions.
- 2026-07-13: added C26 zero-geometry CPU animation and exact Metal property-ring report assertions.

- 2026-07-13: added exact C24 clean/one-dirty Metal prepared-chunk and retained-snapshot authoring report contracts.
- 2026-07-13: added C23 retained cache-pressure and public cache-policy authoring report contracts.
- 2026-07-13: added exact visible/offscreen frame-resource report contracts.
