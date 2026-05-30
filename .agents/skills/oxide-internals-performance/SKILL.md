---
name: oxide-internals-performance
description: "Use when changing or reviewing Oxide internals: ui-core, renderer-api, renderer-metal, renderer-web, Metal, draw list, glyph atlas, text, input, timing, platform hosts, frame scheduling, frame pacing, damage redraw, Scene3D, UI performance, benchmarks, snapshots, GPU profiling, or performance-sensitive runtime architecture."
---

# Oxide Internals Performance Skill



## Scope note

This skill is for contributors working on the Oxide library/runtime itself. It is intentionally allowed to reason about internal crates, renderer boundaries, platform glue, benchmarks, snapshots, and backend architecture. For app code, demos, examples, or docs that use Oxide without changing Oxide internals, prefer `$oxide-app-authoring-performance`.

## Prime directive

Oxide is a Rust-first application UI runtime and renderer. Treat it as a custom UI toolkit with a game-engine-grade renderer backend, not as a thin wrapper over UIKit, SwiftUI, or Web views.

Optimize for sustained user-visible quality: event-to-visible-response latency, hitch rate, missed frames, p95/p99 frame time, idle battery use, thermal drift, text correctness, input correctness, accessibility correctness, and apples-to-apples parity against native UI baselines. Average FPS alone is not a valid success metric.

Before recommending or editing graphics code, state the performance hypothesis: bottleneck class, affected stage, workload, expected counter movement, and visual/correctness risk.

## Repository identity and non-negotiables

Use these assumptions for Oxide unless the user explicitly says otherwise:

- Repository: `victorstewart/oxide`.
- Product: write apps once in Rust, compile to iOS, Android, Web, and other platforms.
- Current concrete host stack: Rust workspace, macOS test host, iOS host, Web host, Metal renderer, renderer-web crate.
- Long-term backend posture: iOS Metal now, Web backend now, Android/Vulkan anticipated by abstractions.
- Rust owns UI state, layout, draw list generation, input semantics, animation policy, and renderer-facing data.
- UIKit is a thin host shell for lifecycle, surface hosting, input/IME, accessibility bridge, haptics, clipboard, and OS services.
- Do not move product behavior into UIKit recognizers, UIKit views, SwiftUI state, or Objective-C scene-specific gesture state.
- Do not hide backend capability behind a lowest-common-denominator abstraction.
- Do not run `cargo fmt` or `cargo clippy` in this repo. Preserve manual formatting: 3-space indentation, Allman braces, and the existing import/style canon.
- Use focused tests, snapshot tests, perf reports, device captures, and reviewed baseline updates instead of broad unverified cleanups.

## Oxide repo map

Treat these crates and folders as architecture boundaries:

| Area | Role | Performance implications |
|---|---|---|
| `oxide/crates/platform-api` | App trait, events, device caps, permissions, OS service interfaces. | Keep OS bridge coarse; do not pay per-widget/per-draw FFI. |
| `oxide/crates/ui-core` | Draw builder, node tree, layout, hit testing, elements, collection view, async layout, animations, scenes/router. | Main UI CPU cost center: invalidation, layout, diff, draw-list generation, virtualization. |
| `oxide/crates/renderer-api` | Draw list types, renderer traits, frame/damage/device caps. | Contract between UI semantics and backend execution. Keep semantic but performance-expressive. |
| `oxide/crates/renderer-metal` | Metal renderer, PSOs, shaders, rings, effects, camera paths, Scene3D. | Main GPU/backend cost center: batching, pass planning, storage modes, draw encoding, GPU timings. |
| `oxide/crates/renderer-web` | Web renderer backend. | Forces portable architecture; must not make Metal assumptions leak into UI core. |
| `oxide/crates/text` | Text shaping, swash rasterization, A8 atlas, SDF/MSDF direction. | Text correctness and atlas/upload behavior must be first-class. |
| `oxide/crates/input` | Gesture helpers, tap/long-press/pan, scroll accumulator. | Rust-owned input semantics; input latency and sample preservation matter. |
| `oxide/crates/timing` | Monotonic timers and animation curves. | Frame pacing, deterministic animation, refresh-rate adaptation. |
| `oxide/crates/perf-runner` | Engine/UI/bridge/authoring performance cases. | Every visible or hot-path change must land with a perf case. |
| `oxide/crates/snapshot-runner` and `goldens` | GPU/readback visual verification. | Ensure optimizations do not silently lower quality. |
| `oxide/host/ios-app` | Thin UIKit/Objective-C iOS shell. | Lifecycle, CADisplayLink, CAMetalLayer, input/IME/a11y bridge. |
| `oxide/host/macos-app` | AppKit + CAMetalLayer host. | Development and snapshot host; not authoritative for iPhone performance. |
| `oxide/benchmarks` | Persisted workspace/device/UI parity reports. | Device baselines and report shape are part of the performance contract. |

## Activation triggers

Use this skill when the task involves any of these:

- Oxide UI elements, scenes, routers, draw builders, retained tree/data flow, layout, hit testing, text, input, gestures, animations, camera preview, visual effects, or accessibility.
- `renderer-api`, `renderer-metal`, `renderer-web`, Metal shaders, GPU timing, draw lists, rings, atlases, layers, damage, offscreen passes, snapshots, and benchmark baselines.
- iOS Metal, CAMetalLayer, CADisplayLink, UIKit/Objective-C host glue, IME, haptics, clipboard, memory warnings, EDR/HDR, ProMotion, and device performance.
- General renderer backend architecture: render graphs, frame graphs, resource lifetime, synchronization, pass planning, shader/pipeline caches, command buffers, descriptor/argument buffers, heaps, transient resources, CPU/GPU overlap.
- Game-engine renderer lessons that apply to application UI backends.

## First-response behavior

When asked to review or design Oxide graphics/UI code:

1. Identify the exact Oxide layer: platform bridge, input, ui-core, text, renderer-api, renderer-metal, renderer-web, perf runner, snapshots, or host.
2. State the bottleneck hypothesis before fixes.
3. Preserve Oxide boundaries: app state -> UI tree -> layout/diff -> draw list -> render list/batches -> backend commands -> present.
4. Prefer deleting or avoiding work over making unnecessary work faster.
5. Include the benchmark/snapshot/device trace needed to prove the change.
6. Do not suggest `cargo fmt` or `cargo clippy`.
7. If repo code is available, cite paths, functions, and concrete edits.

## Source-of-truth hierarchy

Use sources in this order:

1. Oxide source, `AGENTS.md`, `spec.xml`, persisted benchmark reports, snapshot goldens, device captures, CI, and current PR diff.
2. Apple primary sources: Metal documentation, Metal Best Practices, WWDC sessions, Xcode Metal Frame Capture, Metal System Trace, Metal Performance HUD, shader profiler, GPU counters.
3. Production UI/2D renderer projects: Skia/Graphite, Vello, Flutter Impeller, Rive Renderer, Iced, egui, Xilem/Masonry, Taffy, AccessKit, Dear ImGui, Nuklear.
4. Production renderer/game-engine codebases: Filament, bgfx, The Forge, IGL, wgpu, Godot, Unity docs, Unreal docs, Cocos2d-x, MetalPetal, GPUImage3, BBMetalImage.
5. General GPU/API wisdom: Arm tile-based rendering guidance, NVIDIA Vulkan guidance, AMD GPUOpen, Vulkan synchronization examples, Zeux production renderer writing.
6. Tweets/social posts: leads only. Convert them into rules only after verifying against source code, docs, or captures.

## Oxide frame pipeline model

The expected frame pipeline is:

```text
OS events / timers / camera frames
  -> platform-api AppEvent stream
  -> app state update
  -> ui-core event routing
  -> layout / diff / invalidation
  -> text shaping and atlas preparation
  -> draw builder
  -> renderer-api DrawList
  -> renderer-metal/web lowering
  -> resource uploads / atlas updates
  -> pass planning and batching
  -> Metal/Web commands
  -> present
  -> completion handlers / metrics / resource recycling
```

The iOS tick order should remain deterministic:

```text
input coalesce -> App::event -> advance animations -> layout/diff -> DrawList -> Renderer::begin_frame -> encode_pass -> submit -> present
```

Do not make widgets or scenes call Metal directly. Widgets and scenes emit semantic draw commands. Backends lower them into GPU work.

## Frame-time budgets and headline metrics

Use frame time and latency distributions, not averages.

| Target | Budget | Use case |
|---|---:|---|
| 120 Hz | 8.33 ms | ProMotion interactions, high-refresh app UI, drag/scroll/pinch, trivial scenes. |
| 90 Hz | 11.11 ms | Some immersive/display modes. |
| 60 Hz | 16.67 ms | Baseline iPhone/iPad app UI target and Low Power fallback. |
| 30 Hz | 33.33 ms | Explicit quality/thermal fallback only. |

Headline metrics for user-visible flows:

- first frame;
- first interactive;
- event-to-visible-response latency;
- p50/p95/p99/peak frame time;
- hitch ratio and missed frames;
- CPU main-thread time;
- render-thread/worker time;
- GPU command-buffer duration;
- vertex/fragment/pass timing where supported;
- memory and atlas pressure;
- upload bytes;
- dirty-node count;
- layout passes;
- draw-call count;
- encoded bytes;
- power/thermal drift when available.

For official Oxide/UIKit comparisons, use physical-device baselines only. Simulator numbers are diagnostic only.

## Performance contract

Every new Oxide feature, component, animation, user-facing scene, renderer-facing hot path, public authoring API, OS bridge path, camera path, or visible interaction must add or update one of:

- engine microbenchmark;
- representative screen flow;
- user-journey perf case;
- authoring perf case;
- OS bridge benchmark;
- device benchmark;
- snapshot/golden coverage;
- UIKit parity case when the work is user-visible on iOS.

Persisted reports should separate cold/warm/hot cache states. Do not mix cache states in one number.

Accepted regressions require an explicit reviewed baseline update and a written reason. A faster path that renders less, lowers quality, skips effects, or changes input behavior is not a valid win.

