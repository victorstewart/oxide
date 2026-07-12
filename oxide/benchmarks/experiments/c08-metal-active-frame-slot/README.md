# C08 Metal active-frame-slot evidence

This directory records the C08 correctness evidence for routing auxiliary Metal encoders through the frame slot selected by `begin_frame`.

The parent neon-marker and ID-mask composition helpers independently recompute `frame_id % FRAME_RING_SIZE`, which diverges when the preferred slot is busy and `begin_frame` selects the next available slot. The candidate uses `current_frame_slot()` everywhere and exposes snapshot-only controls that mark the next preferred slot busy, inspect the selected slot, and verify command-buffer ownership.

Real-Metal validation tests force alternate-slot selection for all required neon-marker arrays and for the asymmetric ID-mask raster/compositor reference. They assert no skipped backpressure, selected-slot command-buffer ownership, byte-exact neon pixels, and exact ID-mask/JFA reference fields. The locally ignored `raw/` directory records commands and results. Official `latest.*` baselines remain unchanged until C62.
