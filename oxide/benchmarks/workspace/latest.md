# Oxide Performance Report

- Suite: `full`
- Label: `2026-05-17`
- Coverage: 9/9 components, 7/7 animations, 5/5 launch cases, 25/25 primitive lifecycle cases, 17/17 CPU scenes, 17/17 GPU scenes, 7/7 journeys, 5/5 authoring APIs, 3/3 image pipeline cases, 3/3 navigation cases, 4/4 reconcile cases, 9/9 bridge paths

## Contract Coverage

| Section | Status | Notes |
| --- | --- | --- |
| `Engine Microbenchmarks` | `implemented` | Engine coverage currently spans system hot paths, primitive views, animations, primitive lifecycle slices, and author-facing APIs. |
| `Representative Screen Flows` | `implemented` | Flow coverage now spans offscreen launch/lifecycle, router scenes, and explicit user journeys, but hitch and device refresh-mode batteries are still incomplete. |
| `OS-Bridge Benchmarks` | `implemented` | Bridge coverage currently measures only app-owned wrapper overhead, not system-owned surface cost as a renderer win. |
| `Launch & Lifecycle` | `implemented` | Offscreen bootstrap now includes simple-home and heavy-home cold launch, route-driven detail launch, warm resume, and foreground-after-background lifecycle workloads. |
| `Primitive Mount / Update / Destroy` | `implemented` | Flat rects, labels, cards, images, an empty-root slice, a shared control-set slice, and retained-tree remove-all/remount slices are all covered. |
| `Layout & Invalidation` | `implemented` | Flat-grid rotation, deep-stack theme swap, and safe-area inset relayout batteries are all implemented. |
| `Text & Text Input` | `implemented` | Large-editor keystroke, paste, and selection-replace workloads now complement the existing text-field and input-form coverage. |
| `Image Pipeline` | `implemented` | The committed image battery now splits PNG decode, Metal texture upload, and first-visible presentation into separate persisted workloads. |
| `Lists, Grids, & Chat` | `implemented` | Feed, thumbnail-grid, and chat-thread scroll matrices now exist alongside the collection encode and navigation slices. |
| `Navigation & Input Latency` | `implemented` | Direct button-press, slider-scrub, and text-focus response batteries now complement the higher-level journey cases. |
| `Animation & Visual Effects` | `partial` | Representative animations exist, but there is no dedicated hitch-ratio or refresh-mode matrix yet for 60 Hz versus native refresh. |
| `State Mutation & Reconciliation` | `implemented` | Single-node, 1 percent, 10 percent, and full-theme tree mutation batteries now expose diff/apply cost directly. |
| `OS Bridge Overhead` | `implemented` | Permission, sensor, photo import, file import, share payload, and localhost transport/render bridge workloads are all covered without claiming system-owned UI as a renderer win. |
| `Endurance, Memory, & Thermal Drift` | `implemented` | Open/close, tab-switch, and idle-animation endurance loops are now part of the committed Oxide battery. |
| `Stress & Pathological Regressions` | `implemented` | Dedicated 10k-node, 300-animation, and 100 Hz ticker traps now complement the router stress scene. |

- The Oxide report is intentionally explicit about missing contract families so the battery does not over-claim comprehensiveness.
- Current Oxide coverage now spans engine hot paths, launch/lifecycle, representative scenes, and bridge slices; the biggest remaining gaps are hitch-oriented flow metrics and real-device refresh-mode coverage.

## Results