## Oxide UI architecture rules

### State, tree, diff, and invalidation

- Keep stable node IDs across layout, hit testing, animation, input routing, accessibility, text cache, and retained render data.
- Separate invalidation classes: style, layout, text, paint, transform, opacity, clip, image content, camera frame, accessibility, and hit-test data.
- Do not relayout or repaint the whole tree for pointer movement, cursor blink, timer tick, or single-node state changes.
- Virtualize collection/list/grid/chat workloads. Do not layout or draw thousands of offscreen nodes.
- Prefer dirty subtrees and retained draw lists, but do not let damage tracking cost more than repainting.
- Keep app-authored state separate from renderer resources.
- Maintain deterministic update order for same input streams.

### Draw-list contract

`renderer-api::DrawList` is the semantic renderer boundary. Keep it expressive enough for performance without leaking backend implementation details.

Current draw-command families include:

- layered rendering: `LayerBegin`, `LayerEnd`;
- primitives: `Solid`, `RRect`, `Spinner`;
- images: `Image`, `ImageMesh`, `NineSlice`;
- text: `GlyphRun`;
- effects: `Backdrop`, `VisualEffect`;
- camera: `CameraBg`, `NativeCameraPreview`;
- embedded/custom: `TopomapGlobe`;
- clips: `ClipPush`, `ClipPop`.

Add new commands only when they create a measurable renderer win or a clear semantic boundary. Do not add command variants that bake scene-specific business logic into the renderer.

### Render-list lowering

Backends should lower draw commands into batches keyed by:

- pipeline/PSO;
- shader function/constants;
- texture/atlas/image handle;
- sampler;
- blend mode;
- clip/scissor id;
- layer/render target;
- vertex/index span;
- per-pass/per-draw uniform range.

Stable sort by PSO -> texture -> clip where visual order permits. Merge adjacent compatible draws. Preserve visual order where alpha, clips, layers, backdrop, and text require it.

### Layers, damage, and retained rendering

- Treat `LayerBegin { dirty }` as a contract: dirty layers can be re-rendered; clean layers should be reused only if correctness is proven.
- Do not rely on previous `CAMetalDrawable` contents for damage redraw. Presented drawables are not retained canvases. Use explicit layer textures or redraw the final drawable.
- Track damage in logical points; convert to device pixels after scale and snapping rules.
- If partial redraw causes extra complexity or stale pixels, prefer full-surface redraw plus cheaper upstream invalidation.
- Cache expensive layers only when memory pressure, invalidation keys, and visual equivalence are well-defined.
- On memory warnings, flush atlases/layer caches and rebuild lazily.

### Text system

Text is a performance and correctness subsystem, not a primitive draw detail.

- Shape text before drawing. Do not map Unicode scalars directly to glyphs.
- Use `oxide-text` for shaping/rasterization/atlas ownership; keep renderer text PSOs focused on compositing glyph data.
- Cache shaped runs by font, size, features, script, text, layout width, DPI/device scale, bidi level, and fallback chain.
- Keep glyph atlas uploads batched per frame.
- Separate shaping, layout, rasterization, atlas upload, and draw submission.
- Support RTL, bidi, CJK, IME marked text, grapheme clusters, emoji/color glyph strategy, fallback fonts, dynamic type/category changes, and selection/caret geometry.
- Snap glyph origins after scale transform; do not snap filled interior vertices unnecessarily.
- Track text cache hit/miss, shaping time, raster time, atlas upload bytes, evictions, and draw count.

### Images, camera, and video

- Keep camera/video/image frames on the GPU after acquisition.
- Use platform APIs only for the lowest necessary acquisition bridge; Oxide owns visible preview rendering/composition/pacing unless the path is explicitly diagnostic.
- Keep `CameraBg` and `NativeCameraPreview` measurement buckets separate.
- For camera preview work, instrument fine-grained stages: frame acquisition, texture bridge/publication, renderer fetch, command-buffer creation, encoder creation, bind, draw, end encoding, present, commit, submission polling, and host tick overhead.
- Do not treat `AVCaptureVideoPreviewLayer` as a shipping renderer path for Oxide. Use it only as a diagnostic baseline.
- Use NV12/YUV paths where applicable; avoid CPU color conversion in the frame loop.
- Pool upload/staging resources and textures.
- Track texture bytes, decode/upload time, first-visible presentation, camera frame age, and dropped/stale frames.

### Input, gestures, and latency

- Rust/Oxide owns product interaction semantics.
- iOS should forward raw touches/events with phase, coordinate, timestamp, and stable touch identity.
- UIKit recognizers may only act as diagnostic/input-adapter fallbacks. They must not own product gesture state, transforms, physics, or rendering.
- Shared gesture comprehension belongs in `oxide-input`.
- Preserve raw event access for pinch, pan, drag, stylus, hover, keyboard, and IME paths.
- Coalesce move events only where latency and gesture correctness permit.
- Measure event-to-visible-response latency and hitch ratio for scroll, animation, gesture, and text input flows.
- Verify real host path with OS-level gestures or device/manual evidence, not only unit-state tests.

