# Sixteen-case Apple comparison contract

The Apple comparison harness has one named sixteen-case registry. It keeps shared
geometry, font bytes, image bytes, colors, state timelines, and checkpoint semantics
between the Oxide and UIKit adapters. It does not change Oxide production APIs or
renderer policy.

| Group | Cases | Workload coverage |
| --- | --- | --- |
| Core | `shapes`, `text`, `local`, `images`, `animation`, `scroll` | clipping/transparency, wrapped and changing text, local control updates, image crop/scale, card animation, and 1,000-row virtualization |
| Visual | `visual-controls`, `visual-editing`, `visual-typography`, `visual-composition`, `visual-layout`, `visual-pickers`, `visual-opacity`, `visual-images`, `visual-geometry`, `visual-editing-edges` | production controls/editing, shaping/wrapping, clips/layers, layout/collection mutation, picker dynamics, group opacity, image generation, fractional geometry, and input edge cases |

All content is 390 × 844 logical points at 3×. Core checkpoints are 0, 10, and
19.9 seconds; visual checkpoints are 0, 1, and 2. The visual review must retain
matching content, geometry, clipping, order, color, transitions, and resource
restoration. A faster result that changes these properties is invalid.

## Admission and recording

Before a physical recording, provide current-run external visual evidence to
`cargo xtask ios compare-core`. The runner validates the source hash, all selected
checkpoint hashes, and executable hashes when the evidence producer recorded them.
The energy sweep separately requires a human-reviewed external qualification bundle
whose established predicate is `normal_and_stall_probes_valid: true` and
`energy_captures_in_quartet: 4`. Callback timing, simulator output, GPU completion,
or a native-macOS receipt alone is insufficient presentation admission evidence.

A normal case uses five seconds warmup, twenty seconds measured, and five seconds
settled, with five alternating Oxide/UIKit pairs and at most one replacement pair.
Use a Release build, the same physical ProMotion iPhone, matched native-refresh
requests, fixed brightness, nominal thermal state, and no interaction. Keep all
outputs outside Git in a new directory. The scorecard records unavailable or blocked
metrics rather than manufacturing a comparison claim.

The reusable native-macOS background/offscreen tools are diagnostic. They preserve
the same workloads and resource/capture checks, but do not measure or claim iPhone
presentation latency. Foreground native-Mac receipts keep their own drawable and
presentation accounting.

The comparison harness preserves its shared fixtures, hosts, reducers, visual
goldens, and physical-device restrictions. Historical benchmark evidence is not an
admission dependency.