| Case | Layer | Scenario | Variant | Cache | Refresh | P50 | P95 | P99 | Peak | Unit | Gate | Key Metrics |
| --- | --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- |
| `cpu.system.prepare_draws.current` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 6.019 | 9.323 | 9.801 | 9.921 | us/op | regression-gated | `-` |
| `cpu.system.prepare_draws.legacy` | `engine` | `audit-baseline` | `legacy-baseline` | `warm` | `offscreen` | 5.686 | 8.673 | 8.718 | 8.729 | us/op | audit-only | `-` |
| `cpu.system.coalesce_adjacent_draws.current` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 4.600 | 8.447 | 10.426 | 10.921 | us/op | regression-gated | `-` |
| `cpu.system.coalesce_adjacent_draws.legacy` | `engine` | `audit-baseline` | `legacy-baseline` | `warm` | `offscreen` | 284.368 | 341.499 | 344.220 | 344.900 | us/op | audit-only | `-` |
| `cpu.system.gesture_sequence` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 0.135 | 0.347 | 0.394 | 0.406 | us/op | regression-gated | `-` |
| `cpu.system.touch_surface_sequence` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 0.722 | 1.156 | 1.204 | 1.216 | us/op | regression-gated | `-` |
| `cpu.system.timer_schedule_advance` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 2.391 | 4.002 | 4.210 | 4.262 | us/op | regression-gated | `-` |
| `cpu.system.anim_start_replace` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 5.128 | 7.825 | 8.993 | 9.285 | us/op | regression-gated | `-` |
| `cpu.system.text_shape_bake` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 19.973 | 23.818 | 26.079 | 26.644 | us/op | regression-gated | `-` |
| `cpu.component.label.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 11.565 | 23.758 | 25.605 | 26.067 | us/op | regression-gated | `-` |
| `cpu.component.progress_bar.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.006 | 0.011 | 0.015 | 0.016 | us/op | regression-gated | `-` |
| `cpu.component.spinner.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.001 | 0.003 | 0.003 | 0.003 | us/op | regression-gated | `-` |
| `cpu.component.button.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.403 | 1.004 | 1.562 | 1.701 | us/op | regression-gated | `-` |
| `cpu.component.toggle.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.009 | 0.013 | 0.014 | 0.014 | us/op | regression-gated | `-` |
| `cpu.component.slider.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.008 | 0.026 | 0.028 | 0.028 | us/op | regression-gated | `-` |
| `cpu.component.image_view.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.003 | 0.010 | 0.011 | 0.011 | us/op | regression-gated | `-` |
| `cpu.component.nine_slice_image.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.002 | 0.002 | 0.003 | 0.003 | us/op | regression-gated | `-` |
| `cpu.component.collection_view.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 1.087 | 1.924 | 1.974 | 1.987 | us/op | regression-gated | `-` |
| `cpu.animation.spinner_spin` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.001 | 0.003 | 0.003 | 0.003 | us/op | regression-gated | `-` |
| `cpu.animation.progress_indeterminate` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.009 | 0.019 | 0.019 | 0.019 | us/op | regression-gated | `-` |
| `cpu.animation.button_press_scale` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.533 | 0.999 | 1.017 | 1.021 | us/op | regression-gated | `-` |
| `cpu.animation.toggle_thumb_spring` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.017 | 0.036 | 0.040 | 0.040 | us/op | regression-gated | `-` |
| `cpu.animation.slider_thumb_move` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.021 | 0.035 | 0.037 | 0.038 | us/op | regression-gated | `-` |
| `cpu.animation.image_zoom_pan` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.010 | 0.013 | 0.013 | 0.013 | us/op | regression-gated | `-` |
| `cpu.animation.anim_timeline_bars` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 1.584 | 2.336 | 2.481 | 2.517 | us/op | regression-gated | `-` |
| `cpu.launch.simple_home.cold_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 346.438 | 480.411 | 524.616 | 535.667 | us/journey | regression-gated | `-` |
| `cpu.launch.heavy_home.cold_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 232.562 | 626.636 | 862.361 | 921.292 | us/journey | regression-gated | `-` |
| `cpu.launch.detail.deep_link_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 843.938 | 1245.050 | 1268.510 | 1274.375 | us/journey | regression-gated | `-` |
| `cpu.launch.simple_home.warm_resume` | `flow` | `launch-lifecycle` | `oxide` | `warm` | `offscreen` | 178.895 | 194.419 | 196.684 | 197.250 | us/journey | regression-gated | `-` |
| `cpu.launch.heavy_home.foreground_after_background` | `flow` | `launch-lifecycle` | `oxide` | `warm` | `offscreen` | 129.896 | 135.562 | 135.712 | 135.750 | us/journey | regression-gated | `-` |
| `cpu.primitive.empty_root.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 56.460 | 92.218 | 104.483 | 107.549 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 53.820 | 67.305 | 67.308 | 67.309 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 55.959 | 95.504 | 108.944 | 112.304 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.1000.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 97.542 | 112.821 | 114.369 | 114.756 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.10.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.119 | 0.403 | 0.484 | 0.504 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 1.023 | 2.043 | 2.380 | 2.465 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.1000.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 11.669 | 24.889 | 32.652 | 34.592 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.remove_all` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 5.042 | 9.428 | 10.454 | 10.711 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.remount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 6.013 | 8.279 | 8.386 | 8.412 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 85.071 | 156.803 | 162.203 | 163.553 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 669.801 | 1124.218 | 1150.058 | 1156.517 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.1000.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 6564.312 | 11642.993 | 11832.349 | 11879.688 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.10.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 57.045 | 163.370 | 189.779 | 196.381 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.100.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 995.550 | 1675.713 | 1785.398 | 1812.819 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.1000.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 6722.396 | 11222.957 | 12706.041 | 13076.812 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.088 | 0.176 | 0.200 | 0.206 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.757 | 1.245 | 1.299 | 1.312 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.10.mutate_palette` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.069 | 0.161 | 0.163 | 0.163 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.100.mutate_palette` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.626 | 1.099 | 1.121 | 1.127 | us/op | regression-gated | `-` |
| `cpu.primitive.images.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.114 | 0.223 | 0.241 | 0.245 | us/op | regression-gated | `-` |
| `cpu.primitive.images.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.632 | 1.168 | 1.327 | 1.367 | us/op | regression-gated | `-` |
| `cpu.primitive.images.10.mutate_alpha` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.095 | 0.186 | 0.228 | 0.238 | us/op | regression-gated | `-` |
| `cpu.primitive.images.100.mutate_alpha` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.730 | 1.298 | 1.348 | 1.361 | us/op | regression-gated | `-` |
| `cpu.primitive.control_set.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 71.308 | 145.506 | 181.148 | 190.059 | us/op | regression-gated | `-` |
| `cpu.primitive.control_set.mutate_state` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 8.378 | 18.267 | 19.691 | 20.047 | us/op | regression-gated | `-` |
| `cpu.scene.controls.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 22.632 | 44.780 | 45.367 | 45.513 | us/op | regression-gated | `-` |
| `cpu.scene.text_layout.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 66.020 | 94.368 | 94.961 | 95.109 | us/op | regression-gated | `-` |
| `cpu.scene.zoom_image.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 15.291 | 30.320 | 32.858 | 33.492 | us/op | regression-gated | `-` |
| `cpu.scene.anim_timeline.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 18.378 | 39.342 | 42.808 | 43.674 | us/op | regression-gated | `-` |
| `cpu.scene.collection.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 23.504 | 42.486 | 45.155 | 45.822 | us/op | regression-gated | `-` |
| `cpu.scene.damage_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 37.280 | 126.072 | 148.208 | 153.742 | us/op | regression-gated | `-` |
| `cpu.scene.input_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 90.577 | 220.781 | 231.130 | 233.717 | us/op | regression-gated | `-` |
| `cpu.scene.nine_slice.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 17.247 | 31.262 | 34.540 | 35.360 | us/op | regression-gated | `-` |
| `cpu.scene.sdf_text.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 25.421 | 54.934 | 74.746 | 79.699 | us/op | regression-gated | `-` |
| `cpu.scene.snapshot.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 23.123 | 59.208 | 64.953 | 66.389 | us/op | regression-gated | `-` |
| `cpu.scene.camera.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 29.624 | 70.370 | 78.476 | 80.503 | us/op | regression-gated | `-` |
| `cpu.scene.elements_extended.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 89.244 | 138.478 | 151.440 | 154.681 | us/op | regression-gated | `-` |
| `cpu.scene.animation_config.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 161.566 | 252.160 | 298.445 | 310.017 | us/op | regression-gated | `-` |
| `cpu.scene.orchestration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 110.889 | 288.081 | 291.261 | 292.056 | us/op | regression-gated | `-` |
| `cpu.scene.permissions.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 157.498 | 364.779 | 382.136 | 386.476 | us/op | regression-gated | `-` |
| `cpu.scene.integration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 68.060 | 222.767 | 276.361 | 289.760 | us/op | regression-gated | `-` |
| `cpu.scene.stress.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 171.773 | 359.405 | 498.200 | 532.898 | us/op | regression-gated | `-` |
| `gpu.scene.controls.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1.727 | 4.711 | 180.003 | 334.571 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=25.020; damage_prefilter_thresh=0.250` |
| `gpu.scene.text_layout.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.587 | 5.403 | 6.825 | 7.344 | ms/frame | regression-gated | `culled_avg=6.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.zoom_image.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.787 | 17.811 | 40.731 | 49.547 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=80.960; damage_prefilter_thresh=0.250` |
| `gpu.scene.anim_timeline.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.602 | 7.037 | 23.758 | 31.243 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=41.213; damage_prefilter_thresh=0.250` |
| `gpu.scene.collection.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.523 | 6.780 | 20.255 | 31.857 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.damage_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 3.325 | 6.955 | 24.045 | 38.379 | ms/frame | regression-gated | `culled_avg=2.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.input_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.373 | 6.330 | 12.933 | 13.877 | ms/frame | regression-gated | `culled_avg=36.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.nine_slice.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.619 | 28.179 | 31.325 | 32.085 | ms/frame | regression-gated | `culled_avg=2.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.sdf_text.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 3.078 | 9.719 | 20.760 | 26.842 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.snapshot.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.288 | 7.024 | 20.362 | 31.764 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.camera.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.881 | 8.178 | 19.849 | 29.225 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.elements_extended.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 3.203 | 23.284 | 30.443 | 31.807 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.animation_config.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1.544 | 7.196 | 12.458 | 16.452 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.orchestration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 3.358 | 7.550 | 21.592 | 30.544 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.permissions.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.311 | 8.522 | 17.102 | 22.577 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.integration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.866 | 7.738 | 18.434 | 27.346 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.stress.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.549 | 9.417 | 22.404 | 26.962 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `cpu.journey.input_form_submit` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 950.271 | 6140.269 | 7978.954 | 8438.625 | us/journey | regression-gated | `-` |
| `cpu.journey.collection_navigation` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 223.354 | 1673.719 | 2421.844 | 2608.875 | us/journey | regression-gated | `-` |
| `cpu.journey.zoom_image_gesture_cycle` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 120.125 | 423.648 | 609.363 | 655.792 | us/journey | regression-gated | `-` |
| `cpu.journey.orchestration_transition_modal` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 731.146 | 779.634 | 788.394 | 790.584 | us/journey | regression-gated | `-` |
| `cpu.journey.feed_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 14.562 | 15.481 | 15.796 | 15.875 | us/journey | regression-gated | `-` |
| `cpu.journey.thumbnail_grid_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 40.916 | 42.798 | 43.292 | 43.416 | us/journey | regression-gated | `-` |
| `cpu.journey.chat_thread_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 25.645 | 26.567 | 26.747 | 26.792 | us/journey | regression-gated | `-` |
| `cpu.authoring.text_fields.edit_cycle` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 1.612 | 53.081 | 85.609 | 93.741 | us/op | regression-gated | `-` |
| `cpu.authoring.popup_wheel_picker.interaction` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 0.154 | 0.947 | 1.615 | 1.782 | us/op | regression-gated | `-` |
| `cpu.authoring.burst_emitter.sample` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 1.392 | 2.238 | 2.575 | 2.659 | us/op | regression-gated | `-` |
| `cpu.authoring.surface_router.compose` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 388.927 | 498.626 | 502.680 | 503.694 | us/op | regression-gated | `-` |
| `gpu.authoring.scene3d.mixed_frame` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 0.565 | 2.979 | 4.123 | 4.410 | ms/frame | regression-gated | `draws_avg=2.000; encode_ms_median=0.035; mesh_indices=7.000; mesh_vertices=7.000` |
| `cpu.layout.flat_grid.rotation_relayout` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 99.946 | 126.220 | 137.171 | 139.909 | us/op | regression-gated | `dirty_nodes=240.000; layout_passes=2.000` |
| `cpu.layout.deep_stack.theme_swap` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 50.581 | 80.656 | 88.098 | 89.958 | us/op | regression-gated | `dirty_nodes=60.000; layout_passes=2.000` |
| `cpu.layout.grid.safe_area_swap` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 94.439 | 264.464 | 410.441 | 446.936 | us/op | regression-gated | `dirty_nodes=180.000; layout_passes=3.000` |
| `cpu.text_input.large_editor.keystroke_burst` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 23.447 | 51.395 | 61.350 | 63.839 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.text_input.large_editor.paste_10kb` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 1.216 | 2.188 | 2.850 | 3.016 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.text_input.large_editor.selection_replace` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 7.680 | 14.021 | 18.181 | 19.221 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.image_pipeline.png.decode` | `engine` | `image-pipeline` | `oxide` | `cold` | `offscreen` | 65.945 | 120.882 | 130.920 | 133.429 | us/op | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `gpu.image_pipeline.png.upload` | `engine` | `image-pipeline` | `oxide` | `warm` | `offscreen` | 83.185 | 152.407 | 156.179 | 157.122 | us/upload | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `gpu.image_pipeline.png.first_visible` | `engine` | `image-pipeline` | `oxide` | `warm` | `offscreen` | 0.032 | 1.406 | 1.948 | 2.084 | ms/frame | regression-gated | `draw_calls=0.000; encode_ms_median=0.024; encoded_bytes=6797.000; texture_bytes=65536.000` |
| `cpu.navigation.button_press.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 0.559 | 0.676 | 0.683 | 0.684 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.navigation.slider_scrub.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 0.012 | 0.024 | 0.032 | 0.033 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.navigation.text_focus.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 545.791 | 699.488 | 718.356 | 723.073 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.reconcile.single_node_mutation` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 13.734 | 33.848 | 40.427 | 42.072 | us/op | regression-gated | `dirty_nodes=1.000; layout_passes=1.000` |
| `cpu.reconcile.tree_mutation_1pct` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 13.229 | 55.167 | 77.853 | 83.525 | us/op | regression-gated | `dirty_nodes=10.000; layout_passes=1.000` |
| `cpu.reconcile.tree_mutation_10pct` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 15.151 | 19.863 | 19.987 | 20.018 | us/op | regression-gated | `dirty_nodes=100.000; layout_passes=1.000` |
| `cpu.reconcile.theme_swap_full` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 26.515 | 44.430 | 50.996 | 52.637 | us/op | regression-gated | `dirty_nodes=1000.000; layout_passes=2.000` |
| `cpu.endurance.open_close_heavy_screen.100x` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 10305.312 | 17549.375 | 17675.142 | 17706.584 | us/op | regression-gated | `layout_passes=100.000` |
| `cpu.endurance.tab_switch_heavy.500x` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 38623.312 | 49924.257 | 53164.151 | 53974.125 | us/op | regression-gated | `layout_passes=500.000` |
| `cpu.endurance.idle_animation.600_frames` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 14295.719 | 26395.867 | 32035.907 | 33445.916 | us/op | regression-gated | `layout_passes=600.000` |
| `cpu.stress.flat_rects.10000.mount` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 1313.917 | 2284.793 | 2332.481 | 2344.403 | us/op | regression-gated | `dirty_nodes=10000.000; layout_passes=1.000` |
| `cpu.stress.simultaneous_animations.300` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 3.037 | 6.418 | 7.791 | 8.135 | us/op | regression-gated | `draw_calls=300.000` |
| `cpu.stress.ticker_100hz` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 9752.427 | 12373.958 | 13571.675 | 13871.104 | us/op | regression-gated | `layout_passes=100.000` |
| `cpu.bridge.permission_callback_fanout` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 0.116 | 0.327 | 0.348 | 0.353 | us/op | regression-gated | `-` |
| `cpu.bridge.web_backend_surface` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 0.045 | 0.383 | 0.468 | 0.489 | us/op | regression-gated | `-` |
| `cpu.bridge.sensor_location_snapshot` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 0.151 | 3.588 | 6.673 | 7.444 | us/op | regression-gated | `-` |
| `cpu.bridge.bluetooth_cache_update` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 0.952 | 1.356 | 1.401 | 1.412 | us/op | regression-gated | `-` |
| `cpu.bridge.photo_import_thumbnail` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 72.539 | 94.759 | 101.291 | 102.924 | us/op | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `cpu.bridge.file_import_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 455.268 | 970.170 | 1050.437 | 1070.504 | us/op | regression-gated | `dirty_nodes=32.000` |
| `cpu.bridge.share_payload_prepare` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 122.112 | 180.192 | 206.003 | 212.456 | us/op | regression-gated | `encoded_bytes=74.000` |
| `cpu.bridge.local_json_transport_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 59.228 | 70.995 | 75.767 | 76.959 | us/op | regression-gated | `dirty_nodes=48.000; encoded_bytes=2007.000` |
| `cpu.bridge.local_image_transport_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 243.236 | 347.674 | 365.928 | 370.491 | us/op | regression-gated | `encoded_bytes=26533.000; texture_bytes=262144.000` |

