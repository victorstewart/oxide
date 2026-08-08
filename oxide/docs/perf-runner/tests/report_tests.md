# oxide-perf-runner tests `report_tests.rs`

## Intention and purpose

This integration suite freezes performance-report schemas, filtered execution, coverage gates, persisted workload semantics, and explanatory counters.

## Relation to the rest of the code

- Tests launch the `oxide-perf-runner` binary and parse its JSON reports.
- The C18 frame-resource test exercises the production Metal renderer through architecture-matrix cases.
- Ordinary workspace tests keep the canonical inventory, baseline-write rejections, persisted artifact gates, one representative touched-case child route, and the zero-work retired-alias guard active. Deep touched-case child suites remain compiled but explicitly ignored.

## Entry points list

- `repository_provenance_requires_clean_named_stable_source()`: resolves a nested workspace to its Git top level and rejects dirty, detached, or changed source.
- `repository_provenance_rejects_partial_or_malformed_triples()`: rejects incomplete provenance and malformed refs/object IDs.
- `source_bound_report_serializes_and_renders_repository_revision()`, `version_two_report_rejects_missing_repository_revision()`, and `historical_report_omits_and_defaults_repository_revision()`: freeze the version-2 source contract and version-1 compatibility.
- `comparison_rejection_precedes_report_output_resolution_and_writes()`: freezes the source ordering of comparison admission ahead of every report output.
- `rejected_comparison_preserves_existing_report_outputs()`: explicitly runs one touched smoke case against a missing baseline and proves pre-existing JSON and Markdown sentinel bytes are unchanged.

- `child_run_suite_tests_keep_everyday_tiering()` freezes all 55 literal child `--run-suite` sites: 53 must carry `#[ignore = "explicit touched-case perf contract"]`; only `filtered_run_suite_runs_only_the_touched_case()` and the zero-case `retired_exact_aliases_are_not_registered()` guard remain active.
- `metal_frame_resource_rows_freeze_visible_and_offscreen_depth_contracts()` verifies three-slot visible no-growth high water and eight-slot offscreen all-slot cold growth followed by zero warm growth/skips, including C26's 16 KiB completion-protected property buffer per physical slot and nonzero direct-GPU distributions for both frame rows.
- `metal_prepared_chunk_rows_freeze_clean_and_one_dirty_contracts()` requires exact clean and one-dirty prepared-cache work counters, including zero clean immutable upload and one 12,288-byte dynamic uniform-ring slice.
- `metal_prepared_layer_rows_freeze_body_free_clean_and_single_dirty_contracts()` requires body-free clean public-authoring replay and one bounded architecture-control dirty refresh with no new warm texture.
- `metal_architecture_reports_reconciled_renderer_resource_families()` requires the warm static ID-mask row to hit once, skip raster/seed/JFA, encode one compositor pass, retain one cache/in-flight generation, create no target, report nonzero in-flight/total/peak bytes and zero blocked reuse, and stay within its byte budget while preserving the broader resource-family accounting contract. It also freezes C58 no-bloom, one-/three-layer, clipped-viewport, and overlay rows; source/pass/resource/alias/plan counters; indexed raw samples; and reduced conservative-region work.
- `metal_blur_sigma_sweep_freezes_quality_ladder_work()` requires sigma 2 to retain exact samples and exponential taps, sigma 8/16/32/64 to select two paired passes with zero runtime exponentials, and process-resident table bytes to grow only after paired first use.
- `metal_immutable_image_rows_freeze_residency_mip_and_quality_contracts()` requires the C59 production guardrails, isolated Shared/Private mip controls, exact quality equivalence, sampled-plus-staging peak accounting, release accounting, indexed timing samples, and 1,089 public `ImageView` encodes in the authoring row.
- `metal_image_store_rows_freeze_scaling_completion_and_reuse_contracts()` requires C60's 100/1,000/10,000 display-size decode, exact atlas page residency, zero clear upload, one draw, explicit first-frame completion/readback, hard budgets, and exact 64-slot authoring invalidation/reuse contract.
- `retained_spatial_query_has_a_public_authoring_contract()` freezes 512-instance smoke cardinality, one-entry CPU selection, zero vertex visits, metadata residency, and authoring routing.
- `metal_spatial_rows_freeze_small_and_full_damage_contracts()` freezes one selected small-damage instance/command/draw, four shaded pixels, zero vertex/copy/upload work, and full linear 512-draw static-plan replay.
- `filtered_run_suite_supports_retained_snapshot_authoring_case()` keeps the public retained-snapshot authoring row routable.
- `retired_exact_aliases_are_not_registered()` proves filters cannot select any of the eight retired duplicate IDs.
- `gpu_scene_inventory_defers_timeline_work_to_animation_battery()` freezes the 16-row GPU scene inventory after the animation battery becomes the sole owner of timeline GPU work.
- `filtered_run_suite_supports_text_sdf_bake_metrics()` requires the focused
  SDF row to declare a cold cache state, execute both pre-shaped Latin and CJK
  glyph runs, and publish nonzero geometry and dirty-pixel counters.
