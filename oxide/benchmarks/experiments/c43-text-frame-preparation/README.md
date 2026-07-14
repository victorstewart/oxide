# C43 — Frame-scoped text preparation

Status: accepted with an explicit warm-CPU tradeoff.

The C42 parent (`ec8c4de`) published A8 atlas changes while each label was encoded. C43 starts one text frame before UI draw-list construction, shapes and rasterizes only cache misses, prevents eviction while visible runs are being prepared, unions atlas damage under a 75% full-upload policy, and publishes at most one atlas update before renderer encoding. A newly created atlas uses provisional glyph handles that are patched in place at frame finish, preserving command order and retained draw-list identity.

The opt-in profiler exposes visible-label, shaping, rasterization, layout-cache, glyph-cache, atlas upload, eviction, and invalidated-run counters. Its storage is boxed and disabled on the normal path. The warm 1,000-label proof reports zero shaping, rasterization, atlas uploads, and measured-loop allocations; the 200-new-label proof reports one atlas publication. Mixed Latin, CJK, RTL, emoji, fallback-font, scale, and SDF matrices exercise the same frame boundary.

The final production-path Metal comparison uses one exact candidate binary for both the immediate control and frame-scoped candidate, an Apple M2 Max on macOS 26.5.2, and 20 fixed-seed balanced fresh-process pairs. Each side retains 100 measured frames as five bounded 20-frame blocks; every block begins with the same unmeasured 16-frame immediate-path conditioning workload and then three mode-specific warmups. The repository analyzer retains all 2,000 whole-frame samples per side and applies its no-material-regression p50/p95/p99/peak policy. The structural result is deterministic:

| 200-new-label metric | Immediate control | Frame scoped | Change |
| --- | ---: | ---: | ---: |
| Atlas creates | 1 | 1 | unchanged |
| Atlas updates | 17 | 0 | removed |
| Total atlas calls | 18 | 1 | -94.4% |
| Atlas upload bytes | 1,051,132 B | 1,048,576 B | -0.24% |
| Geometry upload | 95,200 B | 95,200 B | unchanged |
| Draws | 200 | 200 | unchanged |

A separate 20-pair parent/candidate CPU population found the cold 200-label path neutral to slightly faster, while the warm 1,000-label path paid about 1.7% for the frame boundary. That warm cost is accepted because the path remains allocation-free with zero text or upload work, the regression is below the predeclared 3% material threshold, and cold frames remove 17 synchronous atlas publications without changing visual work. Aggregate workspace promotion remains deferred to C62.

Rejected iterations are retained under the ignored `raw/` directory. A queued two-phase architecture regressed warm CPU by 41%–89%; a generic callback bridge regressed about 33%; always-on counters added roughly 5%–7% warm and 3.5% cold; a shared tiny-alphabet workload exercised only two atlas calls and could not prove the intended bottleneck; and an expanded 712-byte `TextCtx` layout regressed warm CPU by about 4%. The first exact-tree 1×100-frame population also failed only the raw peak gate after late-burst preparation tails, despite neutral p50 and acceptable p95/p99; it remains rejected rather than filtering the peak. The accepted design keeps only one direct frame flag hot and moves optional diagnostics into boxed cold state.

Verification observed:

- `cargo test -p oxide-text`: shaping, fallback, cache, atlas-lock, mixed-script, and scale tests passed.
- `cargo test -p oxide-ui-core`: frame preparation, provisional-handle ordering, one-upload, zero-warm-work, and allocation tests passed.
- `cargo test -p oxide-perf-runner`: the C43 report contract and existing suite tests passed.
- `cargo test -p oxide-renderer-metal --features snapshot-tests`: all Metal contract and pixel-readback snapshots passed.
- Exact text goldens for SDF text, Unicode, IME composition, grapheme selection, and CJK fallback remained byte-identical.
- The broad test-scenes slice passed except three pre-existing zoom-image expectations that fail unchanged on the C42 parent; the focused C43 and exact-golden slices are green.

The ignored `raw/` directory retains preliminary and exact-tree CPU/Metal populations, indexed prepare/encode/GPU samples, counter payloads, paired inputs, and analyzer reports. The tracked evidence adapter preserves each complete suite report while converting indexed metrics into the arrays validated by the shared paired runner. Candidate source, instrumentation, adapter, and binary hashes are recorded in those reports so the committed tree can be checked without embedding a cyclic Git tree identity here.
