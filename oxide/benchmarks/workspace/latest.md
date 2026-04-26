# Oxide Performance Report

- Suite: `full`
- Label: `2026-04-25`
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
| `cpu.system.prepare_draws.current` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 5.290 | 5.693 | 5.753 | 5.769 | us/op | regression-gated | `-` |
| `cpu.system.prepare_draws.legacy` | `engine` | `audit-baseline` | `legacy-baseline` | `warm` | `offscreen` | 4.608 | 4.671 | 4.678 | 4.680 | us/op | audit-only | `-` |
| `cpu.system.coalesce_adjacent_draws.current` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 3.075 | 3.226 | 3.293 | 3.309 | us/op | regression-gated | `-` |
| `cpu.system.coalesce_adjacent_draws.legacy` | `engine` | `audit-baseline` | `legacy-baseline` | `warm` | `offscreen` | 173.376 | 174.715 | 174.839 | 174.870 | us/op | audit-only | `-` |
| `cpu.system.gesture_sequence` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 0.147 | 0.151 | 0.153 | 0.153 | us/op | regression-gated | `-` |
| `cpu.system.touch_surface_sequence` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 0.363 | 0.371 | 0.371 | 0.371 | us/op | regression-gated | `-` |
| `cpu.system.timer_schedule_advance` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 1.886 | 1.907 | 1.916 | 1.918 | us/op | regression-gated | `-` |
| `cpu.system.anim_start_replace` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 2.494 | 2.527 | 2.528 | 2.529 | us/op | regression-gated | `-` |
| `cpu.system.text_shape_bake` | `engine` | `system` | `oxide` | `warm` | `offscreen` | 20.048 | 21.396 | 22.429 | 22.687 | us/op | regression-gated | `-` |
| `cpu.component.label.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 11.095 | 11.487 | 11.591 | 11.617 | us/op | regression-gated | `-` |
| `cpu.component.progress_bar.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.006 | 0.006 | 0.006 | 0.006 | us/op | regression-gated | `-` |
| `cpu.component.spinner.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.002 | 0.002 | 0.002 | 0.002 | us/op | regression-gated | `-` |
| `cpu.component.button.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.339 | 0.352 | 0.354 | 0.355 | us/op | regression-gated | `-` |
| `cpu.component.toggle.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.006 | 0.006 | 0.006 | 0.006 | us/op | regression-gated | `-` |
| `cpu.component.slider.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.007 | 0.008 | 0.008 | 0.008 | us/op | regression-gated | `-` |
| `cpu.component.image_view.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.003 | 0.003 | 0.003 | 0.003 | us/op | regression-gated | `-` |
| `cpu.component.nine_slice_image.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.002 | 0.003 | 0.003 | 0.003 | us/op | regression-gated | `-` |
| `cpu.component.collection_view.encode` | `engine` | `component` | `oxide` | `warm` | `offscreen` | 0.614 | 0.637 | 0.639 | 0.640 | us/op | regression-gated | `-` |
| `cpu.animation.spinner_spin` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.002 | 0.002 | 0.002 | 0.002 | us/op | regression-gated | `-` |
| `cpu.animation.progress_indeterminate` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.006 | 0.007 | 0.007 | 0.007 | us/op | regression-gated | `-` |
| `cpu.animation.button_press_scale` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.220 | 0.225 | 0.226 | 0.226 | us/op | regression-gated | `-` |
| `cpu.animation.toggle_thumb_spring` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.007 | 0.007 | 0.007 | 0.007 | us/op | regression-gated | `-` |
| `cpu.animation.slider_thumb_move` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.008 | 0.009 | 0.009 | 0.009 | us/op | regression-gated | `-` |
| `cpu.animation.image_zoom_pan` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 0.007 | 0.007 | 0.007 | 0.007 | us/op | regression-gated | `-` |
| `cpu.animation.anim_timeline_bars` | `engine` | `animation` | `oxide` | `warm` | `offscreen` | 1.045 | 1.081 | 1.098 | 1.102 | us/op | regression-gated | `-` |
| `cpu.launch.simple_home.cold_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 131.771 | 176.236 | 188.581 | 191.667 | us/journey | regression-gated | `-` |
| `cpu.launch.heavy_home.cold_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 83.291 | 91.950 | 92.790 | 93.000 | us/journey | regression-gated | `-` |
| `cpu.launch.detail.deep_link_launch` | `flow` | `launch-lifecycle` | `oxide` | `cold` | `offscreen` | 391.167 | 480.910 | 501.115 | 506.166 | us/journey | regression-gated | `-` |
| `cpu.launch.simple_home.warm_resume` | `flow` | `launch-lifecycle` | `oxide` | `warm` | `offscreen` | 130.916 | 145.287 | 150.657 | 152.000 | us/journey | regression-gated | `-` |
| `cpu.launch.heavy_home.foreground_after_background` | `flow` | `launch-lifecycle` | `oxide` | `warm` | `offscreen` | 91.480 | 108.814 | 116.629 | 118.583 | us/journey | regression-gated | `-` |
| `cpu.primitive.empty_root.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 21.774 | 24.832 | 26.009 | 26.304 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 22.200 | 24.735 | 25.608 | 25.826 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 28.486 | 30.052 | 30.167 | 30.196 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.1000.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 64.249 | 69.808 | 73.280 | 74.148 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.10.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.087 | 0.089 | 0.089 | 0.089 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.724 | 0.749 | 0.753 | 0.754 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.1000.mutate_fill` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 8.715 | 8.803 | 8.817 | 8.820 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.remove_all` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 3.469 | 3.565 | 3.587 | 3.592 | us/op | regression-gated | `-` |
| `cpu.primitive.flat_rects.100.remount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 4.058 | 4.176 | 4.215 | 4.225 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 44.351 | 46.333 | 46.833 | 46.958 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 440.149 | 462.372 | 464.152 | 464.597 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.1000.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 4357.219 | 4487.441 | 4497.772 | 4500.355 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.10.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 44.748 | 47.478 | 48.016 | 48.150 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.100.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 450.836 | 519.681 | 552.694 | 560.947 | us/op | regression-gated | `-` |
| `cpu.primitive.labels.1000.mutate_text` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 4366.084 | 4960.963 | 5133.259 | 5176.333 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.053 | 0.054 | 0.055 | 0.055 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.495 | 0.507 | 0.511 | 0.511 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.10.mutate_palette` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.053 | 0.055 | 0.055 | 0.055 | us/op | regression-gated | `-` |
| `cpu.primitive.cards.100.mutate_palette` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.486 | 0.492 | 0.493 | 0.493 | us/op | regression-gated | `-` |
| `cpu.primitive.images.10.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.056 | 0.058 | 0.058 | 0.058 | us/op | regression-gated | `-` |
| `cpu.primitive.images.100.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.367 | 0.377 | 0.377 | 0.377 | us/op | regression-gated | `-` |
| `cpu.primitive.images.10.mutate_alpha` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.030 | 0.031 | 0.031 | 0.031 | us/op | regression-gated | `-` |
| `cpu.primitive.images.100.mutate_alpha` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 0.285 | 0.310 | 0.323 | 0.326 | us/op | regression-gated | `-` |
| `cpu.primitive.control_set.mount` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 51.470 | 53.457 | 53.584 | 53.615 | us/op | regression-gated | `-` |
| `cpu.primitive.control_set.mutate_state` | `engine` | `primitive-lifecycle` | `oxide` | `warm` | `offscreen` | 6.843 | 7.148 | 7.272 | 7.303 | us/op | regression-gated | `-` |
| `cpu.scene.controls.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 15.509 | 15.939 | 15.997 | 16.011 | us/op | regression-gated | `-` |
| `cpu.scene.text_layout.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 32.815 | 34.465 | 34.605 | 34.640 | us/op | regression-gated | `-` |
| `cpu.scene.zoom_image.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 9.296 | 9.683 | 9.683 | 9.683 | us/op | regression-gated | `-` |
| `cpu.scene.anim_timeline.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 12.329 | 12.888 | 12.909 | 12.915 | us/op | regression-gated | `-` |
| `cpu.scene.collection.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 15.782 | 16.374 | 16.438 | 16.454 | us/op | regression-gated | `-` |
| `cpu.scene.damage_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 25.500 | 26.420 | 26.501 | 26.521 | us/op | regression-gated | `-` |
| `cpu.scene.input_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 57.446 | 59.552 | 59.900 | 59.987 | us/op | regression-gated | `-` |
| `cpu.scene.nine_slice.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 11.622 | 12.407 | 12.778 | 12.870 | us/op | regression-gated | `-` |
| `cpu.scene.sdf_text.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 15.455 | 16.037 | 16.215 | 16.259 | us/op | regression-gated | `-` |
| `cpu.scene.snapshot.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 20.874 | 21.337 | 21.375 | 21.385 | us/op | regression-gated | `-` |
| `cpu.scene.camera.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 19.447 | 19.740 | 19.751 | 19.753 | us/op | regression-gated | `-` |
| `cpu.scene.elements_extended.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 49.173 | 50.738 | 50.968 | 51.026 | us/op | regression-gated | `-` |
| `cpu.scene.animation_config.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 89.632 | 93.106 | 93.730 | 93.886 | us/op | regression-gated | `-` |
| `cpu.scene.orchestration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 62.370 | 64.720 | 65.696 | 65.939 | us/op | regression-gated | `-` |
| `cpu.scene.permissions.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 118.076 | 123.060 | 123.458 | 123.558 | us/op | regression-gated | `-` |
| `cpu.scene.integration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 53.928 | 56.523 | 56.722 | 56.772 | us/op | regression-gated | `-` |
| `cpu.scene.stress.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 61.985 | 64.721 | 65.183 | 65.299 | us/op | regression-gated | `-` |
| `gpu.scene.controls.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.099 | 3.441 | 4.770 | 5.508 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=25.020; damage_prefilter_thresh=0.250` |
| `gpu.scene.text_layout.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.273 | 2.042 | 2.997 | 3.062 | ms/frame | regression-gated | `culled_avg=6.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.zoom_image.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.304 | 2.618 | 2.990 | 3.079 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=80.960; damage_prefilter_thresh=0.250` |
| `gpu.scene.anim_timeline.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.246 | 2.141 | 2.897 | 2.957 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=41.213; damage_prefilter_thresh=0.250` |
| `gpu.scene.collection.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.246 | 2.155 | 3.115 | 3.317 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.damage_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.272 | 2.865 | 2.921 | 2.942 | ms/frame | regression-gated | `culled_avg=2.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.input_lab.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.463 | 0.651 | 0.841 | 0.969 | ms/frame | regression-gated | `culled_avg=36.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.nine_slice.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.187 | 2.222 | 2.818 | 2.862 | ms/frame | regression-gated | `culled_avg=2.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.sdf_text.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.284 | 2.695 | 2.904 | 2.908 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.snapshot.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.407 | 2.120 | 2.610 | 2.860 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=3.300; damage_prefilter_thresh=0.250` |
| `gpu.scene.camera.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.084 | 2.525 | 3.191 | 3.436 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.elements_extended.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.396 | 2.004 | 2.578 | 2.859 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.animation_config.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.336 | 2.525 | 2.890 | 3.029 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.orchestration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.393 | 1.956 | 2.847 | 2.919 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.permissions.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.238 | 2.426 | 2.738 | 2.786 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.integration.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.243 | 2.532 | 2.929 | 2.959 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `gpu.scene.stress.frame` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 0.341 | 2.113 | 2.619 | 2.702 | ms/frame | regression-gated | `culled_avg=0.000; damage_enabled=1.000; damage_pct_avg=100.000; damage_prefilter_thresh=0.250` |
| `cpu.journey.input_form_submit` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 454.562 | 528.816 | 557.196 | 564.291 | us/journey | regression-gated | `-` |
| `cpu.journey.collection_navigation` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 147.103 | 152.170 | 152.500 | 152.583 | us/journey | regression-gated | `-` |
| `cpu.journey.zoom_image_gesture_cycle` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 121.166 | 146.625 | 150.225 | 151.125 | us/journey | regression-gated | `-` |
| `cpu.journey.orchestration_transition_modal` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 650.875 | 722.750 | 722.750 | 722.750 | us/journey | regression-gated | `-` |
| `cpu.journey.feed_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 12.646 | 14.719 | 15.544 | 15.750 | us/journey | regression-gated | `-` |
| `cpu.journey.thumbnail_grid_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 36.438 | 39.315 | 40.530 | 40.834 | us/journey | regression-gated | `-` |
| `cpu.journey.chat_thread_scroll_matrix` | `flow` | `screen-flow` | `oxide` | `warm` | `offscreen` | 24.375 | 24.513 | 24.603 | 24.625 | us/journey | regression-gated | `-` |
| `cpu.authoring.text_fields.edit_cycle` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 0.875 | 0.885 | 0.886 | 0.886 | us/op | regression-gated | `-` |
| `cpu.authoring.popup_wheel_picker.interaction` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 0.067 | 0.069 | 0.069 | 0.069 | us/op | regression-gated | `-` |
| `cpu.authoring.burst_emitter.sample` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 0.680 | 0.698 | 0.698 | 0.698 | us/op | regression-gated | `-` |
| `cpu.authoring.surface_router.compose` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 132.080 | 143.307 | 146.983 | 147.902 | us/op | regression-gated | `-` |
| `gpu.authoring.scene3d.mixed_frame` | `engine` | `authoring` | `oxide` | `warm` | `offscreen` | 0.019 | 0.522 | 0.697 | 0.741 | ms/frame | regression-gated | `draws_avg=2.000; encode_ms_median=0.018; mesh_indices=7.000; mesh_vertices=7.000` |
| `cpu.layout.flat_grid.rotation_relayout` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 37.765 | 41.142 | 41.234 | 41.257 | us/op | regression-gated | `dirty_nodes=240.000; layout_passes=2.000` |
| `cpu.layout.deep_stack.theme_swap` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 54.544 | 89.810 | 105.788 | 109.782 | us/op | regression-gated | `dirty_nodes=60.000; layout_passes=2.000` |
| `cpu.layout.grid.safe_area_swap` | `engine` | `layout-invalidation` | `oxide` | `warm` | `offscreen` | 35.585 | 39.982 | 42.372 | 42.970 | us/op | regression-gated | `dirty_nodes=180.000; layout_passes=3.000` |
| `cpu.text_input.large_editor.keystroke_burst` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 2.201 | 2.242 | 2.243 | 2.243 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.text_input.large_editor.paste_10kb` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 0.748 | 0.763 | 0.765 | 0.765 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.text_input.large_editor.selection_replace` | `engine` | `text-input` | `oxide` | `warm` | `offscreen` | 4.566 | 4.760 | 4.831 | 4.849 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.image_pipeline.png.decode` | `engine` | `image-pipeline` | `oxide` | `cold` | `offscreen` | 44.306 | 48.972 | 49.714 | 49.900 | us/op | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `gpu.image_pipeline.png.upload` | `engine` | `image-pipeline` | `oxide` | `warm` | `offscreen` | 14.337 | 16.289 | 16.354 | 16.371 | us/upload | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `gpu.image_pipeline.png.first_visible` | `engine` | `image-pipeline` | `oxide` | `warm` | `offscreen` | 0.013 | 1.381 | 1.389 | 1.392 | ms/frame | regression-gated | `draw_calls=0.000; encode_ms_median=0.012; encoded_bytes=6797.000; texture_bytes=65536.000` |
| `cpu.navigation.button_press.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 0.284 | 0.323 | 0.335 | 0.338 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.navigation.slider_scrub.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 0.008 | 0.009 | 0.009 | 0.009 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.navigation.text_focus.response` | `flow` | `navigation-input` | `oxide` | `warm` | `offscreen` | 323.422 | 352.200 | 368.233 | 372.241 | us/op | regression-gated | `dirty_nodes=1.000` |
| `cpu.reconcile.single_node_mutation` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 6.354 | 6.526 | 6.534 | 6.535 | us/op | regression-gated | `dirty_nodes=1.000; layout_passes=1.000` |
| `cpu.reconcile.tree_mutation_1pct` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 6.403 | 6.623 | 6.658 | 6.667 | us/op | regression-gated | `dirty_nodes=10.000; layout_passes=1.000` |
| `cpu.reconcile.tree_mutation_10pct` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 6.392 | 6.642 | 6.676 | 6.685 | us/op | regression-gated | `dirty_nodes=100.000; layout_passes=1.000` |
| `cpu.reconcile.theme_swap_full` | `engine` | `state-reconcile` | `oxide` | `warm` | `offscreen` | 15.488 | 15.858 | 15.903 | 15.914 | us/op | regression-gated | `dirty_nodes=1000.000; layout_passes=2.000` |
| `cpu.endurance.open_close_heavy_screen.100x` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 6335.917 | 6517.652 | 6618.430 | 6643.625 | us/op | regression-gated | `layout_passes=100.000` |
| `cpu.endurance.tab_switch_heavy.500x` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 23276.896 | 24018.163 | 24198.683 | 24243.812 | us/op | regression-gated | `layout_passes=500.000` |
| `cpu.endurance.idle_animation.600_frames` | `flow` | `endurance-thermal` | `oxide` | `warm` | `offscreen` | 7891.740 | 8058.008 | 8059.768 | 8060.208 | us/op | regression-gated | `layout_passes=600.000` |
| `cpu.stress.flat_rects.10000.mount` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 401.383 | 415.575 | 415.943 | 416.035 | us/op | regression-gated | `dirty_nodes=10000.000; layout_passes=1.000` |
| `cpu.stress.simultaneous_animations.300` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 2.205 | 2.234 | 2.242 | 2.244 | us/op | regression-gated | `draw_calls=300.000` |
| `cpu.stress.ticker_100hz` | `engine` | `stress-pathological` | `oxide` | `warm` | `offscreen` | 6770.750 | 7092.979 | 7104.162 | 7106.958 | us/op | regression-gated | `layout_passes=100.000` |
| `cpu.bridge.permission_callback_fanout` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 0.071 | 0.074 | 0.075 | 0.075 | us/op | regression-gated | `-` |
| `cpu.bridge.sensor_location_snapshot` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 0.089 | 0.091 | 0.091 | 0.091 | us/op | regression-gated | `-` |
| `cpu.bridge.bluetooth_cache_update` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 1.120 | 1.162 | 1.178 | 1.182 | us/op | regression-gated | `-` |
| `cpu.bridge.photo_import_thumbnail` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 47.612 | 49.105 | 49.448 | 49.534 | us/op | regression-gated | `encoded_bytes=6797.000; texture_bytes=65536.000` |
| `cpu.bridge.file_import_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 303.983 | 314.636 | 314.999 | 315.090 | us/op | regression-gated | `dirty_nodes=32.000` |
| `cpu.bridge.share_payload_prepare` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 66.975 | 68.690 | 68.786 | 68.810 | us/op | regression-gated | `encoded_bytes=74.000` |
| `cpu.bridge.local_json_transport_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 40.370 | 42.710 | 43.218 | 43.345 | us/op | regression-gated | `dirty_nodes=48.000; encoded_bytes=2007.000` |
| `cpu.bridge.local_image_transport_render` | `bridge` | `os-bridge` | `oxide` | `warm` | `offscreen` | 149.949 | 157.480 | 158.277 | 158.476 | us/op | regression-gated | `encoded_bytes=26533.000; texture_bytes=262144.000` |

## A/B Audit

- prepare_draws: 0.87x faster than the retained legacy path
- coalesce_adjacent_draws: 56.38x faster than the retained legacy path

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
