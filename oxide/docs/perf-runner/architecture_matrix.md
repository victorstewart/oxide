# oxide-perf-runner::architecture_matrix

## Intention and purpose

`architecture_matrix` owns the deterministic rendering workloads used by the renderer architecture program. It keeps workload construction and measurement out of production UI paths while giving later changes fixed case IDs, scaling points, cache states, latency distributions, direct GPU timings, and explanatory counters.

## Workload contract

- Retained UI: 1,000 label-shaped nodes, 500 image-shaped nodes, depths 16/32, clean replay, one dirty leaf, a 1,500-node hot working set under a 1 MiB hard cache budget, and a complete one-use invalidation workload under a zero-byte direct policy.
- Animation/text: a real 300-node `UiSurface` driven by `Animator`, retained glyph/image replay, nested opacity/clips/transforms, hit testing, and accessibility dirtiness; warm/new/script/fallback/atlas/scale/SDF text cases.
- Dynamic properties: the CPU surface row requires zero chunk/sequence/geometry rebuild after warmup; a 300-instance Metal text/image row alternates full affine and opacity records through the completion-safe property ring.
- Spatial metadata: CPU architecture/authoring rows query 10,000 alternating glyph/image-mesh instances; Metal small damage selects one instance/command with zero vertex scans, copies, or uploads, while full damage replays all 10,000 draws through one validated static plan.
- Layers/effects/damage: CPU command-construction rows plus production Metal submissions for 100 × 100 layer caching, invalidation/resize/navigation/nesting/backdrop/memory-pressure rebuilds, effect layouts, direct/prepass/quarter/eighth target plans, and exact 5/25/100 percent damage sequences over up to 10,000 items.
- ID mask: isolated Metal rows for static, style, viewport, projection, and content changes at 512/1024/2048 with 1/16/256 chunks, plus a two-map same-command-buffer row. Chunk-count variants never alternate inside one timed row, so each declared cache state remains unambiguous.
- Scene3D: isolated Metal rows for 96/1,000/10,000 instances across one/many meshes, alpha ordering, 25 percent viewport, culling, and one/three bloom layers.
- Frame resources: a three-slot visible 4,096-quad row freezes no-growth 327,680/49,152/16-byte VB/IB/UB high water, while an eight-slot 8,192-quad offscreen stress row grows every slot once and requires zero warm growth or backpressure skips.
- Prepared Metal chunks: 256 mixed immutable chunks, each carrying 64 RRects, images, glyph quads, or solid triangles. Clean frames change only a dynamic transform and require 256 hits with zero upload/traversal; the one-dirty row alternates only chunk zero's geometry revision and requires 255 hits plus one bounded rebuild.
- Prepared Metal layers: 100 retained snapshot layers with 100 RRects each. Clean frames require 100 texture hits with no body/offscreen/upload work; one-dirty frames require 99 hits, one texture miss, one offscreen replay from the existing prepared body, and no warm upload, preparation, or texture creation.
- Images and idle: CPU construction plus Metal resource/draw rows for 100/1,000/10,000 unique images and policy/churn variants; authoring rows exercise 100/1,000 unique `ImageView` cover cells and persist semantic image/nine-slice, crop, quad, draw-call, parameter-byte, and shaded-pixel counters; a foreground static row proves zero timers, animations, camera frames, network publications, damage, submissions, and wakeups.
- WebGPU primitives: opt-in browser rows for 1/64/1,024 RRects, 1/64/512 spinners, 64/1,024 neon markers, and 64/512 nine-slices. The 1,024-marker row emits eight production-sized 128-marker passes rather than changing the public per-pass safety limit.
- Metal analytic instances: physical Metal rows for RRect, image, nine-slice, spinner, backdrop, and visual-effect ordered runs at 1/64/1,024/10,000 instances. Every row persists frame/encode/direct-GPU distributions, draws, instances, upload bytes, analytic ring bytes/binds/growth, total growth, and frame-ring residency.

## Measurement boundary

Rust rows are selected with `OXIDE_PERF_RUNNER_FILTER=cpu.architecture.,gpu.architecture.`. GPU rows use production Metal begin/encode/submit methods and collect command-buffer GPU distributions, encode distributions, upload bytes, damage, draw, memory, and backpressure data. Retained cache-pressure rows persist hits, misses, hit rate, admissions/rejections, evictions and bytes, build time, retained chunk/sequence/prepared-GPU bytes, hard budget, completeness, and fallback count. Layer rows persist retained texture bytes plus average structural body scans, body copies, texture creates, hits/misses, offscreen/inline draws, and prevented duplicate renders; effect rows persist prepass/blur-chain/bloom bytes plus first-frame latency and first-use resource creation. ID-mask rows persist cache hits/misses, raster/seed/jump/compositor and total passes, field-cache budget/residency/entries/evictions, target creation, unique in-flight generations/bytes, cache-plus-in-flight and lifetime-peak target bytes, synchronization-blocked reuse, resource creation, target/upload-cache bytes, and exact geometry/style/viewport/map cardinality. `OXIDE_C32_METAL_WARMUPS`, `OXIDE_C32_METAL_FRAMES`, and `OXIDE_C32_METAL_RAW_SAMPLES` control reproducible raw populations. Scene3D rows persist depth, bloom, and mesh-buffer bytes plus pass work. Frame-resource rows persist configured depth, ring bytes, cold/warm growth, upload high water, and skips; their warmup count equals the configured depth so every slot is exercised before warm evidence. Smoke mode shortens measured sample counts but preserves every declared workload size.

