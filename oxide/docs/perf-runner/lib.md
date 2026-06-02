# oxide-perf-runner::lib

## Intention and purpose

`oxide-perf-runner` owns the persisted Rust-side performance contract for Oxide. It exists to benchmark the engine, representative flows, author-facing APIs, and bridge paths under one regression-gated report format instead of leaving performance validation to ad hoc local timing.

## Relation to the rest of the code

- Upstream callers:
  - `oxide/crates/perf-runner/src/main.rs` invokes `run_from_env()`.
  - CI and local commands route through `cargo run -p oxide-perf-runner -- --run-suite ...`.
- Downstream dependencies:
  - `oxide_ui_core`, `oxide_renderer_metal`, `oxide_test_scenes`, `oxide_platform_api`, `oxide_platform_web`, and related crates provide the actual workload surfaces being measured.
  - `oxide/benchmarks/workspace/latest.json` and `oxide/benchmarks/workspace/latest.md` are the persisted outputs consumed by review and CI.

## Entry points list

- `oxide_perf_runner::run_from_env() -> anyhow::Result<()>`
  - Parses CLI arguments from the current process and dispatches into either the persisted suite flow or the legacy summary flow.
  - Main callers: `oxide/crates/perf-runner/src/main.rs`.
- `oxide_perf_runner::run_cli(args: &[String]) -> anyhow::Result<()>`
  - Handles the suite CLI, baseline writes, comparisons, and legacy fallback.
  - Main callers: tests and the binary entry point.
- `oxide_perf_runner::compare_reports(current: &PerfReport, baseline: &PerfReport) -> PerfComparison`
  - Applies regression gating against the persisted baseline.
  - Main callers: suite execution and report tests.
- `oxide_perf_runner::assert_full_coverage(coverage: &CoverageReport) -> anyhow::Result<()>`
  - Enforces that every required battery family has an implemented case.
  - Main callers: suite execution and report tests.
- `oxide_perf_runner::assert_case_metric_contract(cases: &[PerfCaseResult]) -> anyhow::Result<()>`
  - Enforces first-class frame and GPU metric distributions for report rows that claim frame timing or GPU frame timing.
  - Main callers: suite execution and report tests.
- `web_latest_report_satisfies_webgpu_distribution_and_pacing_contract`
- Test-only report gate that validates `benchmarks/web/latest.json` keeps the full 33-row browser WebGPU matrix populated with p50/p95/p99/peak, missed-frame, hitch, resource-lifetime counters, draw-family counters, draw-state and clip-state counters, image-upload temp/scratch counters, upload summaries with direct glyph/RGBA timestamp totals, effect-uniform write/byte/slot counters plus direct GPU timestamp A/B totals, texture-copy/render-pass counters, pass-family counters, timestamp-query attribution status, GPU timestamp stage breakdown with row reconciliation, Chrome browser trace summary fields, per-benchmark User Timing labels, and per-benchmark trace intervals with scoped event/GPU/WebGPU counts from a duplicate benchmark-report run, upload-scratch summaries, effect-uniform summaries, backdrop-batch summaries, Scene3D summaries, mixed-scene current-versus-legacy A/B counters, layer/damage/effects current-versus-legacy A/B counters, command-family current-versus-legacy A/B counters, glyph-run current-versus-legacy A/B counters, neon-marker current-versus-legacy A/B counters, direct-surface current-versus-forced-scene-present A/B counters, draw-state cache A/B coverage, clip-state cache A/B coverage, explicit backend-path coverage rows tying every important WebGPU path family to distributions and explanatory counters, report-level and per-row warm-resource-churn summaries proving current warm rows have zero post-warmup aggregate and family-level GPU resource churn plus zero aggregate and family-level CPU scratch growth, current-row Rust/WASM allocation counters with bounded per-frame budgets and zero reallocations, frame-loop allocation stage attribution with totals matching the frame-loop row, renderer submit sub-stage allocation attribution with totals matching the parent submit stage, zero WASM memory growth plus Chrome JS heap sampling across benchmark marks after prewarm, and pixel-check fields.
- `workspace_latest_gates_mac_metal_animation_and_navigation_pacing_rows`
  - Test-only report gate that validates `benchmarks/workspace/latest.json` keeps the macOS Metal animation refresh-matrix and collection-navigation frame-pacing rows populated with direct GPU distributions, frame distributions, missed-frame/hitch metrics, refresh-mode metadata, and workload diagnostics.
