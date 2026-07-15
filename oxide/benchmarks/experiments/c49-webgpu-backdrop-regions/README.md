# C49 WebGPU backdrop-region copies

## Decision

Accepted the regional-copy implementation at the full candidate staged tree
recorded in the raw paired inputs against C48 parent
`f4b631ef1431a23ef15b37d7e214c89ffe148475`.

The renderer clips each backdrop sample to its visible logical rect, adds the
blur and bilinear-filter outset, converts the result to physical pixels, and
copies that source/destination subregion. Same-epoch regions may collapse to a
single bounding copy only when they share the same source/destination mapping
and the bound remains smaller than the old full-target copy. An intervening
overlap ends the epoch. The initial clear is folded into the first ordinary
render pass unless the first operation needs a cleared backdrop snapshot.

## Evidence

The final 15-pair fixed-seed browser population used fresh arm64 Chrome
processes and 12 samples of 64 frames per side. The shared analyzer accepted
the separated-48 CPU guardrail: parent/candidate process p50 was
0.260/0.260 ms, paired median change was -0.392% with 95% CI
-0.772%..0.769%, and the candidate distribution p95/p99/peak was
0.267/0.268/0.268 ms versus 0.273/0.274/0.274 ms. The deterministic copied-byte
decision passed 15/15 pairs and reduced 45,088,768 to 2,010,432 bytes per frame
(95.541%).

The final GPU population retained 2,010 whole-frame timestamp samples per side
across 15 balanced fresh-process pairs. The no-material-regression decision
passed: parent/candidate p50 was 1.526553/1.537563 ms, p95
2.256552/2.163079 ms, p99 2.475006/2.450269 ms, and peak
3.279872/2.877082 ms; paired median change was -0.053%. A 500-frame
coalescible control improved whole-frame GPU p50 from 0.176736 to 0.154035 ms
and copy-fence p50 from 0.043622 to 0.024083 ms.

Counters for the six required cases were:

| Case | Copies | Copied pixels | Render passes |
| --- | ---: | ---: | ---: |
| separated-48 | 43 -> 43 | 11,272,192 -> 502,608 | 46 -> 45 |
| coalescible-12 | 1 -> 1 | 262,144 -> 124,800 | 4 -> 3 |
| fullscreen | 1 -> 1 | 262,144 -> 262,144 | 4 -> 3 |
| edges/corners | 1 -> 4 | 262,144 -> 28,208 | 4 -> 3 |
| nested layers | 2 -> 2 | 299,520 -> 96,256 | 7 -> 5 |
| mixed sigma | 1 -> 1 | 262,144 -> 143,440 | 4 -> 3 |

All six parent, candidate, and frozen-golden 512x512 PNGs were byte-identical.
Focused renderer tests (13 unit, 5 image-slot, 37 contract), 27 web-host tests,
renderer and host wasm checks, JavaScript syntax validation, and diff checks
passed. The committed aggregate web baseline remains deferred to C62.

## Rejected alternatives and attribution limits

A strict one-copy-per-region implementation was rejected. Although it copied
only 55,488 pixels for the coalescible-12 case, it raised whole-frame GPU p50
from 0.186686 to 0.337923 ms (about 81%) by issuing 12 copies. The retained
same-epoch bounding copy moves 124,800 pixels in one copy and measured
0.163457 ms in the matching preliminary control.

Chrome 150 does not expose command-encoder timestamp writes on this adapter, so
the harness uses opt-in empty compute-pass fences around the copy interval.
That isolated stage population was rejected as an authoritative decision:
paired median speedup was 24.282%, but only 9/15 pairs won and the 95% CI was
-18.168%..31.948%. The fences are disabled on the production path. Copied
bytes plus whole-frame GPU time are the accepted proof; the unstable isolated
stage result is retained only as diagnostic evidence.
