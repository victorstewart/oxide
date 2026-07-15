# C57 — Instance compatible Scene3D draws on Metal

Status: accepted.

Hypothesis: repeated per-instance Metal state calls and inline uniform copies are the CPU-encode bottleneck in compatible opaque Scene3D runs. Compact completion-safe arrays plus adjacent instancing should collapse draw/state calls and improve encode and GPU time at 1,000–10,000 instances, with retained ring capacity as the principal memory cost and transparent/additive ordering as the principal correctness risk.

## Decision

C57 replaces per-instance `setVertexBytes`/`setFragmentBytes`, pipeline, depth, cull, mesh-bind, and draw calls with completion-protected 64-byte MVP and 48-byte material arrays plus adjacent, order-safe instanced draws. Compatible opaque instances batch only when mesh, pipeline, depth, and cull state match. The pass already owns viewport and target compatibility. Transparent, additive, bloom, and otherwise order-sensitive instances retain source order and individual draws.

The physical iPhone result is decisive at scale. At 10,000 compatible instances, draw calls fell from 10,000 to two, encode p50 fell 89.0%, direct GPU p50 fell 92.1%, and frame p50 fell 81.7%. The 1,000-instance row improved encode/GPU/frame p50 by 85.4%/66.4%/22.9%. At 96 instances, absolute frame savings were small, while encode/GPU p50 still improved 56.1%/18.5%. The 96-instance transparent control retained all 96 draws; the 1,000-instance mixed-mesh control retained all 1,000 draws while benefiting from array binding and redundant-state suppression.

Private immutable mesh storage was measured and rejected. In the same final binary over 100 physical-device frames of the 10,000-instance workload, shared/private direct GPU p50 was 0.299/0.306 ms. Private storage did not improve the median and had no consistent tail win, while requiring transient staging buffers and a blit submission for every mesh upload.

## Protocol

- Exact renderer parent: `25e5cfdd252ad19da20c44b5ab5f05be5bfb9386`. Parent binaries used only a benchmark-side telemetry adapter so both implementations emitted the same report shape; the timed parent renderer remained exact.
- Mac: Mac14,6, Apple M2 Max, 96 GB, macOS 26.5.2 (25F84). Rust 1.86.0. Each row used 8 warmups and 24 measured frames.
- Device: physical iPhone 17 Pro Max (iPhone18,2), iOS 26.5.1, native refresh. Xcode 26.6. Each primary/control row used 8 warmups and 30 measured frames and persisted every frame/encode/completed-command-buffer GPU sample.
- Compatible rows used two contiguous immutable-mesh runs at 96, 1,000, and 10,000 instances. Controls covered transparent ordering, cyclic mixed meshes/states, a clipped viewport, and one/three bloom layers.
- Endurance used 30 samples × 120 cycles, with two mesh releases and two creates per cycle: 14,400 resource operations in one renderer.
- `accepted-*-macos.json` and `accepted-*-device.json` contain the exact persisted distributions and counters. Aggregate device-baseline promotion remains reserved for C62.

## Physical-device results

| Instances | Encode p50 parent → candidate | GPU p50 parent → candidate | Frame p50 parent → candidate | Frame p99 parent → candidate | Draws |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 96 | 0.060 → 0.026 ms (-56.1%) | 0.184 → 0.150 ms (-18.5%) | 1.202 → 1.167 ms (-2.9%) | 1.259 → 1.175 ms (-6.7%) | 96 → 2 |
| 1,000 | 0.414 → 0.060 ms (-85.4%) | 0.508 → 0.171 ms (-66.4%) | 1.557 → 1.200 ms (-22.9%) | 2.384 → 1.212 ms (-49.1%) | 1,000 → 2 |
| 10,000 | 4.049 → 0.444 ms (-89.0%) | 3.800 → 0.301 ms (-92.1%) | 8.645 → 1.582 ms (-81.7%) | 10.573 → 1.617 ms (-84.7%) | 10,000 → 2 |

The transparent control kept 96 draws and improved frame/encode/GPU p50 from 1.201/0.061/0.180 ms to 1.179/0.039/0.153 ms. Its direct-GPU p95/p99 rose by 0.053/0.074 ms while whole-frame p95/p99 improved; those sub-millisecond device tails are reported rather than hidden. The 1,000 mixed-mesh control kept 1,000 draws and improved frame/encode/GPU p50 from 1.580/0.438/0.507 ms to 1.440/0.299/0.400 ms.

One-layer bloom kept 192 ordered Scene3D draws and improved frame/encode/GPU p50 from 1.291/0.151/0.807 ms to 1.263/0.125/0.792 ms. Three-layer bloom kept 384 draws, improved frame/encode/GPU p50 from 3.738/0.327/2.586 ms to 3.666/0.264/2.560 ms, and halved instance upload from 43,008 to 21,504 bytes by preparing the unchanged emissive array once.

At 10,000 instances, retained renderer memory rose from 14,025,472 to 15,074,048 bytes because the completion-safe instance ring warmed to its next capacity class. Mesh bytes remained 768. Endurance left exactly two live meshes, or 96 mesh-buffer bytes, after all 14,400 release/create operations; physical-device create/release latency was 0.712/2.010/2.048/2.061 ms p50/p95/p99/peak per 120 cycles.

## Correctness and verification

- Parent and candidate bloom captures are byte-identical: SHA-256 `54a221b3f2d305bb65bf78ff63af6a2d9dcb1d4451a483d1f93c9a9b368aaf`.
- The current toolchain differs from the older committed bloom golden by 55 pixels with maximum channel error 1, but the exact parent reproduces the candidate bytes. Other Scene3D goldens were exact.
- Snapshot coverage requires one opaque draw for 96 compatible instances and 96 draws for both transparent and additive controls. Shader/layout tests freeze the 64/48-byte ABI, indexed instance arrays, base-instance draws, and removal of per-instance `set*Bytes`.
- Real-device reports use Oxide's completed `MTLCommandBuffer` `GPUStartTime`/`GPUEndTime` source of truth. Automated process-scoped Metal System Trace was attempted through both launched and attached capture paths, but this wireless device session made `xctrace` mark the booted phone offline and then failed to resolve a PID that `devicectl` simultaneously listed. No proxy was substituted. Direct energy remains manual-pending as required.