- `workspace_latest_gates_retained_layout_dirty_class_rows`
  - Test-only report gate that validates retained-surface authoring and dirty-class layout rows keep zero-layout, retained-reuse, retained-rebuild, dirty-node, and text-atlas context counters populated in `benchmarks/workspace/latest.json`.
- `workspace_latest_gates_collection_identity_and_prefix_ab_rows`
  - Test-only report gate that validates keyed collection reconciliation, bounded measurement-cache churn, and variable-prefix repair rows keep indexed-vs-scan, cold-repair, dirty-range, revision-query, and measurement-cache diagnostics populated in `benchmarks/workspace/latest.json`.
- `workspace_latest_gates_text_cache_atlas_and_cursor_rows`
  - Test-only report gate that validates wrapped-label and picker text cache A/B rows, atlas pressure/dirty-upload rows, and shaped cursor-map rows keep their cache, eviction, dirty-slot upload, fallback, bidi, cursor-boundary, affinity, and width-span diagnostics populated in `benchmarks/workspace/latest.json`.

## Logic narrative

The crate builds a case inventory spanning component/animation microbenchmarks, primitive lifecycle workloads, scene and journey flows, author-facing API workloads, state-reconciliation workloads, and bridge workloads. Primitive lifecycle coverage now includes empty-root mount, flat-rect mount/mutate/remove-all/remount, label mount/mutate, card mount/mutate, image mount/mutate, and a shared control-set slice so the report can speak to creation, invalidation, teardown, and remount costs instead of only encode hot paths. The system inventory includes text shaping, one-shaped-run prefix width mapping, batched fallback-font label encoding, wrapped-label current-vs-legacy fitting, atlas bake, constrained glyph-atlas pressure, and dirty-rect atlas upload publication so cursor cache misses, fallback glyph segmentation, wrapped-label cache misses, atlas eviction, and incremental glyph texture uploads remain part of the persisted hot-path contract. The layout inventory covers full relayout cases plus `cpu.layout.dirty_subtree.incremental_relayout`, which records per-node visit, skip, update, and child-measure counters for a one-cell layout mutation, `cpu.layout.descendant_only.incremental_relayout`, which isolates fixed-outer internal subtree edits that should avoid parent child-measure scans, `cpu.layout.transform_only.reposition`, which verifies transform-only changes keep layout counters at zero while retained draw/hit-test state updates, `cpu.layout.paint_only.opacity_clip`, which verifies opacity/clip dirty-class edits skip layout while retained sibling subtrees replay, `cpu.layout.node_content_dirty.retained_replay`, which verifies text/image/camera content dirty classes stay node-scoped, `cpu.layout.non_draw_dirty.retained_reuse`, which verifies accessibility/hit-test metadata dirtiness keeps layout and draw caches clean, and `cpu.layout.scoped_tree_mutation.add_remove`, which verifies common structural edits stay scoped to the changed branch. The state-reconciliation inventory includes single-node, 1 percent, 10 percent, and full-theme retained-tree mutation cases so diff/apply costs are visible as their own contract family instead of being inferred from primitive or router rows. The text-input inventory includes large-editor typing/paste/selection paths, a focused IME composition commit cycle, and LTR, RTL, fallback-font, plus mixed-bidi affinity cursor-pick cluster-map microcases for long Unicode single-line input backed by `oxide_text::ShapedCursorMap`; those rows persist cursor count, byte-boundary count, boundary checksum, affinity splits, width span, and fallback run count where applicable. The journey inventory includes a routed IME composition flow through the input scene. Collection journey rows persist measurement-call totals, item-revision query totals, and whether a collection revision hint was active so variable prefix reuse remains visible in reports. The authoring inventory also keeps `cpu.authoring.surface_router.compose` aligned with the public popup lifecycle API by exercising key-popup lookup, touch-region refresh, approval-gated dismissal, and retained current/overlay/popup composition metrics through `SurfaceRouter`, keeps retained-surface clean and dirty-leaf encodes represented with explicit subtree reuse metrics, measures retained surface replay through a live `TextCtx` atlas snapshot path, measures checked multi-atlas retained text draw-list replay through the public `DrawListBuilder` API, A/B measures keyed collection focus reconciliation through indexed lookup versus scan fallback, measures bounded variable collection measurement-cache repair after large key churn, and A/B measures variable collection prefix repair through dirty-range updates versus full signature scans. The underlying `CollectionView` measurement cache is bounded, so these rows validate warm prefix reuse and cold-span repair without allowing unbounded historical key/revision retention. Each case is measured into a `PerfCaseResult` that carries both latency distributions and contract metadata such as layer, scenario, variant, cache state, and refresh mode.

