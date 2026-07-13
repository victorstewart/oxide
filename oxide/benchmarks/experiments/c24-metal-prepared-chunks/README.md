# C24 Metal prepared-chunk evidence

C24 lowers supported immutable `RenderChunk` payloads once into persistent Metal buffers and replays them from a byte-budgeted LRU. The performance hypothesis is that clean retained frames are dominated by repeated compatibility flattening, command traversal, geometry copies, and ring uploads rather than GPU execution. The target workload contains 256 mixed chunks with 64 RRects, images, glyph quads, or solid triangles per chunk. Clean frames change only a dynamic translation; the one-dirty workload changes one chunk revision.

The permanent clean contract requires 256 cache hits, zero misses, zero command traversal, zero geometry copy, and zero immutable upload. The dirty contract requires exactly 255 hits, one miss, 64 traversed commands, and 3,072 uploaded bytes. Both paths retain 256 actual draw calls and 64 image argument-table binds. Prepared residency uses Metal allocated bytes under a 32 MiB default hard budget; logical payload ownership remains separately accounted.

The large visual oracle renders the complete mixed workload through the parent flat path and the prepared path, then compares the 1,200 x 800 BGRA readback byte-for-byte. A focused fractional opaque RRect/image regression additionally freezes the established translation rounding convention under Metal API Validation. Resource updates, release, explicit purge, LRU eviction, unsupported fallback, and iOS critical-memory handling have dedicated tests.

Shared storage is retained after a focused same-binary comparison rejected private storage. On the one-dirty workload, adding one miss-only blit encoder and private copy regressed command-buffer GPU p50 from 0.119875 to 0.213250 ms, lost all 15 pairs, and regressed every tail. No private/staging path remains.

Frame-dynamic records use one completion-protected uniform-ring slice rather than 256 separate `setVertexBytes` calls. The mixed workload writes exactly 12,288 dynamic uniform bytes per frame while clean replay still writes zero immutable geometry. The per-frame glyph ICB remains disabled. Prepared replay reduces CPU encode to the same scale as GPU execution on the target packet, so a reusable static-ICB threshold sweep is not triggered.

The locally ignored `raw/` directory is the stable evidence location for exact parent/candidate identities, instrumentation and binary hashes, balanced pair order, raw encode/GPU/frame samples, environment fingerprints, and the final shared-runner decisions. Aggregate workspace and physical-device baseline promotion remains deferred to C62.
