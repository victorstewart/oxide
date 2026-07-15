# C60 — Cache decoded and GPU-resident image variants

Status: accepted for suitable small static-image populations. Standalone residency remains the required policy for repeatedly minified, oversized, rapidly changing, video, compressed-incompatible, and explicit standalone variants. Browser display proof is deferred to C61 because the controlled in-app browser runtime was unavailable; wasm build and source contracts passed.

## Hypothesis and design

Repeated full-size decode/upload and one texture per small icon inflate cold latency, resource count, binding work, and dense-frame CPU cost. C60 introduces a renderer-independent store with stable slot generations, request serials, asynchronous display-size decode, cancellation, separate decoded/GPU hard budgets, intrusive LRUs, exact chunk references, and device-generation recovery. Suitable static images enter size-classed sRGB pages through append-only uploads with repeated-edge gutters. GPU publication moves rather than clones decoded RGBA, reuses one patch scratch buffer, deduplicates shared-chunk invalidation, and reuses the exact oldest same-class slot in place under pressure. Unsuitable images stay standalone and repeatedly minified variants retain complete mip chains.

The design deliberately accepts page-granularity residency. A sparse 100-icon population can occupy more GPU bytes than tightly sized standalone textures, while dense populations amortize the page and collapse texture/draw work. The policy is accepted because the target contract is bounded scrolling collections with many small reusable assets, not because atlasing wins every individual counter at every cardinality.

## Physical-iPhone Oxide A/B

Release rows ran from the final C60 tree on the physical iPhone 17 Pro Max (`iPhone18,2`, iOS 26.5.1) at native refresh. Each side used the same unique 64-square PNG pixels, decoded to 28 square, with PNG construction excluded from request timing. The first-frame boundary includes explicit renderer readback completion; it is not described as host display latency.

| Icons | Metric | Atlas | Standalone | Result |
| ---: | --- | ---: | ---: | --- |
| 100 | frame p50 | 1.1863 ms | 1.1953 ms | atlas -0.8% |
| 100 | request to completed frame | 11.2898 ms | 12.1149 ms | atlas -6.8% |
| 100 | upload | 0.4084 ms | 0.9490 ms | atlas -57.0% |
| 100 | GPU resident | 1,048,576 B | 313,600 B | atlas +234.4% |
| 1,000 | frame p50 | 1.2179 ms | 1.2266 ms | atlas -0.7% |
| 1,000 | request to completed frame | 18.7260 ms | 22.2152 ms | atlas -15.7% |
| 1,000 | upload | 1.2620 ms | 3.9858 ms | atlas -68.3% |
| 10,000 | frame p50 | 1.3896 ms | 12.4370 ms | atlas -88.8% |
| 10,000 | encode p50 | 0.2522 ms | 1.1350 ms | atlas -77.8% |
| 10,000 | request to completed frame | 172.4578 ms | 221.6631 ms | atlas -22.2% |
| 10,000 | upload | 12.7049 ms | 39.8393 ms | atlas -68.1% |
| 10,000 | direct GPU p50 | 0.4199 ms | 0.4131 ms | atlas +1.6% |
| 10,000 | GPU resident | 41,943,040 B | 31,360,000 B | atlas +33.7% |

The 10,000-icon atlas uses one draw instead of 79 and 40 textures instead of 10,000, with zero page-clear bytes. At 100/1,000 icons it also uses one draw instead of 1/8 and 1/4 pages instead of 100/1,000 textures. Direct GPU time is neutral-to-slightly worse; the accepted win is CPU/frame, upload, request-to-completed-frame latency, and resource-count collapse under dense texture pressure. Store publication itself improved 7.8% at 10,000 images; the remaining request interval includes decode and completed-frame readback.

Three alternating macOS repetitions agree: 100 icons are neutral, 1,000 improve roughly 4–5%, and 10,000 improve 89.3–89.5% in frame p50. The public 1,000-icon authoring journey improved local frame p50 1.6–2.0% in all three repetitions.

On iPhone, that authoring journey had atlas/standalone frame p50 1.2126/1.1928 ms (+1.7%), p95 1.2256/1.2436 ms (-1.4%), request-to-completed-frame 17.9244/41.7836 ms (-57.1%), upload 1.2488/4.1838 ms (-70.2%), and direct GPU p50 0.0999/0.0966 ms (+3.4%). Both sides invalidated exactly 64 referencing chunks; atlas reuse changed exactly 64 slot generations.

## UIKit parity

The final-tree physical-device UIKit battery uses the same 1,000 unique 28-square pixels and six visible scroll phases. The idiomatic `UICollectionView`/`UIImageView` path measured 747.936 ms clock p50, 423.411 ms CPU, 90.961 ms bounded process-scoped GPU time, 76.191 ms/s hitch time, 4 missed frames, and 25,215.824 kB peak memory. The accepted hand-optimized UIKit comparator precomposes one immutable 360x2352 layer and moves it once per phase: 588.011 ms clock p50 (-21.4%), 45.142 ms CPU (-89.3%), 86.591 ms bounded GPU time (-4.8%), effectively zero hitch time, zero missed frames, and 48,333.672 kB peak memory (+91.7%, about 22.6 MiB).

Every phase now waits for display presentation. An earlier state-only precompose produced implausible 5 ms totals and zero GPU work and was rejected as invalid proof. An earlier custom-draw optimized UIKit attempt measured roughly 0.759 ms versus a 0.416 ms idiomatic exploratory control and was rejected. The precomposed comparator is a UIKit baseline only; it does not weaken Oxide ownership or image-store policy.

The strict device report retained process-scoped Metal System Trace GPU intervals. Xcode 26.6 did not forward the target cadence summary into the trace launch stdout, so `xtask` reuses the trace and performs one narrow console-summary pass when that exact output is missing. The trace exposed counter tables but no direct counter samples inside either bounded workload window; direct GPU time and latency remained available. Direct energy remains manual-pending; no proxy is substituted.

## Correctness and lifecycle

- Fifteen image-store tests cover typed configuration rejection, display-size decode, source release, cancellation/stale/malformed completion, gutters, exact/deduplicated invalidation, standalone/mip policy, queued-work cancellation under pressure, device loss, generation reuse, native workers, scrolling churn, and 10,000-request slot reuse within hard budgets.
- Metal integration tests prove atlas/standalone pixels match with no neighbor bleed and one slot eviction invalidates only its referencing prepared chunk.
- Web source tests freeze sRGB empty pages, append-only publication, complete standalone mips, unique device generations, and exact invalidation; wasm builds pass.
- `ImageRegionView` cover fitting stays inside the resolved atlas source rectangle.
- Physical Oxide and UIKit cases ran on the attached real iPhone; simulator numbers are not used.

The locally retained `raw/` directory contains macOS reports, physical-device reports/logs, and UIKit JSON/Markdown/log evidence. Large `.trace`, `.xcresult`, derived-data, and app bundles remain temporary machine artifacts rather than repository evidence. Official `latest.*` promotion remains deferred to C62.

## Decision

Accept C60's store and atlas policy for eligible small static collections. Keep the standalone path as a first-class fallback/control, disclose sparse-page memory and small direct-GPU regressions, and require C61 to complete the displayed browser matrix. Reject the initial custom-draw UIKit implementation and the state-only precompose measurement. No camera architecture changes are part of C60.