CPU-oriented cases run through a shared warmup-and-sample loop that amortizes timer noise by executing enough operations to hit a target sample duration. Journey-style workloads run one full interaction cycle per sample. GPU scene cases execute live Metal frames and persist renderer counters such as draw, encode, cull, and damage summaries when available. The GPU journey row executes collection focus navigation through the real router before Metal encode/submit and persists event-to-visible, frame, direct GPU, missed-frame, and hitch distributions for the interaction flow. Any row reported as `ms/frame` is contract-gated for frame p50/p95/p99/peak plus 60 Hz and 120 Hz missed-frame and hitch metrics, and GPU frame rows are additionally gated for GPU p50/p95/p99/peak distributions. The animation/effects battery includes a dedicated Metal refresh-matrix row that persists direct GPU distributions plus 60 Hz and 120 Hz missed-frame and hitch metrics for the animated scene. The authoring battery now also includes a retained `scene3d` mixed-frame case that renders a persistent 3D mesh pass ahead of a 2D overlay so the Metal backend's shared-frame 2D/3D path stays under regression coverage.

Bridge coverage includes both app-owned OS bridge workloads and the web backend surface available on native fallback builds. The web case exercises browser-backend capabilities, device caps, clipboard cache, network status, permission callbacks, location fallback, haptics, and the explicit unsupported iframe WebView boundary so the WebAssembly backend is represented in the workspace contract even when the browser-only DOM paths are tested separately.

