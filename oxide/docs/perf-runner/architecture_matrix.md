# oxide-perf-runner::architecture_matrix

## Intention and purpose

`architecture_matrix` owns the deterministic rendering workloads used by the renderer architecture program. It keeps workload construction and measurement out of production UI paths while giving later changes fixed case IDs, scaling points, cache states, latency distributions, direct GPU timings, and explanatory counters.

## Workload contract

- Retained UI: 1,000 label-shaped nodes, 500 image-shaped nodes, depths 16/32, clean replay, one dirty leaf, a 1,500-node hot working set under a 1 MiB hard cache budget, and a complete one-use invalidation workload under a zero-byte direct policy.
- Animation/text: a real 300-node `UiSurface` driven by `Animator`, retained glyph/image replay, nested opacity/clips/transforms, hit testing, and accessibility dirtiness; warm/new/script/fallback/atlas/scale/SDF text cases.
- Layers/effects/damage: CPU command-construction rows plus production Metal submissions for 100 × 100 layer caching, invalidation/resize/navigation/nesting/backdrop/memory-pressure rebuilds, effect layouts, direct/prepass/quarter/eighth target plans, and exact 5/25/100 percent damage sequences over up to 10,000 items.
- ID mask: isolated Metal rows for static, style, viewport, and projection changes at 512/1024/2048 with 1/16/256 chunks. Chunk-count variants never alternate inside one timed row, so the static cache state remains static.
- Scene3D: isolated Metal rows for 96/1,000/10,000 instances across one/many meshes, alpha ordering, 25 percent viewport, culling, and one/three bloom layers.
- Frame resources: a three-slot visible 4,096-quad row freezes no-growth 327,680/49,152/16-byte VB/IB/UB high water, while an eight-slot 8,192-quad offscreen stress row grows every slot once and requires zero warm growth or backpressure skips.
- Prepared Metal chunks: 256 mixed immutable chunks, each carrying 64 RRects, images, glyph quads, or solid triangles. Clean frames change only a dynamic transform and require 256 hits with zero upload/traversal; the one-dirty row alternates only chunk zero's geometry revision and requires 255 hits plus one bounded rebuild.
- Images and idle: CPU construction plus Metal resource/draw rows for 100/1,000/10,000 unique images and policy/churn variants; authoring rows exercise 100/1,000 unique `ImageView` cover cells and persist semantic image/nine-slice, crop, quad, draw-call, parameter-byte, and shaded-pixel counters; a foreground static row proves zero timers, animations, camera frames, network publications, damage, submissions, and wakeups.
- WebGPU primitives: opt-in browser rows for 1/64/1,024 RRects, 1/64/512 spinners, 64/1,024 neon markers, and 64/512 nine-slices. The 1,024-marker row emits eight production-sized 128-marker passes rather than changing the public per-pass safety limit.

## Measurement boundary

Rust rows are selected with `OXIDE_PERF_RUNNER_FILTER=cpu.architecture.,gpu.architecture.`. GPU rows use production Metal begin/encode/submit methods and collect command-buffer GPU distributions, encode distributions, upload bytes, damage, draw, memory, and backpressure data. Retained cache-pressure rows persist hits, misses, hit rate, admissions/rejections, evictions and bytes, build time, retained chunk/sequence/prepared-GPU bytes, hard budget, completeness, and fallback count. Layer rows persist retained texture bytes plus average structural body scans, body copies, texture creates, hits/misses, offscreen/inline draws, and prevented duplicate renders; effect rows persist prepass/blur-chain/bloom bytes plus first-frame latency and first-use resource creation; ID-mask rows persist target/upload-cache bytes plus chunk/pass work; Scene3D rows persist depth, bloom, and mesh-buffer bytes plus pass work. Frame-resource rows persist configured depth, ring bytes, cold/warm growth, upload high water, and skips; their warmup count equals the configured depth so every slot is exercised before warm evidence. Smoke mode shortens measured sample counts but preserves every declared workload size.

Prepared rows persist full frame, Metal encode, and command-buffer GPU distributions plus immutable buffer uploads, dynamic uniform-ring upload bytes, copied geometry, traversed commands, draws, image argument-table binds, cache hit/miss and prepared/reused counts, evictions, prepared resident bytes, and total renderer bytes. `OXIDE_C24_FLAT_CONTROL=1` is benchmark-only evidence control: it preflattens the identical snapshots before timing and sends the same visible work through `encode_pass`, allowing the prepared lowering boundary to be compared without changing scene content.