### Accessibility and IME

- Platform accessibility is part of the UI contract, not a post-process.
- Maintain stable accessibility node IDs linked to UI node IDs.
- Update accessibility frames on layout/scale changes.
- Activation should route into Rust callbacks.
- Preserve focused node across layout when geometry remains compatible.
- Model IME composition/commit/selection uniformly across languages.
- Do not optimize away marked text, candidate-bar semantics, selection geometry, or VoiceOver traversal correctness.

## Metal backend rules for Oxide

### iOS host and surface

- `CAMetalLayer` owns presentation; `MetalView` is the host surface, not a UIKit product view tree.
- `contentScaleFactor` and `drawableSize` must mirror device scale and bounds.
- Use `maximumDrawableCount = 3` when available to support triple buffering.
- Acquire `nextDrawable` late, after update/prepass work and immediately before final render pass setup where possible.
- Present from the main frame command buffer.
- Use `presentsWithTransaction = NO` for low-latency rendering unless a transaction path is explicitly required.
- Use `framebufferOnly = YES` for normal drawable rendering unless sampling/readback from drawable is required.
- Recreate/resize resources on bounds/scale/EDR/MSAA changes.

### Command buffers and encoders

- Prefer one command buffer per frame.
- If multiple command buffers are used, attach completion/recycle logic to the last one and order them explicitly.
- Use prepass/blur/blit/atlas upload and main render pass in the same command buffer where possible.
- Use `MTLParallelRenderCommandEncoder` for CPU-side parallel main-pass encoding; do not share one encoder across threads.
- Partition work by state/texture/clip/layer ranges; reserve non-overlapping ring slices per worker.
- Do not call `waitUntilCompleted()` in the frame loop.
- Label command buffers, encoders, resources, passes, and pipelines in profiler builds.

### Load/store and pass planning

Choose load/store actions deliberately:

- `DontCare` load when every pixel will be overwritten.
- `Clear` load when old contents are not needed and not every pixel is overwritten.
- `Load` only when previous contents are required.
- `DontCare` store for temporary attachments not read later.
- `Store` only for drawables/displayable targets or attachments read later.
- Resolve MSAA without storing the multisample texture when only the resolved target is needed.
- Merge compatible render encoders to keep tile data on chip.
- Do not preserve or resolve attachments whose contents are not consumed.

### Storage modes and resource lifetime

- Use `Shared` for CPU-written dynamic buffers on iOS.
- Use `Private` for GPU-only textures/assets after upload.
- Use `Memoryless` for temporary render targets stored only in on-chip tile memory.
- Declare explicit texture usage flags; avoid `Unknown` unless necessary.
- Build resources up front; no per-frame pipeline, shader, texture, sampler, command queue, depth state, or heap churn.
- Use ring buffers for vertices, indices, uniforms, glyph data, per-draw params, and staging.
- Triple-buffer dynamic data; never write into a slice still used by in-flight GPU work.
- If using `HazardTrackingModeUntracked`, prove non-overlap and unique per-frame slices. Validation builds may use tracked mode.

### Pipelines, shaders, and binary archives

- Compile Metal shaders via build tooling; fail builds if required metallib is missing.
- Create PSOs before interaction; prewarm on launch or controlled warmup.
- Use `MTLBinaryArchive` keyed by device, OS, features, shader schema, EDR/MSAA toggles, and pipeline version when available.
- Keep shader variants bounded. Prefer feature constants only for branches that pay back shader cost without variant explosion.
- Use explicit shader struct layout; verify Rust `#[repr(C)]`/packing against Metal structs.
- Avoid `set*Bytes` over small API limits; use buffers/rings for larger per-draw payloads.

### EDR/HDR, color, MSAA

- Default normal UI to BGRA8 sRGB output unless EDR is enabled.
- Use RGBA16F/linear Display-P3 for EDR/HDR paths when supported and requested.
- Keep CPU colors in linear floats; convert deliberately at output boundaries.
- Use premultiplied alpha as the default compositing convention.
- Make `msaa-4x` opt-in and validate store/resolve behavior.
- Do not enable MSAA for paths where analytic antialiasing or SDF/text coverage is cheaper and equivalent.

### Effects and UI shaders

- Keep common UI shaders compact: solid, image, glyph, SDF/MSDF, rounded rect, nine-slice, spinner, blur/backdrop, camera, Scene3D.
- Prefer analytic rounded-rect shaders over tessellation or offscreen layers for simple cards/chips/buttons.
- Use separable blur and downsample chains for backdrop effects; avoid full-resolution Gaussian blur unless needed.
- Cache blurred/static layers only with valid dirty keys.
- Avoid full-screen passes for small local effects.
- Keep visual effect semantics in `renderer-api`; keep sigma/downsample/planning details backend-owned.

### GPU timings and instrumentation

- First source of truth: in-app Metal timings from command-buffer `GPUStartTime`/`GPUEndTime` where available.
- Use counter sample buffers for pass/stage attribution when supported.
- Cross-check with Metal System Trace and Xcode GPU Frame Capture.
- Do not block basic GPU reporting because hardware-counter Instruments profiles are unavailable.
- Persist p50/p95/p99/peak and relevant internal counters.

