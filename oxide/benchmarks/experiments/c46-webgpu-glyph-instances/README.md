# C46 — Compact WebGPU glyph instances

Status: accepted for the measured CPU lowering/upload bottleneck, with no GPU win claimed.

C46 replaces expanded WebGPU glyph vertices and indices with one 36-byte instance containing destination origin/extent, UV origin/extent, and the same RGBA8 color used by the prior WebGPU path. WGSL expands a shared four-corner triangle strip. Consecutive compatible glyph ranges batch by atlas page, A8/SDF pipeline, target, and clip while retaining command order. Dynamic draws, prepared chunk buffers, direct prepared replay, per-chunk bundles, and aggregate snapshot bundles all use the compact stream. Malformed or unsupported quad topology remains on the established fallback path.

The controlled 512-glyph comparison uses exact parent `a011c3d`, release wasm, native-arm64 Chrome, identical shared HTML instrumentation, six samples of 24 frames per fresh process, and 15 balanced alternating pairs. The parent and candidate wasm SHA-256 values are `5bac164645821dcfefe74285c7302eab3f0be011c5824bc9641eb9d82b38a815` and `b2b4c65f0469448f340ab9f83f0fa9c9d076a6e5e528e553efa9b1f6bf30a2e3` for the final proof package. The paired result is persisted in `paired-report.json`; aggregate browser-baseline promotion remains deferred to C62.

The final wasm package is 3,801,877 bytes versus 3,763,810 bytes (+38,067 bytes, +1.0%). That artifact includes the opt-in reproducible language-matrix builder and its two tiny fixture fonts; the one startup observation improved wasm init and first-frame time, but no startup win is claimed from a single process.

| Metric | Expanded parent | Compact candidate | Change |
| --- | ---: | ---: | ---: |
| Glyphs | 512 | 512 | identical |
| Glyph payload | 47,104 B | 18,432 B | -60.9% |
| Total frame upload | 47,140 B | 18,468 B | -60.8% |
| Draws / draw items | 65 / 65 | 3 / 3 | -95.4% |
| Pipeline / texture binds | 3 / 1 | 3 / 1 | unchanged |
| CPU submit p50 across processes | 0.050 ms | 0.046 ms | 9.8% paired median win |
| CPU submit p95 across processes | 0.084 ms | 0.057 ms | 32.6% paired median win |
| CPU submit p99 across processes | 0.084 ms | 0.057 ms | 32.6% paired median win |

The primary paired p50 speedup passed policy at 9.8%, with a 3.9%..14.6% bootstrap 95% confidence interval, 14 candidate wins, and one tie. The exact 6×24 single-process control was 0.040→0.030 ms p50 and 0.041→0.031 ms p95/p99, and reproduced the same counter changes.

Direct GPU timestamps do not support an isolated-row win claim. The last-completed timestamp was bimodal across fresh processes; the paired median change was -5.6%, its 95% confidence interval crossed zero (-9.1%..20.4%), and the candidate won 4/15 pairs. The independent 2,000-frame normal-app RAF population was also mixed: p50 presentation stayed 8.335 ms and 120 Hz hitches stayed zero, but p95/p99 presentation rose from 9.155/9.320 ms to 9.895/10.310 ms and CPU-submit tails worsened, while missed 120 Hz frames fell from 1,081 to 1,033 and GPU p50/p95/p99 improved from 0.196/0.248/0.402 ms to 0.192/0.242/0.293 ms. C46 is therefore accepted for the statistically supported isolated CPU/upload reduction, not presented as a universal GPU or frame-pacing win.

The opt-in browser language matrix uses the same Metal C45 construction: 1,000 wrapped and mixed-color Latin, RTL, CJK, and emoji labels; 19,854 glyphs; four atlas pages; and bitmap plus SDF text. It reproduces Metal's 1,347 source runs and 356 compact draws, while WebGPU records 714,744 glyph bytes, exactly 36 bytes per glyph versus Metal's 48. The exact parent/candidate 512×512 A8/SDF capture and the 1200×800 prepared-chunk capture both have zero differing pixels. Prepared lifecycle guardrails also pass with persistent bundle replay, structural cache reuse, segmentation, resource-generation invalidation, and budget behavior intact.

Reproduce the targeted evidence after packaging the desired wasm side:

```text
node scripts/check_webgpu_browser_golden.mjs --glyph-run-out /tmp/c46-glyph.json --chrome-arch arm64 --mixed-samples 6 --mixed-frames 24
node scripts/check_webgpu_browser_golden.mjs --glyph-matrix-out /tmp/c46-matrix.json --chrome-arch arm64 --mixed-samples 6 --mixed-frames 24
node scripts/c46_webgpu_glyph_paired_adapter.mjs RAW_PAIR_DIR paired-report.json PARENT_FULL.json CANDIDATE_FULL.json
```
