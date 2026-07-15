# C53 WebGPU retained damage

## Decision

Rejected against C52 parent `8a35c073118fffa46d15f45f96ce54bec4574fc4`.
No retained-damage production code remains.

The hypothesis was that a persistent WebGPU scene texture could replace a full
10,001-draw replay with a damage query, 601 intersecting draws, and one final
present for a 5% caret mutation. The provisional implementation did reduce
logical shaded pixels from 960,000 to 48,000 and culled 9,400 draws.

## Evidence

The first 15 balanced fresh-Chrome pairs retained 2,100 direct GPU timestamps
per side. Its provisional single-target path improved aggregate GPU p50 from
0.311620 to 0.126415 ms (59.43%), improved p95/p99 from
0.329828/0.344370 to 0.181832/0.205455 ms, and won all 15 pair medians.
It was not eligible: candidate peak was 0.857945 ms versus 0.392453 ms, and the
subsequent exact capture exposed undefined pixels outside the damage region.
Those samples remain evidence for a rejected, incorrect implementation only.

Explicit ping-pong and full-scene composite variants removed the undefined
region. The final composite capture was visually complete, but the unchanged
parent and candidate were not pixel-identical. Repeated 1200x800 captures were
deterministic on each side, while decoded RGBA SHA-256 values differed. The
direct/offscreen comparison measured SSIM 0.999998, PSNR 62.10 dB, and maximum
derived luma error 5. The same deterministic difference appeared on a full
bootstrap before partial redraw, so it could not be attributed to damage
selection or hidden with a tolerance change.

Raw throughput, guardrail, intermediate, and capture artifacts are preserved
under `raw/`. The strict correctness gate failed before the required doubled
peak sample and displayed-RAF populations became admissible.

## Cleanup

Removed the retained textures, adaptive cost model, clear/copy pipelines,
surface-copy usage, renderer statistics, browser exports, tests, and benchmark
adapter. Kept only this rejection record and ignored raw evidence.
