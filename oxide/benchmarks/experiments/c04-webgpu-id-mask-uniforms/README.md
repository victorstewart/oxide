# C04 WebGPU ID-mask uniform evidence

This directory records the C04 correctness and no-material-regression evidence for replacing repeatedly overwritten WebGPU ID-mask uniform buffers with one aligned frame-local arena.

The exact 17x11 asymmetric oracle encodes two ID-mask draws in one submission: a full-mask distractor followed by the sparse CPU-reference draw. The parent preserves exact raster IDs but corrupts 22 city-field and 110 seam-field records. The candidate matches every raster and final field record exactly while reporting one 5,120-byte upload for 16 immutable uniform slots.

Raw paired reports, browser reports, field dumps, environment controls, commands, hashes, and the fixed-seed order live under the ignored `raw/` directory so evidence collection cannot change the staged candidate tree identity. Official `latest.*` baselines remain unchanged until C62.
