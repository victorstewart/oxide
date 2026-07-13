# C15 WebGPU R8 atlas evidence

C15 changes only WebGPU A8/SDF storage and upload behavior. RGBA images retain their existing `Rgba8Unorm` texture and four-byte row path.

The performance hypothesis is that expanding glyph coverage to RGBA wastes texture residency, upload bandwidth, and CPU conversion work. The affected stages are cold atlas creation, full atlas publication, dirty subrectangle publication, texture residency, and A8/SDF fragment sampling. The target workload creates and fully updates a padded-row 1024x1024 atlas, then applies padded-row 64x64 dirty updates while rendering the same 96-glyph A8/SDF scene. Expected movement is 4,194,304 to 1,048,576 bytes for storage and full upload, 16,384 to 4,096 bytes per dirty upload, and removal of conversion-only CPU bytes. The correctness risk is channel selection, row-stride handling, dirty placement, filtering, and resource recreation.

The locally ignored `raw/` directory retains balanced parent/candidate browser samples, exact package and instrumentation hashes, parent/candidate captures, plans, and reports. Official `latest.*` promotion remains deferred to C62.

An initial design passed padded 1027-byte A8 rows directly to browser `write_texture`. It was rejected after the first matched cold-create diagnostic measured 57.561 ms/frame against the parent's 3.460 ms/frame. The retained design writes tight rows directly and repacks only genuinely strided rows into single-channel storage; the next exploratory cold-create result was 0.482 ms/frame with exact pixels.
