# C51 direct final drawable

## Decision

Accepted against C50 parent `ad56907cd7a84da862d43ad36f0e405597d18900`.

The performance hypothesis was that effect, layer, and camera auxiliary textures
do not require the final color image to persist. Rendering the final main pass
directly into a compatible drawable should therefore remove one full-frame
offscreen store/read/blit path without changing auxiliary passes or pixels.

Metal now makes that distinction explicitly. MSAA resolve, partial-damage
persistence, earlier Scene3D color, incompatible or absent presentation
textures, and explicit offscreen/readback use still allocate and retain the
final target. Resize invalidates stale-sized final targets but does not allocate
a replacement until one of those dependencies needs it. Both flat draw lists
and prepared retained snapshots use the same policy.

## Evidence

The real-drawable benchmark uses a 1200x800 `CAMetalLayer` frame containing a
camera-blur request, a cached layer, and a visual effect. The final population
used 15 balanced parent/candidate process pairs, each with 12 warmups and 120
measured frames. Across 1,800 samples per side:

| Metric | Parent p50 | Candidate p50 | Change |
| --- | ---: | ---: | ---: |
| Frame | 8.1968 ms | 8.1257 ms | -0.87% |
| Encode | 0.3741 ms | 0.3491 ms | -6.68% |
| Direct Metal GPU | 0.5753 ms | 0.4580 ms | -20.39% |

The balanced per-pair median changes were -1.30% frame, -11.59% encode, and
-18.81% GPU. Aggregate GPU p95/p99 improved 10.49%/2.72%. Frame p95 was noisy
at +2.56%, while frame p99 was neutral at -0.04%; isolated peaks favored the
candidate but are not used as the acceptance signal.

The auxiliary case changed exactly one full-frame path:

| Counter per measured frame | Parent | Candidate |
| --- | ---: | ---: |
| Final blit encoders | 1 | 0 |
| Final texture copies | 1 | 0 |
| Final copied bytes | 3,840,000 | 0 |
| Persistent final-target frames | 120 | 0 |
| Persistent final-target resident bytes | 4,112,384 | 0 |

A separate partial-damage guardrail used 10 balanced pairs and 1,200 samples
per side. Both revisions retained one blit, one 3.84 MB copy, and the same
4.11 MB persistent target on every measured frame. Aggregate candidate p50
changes were +0.22% frame, -1.72% encode, and +0.53% GPU, consistent with an
unchanged persistence path.

Twenty-six Metal unit tests, 70 Metal integration/contract tests including 29
snapshot-feature tests, all 11 Metal sequence/capability golden tests, 3
perf-runner unit tests, 9 paired-runner tests, all 111 frozen-report tests, and
8 macOS host tests passed. The new sequence case combines effect,
layer, and camera auxiliary work and is byte-identical between offscreen and
direct final targets. The attached physical iPhones were offline, so device
energy remains manual-pending; no proxy is substituted. The committed aggregate
baseline remains deferred to C62.

## Rejected alternatives

Keeping eager resize allocation would preserve the old memory footprint even
when every frame can present directly, so it was rejected. Treating auxiliary
textures as final-target dependencies retained the 3.84 MB/frame copy and was
the measured baseline. Releasing a compatible persistent target after every
direct frame was also rejected because it would trade bounded retained memory
for allocation churn when applications alternate direct and partial-damage or
readback frames.
