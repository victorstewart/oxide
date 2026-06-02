# Oxide Device Performance Report

- Suite: `oxide-device`
- Label: `2026-04-05`
- Coverage: 0/0 components, 3/3 animations, 0/0 launch cases, 0/0 primitive lifecycle cases, 0/0 CPU scenes, 1/1 GPU scenes, 4/4 journeys, 0/0 authoring APIs, 1/1 image pipeline cases, 2/2 navigation cases, 0/0 reconcile cases, 0/0 bridge paths

## Contract Coverage

| Section | Status | Notes |
| --- | --- | --- |
| `Oxide On-Screen Host Battery` | `partial` | This stale baseline is captured through the real on-screen Oxide MetalView host path, but it predates the required memory and frame-cadence metric contract. |
| `Workspace Engine Battery` | `separate` | The broader offscreen engine and microbenchmark suite remains in benchmarks/workspace and is intentionally not mixed into this device comparison report. |
| `Animation & Visual Effects` | `partial` | The stale baseline carries representative Oxide on-screen animation workloads through the live host path, but lacks required hitch and missed-frame distributions. |
| `Navigation & Input Latency` | `partial` | The stale baseline carries direct Oxide button-press and text-focus response workloads through the live host path, but lacks required hitch and missed-frame distributions. |
| `Representative Journeys` | `partial` | The stale baseline carries representative Oxide journey workloads through the live host path, but lacks required hitch and missed-frame distributions. |
| `Camera Preview` | `partial` | The stale baseline uses the real on-screen Oxide preview path with Oxide owning the visible preview on the phone, but lacks required memory and frame-cadence metrics. |

- Device: `Victor’s iPhone`
- Executable: `OxideHost`
- Device flow: launch the parked host app on the physical iPhone with a live on-screen Oxide workload selected, collect workload and memory summaries from the app console, and collect direct GPU/signpost metrics from a process-scoped launched Metal System Trace when tracing is enabled.
- Metric contract status: stale partial. This 2026-04-05 baseline predates required memory and frame-cadence distributions; rerun the physical-device baseline before using it as a complete official comparison.
- Comparison scope: only on-screen Oxide host cases are persisted here. Offscreen Rust workspace numbers remain separate and are not part of the official device comparison.

## Results

| Case | Layer | Scenario | Variant | Cache | Refresh | P50 | P95 | P99 | Peak | Unit | Gate | Key Metrics |
| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| `gpu.scene.camera.frame` | `onscreen` | `camera_preview` | `oxide_custom_camera_preview` | `warm` | `native` | 7.077 | 7.077 | 7.077 | 7.077 | s | regression-gated | `clock_s=7.077; gpu_latency_s=0.001; gpu_time_s=0.121; workload_s=7.077` |
| `cpu.navigation.button_press.response` | `onscreen` | `button_press_response` | `oxide_host` | `warm` | `native` | 0.017 | 0.025 | 0.025 | 0.025 | s | regression-gated | `clock_s=0.017; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.005` |
| `cpu.navigation.text_focus.response` | `onscreen` | `text_focus_response` | `oxide_host` | `warm` | `native` | 0.017 | 0.025 | 0.025 | 0.025 | s | regression-gated | `clock_s=0.017; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.004` |
| `cpu.animation.spinner_spin` | `onscreen` | `spinner_spin` | `oxide_host` | `warm` | `native` | 0.017 | 0.017 | 0.025 | 0.025 | s | regression-gated | `clock_s=0.017; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.014` |
| `cpu.animation.image_zoom_pan` | `onscreen` | `image_zoom_pan` | `oxide_host` | `warm` | `native` | 0.017 | 0.017 | 0.022 | 0.025 | s | regression-gated | `clock_s=0.017; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.007` |
| `cpu.animation.anim_timeline_bars` | `onscreen` | `anim_timeline_bars` | `oxide_host` | `warm` | `native` | 0.000 | 0.000 | 0.001 | 0.001 | s | regression-gated | `clock_s=0.000; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.004` |
| `cpu.journey.input_form_submit` | `onscreen` | `input_form_submit` | `oxide_host` | `warm` | `native` | 0.017 | 0.025 | 0.025 | 0.025 | s | regression-gated | `clock_s=0.017; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.004` |
| `cpu.journey.collection_navigation` | `onscreen` | `collection_navigation` | `oxide_host` | `warm` | `native` | 0.017 | 0.025 | 0.025 | 0.025 | s | regression-gated | `clock_s=0.017; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.003` |
| `cpu.journey.zoom_image_gesture_cycle` | `onscreen` | `zoom_image_gesture_cycle` | `oxide_host` | `warm` | `native` | 0.017 | 0.025 | 0.025 | 0.025 | s | regression-gated | `clock_s=0.017; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.003` |
| `cpu.journey.orchestration_transition_modal` | `onscreen` | `orchestration_transition_modal` | `oxide_host` | `warm` | `native` | 0.017 | 0.025 | 0.025 | 0.025 | s | regression-gated | `clock_s=0.017; gpu_counter.rt_unit_active=0.000; gpu_latency_s=0.001; gpu_time_s=0.003` |

## Findings

- [info] This device report measures the live on-screen Oxide host path rather than the offscreen Rust perf runner, so it is the authoritative Oxide side of the official device comparison.

## Baseline Workflow

- Update the committed baseline only with review: `PERF_REPORT_DATE=$(date +%F) cargo run --locked -j$(sysctl -n hw.ncpu) -p xtask -- ios oxide-device-perf --write-baseline`
- Latest JSON baseline: `benchmarks/oxide-device/latest.json`
- Latest Markdown baseline: `benchmarks/oxide-device/latest.md`