## Game-engine lessons that apply to Oxide

Oxide should adopt these game-engine practices:

- explicit frame graph / pass graph;
- clear separation between frontend semantics and backend execution;
- transient resource allocation and aliasing;
- pipeline/material/shader cache and warmup;
- immutable frame snapshots;
- frame-local arenas/ring buffers;
- batched render-item sorting;
- multithreaded command generation;
- GPU/CPU profiler labels everywhere;
- performance budgets and CI gating;
- device baselines;
- capture/replay or snapshot tests;
- quality ladders for expensive effects;
- deterministic present pacing;
- content budgets for draws, passes, textures, layers, glyphs, and offscreen targets.

Oxide should not blindly copy these game-engine patterns:

- desktop-first deferred renderers with large G-buffers for ordinary UI;
- complex material graphs for simple controls;
- ECS traversal in the UI hot path when stable UI IDs and dirty lists are better;
- full-scene rebuilds every frame when idle;
- shader permutation explosions;
- always-on post-processing stacks;
- editor/debug UI assumptions that ignore accessibility, IME, localization, or app-platform conventions;
- opaque renderer abstractions that hide load/store, storage modes, swapchain/drawable policy, or pass dependencies.

The boundary: **use game-engine backend discipline with UI-toolkit semantics.**

## General GPU/backend architecture wisdom

These rules apply to Metal, Vulkan, D3D12, WebGPU, OpenGL, software renderers, and abstraction layers.

### Backend layers

A strong renderer backend has layers like this:

1. Scene/UI extraction into immutable render data.
2. Visibility, dirty-region, and scheduling.
3. Display list / draw list.
4. Render item generation and batching.
5. Frame graph / render graph.
6. Resource system: persistent assets, transient resources, heaps/pools, staging uploads.
7. Pipeline/shader system: variants, reflection, binary/cache, warmup.
8. Binding system: argument-buffer/descriptor-table/bind-group abstraction.
9. Command generation: single-threaded or parallel encoding.
10. Presentation and frame pacing.
11. Instrumentation and regression reporting.

### Good abstraction boundaries

Expose enough backend information to be fast:

- load/store actions;
- storage modes/resource usage;
- transient/memoryless attachments;
- render pass boundaries;
- dependency edges;
- synchronization/barriers where needed;
- bind group/descriptor/argument-buffer models;
- indirect draw/dispatch support;
- pipeline caches;
- shader feature/variant keys;
- debug labels;
- timing/counter APIs;
- swapchain/drawable/present modes.

Bad abstraction: `draw(objects)` with hidden render targets, hidden clears, hidden resource creation, hidden uploads, and hidden synchronization.

Good abstraction: semantic UI draw list + explicit backend lowering + backend-specific fast paths.

### Render graph policy

Use a render graph even for UI when effects, layers, camera, or Scene3D are present.

A UI render graph should model:

- final drawable pass;
- retained layer textures;
- dirty layer re-rendering;
- backdrop source pass;
- blur/downsample/upsample passes;
- text/glyph atlas uploads;
- image/camera texture imports;
- embedded 3D scene pass;
- final composite;
- readbacks for snapshots/tests only.

Each pass declares:

- name;
- dimensions and scale;
- pixel format and sample count;
- attachments;
- inputs/outputs;
- load/store/resolve actions;
- storage mode;
- clear values;
- queue/encoder type;
- transient/lifetime range;
- debug label;
- metric collection hooks.

### Synchronization policy

- Prefer immutable snapshots and frame-local append-only data.
- Avoid CPU waiting for GPU.
- Avoid GPU waiting for CPU data that could be prepared earlier.
- Use frame fences/semaphores/in-flight limits to protect dynamic buffers.
- Delay readbacks several frames when possible.
- Use barriers/events only for real dependencies.
- Do not over-synchronize with full-pipeline barriers where narrower scope is valid.

### Caching policy

Cache only with a key, a budget, and invalidation.

Good cache keys:

- text: font, size, features, script, bidi level, string, width, DPI scale, fallback chain;
- layout: node style, constraints, content size, child keys, safe-area metrics;
- paint: style, geometry, transform class, effect policy, DPI scale;
- vector: path, fill rule, stroke style, tolerance, transform class;
- image: source bytes, decode size, color space, alpha policy, mip policy;
- layer: layer id, bounds, scale, effect stack, child dirty generation;
- pipeline: shader, function constants, blend, formats, sample count, vertex layout;
- atlas: glyph/image identity, scale, format, LRU generation.

Every cache reports hit rate, misses, evictions, memory use, rebuild time, invalidation cause, and quality policy.

## UI/2D renderer sources to study

Study these projects for architecture, not for copying APIs:

| Project | Extract for Oxide |
|---|---|
| Skia/Graphite | retained 2D batching, resource caches, backend task graph, text/vector/image depth. |
| Vello | GPU-compute-centric vector 2D, scene encoding, prefix-sum/parallelization ideas, GPU-heavy renderer tradeoffs. |
| Flutter Impeller | predictable frame time, shader/pipeline warmup, UI-focused renderer ownership. |
| Rive Renderer | high-volume vector animation, animation/render separation, asset-driven motion. |
| Iced | Rust GUI architecture, message/update/view separation, backend modularity. |
| egui | immediate-mode repaint scheduling, ID model, paint output, debug tooling. |
| Xilem/Masonry | retained/reactive Rust UI, widget tree updates, integration with Vello/text/accessibility stack. |
| Taffy | layout engine isolation, flex/grid determinism, cacheable layout inputs. |
| Dear ImGui | draw-command lists, atlas model, renderer backend contract, tool UI performance. |
| Nuklear | explicit memory, small renderer contract, portable immediate UI. |
| AccessKit | custom-rendered UI accessibility tree and platform adapters. |

## Engine/backend sources to study

| Source | Extract for Oxide |
|---|---|
| Filament | mobile-first PBR, frame graph, material variants, backend abstraction, clustered/forward ideas. |
| bgfx | cross-platform renderer boundaries, transient buffers, state sorting, “bring your own engine” API design. |
| The Forge | explicit API renderer layout, resource loaders, GPU-driven samples, cross-platform backend discipline. |
| IGL | thin cross-platform GPU interface with native backend lowering. |
| wgpu | modern portable bind-group/resource validation model over Metal/Vulkan/D3D/WebGPU. |
| Godot | split desktop/mobile renderers, feature tiers, renderer architecture, portability tradeoffs. |
| Unity/Unreal docs | production mobile budgets, material/draw-call limits, memoryless render targets, profiler workflows. |
| MetalPetal/GPUImage3/BBMetalImage | image/video filter graphs, texture reuse, camera/video GPU-only pipelines. |
| Cocos2d-x | sprite batching, scene graph tradeoffs, draw-order batching constraints. |
| Satin/MetalSplatter | native Swift/Metal 3D patterns and specialized high-throughput renderer examples. |

## Optimization playbooks

### If ui-core CPU is the bottleneck

1. Split timing by event routing, layout, diff, text measure, draw-list build, and scene/router update.
2. Add dirty reasons and counters.
3. Avoid full-tree layout/paint for local state changes.
4. Virtualize lists/grids/chat.
5. Cache shaped text and stable draw-list spans.
6. Remove heap allocation, string formatting, logs, lock contention, and dynamic dispatch from hot paths.
7. Add or update a perf case in `oxide-perf-runner`.

### If renderer-metal CPU submission is the bottleneck

1. Count draw commands, lowered batches, PSO switches, texture switches, clips, layers, and bytes encoded.
2. Batch by PSO/texture/clip where order permits.
3. Use instancing or span-based large vertex/index buffers for repeated UI primitives.
4. Avoid per-command Metal object calls when offsets/ranges suffice.
5. Use parallel render encoding only after main-thread/encoding cost is proven significant.
6. Avoid ICB paths until they beat direct draws and are stable on device/simulator matrices.
7. Keep `set*Bytes` below small payload thresholds; use ring buffers for larger data.

### If GPU bandwidth is the bottleneck

1. Audit attachments: format, size, MSAA, load/store, storage mode, usage flags.
2. Replace stored temporary attachments with `DontCare`/memoryless where legal.
3. Merge compatible passes.
4. Downsample blur/backdrop/effects.
5. Reduce overdraw and large translucent surfaces.
6. Shrink damage or dirty layers when cache correctness is proven.
7. Use private textures and staged uploads.
8. Track render target bytes loaded/stored and texture upload bytes.

### If shader/fragment cost is the bottleneck

1. Capture shader cost and fragment/vertex stage timing.
2. Reduce overdraw first.
3. Simplify expensive effects and large-radius blurs.
4. Use `half` where quality permits.
5. Remove divergent branches and precision waste.
6. Split variants only when function constants prevent measurable hot-path cost.
7. Avoid full-resolution intermediate passes.
8. Recheck visual parity with snapshots.

### If frame pacing is the bottleneck

1. Inspect CADisplayLink timing, drawable acquisition, command-buffer commit, present, and completion handlers.
2. Keep one command buffer per frame unless a measured reason exists.
3. Acquire drawable late.
4. Keep frames in flight bounded.
5. Remove runtime PSO/resource creation during visible interaction.
6. Use refresh-tier adaptation only with visible-quality and latency policy.
7. Measure p95/p99 and hitch ratio, not just average frame time.

### If idle/battery is the bottleneck

1. Stop display link or suppress frame submission when no damage, animation, camera, timer, or external invalidation exists.
2. Do not poll OS state every frame.
3. Stop optional background animations under Low Power Mode.
4. Track CPU wakeups, GPU submissions, and idle animation loops.
5. Treat battery as an explicit benchmark group.

### If camera preview performance is the bottleneck

1. Separate acquisition, texture bridging, renderer fetch, encode, draw, present, and host tick overhead.
2. Measure frame age at presentation.
3. Keep pure-custom Oxide path separate from system preview-layer diagnostics.
4. Avoid CPU color conversion or copies.
5. Use optimized NV12 shader path and GPU compositing where possible.
6. Track backpressure, in-flight submissions, stale frames, and dropped frames.