## A/B Audit

- prepare_draws: 0.94x faster than the retained legacy path
- coalesce_adjacent_draws: 61.82x faster than the retained legacy path

## Regression Check

- Regression: `cpu.system.prepare_draws.current` median 6.019 vs baseline 5.184 (allowed 5.806, delta +16.11%)
- Regression: `cpu.system.coalesce_adjacent_draws.current` median 4.600 vs baseline 3.072 (allowed 3.441, delta +49.72%)
- Regression: `cpu.system.touch_surface_sequence` median 0.722 vs baseline 0.368 (allowed 0.412, delta +96.01%)
- Regression: `cpu.system.timer_schedule_advance` median 2.391 vs baseline 1.834 (allowed 2.054, delta +30.38%)
- Regression: `cpu.system.anim_start_replace` median 5.128 vs baseline 2.361 (allowed 2.645, delta +117.18%)
- Regression: `cpu.component.progress_bar.encode` median 0.006 vs baseline 0.005 (allowed 0.006, delta +12.98%)
- Regression: `cpu.component.button.encode` median 0.403 vs baseline 0.335 (allowed 0.375, delta +20.21%)
- Regression: `cpu.component.toggle.encode` median 0.009 vs baseline 0.006 (allowed 0.007, delta +52.80%)
- Regression: `cpu.component.slider.encode` median 0.008 vs baseline 0.007 (allowed 0.008, delta +11.68%)
- Regression: `cpu.component.collection_view.encode` median 1.087 vs baseline 0.644 (allowed 0.721, delta +68.85%)
- Regression: `cpu.animation.progress_indeterminate` median 0.009 vs baseline 0.006 (allowed 0.007, delta +38.77%)
- Regression: `cpu.animation.button_press_scale` median 0.533 vs baseline 0.225 (allowed 0.252, delta +137.03%)
- Regression: `cpu.animation.toggle_thumb_spring` median 0.017 vs baseline 0.007 (allowed 0.008, delta +129.74%)
- Regression: `cpu.animation.slider_thumb_move` median 0.021 vs baseline 0.008 (allowed 0.009, delta +156.51%)
- Regression: `cpu.animation.image_zoom_pan` median 0.010 vs baseline 0.007 (allowed 0.008, delta +37.05%)
- Regression: `cpu.animation.anim_timeline_bars` median 1.584 vs baseline 1.026 (allowed 1.149, delta +54.42%)
- Regression: `cpu.launch.simple_home.cold_launch` median 346.438 vs baseline 134.834 (allowed 277.970, delta +156.94%)
- Regression: `cpu.launch.heavy_home.cold_launch` median 232.562 vs baseline 75.271 (allowed 86.562, delta +208.97%)
- Regression: `cpu.launch.detail.deep_link_launch` median 843.938 vs baseline 355.458 (allowed 408.777, delta +137.42%)
- Regression: `cpu.primitive.empty_root.mount` median 56.460 vs baseline 21.416 (allowed 23.557, delta +163.63%)
- Regression: `cpu.primitive.flat_rects.10.mount` median 53.820 vs baseline 21.269 (allowed 25.097, delta +153.05%)
- Regression: `cpu.primitive.flat_rects.100.mount` median 55.959 vs baseline 28.991 (allowed 34.209, delta +93.02%)
- Regression: `cpu.primitive.flat_rects.1000.mount` median 97.542 vs baseline 63.436 (allowed 74.854, delta +53.77%)
- Regression: `cpu.primitive.flat_rects.10.mutate_fill` median 0.119 vs baseline 0.087 (allowed 0.103, delta +36.62%)
- Regression: `cpu.primitive.flat_rects.100.mutate_fill` median 1.023 vs baseline 0.737 (allowed 0.870, delta +38.76%)
- Regression: `cpu.primitive.flat_rects.1000.mutate_fill` median 11.669 vs baseline 8.877 (allowed 10.475, delta +31.45%)
- Regression: `cpu.primitive.flat_rects.100.remove_all` median 5.042 vs baseline 3.489 (allowed 4.116, delta +44.54%)
- Regression: `cpu.primitive.flat_rects.100.remount` median 6.013 vs baseline 4.070 (allowed 4.802, delta +47.75%)
- Regression: `cpu.primitive.labels.10.mount` median 85.071 vs baseline 44.127 (allowed 52.952, delta +92.79%)
- Regression: `cpu.primitive.labels.100.mount` median 669.801 vs baseline 424.581 (allowed 509.497, delta +57.76%)
- Regression: `cpu.primitive.labels.1000.mount` median 6564.312 vs baseline 4341.406 (allowed 5209.688, delta +51.20%)
- Regression: `cpu.primitive.labels.10.mutate_text` median 57.045 vs baseline 46.356 (allowed 55.627, delta +23.06%)
- Regression: `cpu.primitive.labels.100.mutate_text` median 995.550 vs baseline 467.409 (allowed 560.890, delta +112.99%)
- Regression: `cpu.primitive.labels.1000.mutate_text` median 6722.396 vs baseline 4436.062 (allowed 5323.275, delta +51.54%)
- Regression: `cpu.primitive.cards.10.mount` median 0.088 vs baseline 0.055 (allowed 0.064, delta +62.09%)
- Regression: `cpu.primitive.cards.100.mount` median 0.757 vs baseline 0.494 (allowed 0.583, delta +53.17%)
- Regression: `cpu.primitive.cards.10.mutate_palette` median 0.069 vs baseline 0.053 (allowed 0.063, delta +29.27%)
- Regression: `cpu.primitive.cards.100.mutate_palette` median 0.626 vs baseline 0.486 (allowed 0.574, delta +28.84%)
- Regression: `cpu.primitive.images.10.mount` median 0.114 vs baseline 0.056 (allowed 0.066, delta +105.64%)
- Regression: `cpu.primitive.images.100.mount` median 0.632 vs baseline 0.378 (allowed 0.446, delta +67.17%)
- Regression: `cpu.primitive.images.10.mutate_alpha` median 0.095 vs baseline 0.030 (allowed 0.036, delta +211.76%)
- Regression: `cpu.primitive.images.100.mutate_alpha` median 0.730 vs baseline 0.292 (allowed 0.344, delta +150.21%)
- Regression: `cpu.primitive.control_set.mount` median 71.308 vs baseline 49.248 (allowed 56.143, delta +44.79%)
- Regression: `cpu.primitive.control_set.mutate_state` median 8.378 vs baseline 7.095 (allowed 8.088, delta +18.09%)
- Regression: `cpu.scene.controls.frame` median 22.632 vs baseline 15.806 (allowed 18.176, delta +43.19%)
- Regression: `cpu.scene.text_layout.frame` median 66.020 vs baseline 33.113 (allowed 38.080, delta +99.38%)
- Regression: `cpu.scene.zoom_image.frame` median 15.291 vs baseline 9.332 (allowed 10.732, delta +63.85%)
- Regression: `cpu.scene.anim_timeline.frame` median 18.378 vs baseline 12.635 (allowed 14.530, delta +45.46%)
- Regression: `cpu.scene.collection.frame` median 23.504 vs baseline 16.028 (allowed 18.432, delta +46.64%)
- Regression: `cpu.scene.damage_lab.frame` median 37.280 vs baseline 25.839 (allowed 29.715, delta +44.28%)
- Regression: `cpu.scene.input_lab.frame` median 90.577 vs baseline 57.891 (allowed 66.574, delta +56.46%)
- Regression: `cpu.scene.nine_slice.frame` median 17.247 vs baseline 11.491 (allowed 13.214, delta +50.10%)
- Regression: `cpu.scene.sdf_text.frame` median 25.421 vs baseline 15.455 (allowed 17.774, delta +64.48%)
- Regression: `cpu.scene.camera.frame` median 29.624 vs baseline 23.412 (allowed 26.924, delta +26.53%)
- Regression: `cpu.scene.elements_extended.frame` median 89.244 vs baseline 50.113 (allowed 57.630, delta +78.09%)
- Regression: `cpu.scene.animation_config.frame` median 161.566 vs baseline 92.188 (allowed 106.016, delta +75.26%)
- Regression: `cpu.scene.orchestration.frame` median 110.889 vs baseline 63.429 (allowed 72.943, delta +74.82%)
- Regression: `cpu.scene.permissions.frame` median 157.498 vs baseline 123.468 (allowed 141.988, delta +27.56%)
- Regression: `cpu.scene.integration.frame` median 68.060 vs baseline 55.956 (allowed 64.349, delta +21.63%)
- Regression: `cpu.scene.stress.frame` median 171.773 vs baseline 62.547 (allowed 71.929, delta +174.63%)
- Regression: `gpu.scene.text_layout.frame` median 2.587 vs baseline 0.427 (allowed 1.597, delta +505.50%)
- Regression: `gpu.scene.zoom_image.frame` median 2.787 vs baseline 0.247 (allowed 2.782, delta +1029.13%)
- Regression: `gpu.scene.damage_lab.frame` median 3.325 vs baseline 0.399 (allowed 2.596, delta +733.95%)
- Regression: `gpu.scene.input_lab.frame` median 2.373 vs baseline 0.478 (allowed 0.693, delta +396.40%)
- Regression: `gpu.scene.sdf_text.frame` median 3.078 vs baseline 0.382 (allowed 2.937, delta +705.37%)
- Regression: `gpu.scene.elements_extended.frame` median 3.203 vs baseline 0.393 (allowed 2.755, delta +715.96%)
- Regression: `gpu.scene.orchestration.frame` median 3.358 vs baseline 0.296 (allowed 2.640, delta +1035.01%)
- Regression: `gpu.scene.permissions.frame` median 2.311 vs baseline 0.240 (allowed 2.308, delta +861.66%)
- Regression: `cpu.journey.input_form_submit` median 950.271 vs baseline 461.083 (allowed 530.245, delta +106.10%)
- Regression: `cpu.journey.collection_navigation` median 223.354 vs baseline 152.416 (allowed 175.279, delta +46.54%)
- Regression: `cpu.authoring.text_fields.edit_cycle` median 1.612 vs baseline 0.949 (allowed 1.063, delta +69.95%)
- Regression: `cpu.authoring.popup_wheel_picker.interaction` median 0.154 vs baseline 0.065 (allowed 0.071, delta +136.88%)
- Regression: `cpu.authoring.burst_emitter.sample` median 1.392 vs baseline 0.676 (allowed 0.743, delta +105.98%)
- Regression: `cpu.authoring.surface_router.compose` median 388.927 vs baseline 128.637 (allowed 154.364, delta +202.34%)
- Regression: `cpu.layout.flat_grid.rotation_relayout` median 99.946 vs baseline 39.443 (allowed 46.543, delta +153.39%)
- Regression: `cpu.layout.deep_stack.theme_swap` median 50.581 vs baseline 27.975 (allowed 33.011, delta +80.81%)
- Regression: `cpu.layout.grid.safe_area_swap` median 94.439 vs baseline 36.761 (allowed 43.378, delta +156.90%)
- Regression: `cpu.text_input.large_editor.keystroke_burst` median 23.447 vs baseline 12.125 (allowed 14.308, delta +93.38%)
- Regression: `cpu.text_input.large_editor.paste_10kb` median 1.216 vs baseline 0.790 (allowed 0.932, delta +53.83%)
- Regression: `cpu.text_input.large_editor.selection_replace` median 7.680 vs baseline 4.472 (allowed 5.277, delta +71.74%)
- Regression: `cpu.image_pipeline.png.decode` median 65.945 vs baseline 46.102 (allowed 55.322, delta +43.04%)
- Regression: `gpu.image_pipeline.png.upload` median 83.185 vs baseline 14.904 (allowed 23.620, delta +458.15%)
- Regression: `cpu.navigation.button_press.response` median 0.559 vs baseline 0.290 (allowed 0.342, delta +92.75%)
- Regression: `cpu.navigation.slider_scrub.response` median 0.012 vs baseline 0.009 (allowed 0.011, delta +24.49%)
- Regression: `cpu.navigation.text_focus.response` median 545.791 vs baseline 328.806 (allowed 387.991, delta +65.99%)
- Regression: `cpu.reconcile.single_node_mutation` median 13.734 vs baseline 6.175 (allowed 7.286, delta +122.42%)
- Regression: `cpu.reconcile.tree_mutation_1pct` median 13.229 vs baseline 6.375 (allowed 7.523, delta +107.50%)
- Regression: `cpu.reconcile.tree_mutation_10pct` median 15.151 vs baseline 6.355 (allowed 7.499, delta +138.40%)
- Regression: `cpu.reconcile.theme_swap_full` median 26.515 vs baseline 15.808 (allowed 18.653, delta +67.73%)
- Regression: `cpu.endurance.open_close_heavy_screen.100x` median 10305.312 vs baseline 6485.146 (allowed 7652.472, delta +58.91%)
- Regression: `cpu.endurance.tab_switch_heavy.500x` median 38623.312 vs baseline 22849.896 (allowed 26962.877, delta +69.03%)
- Regression: `cpu.endurance.idle_animation.600_frames` median 14295.719 vs baseline 8001.740 (allowed 9442.053, delta +78.66%)
- Regression: `cpu.stress.flat_rects.10000.mount` median 1313.917 vs baseline 398.424 (allowed 478.109, delta +229.78%)
- Regression: `cpu.stress.simultaneous_animations.300` median 3.037 vs baseline 2.310 (allowed 2.772, delta +31.46%)
- Regression: `cpu.stress.ticker_100hz` median 9752.427 vs baseline 6693.792 (allowed 8032.550, delta +45.69%)
- Regression: `cpu.bridge.permission_callback_fanout` median 0.116 vs baseline 0.068 (allowed 0.082, delta +71.09%)
- Regression: `cpu.bridge.sensor_location_snapshot` median 0.151 vs baseline 0.088 (allowed 0.106, delta +70.85%)
- Regression: `cpu.bridge.photo_import_thumbnail` median 72.539 vs baseline 50.859 (allowed 61.030, delta +42.63%)
- Regression: `cpu.bridge.file_import_render` median 455.268 vs baseline 317.139 (allowed 380.567, delta +43.55%)
- Regression: `cpu.bridge.share_payload_prepare` median 122.112 vs baseline 74.036 (allowed 88.844, delta +64.94%)
- Regression: `cpu.bridge.local_json_transport_render` median 59.228 vs baseline 39.154 (allowed 46.985, delta +51.27%)
- Regression: `cpu.bridge.local_image_transport_render` median 243.236 vs baseline 145.441 (allowed 174.529, delta +67.24%)

