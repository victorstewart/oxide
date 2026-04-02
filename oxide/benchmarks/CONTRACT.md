# Oxide/UIKit Perf Contract

## Scope

This benchmark suite exists to support one claim only: on identical pixels, geometry, timings, and interactions, compare Oxide against UIKit honestly.

The contract is Oxide/UIKit-native. External or legacy apps must not appear in the persisted policy, report language, or benchmark definitions.

## Layers

- `engine`: microbenchmarks for primitives, animations, author-facing APIs, layout hot paths, and renderer/runtime internals.
- `flow`: representative user-visible screens and journeys that reflect what a user sees, feels, and waits on.
- `os-bridge`: app-owned wrapper overhead around permissions, pickers, import/export, networking, and other native bridges.

## Required Battery

Each platform report should classify coverage against these workload families:

1. `launch-lifecycle`
2. `primitive-lifecycle`
3. `layout-invalidation`
4. `text-input`
5. `image-pipeline`
6. `lists-grids-chat`
7. `navigation-input`
8. `animation-effects`
9. `state-reconcile`
10. `os-bridge`
11. `endurance-thermal`
12. `stress-pathological`

If a family is not yet implemented, the report must say `missing` or `partial`.

## UIKit Baselines

UIKit parity must maintain two styles for the same workload family:

- `idiomatic`: what a normal UIKit implementation looks like.
- `optimized`: a hand-tuned UIKit implementation that raises the bar for Oxide.

If only one style exists for a family, the report must mark style coverage as partial.

## Shared Scene Spec

Oxide and UIKit cases must share the same:

- strings and fonts
- colors, borders, shadows, and corner radii
- image bytes
- geometry and spacing
- animation curves and durations
- scroll physics and interaction cadence
- visible effects and quality level

Faster is invalid if it silently renders fewer pixels, weaker effects, or lower-quality text.

## Shared Phases

Use the same phase names whenever the workload exposes them:

- `app.launch`
- `screen.mount`
- `layout`
- `text.measure`
- `diff.apply`
- `image.decode`
- `texture.upload`
- `draw.encode`
- `frame.present`
- `first.interactive`
- `transition`
- `scroll`
- `native.bridge`

## Metric Surface

Each persisted report row should move toward this shape:

- `test_id`
- `device`
- `refresh_mode`
- `variant` or `style`
- `cache_state`
- `measure_iterations`
- `benchmark_iterations`
- `p50_ms`
- `p95_ms`
- `p99_ms`
- `peak_ms`
- `hitch_ms_per_s`
- `missed_frames`
- `first_frame_ms`
- `first_interactive_ms`
- `cpu_pct`
- `main_thread_pct`
- `peak_rss_mb`
- `steady_rss_mb`
- `gpu_mem_mb`
- `logical_writes_kb`
- `energy_score`

Image and transport workloads should also split:

- `network_ms`
- `decode_ms`
- `upload_ms`
- `render_ms`

Oxide-owned reports should additionally preserve explanatory counters when available:

- `dirty_nodes`
- `layout_passes`
- `draw_calls`
- `encoded_bytes`
- `texture_bytes`

When a persisted metric is derived from multiple potential collectors, the report should also preserve:

- canonical metric source
- per-metric provenance
- structured fallback modes when trace reduction had to widen attribution

## Device Rules

- Keep `cold`, `warm`, and `hot` cache states separate.
- Run real user-visible scroll and animation flows on real devices, including at least one 60 Hz phone and one ProMotion phone.
- Treat simulator probes as debug-only and never as committed baselines, official comparison numbers, or user-facing summary data.
- Camera preview has two reporting buckets:
  - the official today bucket is the parked microscope comparison between the pure custom Oxide-owned visible preview path and the matching `AVCaptureVideoPreviewLayer` baseline on the same unchanged build and device
  - the actual app-host comparison is a separate shipping-oriented bucket and must be reported as partial or blocked until the UI-test runner path is stable
- Hybrid camera visible-preview-layer paths are diagnostic-only. They must never appear in committed default baselines or user-facing summary tables.
- Treat real-device process-scoped Metal System Trace as the authoritative GPU source.
- Treat manual device-side Power Profiler traces as the authoritative energy source when available.

## Reporting Rules

- Prefer hitch ratio, first frame, first interactive, and input-event-to-visible-response over headline FPS.
- Report partial coverage truthfully instead of implying the battery is already comprehensive.
- Keep app-owned UI separate from system-owned UI; system surfaces count as bridge overhead, not renderer wins.