Prepared rows persist full frame, Metal encode, and command-buffer GPU distributions plus immutable buffer uploads, dynamic uniform-ring upload bytes, copied geometry, traversed commands, draws, image argument-table binds, cache hit/miss and prepared/reused counts, evictions, prepared resident bytes, and total renderer bytes. `OXIDE_C24_FLAT_CONTROL=1` is benchmark-only evidence control: it preflattens the identical snapshots before timing and sends the same visible work through `encode_pass`, allowing the prepared lowering boundary to be compared without changing scene content.

`gpu.architecture.animation.dynamic_properties_300` persists frame/encode/GPU distributions, 120/60 Hz hitch and missed-frame ratios, exact property records/bytes/ring residency, immutable geometry bytes, command traversal, and cache outcomes. `OXIDE_C26_RAW_SAMPLES=1` adds indexed frame/encode/GPU observations for paired evidence. `cpu.authoring.animation.dynamic_properties_300` uses the same public `UiSurface` path as the architecture row.

The C29 rows are `gpu.architecture.prepared_layers.{clean_100x100,one_dirty_100x100}` and `gpu.authoring.retained_snapshot.prepared_layers_clean_100x100`. They persist frame/encode/GPU distributions, layer/body cardinality, body scans/copies, geometry copies, buffer uploads, texture creates, texture hits/misses, offscreen draws, render passes, final draws, prepared chunks, and retained layer bytes. `OXIDE_C29_FLAT_CONTROL=1` preflattens the identical snapshots outside the timed loop and routes them through `encode_pass`; `OXIDE_C29_RAW_SAMPLES=1` adds indexed frame/encode/GPU observations for explicit paired evidence. Neither flag changes ordinary reports.

The C27 rows are `cpu.architecture.spatial_metadata.glyph_mesh_10000`, `gpu.architecture.spatial_metadata.{small_damage,full_damage}_glyph_mesh_10000`, `cpu.authoring.retained_snapshot.spatial_query_10000`, and `gpu.authoring.retained_snapshot.spatial_damage_10000`. They persist query CPU, frame/encode/GPU distributions, visited/matched instances and commands, vertex visits, static plan reuse, draw count, copied/uploaded bytes, and shaded pixels. `OXIDE_C27_RAW_SAMPLES=1` adds indexed Metal frame/encode/GPU observations only for explicit evidence collection.

`OXIDE_C24_RAW_SAMPLES=1` persists every C24 warmup and measured frame/encode/GPU observation under indexed metric keys for the shared paired runner. Normal reports omit those keys.

The C14 rows are selected with `OXIDE_PERF_RUNNER_FILTER=cpu.authoring.image_view_grid.,gpu.authoring.image_view_grid.`. They create unique 29x7 Metal images and encode 24x12 cover cells through the public `ImageView` API, so parent zero-slice nine-slice behavior and candidate source-cropped image behavior share the same authoring and backend path.

`OXIDE_ARCHITECTURE_METAL_WARMUPS` and `OXIDE_ARCHITECTURE_METAL_FRAMES` override the warmup and measured frame counts for non-default statistical runs. When `OXIDE_ARCHITECTURE_METAL_RAW_SAMPLES` is present, each warmup and measured frame/encode/GPU duration is persisted under an indexed metric key. Normal reports omit those raw keys, so the expanded evidence shape is confined to explicit experiment runs.

`OXIDE_ARCHITECTURE_EFFECT_COLD_FIRST_USE=1` is confined to the `target_plan_*` Metal rows. It recreates and resizes the renderer before every post-initial frame, labels the row cold, and turns the raw encode distribution into repeated first-use effect-target samples. Without the flag, the permanent rows retain their normal warm-reuse behavior.

The WebGPU matrix is absent from normal page execution. `scripts/check_webgpu_browser_golden.mjs --architecture-matrix` opts in, prewarms resources, runs one submission per RAF, waits for both `GPUQueue.onSubmittedWorkDone` and browser presentation, collects exactly one timestamp per measured frame, rejects missing samples and zero-pass rows, and restores the normal timestamp sampling interval. Every row reports CPU and GPU p50/p95/p99/peak, draw/pass/bind/upload/resource/scratch counters, and allocation attribution.

