# Oxide Performance Report

- Suite: `full`
- Coverage: 9/9 components, 7/7 animations
- Comparison: `26` matched, `0` regressions, `0` missing baseline cases

## Results

| Case | Median | P95 | Unit | Gate |
| --- | ---: | ---: | --- | --- |
| `cpu.system.prepare_draws.current` | 3.932 | 4.068 | us/op | regression-gated |
| `cpu.system.prepare_draws.legacy` | 4.729 | 4.906 | us/op | audit-only |
| `cpu.system.coalesce_adjacent_draws.current` | 3.953 | 4.181 | us/op | regression-gated |
| `cpu.system.coalesce_adjacent_draws.legacy` | 172.653 | 179.846 | us/op | audit-only |
| `cpu.system.gesture_sequence` | 0.272 | 0.282 | us/op | regression-gated |
| `cpu.system.text_shape_bake` | 19.358 | 20.428 | us/op | regression-gated |
| `cpu.component.label.encode` | 43.455 | 44.367 | us/op | regression-gated |
| `cpu.component.progress_bar.encode` | 0.005 | 0.006 | us/op | regression-gated |
| `cpu.component.spinner.encode` | 0.002 | 0.002 | us/op | regression-gated |
| `cpu.component.button.encode` | 2.873 | 3.021 | us/op | regression-gated |
| `cpu.component.toggle.encode` | 0.006 | 0.011 | us/op | regression-gated |
| `cpu.component.slider.encode` | 0.007 | 0.008 | us/op | regression-gated |
| `cpu.component.image_view.encode` | 0.003 | 0.003 | us/op | regression-gated |
| `cpu.component.nine_slice_image.encode` | 0.002 | 0.002 | us/op | regression-gated |
| `cpu.component.collection_view.encode` | 2.335 | 2.392 | us/op | regression-gated |
| `cpu.animation.spinner_spin` | 0.005 | 0.006 | us/op | regression-gated |
| `cpu.animation.progress_indeterminate` | 0.006 | 0.007 | us/op | regression-gated |
| `cpu.animation.button_press_scale` | 2.453 | 2.558 | us/op | regression-gated |
| `cpu.animation.toggle_thumb_spring` | 0.007 | 0.007 | us/op | regression-gated |
| `cpu.animation.slider_thumb_move` | 0.008 | 0.008 | us/op | regression-gated |
| `cpu.animation.image_zoom_pan` | 0.007 | 0.007 | us/op | regression-gated |
| `cpu.animation.anim_timeline_bars` | 8.227 | 8.702 | us/op | regression-gated |
| `cpu.scene.controls.frame` | 23.220 | 24.060 | us/op | regression-gated |
| `cpu.scene.collection.frame` | 13.532 | 14.354 | us/op | regression-gated |
| `cpu.scene.stress.frame` | 105.218 | 107.088 | us/op | regression-gated |
| `gpu.scene.controls.frame` | 2.103 | 6.744 | ms/frame | regression-gated |
| `gpu.scene.collection.frame` | 1.174 | 6.165 | ms/frame | regression-gated |
| `gpu.scene.stress.frame` | 8.362 | 8.796 | ms/frame | regression-gated |

## A/B Audit

- prepare_draws: 1.20x faster than the retained legacy path
- coalesce_adjacent_draws: 43.68x faster than the retained legacy path

## Regression Check

- No gated regressions detected.

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
