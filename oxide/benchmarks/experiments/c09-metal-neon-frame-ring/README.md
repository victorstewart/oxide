# C09 Metal neon frame-ring evidence

This directory records the C09 correctness and performance evidence for streaming neon-marker instances through the selected frame's reusable uniform ring.

The parent duplicates every marker payload through vertex and fragment `set*Bytes`, splits dense batches at the 4 KiB ceiling, and reports marker count as draw count. The candidate packs once, copies once into one aligned non-overlapping ring slice per compatible batch, binds that slice to both stages, issues one instanced draw, and reports GPU draws separately from instances.

Permanent architecture cases cover 1, 51, 52, 60, 61, 128, and 1,024 total markers; 1,024 is eight compatible 128-marker batches in one frame. Byte-exact Metal snapshots prove that all eight slices remain immutable, and metrics freeze draws, instances, upload bytes, and warm growth. The locally ignored `raw/` directory records paired C08-versus-C09 results. Official `latest.*` baselines remain unchanged until C62.
