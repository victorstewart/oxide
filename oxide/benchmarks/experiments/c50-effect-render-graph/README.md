# C50 reusable effect render graph

## Decision

Accepted the reusable snapshot-epoch graph against C49 parent
`a01f24d2a186ec010d66c217aa5f65b50189f38e`.

The shared renderer plan records targets, physical source/destination/output
regions, formats, sample counts, storage, command lifetimes, explicit pass
reasons, load/store actions, dependencies, capture epochs, and compatible blur
pyramids. An intervening write, target change, source/output dependency, mapping
change, sigma change, or quality change prevents unsafe reuse. Nonoverlapping
transient resources reuse compatible alias slots; persistent resources remain
distinct for backend pooling. Aggregate counters are computed when a plan is
built, so warm cache hits read them in constant time.

Metal builds the semantic key during its existing command classification scan,
retains the plan across identical frames, and drives effect target selection,
capture bounds, blur bounds, and kernel selection from the plan. WebGPU keeps an
eight-entry LRU of plans keyed by an incrementally maintained frame signature;
the graph drives capture regions and the C49 regional-copy executor without a
second warm-frame draw scan.

## Evidence

The final Metal population used 15 balanced parent/candidate pairs with 24
warmups and 240 measured frames per effect case. Paired median p50 changes
were:

| Case | Frame p50 | Encode p50 | GPU p50 |
| --- | ---: | ---: | ---: |
| backdrop coalescible 12 | -0.199% | -1.081% | +0.091% |
| mixed sigma | -1.383% | -0.582% | -2.158% |
| nested layer effects | -1.049% | -5.572% | -3.778% |

Coalescible paired frame p95/p99 improved 0.875%/0.163%, and GPU p95/p99
improved 3.290%/1.298%. Nested frame p95/p99 improved 1.370%/7.685%, with GPU
p95 better by 7.766%. Mixed-sigma frame p95/p99 were noisy at +4.503%/+5.004%
even though GPU p50/p95 improved 2.158%/1.864%; its GPU p99 contained a
candidate outlier (+15.633%). No render pass, shader, sampled region, or pixel
changed, so the p50 and GPU-p95 guardrails are the decision metrics and the
unstable tail is retained rather than presented as a win.

The final native-arm64 Chrome CPU population used 15 balanced fresh-process
pairs, 12 samples, and 64 frames per side. Parent/candidate process medians were
0.038/0.037 ms for p50 and 0.051/0.051 ms for p95; the paired median change was
0.000% for p50, p95, p99, peak, and average. A separate 15-pair population kept
2,010 WebGPU timestamp samples per side. Aggregated parent/candidate CPU submit
p50/p95/p99 were identical at 0.035/0.070/0.180 ms. Aggregated GPU
p50/p95/p99 changed from 0.197779/0.260867/0.277372 ms to
0.197498/0.233414/0.257621 ms (-0.142%/-10.524%/-7.121%).

The six required WebGPU cases retained their C49 copy/pass counts while
reporting graph captures, composite passes, plan reuse, resources, aliases,
lifetimes, and bytes. All six final 512x512 captures were byte-identical to the
frozen C49 goldens (zero differing pixels, zero maximum error, zero MSE).

Five shared graph tests, 9 draw-list tests, 16 retained-chunk tests, 13 Web
renderer tests, 5 image-slot tests, 37 Web contract tests, 25 Metal tests, 3
perf-runner tests, and 27 Web-host tests passed. Renderer and host wasm checks
and JavaScript syntax validation passed. The installed wasm-pack `wasm-opt`
binary cannot parse the toolchain's multi-table module; the symmetric release
WASM inputs were therefore measured before that unsupported optimization step.
The committed aggregate baseline remains deferred to C62.

## Rejected alternatives

The first uncached Metal plan rebuilt every frame and was rejected: its
10-pair mixed-sigma medians regressed encode time by 5.751% and frame time by
0.288%. Retaining the plan removed that cost.

The first WebGPU implementation rescanned every draw to derive the cache key
and was rejected after a 12-pair browser population showed a +2.27%
paired p50 median. The accepted implementation maintains the semantic signature
while lowering draws and performs an O(1) warm lookup. A later review also
removed per-hit graph-stat scans by caching the completed aggregate record in
the plan. Unbounded plan retention and cross-write pyramid sharing were not
considered acceptable alternatives.
