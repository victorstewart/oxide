# Oxide Performance Report

- Suite: `full`
- Coverage: 9/9 components, 7/7 animations, 5/5 launch cases, 25/25 primitive lifecycle cases, 17/17 CPU scenes, 17/17 GPU scenes, 7/7 journeys, 5/5 authoring APIs, 3/3 image pipeline cases, 3/3 navigation cases, 4/4 reconcile cases, 8/8 bridge paths

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
| `cpu.system.prepare_draws.current` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 33.906 | 43.639 | 51.281 | 53.192 | us/op | regression-gated | `-` |
| `cpu.system.prepare_draws.legacy` | `engine` | `audit-baseline` | `legacy-baseline` | `warm` | `offscreen` | 56.980 | 59.900 | 61.032 | 61.316 | us/op | audit-only | `-` |
| `cpu.system.coalesce_adjacent_draws.current` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 56.558 | 68.536 | 68.643 | 68.670 | us/op | regression-gated | `-` |
| `cpu.system.coalesce_adjacent_draws.legacy` | `engine` | `audit-baseline` | `legacy-baseline` | `warm` | `offscreen` | 253.257 | 276.575 | 285.354 | 287.549 | us/op | audit-only | `-` |
| `cpu.system.gesture_sequence` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 3.663 | 4.112 | 4.259 | 4.296 | us/op | regression-gated | `-` |
| `cpu.system.text_shape_bake` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 489.255 | 498.349 | 501.942 | 502.840 | us/op | regression-gated | `-` |
| `cpu.component.label.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 1181.724 | 1203.502 | 1203.671 | 1203.714 | us/op | regression-gated | `-` |
| `cpu.component.progress_bar.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.078 | 0.087 | 0.092 | 0.094 | us/op | regression-gated | `-` |
| `cpu.component.spinner.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.055 | 0.056 | 0.056 | 0.056 | us/op | regression-gated | `-` |
| `cpu.component.button.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 78.598 | 83.045 | 85.556 | 86.184 | us/op | regression-gated | `-` |
| `cpu.component.toggle.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.084 | 0.087 | 0.087 | 0.087 | us/op | regression-gated | `-` |
| `cpu.component.slider.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.097 | 0.101 | 0.102 | 0.102 | us/op | regression-gated | `-` |
| `cpu.component.image_view.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.079 | 0.082 | 0.085 | 0.085 | us/op | regression-gated | `-` |
| `cpu.component.nine_slice_image.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.044 | 0.046 | 0.046 | 0.046 | us/op | regression-gated | `-` |
| `cpu.component.collection_view.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 25.907 | 26.452 | 26.517 | 26.533 | us/op | regression-gated | `-` |
| `cpu.animation.spinner_spin` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.056 | 0.057 | 0.058 | 0.058 | us/op | regression-gated | `-` |
| `cpu.animation.progress_indeterminate` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.077 | 0.093 | 0.105 | 0.108 | us/op | regression-gated | `-` |
| `cpu.animation.button_press_scale` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 70.123 | 73.429 | 73.539 | 73.566 | us/op | regression-gated | `-` |
| `cpu.animation.toggle_thumb_spring` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.104 | 0.108 | 0.110 | 0.111 | us/op | regression-gated | `-` |
| `cpu.animation.slider_thumb_move` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.097 | 0.099 | 0.099 | 0.099 | us/op | regression-gated | `-` |
| `cpu.animation.image_zoom_pan` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.093 | 0.094 | 0.094 | 0.094 | us/op | regression-gated | `-` |
| `cpu.animation.anim_timeline_bars` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 238.454 | 242.091 | 243.515 | 243.871 | us/op | regression-gated | `-` |
| `cpu.launch.simple_home.cold_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 1495.542 | 1654.471 | 1659.661 | 1660.958 | us/journey | regression-gated | `-` |
| `cpu.launch.heavy_home.cold_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 850.458 | 1069.335 | 1120.200 | 1132.916 | us/journey | regression-gated | `-` |
| `cpu.launch.detail.deep_link_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 6862.166 | 10391.611 | 11591.356 | 11891.292 | us/journey | regression-gated | `-` |
| `cpu.launch.simple_home.warm_resume` | `flow` | `launch-lifecycle` | `oxide` | `warm` | `offscreen` | 2225.979 | 2338.877 | 2365.142 | 2371.708 | us/journey | regression-gated | `-` |
| `cpu.launch.heavy_home.foreground_after_background` | `flow` | `launch-lifecycle` | `oxide` | `warm` | `offscreen` | 1525.500 | 1709.973 | 1739.628 | 1747.042 | us/journey | regression-gated | `-` |
| `cpu.primitive.empty_root.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 27.946 | 29.231 | 29.812 | 29.957 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 30.479 | 32.879 | 34.178 | 34.503 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 73.800 | 108.850 | 140.081 | 147.888 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.1000.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 477.919 | 736.796 | 802.559 | 819.000 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.10.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 1.595 | 1.894 | 1.919 | 1.925 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 11.844 | 19.778 | 23.228 | 24.091 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.1000.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 106.779 | 135.576 | 145.644 | 148.161 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.remove_all` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 194.521 | 212.668 | 213.906 | 214.215 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.remount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 228.287 | 563.590 | 826.301 | 891.979 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 4059.113 | 4186.777 | 4273.183 | 4294.785 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 41651.264 | 42383.576 | 42445.360 | 42460.806 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.1000.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 414329.469 | 439111.125 | 441854.525 | 442540.375 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.10.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 4059.981 | 4813.070 | 5529.667 | 5708.816 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.100.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 41061.871 | 50266.129 | 56932.553 | 58599.160 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.1000.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 411034.292 | 521502.050 | 564763.693 | 575579.104 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 1.431 | 2.358 | 2.530 | 2.573 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 34.559 | 3287.247 | 4251.591 | 4492.677 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.10.mutate_palette` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 1.204 | 6.248 | 10.866 | 12.021 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.100.mutate_palette` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 10.373 | 15.749 | 17.577 | 18.034 | us/op | regression-gated | `-` |
| `cpu.primitive.images.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 1.069 | 1.425 | 1.432 | 1.434 | us/op | regression-gated | `-` |
| `cpu.primitive.images.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 8.022 | 8.694 | 9.148 | 9.261 | us/op | regression-gated | `-` |
| `cpu.primitive.images.10.mutate_alpha` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.947 | 0.981 | 0.988 | 0.990 | us/op | regression-gated | `-` |
| `cpu.primitive.images.100.mutate_alpha` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 7.443 | 7.610 | 7.619 | 7.621 | us/op | regression-gated | `-` |
| `cpu.primitive.control_set.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 930.847 | 1320.011 | 1620.057 | 1695.069 | us/op | regression-gated | `-` |
| `cpu.primitive.control_set.mutate_state` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 328.479 | 334.875 | 335.120 | 335.181 | us/op | regression-gated | `-` |
| `cpu.scene.controls.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 659.508 | 754.677 | 816.998 | 832.578 | us/op | regression-gated | `-` |
| `cpu.scene.text_layout.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 4004.932 | 4874.295 | 5076.921 | 5127.578 | us/op | regression-gated | `-` |
| `cpu.scene.zoom_image.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 332.872 | 393.140 | 412.432 | 417.255 | us/op | regression-gated | `-` |
| `cpu.scene.anim_timeline.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 622.229 | 680.094 | 699.179 | 703.951 | us/op | regression-gated | `-` |
| `cpu.scene.collection.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 610.422 | 739.666 | 765.221 | 771.609 | us/op | regression-gated | `-` |
| `cpu.scene.damage_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2175.331 | 2632.665 | 2715.016 | 2735.604 | us/op | regression-gated | `-` |
| `cpu.scene.input_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2569.052 | 2707.747 | 2802.029 | 2825.599 | us/op | regression-gated | `-` |
| `cpu.scene.nine_slice.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 399.163 | 424.317 | 437.923 | 441.325 | us/op | regression-gated | `-` |
| `cpu.scene.sdf_text.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1175.585 | 3446.076 | 4235.624 | 4433.011 | us/op | regression-gated | `-` |
| `cpu.scene.snapshot.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1283.583 | 1664.637 | 1676.582 | 1679.568 | us/op | regression-gated | `-` |
| `cpu.scene.camera.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 641.919 | 674.437 | 678.604 | 679.646 | us/op | regression-gated | `-` |
| `cpu.scene.elements_extended.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2134.018 | 2186.034 | 2217.132 | 2224.906 | us/op | regression-gated | `-` |
| `cpu.scene.animation_config.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 4872.008 | 6487.189 | 7718.217 | 8025.974 | us/op | regression-gated | `-` |
| `cpu.scene.orchestration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2904.917 | 3462.318 | 3648.972 | 3695.635 | us/op | regression-gated | `-` |
| `cpu.scene.permissions.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 6028.258 | 6259.573 | 6405.869 | 6442.443 | us/op | regression-gated | `-` |
| `cpu.scene.integration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 3322.667 | 3528.862 | 3542.702 | 3546.162 | us/op | regression-gated | `-` |
| `cpu.scene.stress.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 3031.581 | 3119.348 | 3151.395 | 3159.406 | us/op | regression-gated | `-` |
| `gpu.scene.controls.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1.376 | 1.715 | 1.778 | 1.785 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=25.020; draw_ms_median=1.292; draws_avg=24.000` |
| `gpu.scene.text_layout.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 4.761 | 5.276 | 5.344 | 5.354 | ms/frame | regression-gated | `culled_avg=6.000; damage_pct_avg=3.300; draw_ms_median=4.671; draws_avg=6.000` |
| `gpu.scene.zoom_image.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.661 | 1.245 | 1.895 | 2.287 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=80.960; draw_ms_median=0.564; draws_avg=4.000` |
| `gpu.scene.anim_timeline.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1.611 | 1.831 | 1.922 | 1.930 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=41.213; draw_ms_median=1.531; draws_avg=16.000` |
| `gpu.scene.collection.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.864 | 1.115 | 1.130 | 1.139 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=100.000; draw_ms_median=0.789; draws_avg=64.000` |
| `gpu.scene.damage_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 2.767 | 3.070 | 3.201 | 3.258 | ms/frame | regression-gated | `culled_avg=2.000; damage_pct_avg=3.300; draw_ms_median=2.682; draws_avg=6.000` |
| `gpu.scene.input_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 5.190 | 5.525 | 5.581 | 5.619 | ms/frame | regression-gated | `culled_avg=36.000; damage_pct_avg=3.300; draw_ms_median=5.089; draws_avg=6.000` |
| `gpu.scene.nine_slice.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.668 | 0.866 | 1.118 | 1.248 | ms/frame | regression-gated | `culled_avg=2.000; damage_pct_avg=3.300; draw_ms_median=0.598; draws_avg=2.000` |
| `gpu.scene.sdf_text.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1.288 | 1.584 | 1.651 | 1.695 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=3.300; draw_ms_median=1.212; draws_avg=4.000` |
| `gpu.scene.snapshot.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1.723 | 1.972 | 2.021 | 2.054 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=3.300; draw_ms_median=1.637; draws_avg=4.000` |
| `gpu.scene.camera.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.937 | 1.106 | 1.687 | 2.133 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=100.000; draw_ms_median=0.873; draws_avg=2.000` |
| `gpu.scene.elements_extended.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 5.173 | 7.361 | 9.354 | 10.417 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=100.000; draw_ms_median=4.946; draws_avg=32.000` |
| `gpu.scene.animation_config.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 11.358 | 12.104 | 12.789 | 12.955 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=100.000; draw_ms_median=11.092; draws_avg=114.000` |
| `gpu.scene.orchestration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 6.738 | 7.903 | 8.425 | 8.626 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=100.000; draw_ms_median=6.581; draws_avg=56.000` |
| `gpu.scene.permissions.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 15.191 | 15.864 | 16.265 | 16.493 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=100.000; draw_ms_median=14.912; draws_avg=178.000` |
| `gpu.scene.integration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 6.314 | 6.843 | 6.893 | 6.904 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=100.000; draw_ms_median=6.200; draws_avg=41.000` |
| `gpu.scene.stress.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 7.749 | 8.311 | 8.420 | 8.474 | ms/frame | regression-gated | `culled_avg=0.000; damage_pct_avg=100.000; draw_ms_median=7.610; draws_avg=64.000` |
| `cpu.journey.input_form_submit` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 8211.291 | 8380.602 | 8394.687 | 8398.208 | us/journey | regression-gated | `-` |
| `cpu.journey.collection_navigation` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 3146.312 | 3344.044 | 3361.609 | 3366.000 | us/journey | regression-gated | `-` |
| `cpu.journey.zoom_image_gesture_cycle` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1500.417 | 1711.916 | 1819.316 | 1846.166 | us/journey | regression-gated | `-` |
| `cpu.journey.orchestration_transition_modal` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 25008.938 | 26421.600 | 26551.320 | 26583.750 | us/journey | regression-gated | `-` |
| `cpu.journey.feed_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 603.042 | 678.637 | 686.527 | 688.500 | us/journey | regression-gated | `-` |
| `cpu.journey.thumbnail_grid_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1214.417 | 1361.637 | 1398.628 | 1407.875 | us/journey | regression-gated | `-` |
| `cpu.journey.chat_thread_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 1168.229 | 1244.146 | 1251.796 | 1253.709 | us/journey | regression-gated | `-` |
| `cpu.authoring.text_fields.edit_cycle` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 4.799 | 4.871 | 4.879 | 4.881 | us/op | regression-gated | `-` |
| `cpu.authoring.popup_wheel_picker.interaction` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 0.538 | 0.555 | 0.561 | 0.563 | us/op | regression-gated | `-` |
| `cpu.authoring.burst_emitter.sample` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 5.876 | 5.998 | 6.021 | 6.026 | us/op | regression-gated | `-` |
| `cpu.authoring.surface_router.compose` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 152.419 | 172.637 | 176.322 | 177.243 | us/op | regression-gated | `-` |
| `gpu.authoring.scene3d.mixed_frame` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 0.054 | 0.620 | 0.645 | 0.651 | ms/frame | regression-gated | `draws_avg=3.000; encode_ms_median=0.034; mesh_indices=7.000; mesh_vertices=7.000` |
| `cpu.layout.flat_grid.rotation_relayout` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 234.522 | 289.237 | 314.308 | 320.576 | us/op | regression-gated | `dirty_nodes=240.000; layout_passes=2.000` |
| `cpu.layout.deep_stack.theme_swap` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 81.973 | 90.700 | 93.664 | 94.405 | us/op | regression-gated | `dirty_nodes=60.000; layout_passes=2.000` |
| `cpu.layout.grid.safe_area_swap` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 207.807 | 330.412 | 357.808 | 364.658 | us/op | regression-gated | `dirty_nodes=180.000; layout_passes=3.000` |
| `cpu.text_input.large_editor.keystroke_burst` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 8861.552 | 9927.622 | 10291.912 | 10382.984 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.text_input.large_editor.paste_10kb` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 74.860 | 76.128 | 76.296 | 76.338 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.text_input.large_editor.selection_replace` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 621.609 | 631.244 | 632.192 | 632.430 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.image_pipeline.png.decode` | `engine` | `image-pipeline` | `oxide` | `cold` | `offscreen` | 945.589 | 960.760 | 967.182 | 968.788 | us/op | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `gpu.image_pipeline.png.upload` | `engine` | `image-pipeline` | `oxide` | `warm` | `offscreen` | 16.626 | 18.893 | 19.299 | 19.400 | us/upload | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `gpu.image_pipeline.png.first_visible` | `engine` | `image-pipeline` | `oxide` | `warm` | `offscreen` | 0.041 | 1.300 | 2.016 | 2.195 | ms/frame | regression-gated | `draw_calls=0.000; encode_ms_median=0.037; encoded_bytes=6797.000; texture_bytes=65536.000` |
| `cpu.navigation.button_press.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 69.254 | 72.200 | 73.422 | 73.727 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.navigation.slider_scrub.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 0.105 | 0.107 | 0.107 | 0.107 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.navigation.text_focus.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 6638.145 | 8647.929 | 10236.415 | 10633.536 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.reconcile.single_node_mutation` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 60.819 | 92.779 | 96.153 | 96.996 | us/op | regression-gated | `dirty_nodes=1.000; layout_passes=1.000` |
| `cpu.reconcile.tree_mutation_1pct` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 59.394 | 63.494 | 63.501 | 63.503 | us/op | regression-gated | `dirty_nodes=10.000; layout_passes=1.000` |
| `cpu.reconcile.tree_mutation_10pct` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 61.270 | 65.489 | 67.570 | 68.090 | us/op | regression-gated | `dirty_nodes=100.000; layout_passes=1.000` |
| `cpu.reconcile.theme_swap_full` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 264.128 | 270.095 | 271.428 | 271.761 | us/op | regression-gated | `dirty_nodes=1000.000; layout_passes=2.000` |
| `cpu.endurance.open_close_heavy_screen.100x` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 320093.823 | 372560.407 | 391837.348 | 396656.583 | us/op | regression-gated | `layout_passes=100.000` |
| `cpu.endurance.tab_switch_heavy.500x` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 1137529.479 | 1182298.466 | 1207353.010 | 1213616.646 | us/op | regression-gated | `layout_passes=500.000` |
| `cpu.endurance.idle_animation.600_frames` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 367956.302 | 374171.087 | 378282.301 | 379310.104 | us/op | regression-gated | `layout_passes=600.000` |
| `cpu.stress.flat_rects.10000.mount` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 3697.120 | 3911.597 | 3931.186 | 3936.083 | us/op | regression-gated | `dirty_nodes=10000.000; layout_passes=1.000` |
| `cpu.stress.simultaneous_animations.300` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 12.800 | 14.473 | 15.409 | 15.643 | us/op | regression-gated | `draw_calls=300.000` |
| `cpu.stress.ticker_100hz` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 307706.552 | 324786.361 | 337637.689 | 340850.520 | us/op | regression-gated | `layout_passes=100.000` |
| `cpu.bridge.permission_callback_fanout` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 1.251 | 1.375 | 1.466 | 1.488 | us/op | regression-gated | `-` |
| `cpu.bridge.sensor_location_snapshot` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 0.970 | 0.985 | 0.989 | 0.990 | us/op | regression-gated | `-` |
| `cpu.bridge.bluetooth_cache_update` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 4.995 | 5.083 | 5.099 | 5.103 | us/op | regression-gated | `-` |
| `cpu.bridge.photo_import_thumbnail` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 942.550 | 962.758 | 967.635 | 968.854 | us/op | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `cpu.bridge.file_import_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 7649.773 | 7806.404 | 7857.151 | 7869.837 | us/op | regression-gated | `dirty_nodes=32.000` |
| `cpu.bridge.share_payload_prepare` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 1561.042 | 1592.819 | 1597.726 | 1598.953 | us/op | regression-gated | `encoded_bytes=74.000` |
| `cpu.bridge.local_json_transport_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 165.106 | 176.980 | 177.141 | 177.182 | us/op | regression-gated | `dirty_nodes=48.000; encoded_bytes=2007.000` |
| `cpu.bridge.local_image_transport_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 3794.017 | 4253.966 | 4329.124 | 4347.913 | us/op | regression-gated | `encoded_bytes=26533.000; texture_bytes=262144.000` |

