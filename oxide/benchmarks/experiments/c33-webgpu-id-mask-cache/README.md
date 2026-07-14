# C33 WebGPU ID-mask field-cache evidence

C33 retains complete immutable WebGPU ID-mask raster and jump-flood fields when geometry and projection are unchanged. The exact key includes mask dimensions, exact mask-scale bits, vertex revision/count, every projection input, and ordered chunk content hashes and ranges. Style, colors, glow, polish, mode, opacity, and viewport placement remain final-compositor inputs and do not invalidate fields.

Cache hits serialize one 768-byte compositor uniform slot and encode one compositor pass. Misses preserve the raster, seed, nine 512-square JFA passes, and compositor sequence with twelve uniform slots. The permanent matrix records static, style-only, and viewport-only cases as one-pass hits; projection and content changes remain twelve-pass misses. Explicit memory-pressure and device-loss paths release all retained fields, and the next frame rebuilds them.

The cache holds at most four immutable field sets under an adaptive 64--512 MiB byte budget. Its accounting reflects the actual two R8 masks plus four RGBA16F fields, or 34 bytes per pixel and 8,912,896 bytes per 512-square map. Queue submission order protects cross-frame reuse; draw order protects same-command-buffer reuse; compatible evicted targets may be rewritten without allocation. Snapshot builds retain only the latest handles needed for exact readback.

The controlled static A/B used unchanged parent `39cfa6d0d6d8418230861e263d087f0eb04bb9a5`, the staged tree recorded in the commit proof, identical browser instrumentation `052f4f232609b385d38a9c2cb6a7eee479df66cfe5d46c783867e7dd10eb5f1a`, native-arm64 Chrome, fresh processes, 64 same-scene RAF warmups, 160 measured submissions per process, and fifteen fixed-seed balanced pairs. The parent/candidate wasm-bindgen artifacts were `0e527d1bcda40d9b97d108a3ab2bd2f986b55cdb9d0fa4f9b03664f37f30e8bc` and `6c5c4fa327c66e3aa2b6069acb5c73cadab4126af1b64073cbed8c75240dcd18`.

Across 2,400 direct GPU timestamps per side, parent/candidate p50 was about 6.2/0.06 ms and every p50/p95/p99/peak gate accepted. Paired median speedup exceeded 99.0%, the 95% confidence lower bound exceeded 99.0%, and all 15 candidate pairs won. Every parent sample encoded twelve passes with nonzero raster, seed, JFA, and compositor time; every candidate sample encoded one compositor pass with zero raster, seed, or JFA time.

The simultaneously captured active CPU-submit distribution improved by more than 10% at the paired median with 15/15 wins. Native RAF pacing passed the predeclared no-material-regression policy at p50, p95, p99, and peak. The exact final CPU and RAF distributions remain in the raw decision reports and the commit proof.

The required one-entry design was tested and rejected for alternating maps. In the exact candidate matrix it produced zero hits, 24 passes, 4.231258 ms GPU p50, and 0.140313 ms CPU p50. The retained small LRU produced two hits, two compositor passes, 0.047038 ms GPU p50, and 0.032500 ms CPU p50 while holding exactly two 8,912,896-byte entries. Both modes had zero steady target creation after warmup.

Parent and candidate presented PNGs are byte-identical at SHA-256 `abc3e7d83112d44ce3cade125d0861f02bb105c1c354d63500e9233f3076d889`. Exact R8 mask and decoded RGBA16F field payloads are also byte-identical at SHA-256 `1e9f3ccd796ad55548a124ab623c1d9866b5355a7cbf303d5768564464c587cf`. The full reference JSON differs only by the expected cache/pass/uniform counters.

The locally ignored `raw/` directory retains the plan, instrumentation, exact source/binary identities, balanced order, every RAF/CPU/GPU/stage sample, matrix, reference fields, screenshots, and deterministic decision reports. Aggregate workspace, browser, and physical-device baseline promotion remains deferred to C62.