The memory-warning layer row invokes the production layer-cache purge hook. All Metal layer rows run under a 16 MiB default benchmark budget, overridable with `OXIDE_ARCHITECTURE_LAYER_CACHE_BUDGET_BYTES`, and report resident, pool, CPU metadata, evictions, recreations, pool reuses, purges, and hard-budget violations. Navigation churn changes all 100 layer IDs per frame so allocation reuse and eviction tails are observable instead of allowing unbounded residency.

## Verification

- Unit tests freeze required scaling points, exact damage percentages, and gap-free 1/16/256 chunk coverage.
- Report tests require the hot retained row to be complete, hit at 100%, and remain within budget; the churn row must retain zero bytes and record one explicit fallback. A separate authoring row covers unchanged-policy hot access through the public `UiSurface` policy API.
- Report tests exercise retained, animation, idle, layer, ID-mask, and Scene3D rows; freeze `family=architecture` plus `scenario=rendering-architecture` metadata; and require nonzero Metal bytes for every previously omitted resource family.
- The warm static ID-mask report contract requires one cache hit, zero raster/seed/jump work, one compositor/total pass, one resident/in-flight generation, zero target creation or blocked reuse, nonzero generation-aware bytes, and residency no greater than the reported hard budget.
- C26 report tests require the warm CPU row to rebuild/copy zero geometry and the Metal row to record 300 hits, 300 changed records, 14,400 property bytes, zero geometry/traversal, and zero smoke 120 Hz misses.
- C27 report tests require one visited/matched instance and command, zero vertex/copy/upload work, one small-damage draw/four shaded pixels, zero query work on full damage, one reused full plan, and 512 full smoke draws.
- C29 report tests require clean architecture and public-authoring rows to record 100 hits, one pass, 100 composites, and zero body/copy/upload/texture/offscreen/preparation work. The dirty row requires 99 hits, one miss, one offscreen replay, two passes, 101 draws, and zero warm copy/upload/preparation/texture creation.
- Prepared report tests require clean 256/0 hit/miss, zero upload/copy/traversal, and one-dirty 255/1 hit/miss with exactly one 64-command, 3,072-byte rebuild. The authoring registry separately exercises the public retained-snapshot Metal entry point.
- The image-view report test freezes both 100/1,000 authoring rows, zero nine-slices, one crop and quad per image, bounded logical coverage, cross-texture Metal draw-call batching, and total inline-plus-argument parameter bytes.
- Browser source tests freeze all ten WebGPU primitive IDs, opt-in routing, queue/RAF pacing, timestamp settlement, and counter serialization.

## Changelog
- 2026-07-14: added C36 target creation, in-flight generation/byte, total/peak storage, and synchronization-blocked reuse metrics to every Metal ID-mask architecture row.
- 2026-07-14: added C32 content-changing and same-frame two-map Metal ID-mask rows with raw samples and complete cache/stage/residency counters.
- 2026-07-14: upgraded the 100-layer Metal matrix for C31 with a hard budget, production memory-warning purge, navigation-churn pool reuse, byte/counter telemetry, and a zero-budget-violation metric.
- 2026-07-13: added C29 100 × 100 prepared-layer clean/one-dirty Metal rows, public retained-snapshot authoring coverage, raw evidence controls, and exact work counters.
- 2026-07-13: added C27 10,000-instance spatial-query and small/full Metal glyph/image-mesh rows plus public retained-snapshot authoring coverage.
- 2026-07-13: added C26 CPU/authoring zero-geometry animation metrics and the 300-instance Metal property-ring row.
- 2026-07-13: added C24 clean and one-dirty persistent Metal prepared-chunk rows plus public retained-snapshot authoring coverage.
- 2026-07-13: Added C23 retained hot-reuse and zero-budget one-use cache-pressure rows with full cache-policy counters.

- 2026-07-13: added visible high-water and offscreen all-slot growth-stress rows with explicit depth, residency, growth, upload, and backpressure contracts.
- 2026-07-12: added direct/prepass/quarter/eighth Metal effect-target rows with first-use creation, residency, and first-frame metrics.

- 2026-07-13: added C14 100/1,000-image public-authoring and Metal cover-grid rows with raw frame/encode/GPU distributions and semantic work counters.
- 2026-07-12: Added C05 Metal layer ownership counters to the clean, dirty, resize, churn, nested, unsupported-effect, and rebuild rows.
- 2026-07-12: Added C02 Metal resource-family and frame-work report metrics for layers, ID masks, depth, bloom, meshes, chunks, and render passes.
- 2026-07-12: Added the C01 rendering architecture proof matrix with isolated CPU, Metal, and opt-in RAF-paced WebGPU workloads.
