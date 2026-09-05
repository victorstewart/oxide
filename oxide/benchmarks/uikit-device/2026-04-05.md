# UIKit Device Perf Report

- Suite: `device`
- Device: `Victor’s iPhone`
- Energy: Direct device GPU time comes from process-scoped Metal System Trace on real iPhone hardware. Direct energy is intentionally skipped in this run and remains manual-pending until per-case Power Profiler traces are imported.
- CPU columns measure UIKit-side orchestration cost around a GPU-backed rendering pipeline; GPU columns come from direct physical-device Instruments traces.
- Metrics reflect 3-6 depending on case plus automated per-case process-scoped Metal System Trace captures attached only to the single launched OxideHost process on the same physical iPhone. Energy is manual-pending and is intentionally omitted from this run. Shared workload/phase signposts still bound the device traces even when the XCTest result bundle is carrying only the stable core metrics.
- Canonical device `signpost_*` metrics come from `xctrace`; any XCTest signpost metrics are preserved separately under `xctest_*` keys in JSON. For the official matched on-screen Oxide/UIKit rows, `clock_s` is promoted from the case headline signpost and the originating metric key is recorded under `headline_metric`. Per-case JSON also persists `measure_iterations`, `benchmark_iterations`, and per-metric `source` plus `fallback_modes`.
- Label: `2026-04-05`

## Contract Coverage

| Section | Status | Notes |
| --- | --- | --- |
| `Engine Microbenchmarks` | `implemented` | UIKit engine coverage currently spans primitive views, animation effects, and primitive lifecycle slices. |
| `Representative Screen Flows` | `implemented` | Flow coverage now spans launch/lifecycle and user-journey cases, but some committed journey families are still missing from the native device battery. |
| `OS-Bridge Benchmarks` | `missing` | Bridge coverage measures app-owned wrapper overhead separately from system-owned UI surfaces. |
| `Idiomatic UIKit` | `implemented` | Idiomatic retained-view parity is the default UIKit baseline in this suite. |
| `Hand-Optimized UIKit` | `partial` | The optimized UIKit slice now covers the full currently implemented journey, bridge, and endurance families, plus primitive-lifecycle, animation-effect, image-pipeline, and large-editor text-input peers; launch/lifecycle, layout/invalidation, authoring, component microbenchmarks, and stress/pathological traps still need tuned peers. |
| `Launch & Lifecycle` | `partial` | The current XCTest harness does not yet run a dedicated launch/resume/deep-link battery with XCTApplicationLaunchMetric. |
| `Primitive Mount / Update / Destroy` | `partial` | Flat rects, labels, cards, and images cover mount plus mutate; the empty-root, shared control-set, and retained-view remove-all/remount slices are still incomplete. |
| `Layout & Invalidation` | `partial` | Dedicated relayout batteries now exist, but not every required flat/deep/grid invalidation slice is present yet. |
| `Text & Text Input` | `partial` | UILabel parity and the input-form journey exist, but the full large-editor typing, paste, and selection battery is still incomplete. |
| `Image Pipeline` | `partial` | UIImageView and zoom workloads exist, but bytes-ready, decode, upload, and first-visible phases are not yet split into separate metrics. The official camera-preview battery includes the parked pure-custom NV12 live preview path and the matching AVCaptureVideoPreviewLayer baseline on the same build and device. The shipping-oriented actual app-host camera pair remains a separate bucket and may still be partial or blocked until the UI-test runner path is stable. |
| `Lists, Grids, & Chat` | `partial` | Collection-view encode and collection-navigation journey coverage exist, but the full feed/grid/chat scroll matrices are still incomplete. |
| `Navigation & Input Latency` | `partial` | Navigation, orchestration, and zoom journeys exist, but direct input-event-to-response batteries are still missing. |
| `Animation & Visual Effects` | `partial` | Idiomatic and hand-tuned animation-effect cases now exist, but the native device battery still lacks full hitch-ratio coverage across that family. |
| `State Mutation & Reconciliation` | `partial` | Primitive mutate and orchestration workloads exist, but explicit diff/apply batteries for tree mutation rates and theme swaps are still missing. |
| `OS Bridge Overhead` | `partial` | Permission, location, and Bluetooth wrapper overhead is covered, but photo import, file import, share sheet, and transport/decode/render bridge batteries remain missing. |
| `Endurance, Memory, & Thermal Drift` | `partial` | There is still not a complete long-run open/close, tab-switch, and idle-animation endurance battery in the current UIKit suite. |
| `Stress & Pathological Regressions` | `partial` | The explicit 10k-node, 300-animation, and 100 Hz ticker traps are still incomplete in the UIKit suite. |

