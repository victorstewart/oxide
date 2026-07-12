# oxide-perf-runner::architecture_matrix

## Intention and purpose

`architecture_matrix` owns the deterministic rendering workloads used by the renderer architecture program. It keeps workload construction and measurement out of production UI paths while giving later changes fixed case IDs, scaling points, cache states, latency distributions, direct GPU timings, and explanatory counters.

## Workload contract

- Retained UI: 1,000 label-shaped nodes, 500 image-shaped nodes, depths 16/32, clean replay, and one dirty leaf.
- Animation/text: a real 300-node `UiSurface` driven by `Animator`, retained glyph/image replay, nested opacity/clips/transforms, hit testing, and accessibility dirtiness; warm/new/script/fallback/atlas/scale/SDF text cases.
- Layers/effects/damage: CPU command-construction rows plus production Metal submissions for 100 × 100 layer caching, invalidation/resize/navigation/nesting/backdrop/memory-pressure rebuilds, effect layouts, and exact 5/25/100 percent damage sequences over up to 10,000 items.
- ID mask: isolated Metal rows for static, style, viewport, and projection changes at 512/1024/2048 with 1/16/256 chunks. Chunk-count variants never alternate inside one timed row, so the static cache state remains static.
- Scene3D: isolated Metal rows for 96/1,000/10,000 instances across one/many meshes, alpha ordering, 25 percent viewport, culling, and one/three bloom layers.
- Images and idle: CPU construction plus Metal resource/draw rows for 100/1,000/10,000 unique images and policy/churn variants; a foreground static row proves zero timers, animations, camera frames, network publications, damage, submissions, and wakeups.
- WebGPU primitives: opt-in browser rows for 1/64/1,024 RRects, 1/64/512 spinners, 64/1,024 neon markers, and 64/512 nine-slices. The 1,024-marker row emits eight production-sized 128-marker passes rather than changing the public per-pass safety limit.

## Measurement boundary

Rust rows are selected with `OXIDE_PERF_RUNNER_FILTER=cpu.architecture.,gpu.architecture.`. GPU rows use production Metal begin/encode/submit methods and collect command-buffer GPU distributions, encode distributions, upload bytes, damage, draw, memory, and backpressure data. Layer rows persist retained texture bytes; ID-mask rows persist target/upload-cache bytes plus chunk/pass work; Scene3D rows persist depth, bloom, and mesh-buffer bytes plus pass work. Smoke mode shortens warmup/sample counts but preserves every declared workload size.

The WebGPU matrix is absent from normal page execution. `scripts/check_webgpu_browser_golden.mjs --architecture-matrix` opts in, prewarms resources, runs one submission per RAF, waits for both `GPUQueue.onSubmittedWorkDone` and browser presentation, collects exactly one timestamp per measured frame, rejects missing samples and zero-pass rows, and restores the normal timestamp sampling interval. Every row reports CPU and GPU p50/p95/p99/peak, draw/pass/bind/upload/resource/scratch counters, and allocation attribution.

The current memory-warning layer row recreates the renderer after an explicit benchmark pressure event because the renderer API does not yet expose cache-specific purge. The stable row must move to the production purge hook when that hook lands; until then it measures the complete rebuild boundary without adding a production-only testing branch.

## Verification

- Unit tests freeze required scaling points, exact damage percentages, and gap-free 1/16/256 chunk coverage.
- Report tests exercise retained, animation, idle, layer, ID-mask, and Scene3D rows; freeze `family=architecture` plus `scenario=rendering-architecture` metadata; and require nonzero Metal bytes for every previously omitted resource family.
- Browser source tests freeze all ten WebGPU primitive IDs, opt-in routing, queue/RAF pacing, timestamp settlement, and counter serialization.

## Changelog

- 2026-07-12: Added C02 Metal resource-family and frame-work report metrics for layers, ID masks, depth, bloom, meshes, chunks, and render passes.
- 2026-07-12: Added the C01 rendering architecture proof matrix with isolated CPU, Metal, and opt-in RAF-paced WebGPU workloads.
