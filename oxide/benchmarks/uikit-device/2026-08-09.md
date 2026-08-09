# UIKit Device Perf Report

- Suite: `device`
- Device: `Victor’s iPhone`
- Energy: Direct device GPU time comes from process-scoped Metal System Trace on real iPhone hardware. Direct energy is intentionally skipped in this run and remains manual-pending until per-case Power Profiler traces are imported.
- CPU columns measure UIKit-side orchestration cost around a GPU-backed rendering pipeline; GPU columns come from direct physical-device Instruments traces.
- Fairness scope: headline rows use visible workload, transition, interaction, or present signposts where available. CPU and memory columns are app-process attribution metrics; they do not pretend that UIKit/iOS system-framework, compositor, or service work outside the app process was free.
- Metrics reflect 3-6 depending on case plus automated per-case process-scoped Metal System Trace captures attached only to the single launched OxideHost process on the same physical iPhone. Energy is manual-pending and is intentionally omitted from this run. Shared workload/phase signposts still bound the device traces even when the XCTest result bundle is carrying only the stable core metrics.
- Canonical device `signpost_*` metrics come from `xctrace`; any XCTest signpost metrics are preserved separately under `xctest_*` keys in JSON. For the official matched on-screen Oxide/UIKit rows, `clock_s` is promoted from the case headline signpost and the originating metric key is recorded under `headline_metric`. Per-case JSON also persists `measure_iterations`, `benchmark_iterations`, and per-metric `source` plus `fallback_modes`.
- Label: `2026-08-09`
- Repository ref: `refs/heads/agent/oxide-final-clean-candidate`
- Repository HEAD: `add4de82722b239a4dc196bf4e20f76c7974e3c3`
- Repository tree: `d68e766d5016f32d29c2afa9da2bb3a109dea860`

## Contract Coverage

| Section | Status | Notes |
| --- | --- | --- |
| `Engine Microbenchmarks` | `implemented` | Engine coverage reflects only the selected primitive-view, animation-effect, and primitive-lifecycle rows. |
| `Representative Screen Flows` | `implemented` | Flow coverage reflects only selected launch/lifecycle and journey rows; absent flow families are not implied. |
| `OS-Bridge Benchmarks` | `missing` | Bridge coverage measures app-owned wrapper overhead separately from system-owned UI surfaces. |
| `Idiomatic UIKit` | `implemented` | Idiomatic retained-view parity is the default UIKit baseline in this suite. |
| `Hand-Optimized UIKit` | `partial` | Selected hand-optimized UIKit rows are present, but this report does not contain tuned peers across every registered family. |
| `Launch & Lifecycle` | `missing` | This report does not establish complete launch, resume, and deep-link coverage; registered rows that were not selected are not implied. |
| `Primitive Mount / Update / Destroy` | `partial` | This report does not establish complete mount, update, and destroy coverage; component-encode signals remain partial and unselected lifecycle rows are not implied. |
| `Layout & Invalidation` | `missing` | This report does not establish complete flat-grid, deep-stack, and safe-area invalidation coverage. |
| `Text & Text Input` | `missing` | This report does not establish complete keystroke, paste, selection, IME, and cache-state text-input coverage. |
| `Image Pipeline` | `partial` | This report does not establish complete bytes-ready, decode, upload, and first-visible image-pipeline coverage. The official camera-preview battery includes the parked pure-custom NV12 live preview path and the matching AVCaptureVideoPreviewLayer baseline on the same build and device. The shipping-oriented actual app-host camera pair remains a separate bucket and may still be partial or blocked until the UI-test runner path is stable. |
| `Lists, Grids, & Chat` | `partial` | This report does not establish complete feed, grid, and chat scrolling coverage; selected collection signals remain partial. |
| `Navigation & Input Latency` | `partial` | This report does not establish complete button, slider, and text-focus input-event-to-response coverage. |
| `Animation & Visual Effects` | `partial` | This report does not establish complete animation/effect or hitch-ratio coverage; selected animation rows remain partial. |
| `State Mutation & Reconciliation` | `missing` | This report does not establish single-node, percentage-tree, and full-theme reconciliation coverage. |
| `OS Bridge Overhead` | `missing` | This report does not establish complete permission, sensor, import, share, and transport bridge coverage. |
| `Endurance, Memory, & Thermal Drift` | `missing` | This report does not establish complete long-run open/close, tab-switch, idle-animation, memory, and thermal coverage. |
| `Stress & Pathological Regressions` | `missing` | This report does not establish complete 10k-node, 300-animation, and 100 Hz ticker stress coverage. |

