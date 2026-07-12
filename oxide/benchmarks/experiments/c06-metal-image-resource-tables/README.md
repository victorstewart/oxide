# C06 Metal image resource-table evidence

This directory records the C06 correctness and performance evidence for replacing Metal's repeatedly overwritten per-frame image argument payload with aligned immutable slices.

The parent path bound every image range to offset zero and then rewrote that payload for later ranges. The candidate finalizes each distinct handle-ordered table once, binds its stable slice for every compatible range, preserves the 128-texture split, and grows each frame-ring slot only during warmup. Renderer counters expose encodes, binds, finalized tables, reuse, bytes, and buffer growth.

Real-Metal snapshot tests use exact pixels and Metal API validation across alternating image/RRect/clip/layer/effect ranges, multiple render passes, 130 consecutive unique textures, and cold-to-warm slice growth. The raw evidence contains the parent reproduction and the release paired-process results under the locally ignored `raw/` directory. Official `latest.*` baselines remain unchanged until C62.
