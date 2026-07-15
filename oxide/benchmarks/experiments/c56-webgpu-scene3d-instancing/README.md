# C56 — Instance compatible Scene3D draws on WebGPU

Status: accepted.

## Decision

C56 replaces one 256-byte dynamic-uniform allocation, bind, and draw per Scene3D instance with an 80-byte persistent storage record and adjacent, order-safe instanced draws. Compatible opaque instances batch only when mesh generation, pipeline/material state, cull/depth state, viewport, and target match. Transparent and additive instances retain source order and individual draws. The implementation also adds front/back/none cull variants, physical viewport/scissor handling, and generation-checked mesh-slot recycling.

The accepted result is large at scale: CPU submit p50 improved 62.7%, 78.1%, and 83.7% at 96, 1,000, and 10,000 compatible instances. Draws fell from the instance count to two (one per contiguous mesh run), bind-group binds fell to one per pass, and upload bytes fell 68.8%. GPU p50 was effectively neutral at 96 instances because the absolute changes were microseconds and tail direction varied, then improved 54.2% at 1,000 and 81.5% at 10,000 with all recorded GPU percentiles improving in both larger rows.

No measured implementation was rejected. Three unsafe design alternatives were excluded before measurement: global state sorting, transparent/additive batching, and a fragment-stage instance-buffer color read. The accepted path preserves command order and performs the color multiply in the vertex stage.

## Protocol

- Exact parent: `bf0ffb07f140c82ba9cb0bae3072dd65d1963c21`.
- Host: Mac14,6, Apple M2 Max, 96 GB, macOS 26.5.2 (25F84).
- Browser/toolchain: native-arm64 Google Chrome 150.0.7871.124; Rust 1.86.0; release wasm built with Cargo and packaged with the installed `wasm-bindgen` CLI.
- Each primary/control row used 15 CPU samples × 24 frames in an isolated browser report. GPU distributions contain the completed WebGPU timestamp-query samples drained after each row.
- Compatible rows used 96, 1,000, and 10,000 opaque instances split evenly across two immutable meshes. Controls covered mixed meshes/states, 96 transparent instances, and a clipped subviewport.
- Endurance used 30 × 120-frame normal and stress recreate variants for more than 14,000 releases/creates in one renderer. The slot unit test adds 10,000 warm-capacity churn cycles plus stale-handle, double-release, generation-exhaustion, and capacity guards.
- `accepted-compatible-report.json`, `accepted-controls-report.json`, and `accepted-endurance-report.json` contain the exact persisted distributions and counters. Aggregate web-baseline promotion remains reserved for C62.

## Results

| Instances | CPU p50 parent → candidate | GPU p50 parent → candidate | Draws | Upload bytes |
| ---: | ---: | ---: | ---: | ---: |
| 96 | 0.059 → 0.022 ms (-62.7%) | 11.416 → 10.498 µs (-8.0%; neutral) | 96 → 2 | 24,576 → 7,680 |
| 1,000 | 0.393 → 0.086 ms (-78.1%) | 46.079 → 21.125 µs (-54.2%) | 1,000 → 2 | 256,000 → 80,000 |
| 10,000 | 3.836 → 0.625 ms (-83.7%) | 427.704 → 79.000 µs (-81.5%) | 10,000 → 2 | 2,560,000 → 800,000 |

The mixed 1,000-instance control produced 670 ordered draws across 12 pipeline and mesh runs. The transparent control retained 96 draws. The subviewport control retained two compatible draws and one viewport/scissor application. The renderer's logical shaded-damage counter remained the same 512×512 target area; this is damage accounting, not a hardware fragment counter.

Resource endurance removed the parent's append-only table growth: after more than 14,000 releases/creates, resource-table scratch was 840 bytes versus 917,824 bytes, while both paths retained exactly 576 live GPU mesh bytes. Candidate total scratch was 15,636 bytes versus 958,604 bytes.

## Correctness and verification

- Parent, candidate, and committed Scene3D capture SHA-256: `f7b9a187792f891758861aebd95f14264eabc579393ce3debbf78185be6796d2`; pixel diff 0, maximum channel error 0, MSE 0.000.
- Pure viewport tests cover scaling, clipping, invalid input, and fully offscreen rectangles.
- Mesh-slot tests cover exact-generation lookup/release, stale handles, double release, generation exhaustion, hard capacity, and 10,000 reuse cycles.
- The complete WebGPU browser report passed after the isolated matrix, including the new Scene3D counters and GPU-distribution parser.
- `cargo check -p oxide-renderer-web --target wasm32-unknown-unknown`, `cargo check -p oxide-host-web --target wasm32-unknown-unknown`, renderer-web tests, and host-web tests passed. The final verification commands are recorded in the enclosing commit handoff.

`wasm-pack build --release` completed Rust compilation and package generation, but its bundled older `wasm-opt` rejected the toolchain's modern multi-table Wasm. The tested release package therefore uses the repository's established direct installed-`wasm-bindgen` packaging route; this is a packaging-tool limitation, not a renderer failure.