- The UIKit reports now persist explicit contract coverage so the suite does not over-claim comprehensiveness.
- Official camera preview rows use the parked microscope full-custom NV12 path (`testCameraNV12LegacyLivePreview`) against the parked AVFoundation preview-layer baseline (`testCameraAVFoundationPreviewLayerLivePreview`). Hybrid preview-layer visible-preview cases remain diagnostic-only and stay out of the default battery.
- The device report is the authoritative GPU source. Manual per-case Power Profiler traces still gate true energy coverage.
- The shipping-oriented actual app-host camera comparison remains an explicit bucket. Keep `testCameraNV12LegacyRealAppLivePreview` and `testCameraAVFoundationPreviewLayerRealAppLivePreview` out of the default device battery until the UI-test runner launch path is stable enough to produce repeatable JSON and trace outputs.

## Case Table

| UIKit Case | Layer | Scenario | Style | Cache | Refresh | Measure iters | Bench iters | P50 ms | P95 ms | P99 ms | Peak ms | CPU ms | Peak kB | GPU time ms | GPU latency ms | Hitch ms/s | Missed frames | Missed/s | Energy J | Launch/Mount ms | Layout ms | Text ms | Diff ms | Draw ms | Present ms | Scroll ms | Transition ms | Bridge ms | GPU counters |
| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `uikit.component.collection_view.encode` | `engine` | `primitive-view` | `idiomatic` | `warm` | `native` | 6 | 24 | 0.024 | 0.042 | 0.286 | 0.358 | 1.290 | 69649.232 | 0.000 | 0.000 | 0.000 | 0.000 | 0.000 | manual pending | `-` | 0.015 | `-` | `-` | 0.009 | 0.024 | 0.002 | `-` | `-` | `0 direct` |
| `uikit.optimized.component.collection_view.encode` | `engine` | `primitive-view` | `optimized` | `warm` | `native` | 6 | 24 | 0.028 | 0.044 | 0.109 | 0.128 | 11.944 | 67158.864 | 2.286 | 3.339 | 500.000 | 3.000 | 44.996 | manual pending | `-` | 0.010 | `-` | `-` | 1.193 | 0.028 | 0.002 | `-` | `-` | `1 direct` |
| `uikit.animation.spinner_spin` | `engine` | `animation-effect` | `idiomatic` | `warm` | `native` | 4 | 96 | 9.338 | 9.628 | 9.665 | 9.671 | 58.615 | 48448.336 | 3.625 | 0.705 | 0.000 | 0.000 | 0.000 | manual pending | `-` | 0.064 | `-` | `-` | 0.025 | 9.170 | `-` | 9.338 | `-` | `0 direct` |
| `uikit.optimized.animation.spinner_spin` | `engine` | `animation-effect` | `optimized` | `warm` | `native` | 4 | 96 | 9.508 | 9.740 | 9.753 | 9.823 | 69.444 | 48530.256 | 3.585 | 0.755 | 0.000 | 0.000 | 0.000 | manual pending | `-` | 0.028 | `-` | `-` | 0.153 | 9.323 | `-` | 9.508 | `-` | `0 direct` |
| `uikit.idiomatic.navigation.button_press.response` | `flow` | `navigation-input` | `idiomatic` | `warm` | `native` | 6 | 48 | 8.651 | 10.749 | 16.854 | 22.208 | 72.632 | 13263.576 | 24.302 | 1.115 | 21.278 | 1.000 | 2.307 | manual pending | `-` | 0.479 | `-` | 0.257 | 0.027 | 7.689 | `-` | `-` | `-` | `1 direct` |
| `uikit.optimized.navigation.button_press.response` | `flow` | `navigation-input` | `optimized` | `warm` | `native` | 5 | 64 | 13.421 | 15.044 | 20.481 | 24.140 | 237.640 | 63685.456 | 29.313 | 0.883 | 201.835 | 9.000 | 9.907 | manual pending | `-` | 0.015 | `-` | 0.000 | 8.895 | 4.659 | `-` | `-` | `-` | `1 direct` |
| `uikit.journey.collection_navigation` | `flow` | `screen-flow` | `idiomatic` | `warm` | `native` | 6 | 18 | 30.105 | 34.277 | 34.344 | 34.360 | 293.805 | 32850.768 | 27.171 | 0.911 | 99.604 | 5.000 | 6.593 | manual pending | 7.548 | 0.023 | `-` | `-` | 0.019 | 4.656 | 30.105 | `-` | `-` | `1 direct` |
| `uikit.optimized.journey.collection_navigation` | `flow` | `screen-flow` | `optimized` | `warm` | `native` | 6 | 18 | 33.865 | 34.732 | 34.820 | 34.842 | 222.408 | 57836.368 | 33.436 | 0.957 | 0.000 | 0.000 | 0.000 | manual pending | `-` | 0.026 | `-` | `-` | 2.785 | 4.569 | 33.865 | `-` | `-` | `1 direct` |
| `uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live` | `engine` | `image-pipeline` | `optimized` | `warm` | `native` | 3 | 1 | 1012.243 | 1018.600 | 1019.165 | 1019.306 | 93.213 | 89555.768 | 52.506 | 0.745 | 0.000 | 0.000 | 0.000 | manual pending | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `1 direct` |
| `uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live` | `engine` | `image-pipeline` | `idiomatic` | `warm` | `native` | 3 | 1 | 1051.363 | 1051.720 | 1051.752 | 1051.759 | 55.170 | 48579.336 | 11.844 | 0.878 | 0.000 | 0.000 | 0.000 | manual pending | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `1 direct` |

