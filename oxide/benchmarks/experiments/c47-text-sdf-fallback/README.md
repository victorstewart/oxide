# C47 — SDF and fallback preparation

Status: accepted for the measured text-preparation bottlenecks.

C47 replaces the duplicated 17x17 per-texel SDF neighborhood searches with one shared exact two-pass squared Euclidean distance transform. `RasterCtx` retains the Swash output image and every EDT/output buffer, and `Font` retains the stable parsed Swash offset/key so `ScaleContext` can reuse its font cache. Fallback shaping keeps bounded coverage and scalar/multi-codepoint decision caches for one exact font-generation, primary-font, scale, feature, and fallback-chain context; a context change clears them. The retired brute-force implementation exists only in the test oracle.

The acceptance population compares exact parent `a3625b4` with the final candidate source using two existing production perf cases. Seven alternating parent/candidate process pairs each contain the runner's 12 full-suite samples. Both rows won all seven pairs.

| Workload | Parent process p50 range | Candidate process p50 range | Paired p50 win | Paired p95 / p99 win |
| --- | ---: | ---: | ---: | ---: |
| Five-script cold screen/fallback matrix | 248.4–254.7 us | 80.0–103.5 us | 68.0% (95% CI 67.5–68.3%) | 68.2% / 68.2% |
| 2x/3x by 48/96 px SDF matrix | 1,941.0–1,980.8 us | 150.2–154.5 us | 92.3% (95% CI 92.2–92.3%) | 92.2% / 92.2% |

The isolated 31-sample phase probe records the tradeoffs that the screen-level cases can hide:

| Phase | Parent p50 / p95 / p99 | Candidate p50 / p95 / p99 | Allocations / bytes per op |
| --- | ---: | ---: | ---: |
| One-shot cold fallback | 18.33 / 21.61 / 23.41 us | 20.10 / 22.56 / 23.51 us | 29 / 4,184 B -> 30 / 4,216 B |
| Repeated warm fallback | 17.68 / 18.98 / 20.66 us | 6.15 / 7.20 / 8.11 us | 29 / 4,184 B -> 23 / 2,784 B |
| Cold SDF scratch | 1,317.52 / 1,399.84 / 1,420.58 us | 70.77 / 73.77 / 74.88 us | 13 / 6,645 B -> 19 / 42,038 B |
| Warm SDF scratch | 1,312.99 / 1,370.35 / 1,388.20 us | 67.72 / 72.93 / 79.04 us | 6 / 5,157 B -> 0 / 0 B |

The one-shot fallback p50 regression is 9.7%, while p95 is 4.4% slower and p99 is effectively unchanged; it is accepted as a bounded cold-cache setup cost because the representative cold screen improves 68.0%, subsequent calls improve materially, and the cold allocation footprint is essentially unchanged. The exact EDT deliberately allocates more scratch on its first large glyph; it converts that bounded capacity into a 94.6% isolated cold-time win and zero warm allocations.

Correctness uses a predeclared zero-byte SDF tolerance. The exact implementation matches the reference for holes, thin strokes, edge contact, every rendered Latin/CJK fixture glyph, both 2x/3x scale-pressure variants, and 48/96 px sizes. Because the atlas SDF bytes are identical, the established A8/SDF shader input and rendered text golden remain byte-for-byte unchanged. Fallback tests also prove font-database and fallback-chain invalidation; the cache key includes scale and the fixed empty feature set.

Reproduce the production rows with:

```text
OXIDE_PERF_RUNNER_FILTER=cpu.architecture.text.script_fallback_matrix,cpu.architecture.text.scale_sdf_matrix ./target/release/oxide-perf-runner --run-suite --json-out /tmp/c47.json
cargo test --locked -p oxide-text --tests
```
