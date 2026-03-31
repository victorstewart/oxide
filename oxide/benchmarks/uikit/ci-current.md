# UIKit Perf Report

- Suite: `simulator`
- Device: `iPhone 16`
- Energy: True energy metrics are unavailable on iOS Simulator; Apple Power Profiler is unsupported there. CPU cycles are retained as the stable on-simulator energy proxy while direct device GPU and energy reports live under benchmarks/uikit-device/.
- CPU columns measure UIKit-side orchestration cost (layout, animation stepping, layer updates, command submission) around a GPU-backed rendering pipeline; they do not imply final rasterization happened on the CPU.
- Metrics reflect 10 XCTest iterations per case on the same simulator target used for CI.
- Baseline matches: `16`
- Missing baseline cases: `0`
- Regressions: `0`

## Case Table

| UIKit Case | Oxide Case | Clock ms | CPU orchestration ms | CPU cycles kC | CPU instr kI | RSS kB | Peak kB | Energy |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `uikit.animation.anim_timeline_bars` | `cpu.animation.anim_timeline_bars` | 0.818 | 1.219 | 3546.069 | 11348.096 | 16.384 | 30558.704 | `proxy 3546.069 kC` |
| `uikit.animation.button_press_scale` | `cpu.animation.button_press_scale` | 1.919 | 2.536 | 7439.769 | 20349.310 | 0.000 | 31754.760 | `proxy 7439.769 kC` |
| `uikit.animation.image_zoom_pan` | `cpu.animation.image_zoom_pan` | 0.829 | 1.141 | 3748.062 | 12565.307 | 0.000 | 32459.272 | `proxy 3748.062 kC` |
| `uikit.animation.progress_indeterminate` | `cpu.animation.progress_indeterminate` | 0.979 | 1.630 | 4998.447 | 13573.594 | 0.000 | 33671.688 | `proxy 4998.447 kC` |
| `uikit.animation.slider_thumb_move` | `cpu.animation.slider_thumb_move` | 2.466 | 3.243 | 10162.836 | 29565.502 | 0.000 | 33982.984 | `proxy 10162.836 kC` |
| `uikit.animation.spinner_spin` | `cpu.animation.spinner_spin` | 0.944 | 1.286 | 4167.746 | 14271.901 | 0.000 | 34064.904 | `proxy 4167.746 kC` |
| `uikit.animation.toggle_thumb_spring` | `cpu.animation.toggle_thumb_spring` | 0.834 | 1.183 | 3807.786 | 13353.833 | 0.000 | 34114.056 | `proxy 3807.786 kC` |
| `uikit.component.button.encode` | `cpu.component.button.encode` | 0.950 | 1.557 | 4826.158 | 12718.542 | 0.000 | 31296.008 | `proxy 4826.158 kC` |
| `uikit.component.collection_view.encode` | `cpu.component.collection_view.encode` | 0.288 | 0.634 | 2074.256 | 5931.094 | 0.000 | 32016.904 | `proxy 2074.256 kC` |
| `uikit.component.image_view.encode` | `cpu.component.image_view.encode` | 0.287 | 0.607 | 1953.756 | 5953.722 | 0.000 | 32360.968 | `proxy 1953.756 kC` |
| `uikit.component.label.encode` | `cpu.component.label.encode` | 4.485 | 5.143 | 16074.252 | 49955.125 | 16.384 | 33212.936 | `proxy 16074.252 kC` |
| `uikit.component.nine_slice_image.encode` | `cpu.component.nine_slice_image.encode` | 0.273 | 0.626 | 2048.149 | 5827.465 | 0.000 | 33573.384 | `proxy 2048.149 kC` |
| `uikit.component.progress_bar.encode` | `cpu.component.progress_bar.encode` | 0.911 | 1.500 | 4726.564 | 13259.964 | 0.000 | 33688.072 | `proxy 4726.564 kC` |
| `uikit.component.slider.encode` | `cpu.component.slider.encode` | 1.689 | 2.506 | 8136.003 | 21626.845 | 0.000 | 33851.912 | `proxy 8136.003 kC` |
| `uikit.component.spinner.encode` | `cpu.component.spinner.encode` | 0.557 | 0.870 | 2840.684 | 9574.964 | 0.000 | 34032.136 | `proxy 2840.684 kC` |
| `uikit.component.toggle.encode` | `cpu.component.toggle.encode` | 0.531 | 0.850 | 2808.966 | 8984.404 | 0.000 | 34097.672 | `proxy 2808.966 kC` |

## Comparison

- No UIKit perf regressions against the committed baseline.

## Notes

- Scheme: OxideUIKitPerf
- Harness: standalone iOS simulator XCTest bundle running UIKit parity views.
- True iOS energy capture remains device-only. The simulator report persists CPU cycles as the stable energy proxy; direct GPU and energy baselines live under `benchmarks/uikit-device/`.
