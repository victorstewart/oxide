# Oxide Device Performance Report

- Suite: `oxide-device`
- Label: `2026-08-09`
- Repository ref: `refs/heads/agent/oxide-final-clean-candidate`
- Repository HEAD: `add4de82722b239a4dc196bf4e20f76c7974e3c3`
- Repository tree: `d68e766d5016f32d29c2afa9da2bb3a109dea860`
- Cases: `5`

## Contract Coverage

| Section | Status | Notes |
| --- | --- | --- |
| `Oxide On-Screen Host Battery` | `implemented` | This device report is captured through the real on-screen Oxide MetalView host path instead of the offscreen Rust perf runner. |
| `Workspace Engine Battery` | `separate` | The broader offscreen engine and microbenchmark suite remains in benchmarks/workspace and is intentionally not mixed into this device comparison report. |
| `Launch & Lifecycle` | `missing` | The on-screen device battery does not yet persist launch, resume, or foreground lifecycle rows; those remain covered by the separate workspace battery. |
| `Primitive Mount / Update / Destroy` | `partial` | Selected component-encode rows provide a partial primitive signal, but the report does not include a mount/update/destroy lifecycle matrix on physical hardware. |
| `Layout & Invalidation` | `missing` | Dedicated physical-device layout and invalidation rows are not yet part of this report. |
| `Text & Text Input` | `missing` | This report contains no matched physical-device text-input row; focus and form cases remain explicit-only. |
| `Image Pipeline` | `partial` | The selected custom-camera preview provides a partial image-pipeline signal; decode, upload, and first-visible rows remain outside this physical-device report. |
| `Lists, Grids, & Chat` | `partial` | Selected collection encode or navigation rows provide a partial list signal, but the report does not include the full feed, grid, and chat scroll matrix. |
| `Navigation & Input Latency` | `partial` | Selected matched navigation/input-response rows run through the live Oxide host path. |
| `Animation & Visual Effects` | `partial` | Selected animation rows provide a partial signal, but the broader effect and interaction matrix is not present. |
| `State Mutation & Reconciliation` | `missing` | Single-node, percentage, and full-theme reconciliation rows are not yet captured in the physical-device Oxide report. |
| `OS Bridge Overhead` | `missing` | Permission, sensor, import, share, and localhost bridge rows are not yet captured in the physical-device Oxide report. |
| `Endurance, Memory, & Thermal Drift` | `missing` | Open/close, tab-switch, idle-animation endurance, and thermal drift rows are not yet captured in the physical-device Oxide report. |
| `Stress & Pathological Regressions` | `missing` | This report contains no physical-device stress or pathological-regression row. |
| `Representative Journeys` | `partial` | Selected Oxide journeys run through the live MetalView host path; this supplemental bucket is separate from the required workload-family rows above. |
| `Renderer Scene GPU Paths` | `missing` | This report contains no dedicated damage, static-idle, or nine-slice renderer-scene row. |
| `Camera Preview` | `implemented` | The selected custom-camera row uses the real on-screen Oxide preview path with Oxide owning the visible preview on the phone. |

- Device: `Victor’s iPhone`
- Executable: `OxideHost`
- Device flow: launch the parked host app on the physical iPhone with a live on-screen Oxide workload selected, collect workload and memory summaries from the app console, and collect direct GPU/signpost metrics from a process-scoped launched Metal System Trace when tracing is enabled.
- Fairness scope: headline rows use visible workload, transition, interaction, or present signposts; app-process CPU and memory are attribution metrics, not a claim that all iOS framework or compositor work is charged to the app process.
- Comparison scope: only on-screen Oxide host cases are persisted here. Offscreen Rust workspace numbers remain separate and are not part of the official device comparison.

## Results

| Case | Layer | Scenario | Variant | Cache | Refresh | P50 | P95 | P99 | Peak | Unit | Gate | Key Metrics |
| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| `cpu.component.collection_view.encode` | `onscreen` | `collection_view_encode` | `oxide_host` | `warm` | `native` | 0.000 | 0.000 | 0.000 | 0.000 | s | regression-gated | `hitch_ms_per_s=492.597; missed_frames=46.000; missed_frames_per_s=28.018; frame_interval_ms_p50=16.662; frame_interval_ms_p95=16.834; clock_s=0.000` |
| `cpu.animation.spinner_spin` | `onscreen` | `spinner_spin` | `oxide_host` | `warm` | `native` | 0.017 | 0.017 | 0.017 | 0.017 | s | regression-gated | `hitch_ms_per_s=496.831; missed_frames=187.000; missed_frames_per_s=29.102; frame_interval_ms_p50=16.666; frame_interval_ms_p95=16.791; clock_s=0.017` |
| `cpu.navigation.button_press.response` | `onscreen` | `button_press_response` | `oxide_host` | `warm` | `native` | 0.017 | 0.017 | 0.017 | 0.017 | s | regression-gated | `hitch_ms_per_s=496.183; missed_frames=58.000; missed_frames_per_s=26.562; frame_interval_ms_p50=16.668; frame_interval_ms_p95=16.668; clock_s=0.017` |
| `cpu.journey.collection_navigation` | `onscreen` | `collection_navigation` | `oxide_host` | `warm` | `native` | 0.017 | 0.017 | 0.017 | 0.017 | s | regression-gated | `hitch_ms_per_s=489.933; missed_frames=32.000; missed_frames_per_s=25.769; frame_interval_ms_p50=16.668; frame_interval_ms_p95=16.668; clock_s=0.017` |
| `gpu.scene.camera.frame` | `onscreen` | `camera_preview` | `oxide_custom_camera_preview` | `warm` | `native` | 1.000 | 1.000 | 1.000 | 1.000 | s | regression-gated | `hitch_ms_per_s=0.000; missed_frames=0.000; missed_frames_per_s=0.000; frame_interval_ms_p50=8.334; frame_interval_ms_p95=8.334; clock_s=1.000` |

## Findings

- [info] This device report measures the live on-screen Oxide host path rather than the offscreen Rust perf runner, so it is the authoritative Oxide side of the official device comparison.

## Baseline Workflow

- Update the committed baseline only with review: `PERF_REPORT_DATE=$(date +%F) cargo run --locked -j$(sysctl -n hw.ncpu) -p xtask -- ios oxide-device-perf --write-baseline`
- Latest JSON baseline: `benchmarks/oxide-device/latest.json`
- Latest Markdown baseline: `benchmarks/oxide-device/latest.md`