- `filtered_run_suite_classifies_first_visible_images_as_cold()` runs the linear and nearest first-visible image rows, requires cold resource metadata, and freezes the render-only note boundary after texture publication and draw-list construction.
- `canonical_smoke_suite_keeps_exact_inventory()` freezes the exact 23-row canonical count and ID digest.
- `persisted_report_case_id_sets_are_frozen()` requires the committed workspace report to contain that same 23-row canonical ID set; device and browser report inventories keep their independent freezes.
- The device freezes are the canonical comparison surface: five unique Oxide rows and ten UIKit rows forming five idiomatic/optimized pairs.
- `persisted_workspace_canonical_renderer_metric_keys_are_frozen()` freezes metric keys only for canonical native renderer rows.
- `workspace_latest_gates_canonical_retained_and_layout_rows()` requires retained dirty-leaf reuse/rebuild evidence and incremental dirty-subtree work bounded below cold layout.
- `filtered_run_suite_runs_only_the_touched_case()` proves an explicit filter executes and validates only its requested noncanonical row.
- Explicitly ignored focused collection, text-cache, atlas, wrapped/picker, cursor-map, renderer, and WebGPU-profile tests preserve noncanonical work contracts without adding them to the everyday battery or persisted workspace rows.
- `baseline_write_rejects_smoke_sampling()` and `baseline_write_rejects_touched_filter()` prevent sampled or touched-only runs from replacing the canonical baseline.
- Other test functions cover report comparison, contract coverage, architecture rows, authoring rows, and persisted baseline requirements.

## Logic narrative

Each deep filtered integration test remains independently runnable: it writes a process-unique temporary report, verifies the child process succeeded, isolates the requested rows, and asserts exact semantic counters before deleting the artifact. These tests are explicitly ignored by default so their workload coverage remains available without multiplying ordinary workspace-test execution. The frame-resource row still asserts exact ring residency and upload bytes so a timing-only result cannot hide reduced depth, omitted stress, or unexercised growth.

Unfiltered smoke freezes the minimal canonical inventory. Diagnostic matrices
remain reachable through explicit touched filters, and no integration path
requests every registered case at once.

The everyday tier keeps the in-process canonical smoke inventory, both baseline-write rejection tests, persisted report gates, one noncanonical child routing/runtime test, and the zero-work retired-alias guard active. A source-structure test prevents a new literal child suite launch from silently joining that tier.

## Preconditions and postconditions

