# C41 WebGPU analytic neon-marker evidence

C41 replaces each WebGPU neon marker's three analytic RRect records with one 60-byte center/shape/alpha/color/viewport instance. One prebuilt pipeline expands a six-vertex quad and evaluates the corrected Metal core falloff, triangular ring, Gaussian halo, and color/alpha composition in WGSL. Adjacent target/clip-compatible markers retain one ordered draw; no generic vertices or indices are emitted.

The exact C40 parent package was `1de96d644a1d0ed33335a978e1b08893107b6d594e194d3de5d5a7b4cc2f7570`; the C41 candidate package was `523567382bbfcb5d600e555c5982c66b3cc0ab026b2eabb8b3756d4f025ea685`. Two balanced native-arm64 Chrome/Metal populations each used six CPU samples of 24 queue-completed frames and 144 in-app WebGPU timestamp samples per row.

At 64 markers, mean CPU p50 changed from 0.208 to 0.217 ms (+4.3%) while p95 improved from 0.333 to 0.315 ms (-5.3%). Direct GPU p50 improved from 0.0902 to 0.0867 ms (-3.9%) and p95 from 0.0983 to 0.0927 ms (-5.7%). At 1,024 markers, CPU p50 fell from 0.456 to 0.332 ms (-27.1%) and p95 from 0.529 to 0.361 ms (-31.8%). Direct GPU p50 improved from 0.3806 to 0.3658 ms (-3.9%), while p95 rose from 0.3865 to 0.4098 ms (+6.1%) and peak from 0.4361 to 0.5098 ms (+16.9%). The dense GPU tail cost is disclosed: the candidate computes the required Gaussian/ring semantics, whereas the parent drew three flat discs.

The parent emitted 192/3,072 RRect instances, 384/6,144 triangles, and 6,912/110,592 upload bytes at 64/1,024 markers. The candidate emits 64/1,024 marker instances, 128/2,048 triangles, and 3,840/61,440 upload bytes: 66.7% fewer triangles and 44.4% fewer bytes, with one draw and one draw item on both sides. The goal's older 6,912-triangle/roughly-310-KiB 64-marker estimate was already superseded by C37's analytic RRects; these are the achieved C40-parent counters.

Real Dawn pipeline creation and DPR 1/2/3 captures render the 8x8 varied marker grid and pass explicit colorful halo, bright-core, and dark-background pixel checks. Structural coverage freezes the 60-byte ABI and checks the WebGPU core/ring/Gaussian formulas against the corrected Metal shader source. No pre-C41 WebGPU pixel comparison is claimed because the retired three-disc approximation is not the required visual semantic.

C41 is accepted for the decisive dense CPU win, 44.4% upload reduction, 66.7% triangle reduction, corrected cross-backend marker semantics, and bounded sub-0.08-ms dense GPU-tail cost. Aggregate committed browser/workspace baseline promotion remains deferred to C62.