The resulting `PerfReport` is serialized to JSON and Markdown. Baseline comparison is median-based and only applies to gated cases. Coverage validation is structural: the suite is considered incomplete if any required family lacks a case. The separate browser WebGPU baseline under `benchmarks/web/latest.json` is guarded by a report-shape test so the 33-row browser matrix cannot drop latency distributions, 60 Hz/120 Hz missed-frame and hitch metrics, upload summaries with direct glyph/RGBA timestamp totals, upload-scratch summaries, effect-uniform summaries with direct GPU timestamp A/B totals, backdrop-batch summaries, Scene3D summaries, mixed-scene current-versus-legacy A/B coverage, layer/damage/effects current-versus-legacy A/B coverage, command-family current-versus-legacy A/B coverage, glyph-run current-versus-legacy A/B coverage, neon-marker current-versus-legacy A/B coverage, direct-surface current-versus-forced-scene-present A/B coverage, draw-state cache A/B coverage, clip-state cache A/B coverage, explicit backend-path coverage rows tying every important WebGPU path family to distributions and explanatory counters, the report-level and per-row warm-resource-churn zero-growth summaries for current warm rows, family-level GPU resource attribution for draw, image, target, Scene3D, effect, and ID-mask resources, family-level scratch attribution for draw, Scene3D, effect, ID-mask, image-upload, and resource-table growth, current-row Rust/WASM allocation counters with bounded per-frame budgets and zero reallocations, frame-loop allocation stage attribution with totals matching the frame-loop row, renderer submit sub-stage allocation attribution with totals matching the parent submit stage, zero WASM memory growth plus Chrome JS heap sampling across benchmark marks after prewarm, Chrome browser trace event counts, benchmark User Timing labels, per-benchmark trace intervals with scoped event/GPU/WebGPU counts from a duplicate benchmark-report run, resource lifetime counters, image-upload temp/scratch counters, effect-uniform counters, texture-copy/render-pass counters, pass-family counters, timestamp-query attribution status, GPU timestamp stage-breakdown totals reconciled to every source row, or pixel checks while still looking like a valid persisted browser report.
The committed workspace baseline is also guarded for the macOS Metal animation refresh-matrix and collection-navigation frame-pacing rows, so local animation hitch proof cannot silently degrade into CPU-only or metadata-only coverage.
Retained layout coverage is guarded at the persisted-report level as well: clean retained encodes must keep full reuse counters, dirty-leaf encodes must keep node reuse/rebuild counters, dirty-class layout rows must keep zero-layout counters, and non-draw dirty rows must prove retained reuse without draw rebuilds.
Collection and text coverage are guarded at the persisted-report level too: collection rows must preserve keyed-index and dirty-prefix A/B diagnostics, and text rows must preserve shaped-run cache, dirty atlas upload, atlas pressure, fallback-font, bidi, and shaped cursor-map diagnostics.
Contract coverage battery entries share one status-and-note helper so each required case set is evaluated once, then rendered consistently into the persisted report.

## Preconditions and postconditions

- Preconditions:
  - The workspace must build with the relevant crates and test scenes available.
  - Benchmark cases must record at least one sample.
  - Persisted baselines must be comparable to the current report schema when regression checks are requested.
- Postconditions:
  - A successful suite run produces a complete `PerfReport` with coverage accounting.
  - Baseline comparisons either report no gated regressions or fail with explicit mismatches.
- Invariants maintained:
  - Every persisted case carries contract metadata alongside latency distributions.
  - Frame rows cannot pass the full-suite contract without frame distribution and pacing metrics.
  - GPU frame rows cannot pass the full-suite contract without direct GPU timing distributions.
  - The persisted macOS Metal animation and collection-navigation frame-pacing rows cannot pass report tests without direct GPU timing, frame timing, 60 Hz and 120 Hz missed-frame/hitch metrics, and `oxide-metal` refresh-mode metadata.
  - Retained authoring and dirty-class layout rows cannot pass report tests without retained reuse/rebuild counters and explicit zero-layout evidence for non-layout dirtiness.
  - Collection/text rows cannot pass report tests without the diagnostics that distinguish indexed versus scan reconciliation, incremental versus full prefix repair, cached versus legacy text shaping/upload, atlas evictions, atlas dirty updates, fallback fonts, bidi boundaries, and cursor maps.
  - Browser WebGPU report rows cannot pass the report tests without p50/p95/p99/peak, missed-frame, hitch, pixel-check, draw-family counters, draw-state counters, clip-depth/scissor counters, pass-family counters that sum to total render passes, resource-lifetime counters, image-upload temp/scratch counters, upload A/B fields plus direct glyph/RGBA timestamp totals, effect-uniform counters plus current and legacy direct GPU timestamp totals for the effect-uniform A/B row, texture-copy counters, upload-scratch A/B fields, effect-uniform A/B fields, backdrop-batch A/B fields, Scene3D A/B fields, mixed-scene current-versus-legacy A/B fields, layer/damage/effects current-versus-legacy A/B fields, command-family current-versus-legacy A/B fields, glyph-run current-versus-legacy A/B fields, neon-marker current-versus-legacy A/B fields, direct-surface GPU timestamp/pass reduction fields, draw-state cache A/B fields, clip-state cache A/B fields, explicit backend-path coverage rows tying every important WebGPU path family to distributions and explanatory counters, report-level and per-row warm-resource-churn zero-growth summaries including family-level GPU resource and CPU scratch growth totals, current-row Rust/WASM allocation counters with bounded per-frame budgets and zero reallocations, frame-loop allocation stage attribution with no unattributed allocations, renderer submit sub-stage allocation attribution with no unattributed parent-submit allocations, zero WASM memory growth across benchmark marks after prewarm, Chrome JS heap sampling/GC support and finite heap growth fields for each benchmark mark, Chrome browser trace event counts with `capture_phase=benchmark-report` and `timing_source=untraced-baseline-report`, traced browser User Timing labels and positive trace intervals for every benchmark family, timestamp-query attribution status, GPU timestamp stage breakdown totals reconciled to every row, and collected timestamp pass counts that match render-pass counts when timestamp queries are supported.
  - Coverage counts and covered-name inventories stay synchronized with the registered case inventory.
  - Missing metrics default safely through serde so older baselines remain readable while the schema grows.