## Notes

- Scheme: OxideUIKitPerf
- Device flow: build/install the host app once, collect CPU metrics through one native-only batched xcodebuild test-without-building run, then record per-case process-scoped Metal traces on the phone. Parked and launch-handshake workloads are launched through xctrace and driven by the shared Darwin ready/start/complete notifications; camera cases that still need console summaries retain the device-console launch path.
- GPU trace: process-scoped Metal System Trace + Points of Interest, with Metal GPU Counters enabled when the device supports that counter profile.
- Energy trace: manual per-case Power Profiler import from an exported .trace or raw .atrc captured for the same OxideHost workload.
- Refresh mode: native
- Refresh policy: the official device harness is native-only. The old 60 Hz/device-default matrix was removed to keep the committed battery focused on the target shipping refresh path.
- `uikit.component.collection_view.encode`: GPU counter status: the attached device explicitly rejected the Metal GPU Counters profile, so this case was retried with direct GPU time and GPU latency only.
- `uikit.component.collection_view.encode`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.component.collection_view.encode`: GPU counter status: an earlier trace in this command established that the Metal GPU Counters profile is unavailable on the attached device/toolchain, so this case requested direct GPU time and GPU latency only.
- `uikit.optimized.component.collection_view.encode`: Direct counter: `gpu_counter.rt_unit_active`
- `uikit.animation.spinner_spin`: GPU counter status: the attached device explicitly rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.animation.spinner_spin`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.animation.spinner_spin`: GPU counter status: the attached device explicitly rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.animation.spinner_spin`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.idiomatic.navigation.button_press.response`: GPU counter status: an earlier trace in this command established that the Metal GPU Counters profile is unavailable on the attached device/toolchain, so this case requested direct GPU time and GPU latency only.
- `uikit.idiomatic.navigation.button_press.response`: Direct counter: `gpu_counter.rt_unit_active`
- `uikit.optimized.navigation.button_press.response`: GPU counter status: an earlier trace in this command established that the Metal GPU Counters profile is unavailable on the attached device/toolchain, so this case requested direct GPU time and GPU latency only.
- `uikit.optimized.navigation.button_press.response`: Direct counter: `gpu_counter.rt_unit_active`
- `uikit.journey.collection_navigation`: GPU counter status: an earlier trace in this command established that the Metal GPU Counters profile is unavailable on the attached device/toolchain, so this case requested direct GPU time and GPU latency only.
- `uikit.journey.collection_navigation`: Direct counter: `gpu_counter.rt_unit_active`
- `uikit.optimized.journey.collection_navigation`: GPU counter status: an earlier trace in this command established that the Metal GPU Counters profile is unavailable on the attached device/toolchain, so this case requested direct GPU time and GPU latency only.
- `uikit.optimized.journey.collection_navigation`: Direct counter: `gpu_counter.rt_unit_active`
- `uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live`: GPU counter status: an earlier trace in this command established that the Metal GPU Counters profile is unavailable on the attached device/toolchain, so this case requested direct GPU time and GPU latency only.
- `uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live`: Direct counter: `gpu_counter.rt_unit_active`
- `uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live`: GPU counter status: an earlier trace in this command established that the Metal GPU Counters profile is unavailable on the attached device/toolchain, so this case requested direct GPU time and GPU latency only.
- `uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live`: Direct counter: `gpu_counter.rt_unit_active`