## A/B Audit

- prepare_draws: 1.68x faster than the retained legacy path
- coalesce_adjacent_draws: 4.48x faster than the retained legacy path

## Findings

- [fixed] DrawListBuilder::clear now clears retained vertex and index storage, eliminating stale geometry accumulation when builders are reused across frames.
- [fixed] ui-core::prepare_draws now keeps cumulative clip intersections on the stack instead of rebuilding the full stack on every ClipPop.
- [fixed] ui-core::coalesce_adjacent_draws now uses a single linear compaction pass instead of Vec::remove-based quadratic merging.
- [candidate] renderer-metal still encodes rounded rectangles one draw at a time with per-draw parameter binding; that remains the clearest GPU-side batching opportunity on real Metal targets.
- [candidate] The macOS glyph indirect-command-buffer path is now default-disabled because Metal validation exposed CPU access to private ICB storage and an invalid ICB pipeline configuration; restoring it with a truly valid text ICB path remains a high-value GPU follow-up.
- [candidate] Label wrapping still re-shapes tentative strings per word and clones intermediate Strings, which is likely the next CPU hotspot for text-heavy wrapped layouts.

## Baseline Workflow

- Update the committed baseline only with review: `PERF_REPORT_DATE=$(date +%F) cargo run --release -j$(sysctl -n hw.ncpu) -p oxide-perf-runner -- --run-suite --write-baseline`
- Latest JSON baseline: `benchmarks/workspace/latest.json`
- Latest Markdown baseline: `benchmarks/workspace/latest.md`
