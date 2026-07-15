# C52 wide blur kernels

## Decision

Accepted against C51 parent `def6d456b85ffea9cbcbbcebd2130f28e420ea75`.

The performance hypothesis was that wide separable Gaussian passes were fragment-bound by `2r + 1` texture samples and one runtime exponential evaluation per positive tap. Precomputed normalized adjacent-tap pairs should exploit linear filtering to remove every runtime exponential and approximately halve samples without changing the render graph, target resolution, pass count, or visible material contract.

Metal now retains the exact path for pass sigma below 2, non-finite values, non-1/16 buckets, and radius mismatches. Canonical kernels are generated lazily once per process, retained by sigma bucket, and prepacked as horizontal/vertical normalized constant blocks so each pass needs one Metal binding. Exact and paired work use separate persistent pipeline states so the paired shader does not carry the exact loop or its exponentials. This operates inside the existing quarter/eighth downsample chain and reuses its persistent intermediate targets.

## Evidence

The production sweep covers local sigma 2/8 guardrails and full-screen sigma 16/32/64 workloads at 1200x800. Every row records frame, encode, direct command-buffer GPU, pass, target-memory, exact/paired selection, source/encoded samples, exponential-tap proxy, and process-resident table bytes.

The paired rows retain two blur passes and zero runtime exponential taps. Samples fall from 26 to 14 at source sigma 8, 50 to 26 at sigma 16, 98 to 50 at sigma 32, and 194 to 98 at sigma 64: a 46.2–49.5% reduction. The small source-sigma-2 control remains exact at 10 samples and four exponential taps.

The authoritative full-screen sigma-64 run used 15 balanced fresh-process pairs, 12 warmups, and 140 measured frames per process. Across 2,100 direct command-buffer timestamps per side, GPU p50 improved from 0.3836 ms to 0.2588 ms. The paired median speedup was 33.16%, its 95% confidence interval was 30.78–33.56%, and the candidate won all 15 pairs. Aggregate GPU p95/p99/peak improved from 0.6831/1.3817/1.5606 ms to 0.5640/1.2992/1.5367 ms. Whole-frame p50 was neutral at 1.5626 to 1.5610 ms, and encode p50 moved from 0.0515 to 0.0499 ms; their paired median speedups were 0.07% and 0.55%, showing that this isolated workload's measured win is fragment-side.

The sigma-8 local guard initially shaded too little work for stable command-buffer timing. The final identical parent/candidate workload uses an 800×480 local region while sigma 2 retains the smaller control. Across 15 balanced pairs and 2,100 timestamps per side, sigma-8 GPU p50 improved from 0.2465 to 0.2422 ms; its paired median speedup was 1.84%. P95/p99/peak improved from 0.4634/1.2610/1.4965 ms to 0.4516/1.2516/1.4109 ms. The shared no-material-regression policy accepted the local guard with every tail limit satisfied.

Exact-render controls cover sigma 2/8/16/32/64. Sigma 2 remains byte-identical. Sigma 8/16/32/64 have maximum channel error 1/255, MAE 0.00035–0.00132, and 0.14–0.53% changed pixels; sigma 8 specifically records MAE 0.000854 and 0.34% changed pixels. The existing quarter/eighth sequence goldens also remain within 1/255, MAE at most 0.00197, and 0.79% changed pixels. The tests enforce tighter bounds than those observed, so default quality cannot drift silently.

## Rejected alternatives

A full eager table for all 1,024 sigma buckets was rejected because it performs unnecessary startup exponentials and retains roughly 400 KiB even when one kernel is used. A single uniform-branched exact/paired shader was also rejected: despite halving samples, the required 15-pair run was neutral at -0.09% median with only 7/15 wins. Splitting the already-persistent pipeline state removed the exact loop from the wide shader and produced the accepted 33.16% result. More aggressive Kawase or additional downsample levels were rejected because the existing render graph already supplies the measured quarter/eighth path and another level would add target/pass cost and larger image error after the paired path had cleared the performance bar.

The goal stages the current physical-device matrix in C61 and permits official baseline promotion only in C62. Direct iPhone energy and thermal evidence therefore remains pending that mandatory final matrix; no proxy is substituted and no intermediate official device baseline is changed here.
