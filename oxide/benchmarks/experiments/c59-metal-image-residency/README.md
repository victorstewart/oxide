# C59 — Optimize immutable Metal image residency and mip policy

Status: accepted with staged Private storage rejected for the production policy.

Hypothesis: immutable images that are sampled many times may benefit from a Shared staging upload followed by Private sampled storage, while repeatedly minified images should benefit from a complete mip chain. The likely costs are an extra texture, copy submission, startup latency, and more peak memory. Dynamic atlases, video, and camera textures must remain directly updateable.

## Decision

C59 accepts explicit immutable-image ownership, complete mip generation for repeatedly minified standalone images, linear mip sampling, exact mip regeneration after partial updates, and residency/upload telemetry. The production policy keeps both non-minified and repeatedly minified immutable images in Shared storage. Repeatedly minified images receive a complete chain; ordinary immutable and all dynamic images remain single-level.

The staged Private candidate is rejected for every production size/reuse class. On the physical iPhone it did not materially improve steady GPU sampling, while it increased creation work and peak texture memory. The hidden benchmark selector remains only so the rejected storage combinations stay measurable in the same build.

## Protocol

- Candidate source: final reviewed C59 source, Release-built with Xcode 26.6 and installed directly on the physical device.
- Device: physical iPhone 17 Pro Max (iPhone18,2), iOS 26.5.1, native refresh. Primary storage A/B rows used 8 warmups and 30 measured frames.
- Mac: Mac14,6, Apple M2 Max, 96 GB, macOS 26.5.2. The final isolated mip A/B used 8 warmups and 100 measured frames.
- Large-static rows render one 1,024-square source at one-to-one scale. Minified rows render the identical checkerboard 1,089 times into 31-square cells. Small one-use rows create, render, complete/read back, and release a 64-square image per observation.
- Completed `MTLCommandBuffer` start/end timestamps provide direct GPU duration. Reports also retain p50/p95/p99/peak CPU distributions, creation and first-visible time, current residency, cumulative staging/mip work, sampled bytes, sampled-plus-staging creation peak, and output variance.

## Physical-device A/B

| Workload | Shared | Staged Private | Decision |
| --- | --- | --- | --- |
| Large static | frame p50 0.0352 ms; encode p50 0.0297 ms; GPU p50 0.2995 ms; creation 1.0970 ms; peak 4,227,072 B | frame p50 0.0483 ms (+37.1%); encode p50 0.0415 ms (+39.5%); GPU p50 0.2949 ms (-1.5%, neutral); creation 1.5785 ms (+43.9%); peak 8,454,144 B (+100%) | Private rejected |
| Minified, both with 11 mips | frame p50 0.0892 ms; encode p50 0.0835 ms; GPU p50 0.3630 ms; creation 2.2157 ms; first visible 6.1221 ms; peak 5,652,480 B | frame p50 0.1338 ms (+50.1%); encode p50 0.1258 ms (+50.7%); GPU p50 0.3636 ms (+0.2%, tied); creation 2.3629 ms (+6.6%); first visible 7.0397 ms (+15.0%); peak 9,879,552 B (+74.8%) | Shared+mips accepted; Private rejected |
| Small one-use | creation p50 0.0250 ms; encode p50 0.0301 ms; GPU p50 0.0261 ms; peak 32,768 B | creation p50 0.0588 ms (+135.6%); encode p50 0.0402 ms (+33.3%); GPU p50 0.0536 ms (+105.6%); peak 65,536 B (+100%) | Private rejected |

The isolated minified Shared/Private GPU p95 values were 0.7219/0.7901 ms and p99 values were 1.1878/1.2153 ms. Private therefore supplied no tail benefit to offset its copy, startup, and memory costs. Large-static first-visible was noisy in the opposite direction in the final run, but its creation CPU, steady CPU, and two-times peak-memory costs still reject Private; an earlier same-workload device pass also showed worse Private startup.

The production auto rows confirm the selected paths: large static retains one 4,227,072-byte Shared single-level texture with zero staging; small one-use retains 32,768 bytes while live, stages zero bytes, and returns both current residency counts to zero; the public authoring row executes 1,089 `ImageView::encode` calls and retains one 5,652,480-byte Shared texture with 11 levels and zero staging.

## Quality, updates, and lifecycle

- The physical-device no-mip checkerboard reports spatial variance 947.006. Both complete-mip controls report zero variance and byte-identical output; the 30-frame Shared-mip row has direct GPU p50 0.3630 ms. The earlier same-build 30-frame no-mip control was 1.0813 ms, so mips reduced steady GPU duration about 66% as well as removing aliasing.
- Standalone textures clamp to their own edge and need no gutter. Atlas gutters and per-slot mip handling belong to C60, where neighboring content exists.
- Dynamic RGBA8/A8, glyph, video, and camera resources remain Shared and never enter immutable staging policy.
- Shared-mip and Private-mip partial updates refresh level zero and regenerate every lower level on the serial renderer queue. Exact tests compare the result with a freshly rebuilt chain.
- Memory-pressure cache purges preserve app-owned image textures. Renderer/device replacement drops old-device handles; replaying the owner-held source into the new renderer reproduces exact pixels. C60 adds the cross-backend source store that automates that replay.

The Mac 100-frame cross-check agrees with the device decision: Private improved GPU p50 only 1.4%, but Shared improved GPU p95 0.6%, frame p50 6.2%, first-visible 1.7%, and creation peak memory 42.8%. The tie resolves to the simpler Shared path.

Automated process-scoped Metal System Trace was unavailable because `xctrace` listed the otherwise `devicectl`-reachable phone as offline. No proxy was substituted. Direct device GPU timings remain in-app Metal measurements; direct Power Profiler energy is manual-pending.
