# C48 — Remove the per-span bitmap text renderer

Status: accepted.

C48 deletes the renderer-owned per-span bitmap-text rasterizer. Callers rasterize through the shared text stack, publish A8 atlas content, and emit `GlyphRun` commands. That leaves one shaping/rasterization owner, preserves cache reuse, and lets the compact glyph-instance paths from C45/C46 batch the result.

The final 15-pair CPU population moved p50 from 11.6124 to 1.4804 us/op, an 87.250% paired median speedup with a 95% confidence interval of 87.067%..87.355% and 15/15 wins. P95, p99, and peak all passed. Commands fell from 2,115 to 11, label solids from 2,108 to zero, renderer locks from four to zero, and warm allocations from 16 to zero.

The matching 15-pair Metal population moved completed-command-buffer GPU p50 from 0.2571 to 0.0400 ms, an 84.440% paired median speedup with a 95% confidence interval of 83.733%..88.343% and 15/15 wins. Commands/draws fell from 16,920/16,896 to 88/40. Exact text output and the established A8/SDF atlas contracts passed.

The first single-popover tail population and the original renderer-owned rasterizer were rejected. The accepted design removes that ownership split instead of tuning it. Raw paired reports, counters, captures, identities, and rejection records are retained under the ignored `raw/` directory.