## Findings

- [fixed] DrawListBuilder::clear now clears retained vertex and index storage, eliminating stale geometry accumulation when builders are reused across frames.
- [fixed] ui-core::prepare_draws now keeps cumulative clip intersections on the stack instead of rebuilding the full stack on every ClipPop.
- [fixed] ui-core::coalesce_adjacent_draws now uses a single linear compaction pass instead of Vec::remove-based quadratic merging.
- [fixed] oxide-input::TouchSurfaceRecognizer now keeps common active touches inline and derives the active one/two-touch frame with a deterministic scan instead of hashing, allocating, and sorting on every raw touch event.
- [fixed] oxide-input::GestureRecognizer now keeps common active tracks inline and writes event-only and feedback outputs through monomorphized sinks, removing hashing and the second vector path from common touch handling.
- [fixed] oxide-timing timers now use atomic id generation and drain due callbacks before execution, reducing scheduler overhead while avoiding callback execution under the timer map lock.
- [fixed] oxide-timing animation start now uses atomic reduce-motion state and a consistent RUNNING_PROP-to-ANIMS lock order, reducing property-animation replacement overhead.
- [fixed] oxide-ui-core label encoding now avoids non-wrapped internal label clones, skips disabled diagnostic string formatting on the hot path, and preallocates the common wrapped-line buffers.
- [fixed] renderer-metal now batches consecutive rounded rectangles through the instanced shader path, reducing per-rect command encoding and parameter binding.
- [fixed] renderer-metal now reuses retained scratch buffers across the remaining small batch encode paths instead of allocating per-frame vectors for each batch group.
- [fixed] renderer-metal damage prefiltering now borrows source geometry backing storage, preserving small-damage culling without cloning full vertex and index arrays.
- [candidate] The macOS glyph indirect-command-buffer path is now default-disabled because Metal validation exposed CPU access to private ICB storage and an invalid ICB pipeline configuration; restoring it with a truly valid text ICB path remains a high-value GPU follow-up.
- [candidate] Label wrapping still re-shapes tentative strings per word and clones intermediate Strings, which is likely the next CPU hotspot for text-heavy wrapped layouts.

## Baseline Workflow

- Update the committed baseline only with review: `PERF_REPORT_DATE=$(date +%F) cargo run --release -j$(sysctl -n hw.ncpu) -p oxide-perf-runner -- --run-suite --write-baseline`
- Latest JSON baseline: `benchmarks/workspace/latest.json`
- Latest Markdown baseline: `benchmarks/workspace/latest.md`