- Real Metal row tests require macOS and are compile-time guarded.
- Passing C18 coverage proves every configured slot was exercised and warm submissions allocate no replacement ring buffers.
- Passing C23 coverage proves the hot retained working set is complete, reports a 100% hit rate, and remains within its hard byte budget, while the one-use path retains zero node-cache bytes and records one explicit fallback. The public authoring row must preserve its configured CPU/prepared-GPU budgets on unchanged-policy access.
- Passing C24 coverage proves clean mixed replay has 256 hits and zero uploads/copies/traversal, while alternating one dirty chunk produces exactly 255 hits, one miss, 64 traversed commands, and 3,072 uploaded bytes per frame.
- Passing C27 coverage proves small damage never scans unrelated glyph/mesh vertices and full damage bypasses querying while reusing the unchanged plan.
- Passing C29 coverage proves clean layer replay performs 100 composites with zero body/copy/upload/offscreen/preparation work, while the dirty row records exactly one miss, offscreen replay from the prepared body, and additional render pass with zero warm copy/upload/preparation.
- Passing C32/C36 accounting coverage proves the warm static ID-mask row performs no chunk preparation, field-building pass, or target creation; retains one in-flight generation; reports its actual storage; and cannot silently exceed its field-cache budget or recycle a busy generation.
- Passing C52 coverage proves the production quarter-resolution sweep preserves its declared sigma/radius, exact subthreshold branch, 46–49% paired sample reduction, zero paired runtime exponential taps, unchanged pass count, and bounded lazy kernel-table residency.
- Passing C59 coverage proves large-static and small-one-use auto policy avoid Private staging, both complete-mip storage modes materially suppress minification aliasing with identical output variance, released resources leave zero current residency, and the public authoring path selects the same mip contract.
- Passing C60 coverage proves all scaling rows publish every requested image at display size, allocate exactly 1/4/40 bounded pages, render nonblank completed pixels, and keep the offscreen completion metric distinct from host-owned display latency. The authoring row must invalidate and republish exactly the 64 released slots.
- Passing the focused SDF coverage proves both required representative runs were
  baked; font registration or shaping failure aborts the benchmark instead of
  silently shrinking its workload.

## Edge cases and failure modes

- Missing rows, malformed JSON, nonzero backpressure, unexpected growth, or changed workload cardinality fail explicitly.
- Temporary report names include the process id to avoid parallel-test collisions.

## Concurrency and memory behavior

Child processes own independent renderer instances. The everyday tier launches only the representative touched-case suite plus the zero-work retired-alias check; explicitly selected deep contracts build their large draw lists once per row and reuse them across frames.

## Performance notes

The 4,096-quad visible row stays within initial 512/64/72 KiB capacity, whose uniform size also covers the existing 1,024-marker high-water workload. The 8,192-quad row deliberately exceeds VB/IB capacity in all eight offscreen slots, then verifies retained geometric growth eliminates warm allocation.

## Feature flags and cfgs

Metal-specific report tests use `#[cfg(target_os = "macos")]`.

## Testing and benchmarks

Run `cargo test --locked -p oxide-perf-runner --test report_tests` for the everyday tier. Run one deep touched-case contract explicitly, never the ignored set as a batch:

`cargo test --locked -p oxide-perf-runner --test report_tests filtered_run_suite_supports_text_sdf_bake_metrics -- --ignored --exact`

## Examples

Set `OXIDE_PERF_RUNNER_FILTER=gpu.architecture.frame_resources.` with `--run-suite --smoke --json-out <path>` to inspect both C18 rows.

## Changelog
- 2026-08-07: added pre-write comparison-order coverage and an explicit sentinel test proving rejected comparisons preserve prior JSON and Markdown.
- 2026-08-07: added nested-top-level, dirty/detached/drift, version-2 rejection, version-1 compatibility, JSON, and Markdown provenance coverage.
- 2026-08-07: kept one representative touched-case child route and the zero-work retired-alias guard active; marked the other 54 literal child suite contracts plus the implicit filtered-registry suite explicitly ignored for one-by-one execution.
- 2026-08-07: froze the exact 23-row canonical smoke inventory and explicit touched-only execution after retiring the exhaustive workspace mode.
- 2026-08-07: aligned persisted device report freezes with the five-row Oxide and ten-row UIKit canonical comparison batteries.
- 2026-08-07: retired eight exact duplicate IDs, kept their canonical public rows, and froze the 16-row GPU scene inventory after timeline GPU work moved solely to the animation battery.
- 2026-08-06: required linear and nearest first-visible image rows to report cold resource state and an explicit begin/encode/submit timing boundary.
- 2026-08-06: froze the 401-row workspace case set after the SDF addition and semantic cutover to hit-test-only dynamic-animation row IDs.
- 2026-07-15: froze C60 image-store scaling, completed-frame readback, exact page/draw/budget counters, and authoring release/reuse invalidation.
- 2026-07-15: required C18 frame-resource rows to retain their completed-command-buffer GPU distributions so the complete C61 report satisfies the frame metric contract.
- 2026-07-15: froze C59 large-static, minified, small-one-use, and public-authoring image policy rows; residency/upload/mip counters; release; indexed samples; and output-quality equivalence.
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
