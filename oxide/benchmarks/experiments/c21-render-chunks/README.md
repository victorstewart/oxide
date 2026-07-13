# C21 immutable render chunk evidence

C21 replaces repeated retained `DrawList` flattening on one real mixed `UiSurface` path with immutable, versioned local-space chunks and ordered `RenderSnapshot` references. The performance hypothesis is that clean replay and dirty-leaf rebuilding are dominated by command/index normalization, geometry copies, ancestor reflattening, and short-lived allocations. Chunk creation validates and canonicalizes spans once; clean composition then copies zero command, vertex, or index bytes, while one dirty leaf rebuilds one surface chunk and preserves the independent glyph/image chunks.

The permanent deep workloads retain 1,500 semantic nodes at depths 16 and 32 and compose actual packed glyph geometry plus image commands. The broad authoring workloads retain 1,000 visible nodes. The 300-node animation case is the production-shaped mixed-content consumer. Every permanent row reports chunk reuse/rebuild, copied command/geometry bytes, retained bytes, and flat-fallback count. Resource-generation, ordering, canonical-index replay, and compatibility flattening are covered by focused renderer-api and ui-core tests.

The locally ignored `raw/` directory contains fresh-process parent/candidate reports, exact identities, binary and instrumentation hashes, pair order, raw results, and the temporary allocation probe. Post-review 15-pair measurements accepted every workload:

- Deep clean p50/p95/p99/peak: 0.1322/0.1337/0.1342/0.1343 to 0.1147/0.1156/0.1158/0.1158 us/op; 13.29% paired median speedup, 95% CI 12.73% to 13.63%, 15/15 wins.
- Deep dirty: 99.842/100.406/100.412/100.413 to 13.150/13.500/14.023/14.154 us/op; 86.83%, CI 86.79% to 86.88%, 15/15.
- Broad clean: 11.260/11.386/11.399/11.403 to 0.0818/0.0833/0.0854/0.0860 us/op; 99.27%, CI 99.27% to 99.28%, 15/15.
- Broad dirty: 40.739/41.091/41.147/41.161 to 19.102/19.201/19.209/19.211 us/op; 53.29%, CI 52.93% to 53.64%, 15/15.
- Animated mixed content: 30.287/30.504/30.508/30.509 to 26.263/26.457/26.490/26.498 us/op; 13.14%, CI 12.49% to 14.03%, 15/15.
- Dirty broad allocation probe: 1,022 calls and 401,264 bytes to 17 calls and 171,000 bytes per operation; allocation calls fell 98.34% and bytes fell 57.38%, both with exact confidence intervals and 15/15 wins.

The commit proof records the final staged-tree identity and hashes. Official aggregate workspace baselines remain deferred to C62.