## Code-review checklist

### Oxide boundary checks

- Is UI behavior in Rust, not UIKit recognizers or Objective-C scene state?
- Does `ui-core` remain independent of `platform-ios` and `renderer-metal`?
- Does `renderer-metal` depend only on renderer abstractions and host/device bridges it actually needs?
- Are app state, layout, draw-list, render-list, and backend commands separated?
- Are stable IDs preserved for layout, input, accessibility, text, animation, and layers?
- Does the change add a benchmark/snapshot/device report when user-visible or hot-path?

### Hot-path red flags

- Per-frame PSO, shader, texture, sampler, command queue, heap, or depth/stencil state creation.
- Per-frame `Vec` growth, string formatting, logs, locks, Objective-C calls, or heap allocation in render hot paths.
- `waitUntilCompleted()` in normal frame loop.
- Drawable acquired at frame start.
- Full-tree layout/paint/draw-list build for local changes.
- Text shaping or image decoding during draw encoding.
- Unbounded layer cache, glyph atlas, image atlas, or retained texture cache.
- `Load`/`Store` defaults on temporary render targets.
- Depth/stencil/MSAA store when result is never read.
- Full-resolution blur/backdrop for small local effect.
- System preview/UI path counted as Oxide renderer win.
- Simulator performance treated as official device result.

### Metal-specific checks

- Are storage modes correct for iOS and macOS hosts?
- Are texture usage flags explicit?
- Are dynamic buffers triple-buffered with non-overlapping in-flight slices?
- Are ring buffers aligned and capacity growth bounded?
- Are atlas uploads before the main pass and visible without unnecessary fences?
- Are prepass/main pass ordered on the same command buffer where possible?
- If multiple queues/command buffers are used, is ordering explicit with events/fences?
- Are pipeline states/binary archives warmed before interaction?
- Are GPU timing and stage sample paths behind robust capability checks?
- Are validation/simulator fallbacks deterministic?

### Benchmark checks

- Does the benchmark match the real user path?
- Are Oxide and UIKit cases apples-to-apples: same strings, fonts, colors, radii, effects, image bytes, geometry, animation curves, cache state, refresh mode, and interaction path?
- Are p50/p95/p99/peak persisted?
- Are hitch/missed-frame/input-latency metrics present for scroll/animation/input flows?
- Are cold/warm/hot cache states separated?
- Are device reports used for official comparisons?
- Are unresolved gaps marked partial/missing instead of over-claiming completeness?

## Practical test commands and checks

Use focused commands that match the touched area. Do not run automated formatting/clippy.

Examples:

```bash
cargo test -p oxide-input
cargo test -p oxide-ui-core
cargo test -p oxide-renderer-metal --features snapshot-tests
cargo test --locked -j12 -p oxide-input -p oxide-test-scenes -p oxide-renderer-metal -p oxide-perf-runner -p xtask -p oxide-platform-ios -p oxide-host-ios --all-targets
git diff --check
```

For performance changes, update the appropriate persisted report only when the benchmark was intentionally rerun and reviewed.

## Review output shape

When responding to an Oxide performance/design task, structure the answer like this:

```text
Layer:
- ui-core / renderer-api / renderer-metal / platform-ios / text / input / perf-runner / host

Bottleneck hypothesis:
- Class, affected phase, target workload, expected counter movement, risk.

Findings:
- Concrete code paths or design issues.

Fixes, ranked:
1. Highest-impact safe change.
2. Riskier architecture change.
3. Cleanup after measurement.

Bench/snapshot/device proof:
- Exact case or new case to add.
- Required counters and reports.

Regression risk:
- Visual parity, input/IME/a11y, device/simulator, cache state, thermal.
```

## Research basis and durable links

### Oxide

- Repository: https://github.com/victorstewart/oxide
- Workspace README: https://github.com/victorstewart/oxide/blob/main/oxide/README.md
- Spec: https://github.com/victorstewart/oxide/blob/main/spec.xml
- Repo rules: https://github.com/victorstewart/oxide/blob/main/AGENTS.md
- Renderer API: https://github.com/victorstewart/oxide/blob/main/oxide/crates/renderer-api/src/lib.rs
- Metal renderer: https://github.com/victorstewart/oxide/blob/main/oxide/crates/renderer-metal/src/lib.rs
- Renderer Metal shaders: https://github.com/victorstewart/oxide/tree/main/oxide/crates/renderer-metal/shaders
- Workspace benchmarks: https://github.com/victorstewart/oxide/tree/main/oxide/benchmarks/workspace

### Apple Metal and profiling