## Changelog

- 2026-06-01: Added per-benchmark browser User Timing mark gates to the WebGPU report and duplicate Chrome trace capture.
- 2026-06-02: Added GPU timestamp stage-breakdown reconciliation gates to the browser WebGPU report.
- 2026-06-02: Added per-benchmark Chrome trace interval gates to the WebGPU report.
- 2026-06-02: Added direct glyph/RGBA timestamp-total fields to the browser WebGPU upload A/B summary gate.
- 2026-06-02: Added browser WebGPU Rust/WASM frame allocation audit counters and budget gates.
- 2026-06-02: Added browser WebGPU submit sub-stage WASM allocation attribution gates.
- 2026-06-02: Added browser WebGPU frame-loop allocation stage attribution gates.
- 2026-06-02: Added neon-marker current-versus-legacy A/B coverage to the browser WebGPU report gate.
- 2026-06-02: Expanded the browser WebGPU report gate to enforce the 33-row matrix with glyph-run current-versus-legacy-rebind A/B coverage.
- 2026-06-02: Added direct-surface current-versus-forced-scene-present A/B coverage to the browser WebGPU report gate.
- 2026-06-02: Expanded the browser WebGPU report gate with command-family current-versus-legacy A/B coverage.
- 2026-06-02: Expanded the browser WebGPU report gate to enforce the 26-row matrix with layer/damage/effects current-versus-legacy A/B coverage.
- 2026-06-02: Expanded the browser WebGPU report gate to enforce the 25-row matrix with mixed-scene current-versus-legacy A/B coverage.
- 2026-06-01: Added direct GPU timestamp-total fields to the browser WebGPU effect-uniform A/B summary.
- 2026-06-01: Added the browser WebGPU Chrome trace summary gate and tied it to duplicate benchmark-report trace capture.
- 2026-06-01: Added the browser WebGPU warm-resource-churn summary gate for current warm rows.
- 2026-06-01: Expanded the browser WebGPU report gate to enforce the 24-row matrix with backdrop-batch A/B coverage and texture-copy/render-pass reductions.
- 2026-06-01: Expanded the browser WebGPU report gate to enforce the 20-row matrix with effect-uniform A/B coverage and effect-uniform counters.
- 2026-06-01: Expanded the browser WebGPU report gate to enforce the 22-row matrix with clip-state cache A/B coverage and clip-depth/scissor counters.
- 2026-06-01: Expanded the browser WebGPU report gate to enforce the 18-row matrix with upload-scratch A/B coverage and image-upload temp/scratch counters.
- 2026-06-02: Added browser WebGPU warm scratch family attribution gates for draw, Scene3D, effect, ID-mask, image-upload, and resource-table growth.
- 2026-06-02: Added browser WebGPU GPU resource family attribution gates for draw, image, target, Scene3D, effect, and ID-mask resource churn.
- 2026-06-01: Expanded the browser WebGPU report gate to enforce the 16-row matrix with draw-state cache A/B coverage.
- 2026-06-01: Expanded the browser WebGPU report gate to enforce the 14-row matrix with generic command-family coverage.
- 2026-06-01: Expanded the browser WebGPU report gate with dedicated layer/damage/effects coverage.
- 2026-06-01: Added browser WebGPU timestamp-query collection gates and per-row timestamp pass count checks.
- 2026-06-01: Expanded the browser WebGPU report gate with resource/explanatory counters.
- 2026-06-01: Added browser WebGPU pass-family attribution counters and timestamp-query capability status to the persisted web report gate.
- 2026-06-01: Added cursor-map structural metrics to text-input cursor-pick rows: cursor count, byte boundaries, boundary checksum, affinity splits, width span, and fallback shape-run count.
- 2026-06-01: Added `cpu.authoring.collection_measure_cache.bounded_churn` so bounded variable collection measurement-cache repair after large key churn is represented in workspace baselines.
- 2026-06-01: Added persisted eviction, dirty-rect, resident-glyph, revision, vertex, and index metrics to `cpu.system.text_atlas_pressure`.
- 2026-06-01: Added persisted workspace report gates for collection keyed/prefix A/B rows and text cache/atlas/cursor rows.
- 2026-06-01: Added a persisted workspace report gate for retained-surface authoring and dirty-class layout reuse rows.
- 2026-06-01: Added a persisted workspace report gate for macOS Metal animation/effects and collection-navigation frame-pacing rows.
- 2026-06-01: Added a persisted WebGPU report contract test for browser frame distributions, missed-frame/hitch metrics, pixel checks, and ID-mask current-vs-legacy A/B fields.
- 2026-06-01: Added a focused report test for the state-reconciliation battery so single-node, 1 percent, 10 percent, and full-theme mutation rows stay gated.
- 2026-06-01: Bounded `CollectionView` variable measurement cache retention while keeping collection key-reconcile and prefix-repair A/B rows as the workspace guard.
- 2026-06-01: Added `cpu.system.wrapped_label_cached_encode` and `cpu.system.wrapped_label_legacy_fit_shape` so ASCII wrapped-label cache-miss fitting has current-vs-legacy workspace A/B evidence.
- 2026-05-31: Added `gpu.journey.collection_navigation.frame_pacing` so a real collection navigation flow persists macOS Metal event-to-visible, frame, GPU, missed-frame, and hitch distributions.
- 2026-05-31: Added `cpu.layout.scoped_tree_mutation.add_remove` so scoped surface add/remove mutations are measured with layout skip and retained sibling replay counters.
- 2026-05-31: Broadened the case metric contract so all frame rows require frame distribution plus missed-frame/hitch metrics, and GPU frame rows require GPU distribution metrics.
- 2026-05-31: Updated the cursor-pick cluster-map note to reflect `oxide_text::ShapedCursorMap` as the text-input backing artifact.
- 2026-05-31: Added `cpu.text_input.cursor_pick.mixed_bidi_affinity` so mixed LTR/RTL upstream/downstream cursor-map positions are measured beside LTR, RTL, and fallback-font cursor picking.
- 2026-05-31: Added `cpu.system.text_fallback_label_encode` so visible fallback-font glyph encoding is measured beside cursor fallback.
- 2026-05-31: Added `cpu.text_input.cursor_pick.fallback_cluster_map` so configured fallback-font cursor widths are measured beside the LTR and RTL cursor-pick cases.
- 2026-05-31: Added `cpu.text_input.cursor_pick.rtl_cluster_map` so descending RTL cursor-map picking is measured beside the LTR Unicode cursor-pick case.
- 2026-05-31: Added `cpu.layout.non_draw_dirty.retained_reuse` so accessibility/hit-test-only dirty classes are measured and gated with zero layout work plus retained draw-list reuse counters.
- 2026-05-31: Added `cpu.layout.node_content_dirty.retained_replay` so node-scoped text/image/camera dirty classes are measured and gated with zero layout work plus retained subtree replay counters.
- 2026-05-31: Added `cpu.system.text_atlas_dirty_rect_upload` so incremental text atlas dirty-rect publication is persisted with create/update counts and upload-area ratios.
- 2026-05-31: Added `gpu.animation.effects.refresh_matrix` so animation/effects frame pacing has a dedicated Metal row with direct GPU and 60/120 Hz hitch metrics.
- 2026-05-31: Updated `cpu.authoring.drawlist_text_replay.multi_atlas` to exercise the checked retained draw-list replay API rather than raw cached append.
- 2026-05-31: Collection scroll journey rows now persist item-revision query totals and revision-hint state for variable prefix reuse.
- 2026-05-31: Added `cpu.layout.paint_only.opacity_clip` so opacity/clip dirty-class edits are measured and gated with zero layout work plus retained subtree replay counters.
- 2026-05-31: Added `cpu.layout.transform_only.reposition` so transform-only dirty-class edits are measured and gated with zero layout work.
- 2026-05-31: Added `cpu.layout.descendant_only.incremental_relayout` so parent measurement avoidance for fixed-outer internal subtree edits is represented in workspace baselines.
- 2026-05-31: Added `cpu.text_input.cursor_pick.cluster_map` so long-line Unicode pointer-to-cursor mapping remains in the text-input perf contract.
- 2026-06-01: Added persisted text-byte, prefix-boundary, width-entry, and shaped-run metrics to `cpu.system.text_prefix_width_map`.
- 2026-05-31: Added `cpu.system.text_prefix_width_map` so one-shaped-run caret prefix maps are represented in workspace baselines.
- 2026-05-31: Added `cpu.layout.dirty_subtree.incremental_relayout` so per-node layout dirtiness and clean-subtree skips are represented in workspace baselines.
- 2026-05-31: Added `cpu.authoring.drawlist_text_replay.multi_atlas` for the public multi-atlas cached text draw-list replay API.
- 2026-05-31: Updated `cpu.authoring.surface_router.compose` to persist retained current/overlay/popup composition reuse metrics.
- 2026-05-31: Added `cpu.authoring.collection_key_reconcile.indexed` and `cpu.authoring.collection_key_reconcile.scan` so the public collection key-index hook has persisted A/B coverage.
- 2026-05-31: Added `cpu.authoring.collection_prefix_update.incremental` and `cpu.authoring.collection_prefix_update.full_scan` so dirty-range variable prefix repair has persisted A/B coverage.
- 2026-05-31: Added `cpu.authoring.surface_retained.text_atlas_context` for retained surface replay with live `TextCtx` text-atlas revision context.
- 2026-05-31: Added `cpu.text_input.ime.composition_commit_cycle` and `cpu.journey.text_ime_composition_cycle` so IME marked-text, commit, selection, keyboard geometry, and routed redraw paths are represented in persisted workspace reports.
- 2026-05-31: Added `cpu.authoring.surface_retained.dirty_leaf_encode` so paint-only dirty-leaf retained surface reuse is measured with retained-node reuse/rebuild counters.
- 2026-05-31: Added `cpu.system.text_atlas_pressure` so constrained glyph-atlas eviction is measured in the workspace suite.
- 2026-05-14: Added `cpu.bridge.web_backend_surface` and refreshed the workspace baseline to keep the WebAssembly backend represented in the Rust-side performance contract.
- 2026-04-30: Removed the one-use perf-filter activity wrapper; suite coverage gating now checks the parsed filter list directly.
- 2026-04-18: Collapsed duplicated contract-battery status and note conditionals into shared helper logic while preserving report output semantics.
- 2026-04-14: Collapsed repetitive coverage assertions into one shared check table while preserving the same incomplete-family error messages.
- 2026-04-11: Removed redundant manual `Default` implementations for internal CLI options and `PerfCaseResult`; derived defaults preserve the same serde fallback behavior with less implementation surface.
- 2026-04-23: Added authoring coverage for mixed retained `scene3d` plus 2D Metal frames.