`OXIDE_C24_RAW_SAMPLES=1` persists every C24 warmup and measured frame/encode/GPU observation under indexed metric keys for the shared paired runner. Normal reports omit those keys.

The C14 rows are selected with `OXIDE_PERF_RUNNER_FILTER=cpu.authoring.image_view_grid.,gpu.authoring.image_view_grid.`. They create unique 29x7 Metal images and encode 24x12 cover cells through the public `ImageView` API, so parent zero-slice nine-slice behavior and candidate source-cropped image behavior share the same authoring and backend path.

`OXIDE_ARCHITECTURE_METAL_WARMUPS` and `OXIDE_ARCHITECTURE_METAL_FRAMES` override the warmup and measured frame counts for non-default statistical runs. When `OXIDE_ARCHITECTURE_METAL_RAW_SAMPLES` is present, each warmup and measured frame/encode/GPU duration is persisted under an indexed metric key. Normal reports omit those raw keys, so the expanded evidence shape is confined to explicit experiment runs.

`OXIDE_ARCHITECTURE_EFFECT_COLD_FIRST_USE=1` is confined to the `target_plan_*` Metal rows. It recreates and resizes the renderer before every post-initial frame, labels the row cold, and turns the raw encode distribution into repeated first-use effect-target samples. Without the flag, the permanent rows retain their normal warm-reuse behavior.

The WebGPU matrix is absent from normal page execution. `scripts/check_webgpu_browser_golden.mjs --architecture-matrix` opts in, prewarms resources, runs one submission per RAF, waits for both `GPUQueue.onSubmittedWorkDone` and browser presentation, collects exactly one timestamp per measured frame, rejects missing samples and zero-pass rows, and restores the normal timestamp sampling interval. Every row reports CPU and GPU p50/p95/p99/peak, draw/pass/bind/upload/resource/scratch counters, and allocation attribution.

The current memory-warning layer row recreates the renderer after an explicit benchmark pressure event because the renderer API does not yet expose cache-specific purge. The stable row must move to the production purge hook when that hook lands; until then it measures the complete rebuild boundary without adding a production-only testing branch.

## Verification

- Unit tests freeze required scaling points, exact damage percentages, and gap-free 1/16/256 chunk coverage.
- Report tests require the hot retained row to be complete, hit at 100%, and remain within budget; the churn row must retain zero bytes and record one explicit fallback. A separate authoring row covers unchanged-policy hot access through the public `UiSurface` policy API.
- Report tests exercise retained, animation, idle, layer, ID-mask, and Scene3D rows; freeze `family=architecture` plus `scenario=rendering-architecture` metadata; and require nonzero Metal bytes for every previously omitted resource family.
- Prepared report tests require clean 256/0 hit/miss, zero upload/copy/traversal, and one-dirty 255/1 hit/miss with exactly one 64-command, 3,072-byte rebuild. The authoring registry separately exercises the public retained-snapshot Metal entry point.
- The image-view report test freezes both 100/1,000 authoring rows, zero nine-slices, one crop and quad per image, bounded logical coverage, cross-texture Metal draw-call batching, and total inline-plus-argument parameter bytes.
- Browser source tests freeze all ten WebGPU primitive IDs, opt-in routing, queue/RAF pacing, timestamp settlement, and counter serialization.

## Changelog
- 2026-07-13: added C24 clean and one-dirty persistent Metal prepared-chunk rows plus public retained-snapshot authoring coverage.
- 2026-07-13: Added C23 retained hot-reuse and zero-budget one-use cache-pressure rows with full cache-policy counters.

- 2026-07-13: added visible high-water and offscreen all-slot growth-stress rows with explicit depth, residency, growth, upload, and backpressure contracts.
- 2026-07-12: added direct/prepass/quarter/eighth Metal effect-target rows with first-use creation, residency, and first-frame metrics.

- 2026-07-13: added C14 100/1,000-image public-authoring and Metal cover-grid rows with raw frame/encode/GPU distributions and semantic work counters.
- 2026-07-12: Added C05 Metal layer ownership counters to the clean, dirty, resize, churn, nested, unsupported-effect, and rebuild rows.
- 2026-07-12: Added C02 Metal resource-family and frame-work report metrics for layers, ID masks, depth, bloom, meshes, chunks, and render passes.
- 2026-07-12: Added the C01 rendering architecture proof matrix with isolated CPU, Metal, and opt-in RAF-paced WebGPU workloads.
