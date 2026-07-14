# C45 — Compact Metal glyph instances

Status: accepted for the CPU command-encoding bottleneck, with an explicit GPU tradeoff.

C45 replaces four expanded vertices, six uploaded indices, and one color uniform per glyph run with one 48-byte glyph instance. Consecutive runs batch by atlas page and bitmap/SDF pipeline while preserving command order, clip boundaries, target, exact float color, and retained property transforms. The vertex shader expands an immutable four-corner triangle strip, so each glyph uses four vertex invocations without a per-frame index upload. Prepared chunks persist one compact instance buffer and coalesce consecutive damage-selected ranges. The disabled per-frame glyph indirect-command-buffer path, its environment switch, pipeline flags, resource creation, and retry logic are deleted.

The exact release comparison uses the C44 parent plus benchmark-only instrumentation against candidate tree `4b36dce691b784683b1da9eac8976f6efb80433d`, 20 balanced fresh-process pairs, 100 measured frames and three warmups per side. Both sides render 1,000 wrapped, mixed-color Latin/RTL/CJK/emoji labels, 19,854 glyphs, bitmap and SDF text, and four atlas pages at identical pixels.

| Metric | Expanded control | Compact instances | Change |
| --- | ---: | ---: | ---: |
| Glyph record/upload bytes | 1,848,120 | 952,992 | -48.4% |
| Bytes per glyph | 93.09 | 48.00 | -48.4% |
| Draws | 1,347 | 356 | -73.6% |
| Explicit glyph buffer binds | 2,694 | 356 | -86.8% |
| CPU encode p50 | 0.312875 ms | 0.252251 ms | -19.4% |
| CPU encode p95 | 0.753900 ms | 0.422963 ms | -43.9% |
| CPU encode p99 | 1.297332 ms | 0.985216 ms | -24.1% |
| Whole-frame p50 | 1.565334 ms | 1.515479 ms | -3.2% |
| Whole-frame p95 | 2.428656 ms | 2.147302 ms | -11.6% |
| Whole-frame p99 | 2.598100 ms | 2.260775 ms | -13.0% |

The CPU encode population passed the performance policy with 18.9727% paired median speedup (95% CI 17.6870%..20.5833%, 20/20 wins). The whole-frame population improved 3.1163% (20/20 wins, CI 2.8925%..3.5908%) and every reported tail, but is retained as a strict-policy rejection because that policy requires at least 5% median speedup.

Direct in-app Metal GPU duration increased in the final population (p50 0.111167 to 0.137458 ms, p95 0.232042 to 0.261375 ms, p99 0.248167 to 0.281625 ms). The total frame still improves, especially in the tails, so C45 is accepted for its measured CPU bottleneck rather than presented as a universal GPU win. Exact IME, grapheme, CJK fallback, SDF, prepared-layer, damage, recycling, and mixed-buffer snapshots remain pixel-identical. Aggregate baseline promotion remains deferred to C62.