- Metal: https://developer.apple.com/metal/
- Metal documentation: https://developer.apple.com/documentation/metal
- Metal Best Practices Guide: https://developer.apple.com/library/archive/documentation/3DDrawing/Conceptual/MTLBestPracticesGuide/
- Resource options: https://developer.apple.com/library/archive/documentation/3DDrawing/Conceptual/MTLBestPracticesGuide/ResourceOptions.html
- Triple buffering: https://developer.apple.com/library/archive/documentation/3DDrawing/Conceptual/MTLBestPracticesGuide/TripleBuffering.html
- Load and store actions: https://developer.apple.com/library/archive/documentation/3DDrawing/Conceptual/MTLBestPracticesGuide/LoadandStoreActions.html
- Command buffers: https://developer.apple.com/library/archive/documentation/3DDrawing/Conceptual/MTLBestPracticesGuide/CommandBuffers.html
- Argument buffers: https://developer.apple.com/documentation/metal/buffers/using_argument_buffers_to_streamline_resource_management
- Heaps: https://developer.apple.com/documentation/metal/placing_resources_in_heaps
- Indirect command buffers: https://developer.apple.com/documentation/metal/encoding_indirect_command_buffers_on_the_gpu
- Metal binary archives: https://developer.apple.com/documentation/metal/mtlbinaryarchive
- Metal Performance HUD: https://developer.apple.com/documentation/metal/displaying-performance-statistics-using-the-metal-performance-hud
- Metal debugger: https://developer.apple.com/documentation/xcode/metal-debugger
- Metal shader profiler: https://developer.apple.com/documentation/xcode/profiling-the-shaders-in-your-metal-app

### UI and 2D renderer references

- Skia: https://github.com/google/skia
- Skia Graphite: https://skia.org/docs/user/graphite/
- Vello: https://github.com/linebender/vello
- Flutter Impeller: https://docs.flutter.dev/perf/impeller
- Flutter engine Impeller source: https://github.com/flutter/engine/tree/main/impeller
- Rive renderer: https://rive.app/blog/rive-renderer-now-open-source-and-available-on-all-platforms
- Iced: https://github.com/iced-rs/iced
- egui: https://github.com/emilk/egui
- Xilem: https://github.com/linebender/xilem
- Taffy: https://github.com/DioxusLabs/taffy
- Dear ImGui: https://github.com/ocornut/imgui
- Nuklear: https://github.com/Immediate-Mode-UI/Nuklear
- AccessKit: https://github.com/AccessKit/accesskit
- MetalPetal: https://github.com/MetalPetal/MetalPetal
- GPUImage3: https://github.com/BradLarson/GPUImage3
- BBMetalImage: https://github.com/Silence-GitHub/BBMetalImage

### Game engines and backend architecture

- Filament: https://github.com/google/filament
- bgfx: https://github.com/bkaradzic/bgfx
- The Forge: https://github.com/ConfettiFX/The-Forge
- IGL: https://github.com/facebook/igl
- wgpu: https://github.com/gfx-rs/wgpu
- Godot renderers: https://docs.godotengine.org/en/stable/tutorials/rendering/renderers.html
- Godot engine: https://github.com/godotengine/godot
- Unity Metal optimization: https://docs.unity3d.com/Manual/metal-optimize.html
- Unreal mobile optimization: https://dev.epicgames.com/documentation/en-us/unreal-engine/optimization-and-development-best-practices-for-mobile-projects-in-unreal-engine
- Cocos2d-x: https://github.com/cocos2d/cocos2d-x
- Satin: https://github.com/Hi-Rez/Satin
- MetalSplatter: https://github.com/scier/MetalSplatter
- Learn Metal C++ iOS: https://github.com/metal-by-example/learn-metal-cpp-ios

### General GPU architecture

- NVIDIA Vulkan Do’s and Don’ts: https://developer.nvidia.com/blog/vulkan-dos-donts/
- AMD Vulkan barriers explained: https://gpuopen.com/learn/vulkan-barriers-explained/
- AMD Vulkan render passes: https://gpuopen.com/learn/vulkan-renderpasses/
- Arm Vulkan mobile best-practice samples: https://github.com/ARM-software/vulkan_best_practice_for_mobile_developers
- Vulkan synchronization examples: https://github.com/KhronosGroup/Vulkan-Docs/wiki/Synchronization-Examples
- Zeux efficient Vulkan renderer: https://zeux.io/2020/02/27/writing-an-efficient-vulkan-renderer/
- Zeux three years of Metal: https://zeux.io/2020/01/12/three-years-of-metal/

## Standing answer bias

For Oxide, prefer:

- Rust-owned UI semantics over platform-owned widget behavior.
- Retained data and invalidation over blind per-frame rebuilds.
- Draw-list semantics over widget-calls-Metal.
- Explicit Metal fast paths over lowest-common-denominator portability.
- Measured performance contracts over intuition.
- Device baselines over simulator baselines.
- Visual parity over benchmark cheating.
- Input/IME/a11y correctness over raw draw throughput.
- Battery/thermal discipline over short benchmark wins.


## Skill routing inside Oxide

Use `$oxide-internals-performance` for internal runtime, renderer, text, input, host, benchmark, snapshot, and backend work.

Use `$oxide-app-authoring-performance` for apps, demos, examples, and docs that use Oxide as a library without changing internal runtime behavior.

When a task starts in app code but exposes a renderer/runtime defect, switch to the internals skill before editing Oxide internals. Keep the final answer explicit about the boundary crossed.