- The UIKit reports now persist explicit contract coverage so the suite does not over-claim comprehensiveness.
- Official camera preview rows use the parked microscope full-custom NV12 path (`testCameraNV12LegacyLivePreview`) against the parked AVFoundation preview-layer baseline (`testCameraAVFoundationPreviewLayerLivePreview`). Hybrid preview-layer visible-preview cases remain diagnostic-only and stay out of the default battery.
- The device report is the authoritative GPU source. Manual per-case Power Profiler traces still gate true energy coverage.
- The shipping-oriented actual app-host camera comparison remains an explicit bucket. Keep `testCameraNV12LegacyRealAppLivePreview` and `testCameraAVFoundationPreviewLayerRealAppLivePreview` out of the default device battery until the UI-test runner launch path is stable enough to produce repeatable JSON and trace outputs.

## Case Table

| UIKit Case | Layer | Scenario | Style | Cache | Refresh | Measure iters | Bench iters | P50 ms | P95 ms | P99 ms | Peak ms | CPU ms | Peak kB | GPU time ms | GPU latency ms | Energy J | Launch/Mount ms | Layout ms | Text ms | Diff ms | Draw ms | Present ms | Scroll ms | Transition ms | Bridge ms | GPU counters |
| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live` | `engine` | `image-pipeline` | `optimized` | `warm` | `native` | 3 | 1 | 1011.361 | 1020.161 | 1020.943 | 1021.138 | 90.267 | 91423.568 | 157.125 | 1.627 | manual pending | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `0 direct` |
| `uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live` | `engine` | `image-pipeline` | `idiomatic` | `warm` | `native` | 3 | 1 | 1050.256 | 1051.469 | 1051.577 | 1051.604 | 53.601 | 18875.144 | 152.219 | 1.621 | manual pending | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `-` | `0 direct` |
| `uikit.idiomatic.navigation.button_press.response` | `flow` | `navigation-input` | `idiomatic` | `warm` | `native` | 6 | 48 | 10.185 | 10.625 | 10.811 | 10.916 | 72.844 | 21480.200 | 40.927 | 0.894 | manual pending | `-` | 0.388 | `-` | 0.242 | 0.026 | 9.546 | `-` | `-` | `-` | `0 direct` |
| `uikit.optimized.navigation.button_press.response` | `flow` | `navigation-input` | `optimized` | `warm` | `native` | 5 | 64 | 13.983 | 14.671 | 17.121 | 19.777 | 236.737 | 39322.448 | 38.634 | 0.832 | manual pending | `-` | 0.015 | `-` | 0.000 | 9.210 | 4.748 | `-` | `-` | `-` | `0 direct` |
| `uikit.idiomatic.navigation.text_focus.response` | `flow` | `navigation-input` | `idiomatic` | `warm` | `native` | 6 | 24 | 11.804 | 26.670 | 103.750 | 126.442 | 113.748 | 28722.000 | 31.966 | 0.788 | manual pending | `-` | 0.332 | `-` | 0.011 | 0.007 | 4.392 | `-` | `-` | `-` | `0 direct` |
| `uikit.optimized.navigation.text_focus.response` | `flow` | `navigation-input` | `optimized` | `warm` | `native` | 6 | 24 | 7.927 | 12.017 | 12.490 | 12.626 | 80.614 | 38601.552 | 20.806 | 0.981 | manual pending | `-` | 0.027 | `-` | 0.002 | 3.274 | 4.607 | `-` | `-` | `-` | `0 direct` |
| `uikit.animation.spinner_spin` | `engine` | `animation-effect` | `idiomatic` | `warm` | `native` | 4 | 96 | 9.201 | 9.509 | 9.533 | 9.542 | 50.570 | 27968.336 | 27.815 | 0.777 | manual pending | `-` | 0.037 | `-` | `-` | 0.015 | 9.104 | `-` | 9.201 | `-` | `0 direct` |
| `uikit.optimized.animation.spinner_spin` | `engine` | `animation-effect` | `optimized` | `warm` | `native` | 4 | 96 | 9.231 | 9.503 | 9.562 | 9.584 | 64.321 | 27919.184 | 29.977 | 0.794 | manual pending | `-` | 0.016 | `-` | `-` | 0.078 | 9.137 | `-` | 9.231 | `-` | `0 direct` |
| `uikit.animation.image_zoom_pan` | `engine` | `animation-effect` | `idiomatic` | `warm` | `native` | 4 | 96 | 9.245 | 9.494 | 9.528 | 9.538 | 51.702 | 28656.464 | 44.408 | 0.770 | manual pending | `-` | 0.027 | `-` | `-` | 0.002 | 9.183 | `-` | 9.245 | `-` | `0 direct` |
| `uikit.optimized.animation.image_zoom_pan` | `engine` | `animation-effect` | `optimized` | `warm` | `native` | 4 | 96 | 13.435 | 14.156 | 14.873 | 18.134 | 952.516 | 30557.008 | 43.098 | 0.772 | manual pending | `-` | 0.019 | `-` | `-` | 8.782 | 4.654 | `-` | 13.435 | `-` | `0 direct` |
| `uikit.animation.anim_timeline_bars` | `engine` | `animation-effect` | `idiomatic` | `warm` | `native` | 6 | 24 | 9.171 | 9.410 | 9.444 | 9.454 | 16.989 | 11666.160 | 20.425 | 0.902 | manual pending | `-` | 0.096 | `-` | `-` | 0.020 | 9.171 | `-` | 9.327 | `-` | `0 direct` |
| `uikit.optimized.animation.anim_timeline_bars` | `engine` | `animation-effect` | `optimized` | `warm` | `native` | 6 | 24 | 9.068 | 9.447 | 9.473 | 9.479 | 48.532 | 42517.328 | 27.844 | 1.089 | manual pending | `-` | 0.026 | `-` | `-` | 0.932 | 9.068 | `-` | 10.017 | `-` | `0 direct` |
| `uikit.journey.input_form_submit` | `flow` | `screen-flow` | `idiomatic` | `warm` | `native` | 6 | 24 | 12.512 | 22.543 | 50.862 | 58.999 | 215.594 | 55755.624 | 29.953 | 0.884 | manual pending | 5.528 | 3.193 | `-` | `-` | 0.005 | 3.300 | `-` | 12.512 | `-` | `0 direct` |
| `uikit.optimized.journey.input_form_submit` | `flow` | `screen-flow` | `optimized` | `warm` | `native` | 6 | 24 | 7.608 | 12.282 | 12.449 | 12.493 | 84.254 | 44745.576 | 23.649 | 1.070 | manual pending | `-` | 0.032 | `-` | 0.007 | 2.925 | 4.658 | `-` | 7.608 | `-` | `0 direct` |
| `uikit.journey.collection_navigation` | `flow` | `screen-flow` | `idiomatic` | `warm` | `native` | 6 | 18 | 30.987 | 34.707 | 34.897 | 34.945 | 299.407 | 28754.768 | 40.037 | 0.810 | manual pending | 7.428 | 0.025 | `-` | `-` | 0.020 | 4.685 | 30.987 | `-` | `-` | `0 direct` |
| `uikit.optimized.journey.collection_navigation` | `flow` | `screen-flow` | `optimized` | `warm` | `native` | 6 | 18 | 34.777 | 36.111 | 37.662 | 38.049 | 227.845 | 37028.688 | 38.574 | 0.890 | manual pending | `-` | 0.034 | `-` | `-` | 3.004 | 4.592 | 34.777 | `-` | `-` | `0 direct` |
| `uikit.journey.zoom_image_gesture_cycle` | `flow` | `screen-flow` | `idiomatic` | `warm` | `native` | 6 | 24 | 14.889 | 19.757 | 19.811 | 19.826 | 40.738 | 28246.864 | 31.753 | 0.918 | manual pending | 0.451 | 0.045 | `-` | `-` | 0.003 | 4.392 | `-` | 14.889 | `-` | `0 direct` |
| `uikit.optimized.journey.zoom_image_gesture_cycle` | `flow` | `screen-flow` | `optimized` | `warm` | `native` | 6 | 24 | 29.936 | 35.720 | 40.576 | 42.024 | 175.599 | 31343.440 | 33.962 | 0.878 | manual pending | `-` | 0.014 | `-` | `-` | 10.290 | 4.702 | `-` | 29.936 | `-` | `0 direct` |
| `uikit.journey.orchestration_transition_modal` | `flow` | `screen-flow` | `idiomatic` | `warm` | `native` | 6 | 20 | 51.250 | 52.577 | 52.778 | 52.828 | 117.008 | 28492.624 | 58.303 | 0.788 | manual pending | 0.823 | 0.067 | `-` | `-` | 0.016 | 9.070 | `-` | 51.250 | `-` | `0 direct` |
| `uikit.optimized.journey.orchestration_transition_modal` | `flow` | `screen-flow` | `optimized` | `warm` | `native` | 6 | 20 | 7.095 | 11.892 | 11.913 | 11.918 | 44.765 | 37012.304 | 18.368 | 1.071 | manual pending | `-` | 0.032 | `-` | 0.002 | 2.283 | 4.677 | `-` | 7.095 | `-` | `0 direct` |

## Notes

- Scheme: OxideUIKitPerf
- Device flow: build/install the host app once, collect CPU metrics through one native-only batched xcodebuild test-without-building run, then record per-case process-scoped Metal traces on the phone. Parked and launch-handshake workloads are launched through xctrace and driven by the shared Darwin ready/start/complete notifications; camera cases that still need console summaries retain the device-console launch path.
- GPU trace: process-scoped Metal System Trace + Points of Interest, with Metal GPU Counters enabled when the device supports that counter profile.
- Energy trace: manual per-case Power Profiler import from an exported .trace or raw .atrc captured for the same OxideHost workload.
- Refresh mode: native
- Refresh policy: the official device harness is native-only. The old 60 Hz/device-default matrix was removed to keep the committed battery focused on the target shipping refresh path.
- `uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.image_pipeline.camera_preview.nv12_legacy_live`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.idiomatic.image_pipeline.camera_preview.avfoundation_preview_layer_live`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.idiomatic.navigation.button_press.response`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.idiomatic.navigation.button_press.response`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.navigation.button_press.response`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.navigation.button_press.response`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.idiomatic.navigation.text_focus.response`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.idiomatic.navigation.text_focus.response`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.navigation.text_focus.response`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.navigation.text_focus.response`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.animation.spinner_spin`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.animation.spinner_spin`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.animation.spinner_spin`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.animation.spinner_spin`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.animation.image_zoom_pan`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.animation.image_zoom_pan`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.animation.image_zoom_pan`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.animation.image_zoom_pan`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.animation.anim_timeline_bars`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.animation.anim_timeline_bars`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.animation.anim_timeline_bars`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.animation.anim_timeline_bars`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.journey.input_form_submit`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.journey.input_form_submit`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.journey.input_form_submit`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.journey.input_form_submit`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.journey.collection_navigation`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.journey.collection_navigation`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.journey.collection_navigation`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.journey.collection_navigation`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.journey.zoom_image_gesture_cycle`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.journey.zoom_image_gesture_cycle`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.journey.zoom_image_gesture_cycle`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.journey.zoom_image_gesture_cycle`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.journey.orchestration_transition_modal`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.journey.orchestration_transition_modal`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
- `uikit.optimized.journey.orchestration_transition_modal`: GPU counter status: the attached device rejected the Metal GPU Counters profile, so this case includes direct GPU time and GPU latency only.
- `uikit.optimized.journey.orchestration_transition_modal`: GPU counter status: the trace exposed GPU counter tables, but there were no direct counter samples inside the bounded workload window; GPU time and GPU latency remained available from Metal System Trace.
