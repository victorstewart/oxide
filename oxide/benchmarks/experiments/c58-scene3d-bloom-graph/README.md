# C58 — Integrate Scene3D bloom into the common render graph

Status: accepted.

Hypothesis: three-layer Scene3D bloom is dominated by redrawing every emissive instance once per layer and by retaining independent layer work. Extracting the emissive source once, scheduling extraction/filter/composite in the common graph, and aliasing serial intermediates should cut Scene3D draws, render passes, CPU encode time, and GPU/frame time without changing pixels. The main risk is one extra retained source texture for multi-layer bloom plus viewport-edge or later-overlay corruption.

## Decision

C58 is accepted. Metal now renders emissive content once at bloom resolution, executes each separable blur and additive upsample composite from the common `EffectGraphPlan`, reuses warm plans, and scissors work to conservative viewport-plus-kernel regions. One bloom layer aliases vertical output back onto the dead source and retains two physical intermediates. Multiple layers retain the source plus two serial filter slots: three physical intermediates, the minimum safe set without redrawing the source.

Depth remains private and stored. `encode_scene3d()` is not a provably terminal frame pass because public callers may encode later 2D/3D work, so memoryless depth would violate the load/store contract.

## Protocol

- Exact parent: `ddf6b0a`. Parent and candidate used the same 1,024-pixel-square offscreen Scene3D scene, strings/materials, bloom layers, target format, downsample divisor, and workload order.
- Mac: Mac14,6, Apple M2 Max, 96 GB, macOS 26.5.2. Each row used 8 warmups and 50 measured frames.
- Device: physical iPhone 17 Pro Max (iPhone18,2), iOS 26.5.1, native refresh, Xcode 26.6. Each row used 8 warmups and 30 measured frames; the retained candidate report was rebuilt from the final reviewed source and run directly in the installed app.
- Primary rows cover one/three bloom layers at 96 and 1,000 emissive instances. Compatible no-bloom rows are guardrails. Candidate-only rows cover a 25% viewport and a later 2D overlay.
- Reports retain every frame, encode, and completed-command-buffer GPU sample plus passes, draws, graph resources/aliases/reuse, logical/physical/aliased bytes, region pixels, bandwidth estimate, and retained bloom bytes.

## Physical-device results

| Workload | Frame p50 parent → candidate | Encode p50 parent → candidate | GPU p50 parent → candidate | Passes | Scene3D draws |
| --- | ---: | ---: | ---: | ---: | ---: |
| 96, one layer | 1.434 → 1.263 ms (-11.9%) | 0.206 → 0.122 ms (-40.7%) | 0.799 → 0.792 ms (-0.8%) | 5 → 5 | 192 → 192 |
| 96, three layers | 4.094 → 3.596 ms (-12.2%) | 0.658 → 0.192 ms (-70.9%) | 2.566 → 2.491 ms (-2.9%) | 13 → 11 | 384 → 192 |
| 1,000, one layer | 3.875 → 2.886 ms (-25.5%) | 1.572 → 0.614 ms (-61.0%) | 1.166 → 1.158 ms (-0.7%) | 5 → 5 | 2,000 → 2,000 |
| 1,000, three layers | 6.760 → 4.088 ms (-39.5%) | 2.208 → 0.684 ms (-69.0%) | 2.969 → 2.868 ms (-3.4%) | 13 → 11 | 4,000 → 2,000 |

Three-layer device frame p95/p99 improved 19.5%/29.1% at 96 instances and 46.0%/46.0% at 1,000. Compatible no-bloom frame p50 improved 3.3%/3.1% at 96/1,000 with unchanged one pass and two draws, so the graph integration did not regress the disabled path.

The Mac cross-check showed the same structural result. Three-layer frame/encode/GPU p50 improved 2.8%/19.7%/39.8% at 96 instances and 21.1%/51.3%/26.8% at 1,000. One-layer Mac medians were effectively neutral to slightly better. No-bloom Mac p50 was neutral; its high-percentile GPU samples were externally noisy and are not used over the physical-device result.

## Storage, regions, and correctness

- One layer reports 3 logical resources in 2 physical slots: 6,291,456 logical bytes, 4,194,304 physical bytes, and 2,097,152 aliased bytes.
- Three layers report 7 logical resources in 3 physical slots: 14,680,064 logical bytes, 6,291,456 physical bytes, and 8,388,608 aliased bytes.
- Actual retained bloom allocation is 4,227,072 bytes for one layer and 6,340,608 bytes for three. The 50% increase is the one extra source-preservation texture that removes repeated extraction.
- The 25% viewport reduced estimated bandwidth from 77,594,624 to 20,197,152 bytes and graph region work from 4,980,736 to 1,301,522 pixels, both about 74%.
- The exact parent/candidate 192×192 bloom captures are byte-identical at SHA-256 `54a221b3f2d305bb65bf78ff63afad6a2d9dcb1d4451a483d1f93c9a9b368aaf`: zero differing pixels, zero maximum channel error, zero MSE.
- Both exact parent and candidate differ from the older committed toolchain golden by the same 55 pixels with maximum channel error 1 and MSE 0.000461; C58 does not hide that pre-existing baseline drift by widening global tolerance.
- Snapshot coverage proves one extraction, one source draw, expected pass/resource counts, one-/three-layer alias counts, second-frame plan reuse, clipped viewport output, clear outside pixels, and a later 2D overlay remaining visible.

Oxide's completed `MTLCommandBuffer` GPU timestamps are the direct device timing source. `xctrace` continued to list the simultaneously `devicectl`-reachable phone as offline, so automated process-scoped Metal System Trace was unavailable and no proxy was substituted. Direct Power Profiler energy remains manual-pending.
