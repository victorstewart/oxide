# C03 renderer parity evidence

This is the stable evidence root for C03 cross-backend correctness work. Generated baseline reports and JSON field dumps live in the tracked `raw/` evidence directory.

Current parent-defect fixture:

- CPU reference: exact 17x11 sparse city/neighborhood masks, seed records, and JFA jumps 16/8/4/2/1.
- Metal: exact raster and final city/seam field parity with the CPU records.
- WebGPU parent: exact raster IDs, with final-field mismatch expected until C04 isolates uniform records per encoded pass.
- Parent mismatch result: 0 city raster mismatches, 0 neighborhood raster mismatches, 22 city-field mismatches, and 110 seam-field mismatches.

The parent mismatch is intentionally narrow: C03 does not change production ID-mask uniform lifetime or weaken image tolerances. C04 must rerun the same fixture with `scripts/compare_id_mask_reference.mjs --expect match`.

The same parity manifest freezes every still-image family, the five layout/state variants at DPR 1/2/3, MSAA/EDR capability rows, and the five required multi-frame sequences.

Committed correctness evidence:

- Exact CPU reference PNGs for the asymmetric seed and all five JFA jumps.
- Exact Metal raster/final-field parity with the CPU oracle.
- Exact Metal single-sample and 4x MSAA images plus exact 64-bit BGRA10_XR component readback. The XR decoder follows Apple's four padded 16-bit component layout and `(value - 384) / 510` mapping.
- Narrow Metal component goldens: at most 16 changed pixels, 3 channel levels, and 0.02 MSE for ordinary antialiased scene captures; capability and sequence images are byte-exact.
- Metal sequence images for full-direct to partial damage, renderer recreation controls for memory warning and device loss, resize, and A8 atlas eviction/rebuild.
- The full-direct/partial sequence intentionally freezes the C10 parent defect next to the correct complete-scene image: the parent loses untouched retained content because its frame-global damage scissor is not applied before ordinary draws.

Same-day starting-SHA baseline context was captured on 2026-07-12 against parent `5b2875457a09d4a85a59e1ec7695c69c475dacb8`:

- Workspace full suite, 293 rows: `raw/workspace-starting-sha.json`, SHA-256 `89b4735f4602204faafc3522feccc232436b6f79a2d505fbe3a9aa2c312dd2b9`.
- Workspace Markdown: `raw/workspace-starting-sha.md`, SHA-256 `7d13661b43171b47b68784e00e36d2daf033f61acba3309a7135e83462972c9c`.
- WebGPU report, one production submission per RAF for 2,000 frames: `raw/webgpu-starting-sha.json`, SHA-256 `edca2d135ec893167669b216ce36871fff3e7ee92d14d7c16462d510aa81d5b8`.
- Browser startup, 25 fresh profiles: `raw/browser-startup-starting-sha.json`, SHA-256 `7405b2cb3cc7191a2cb96c7dcaabf603848ed7f34a3dc9431544c87f81dfb2e8`.
- Release macOS Metal architecture diagnostic, 87 rows: `raw/macos-metal-starting-sha.json`, SHA-256 `efd842cda7733390ec077eb1bced798940a31de434fd48f65507e3ed8ea82057`.
- Metal Markdown: `raw/macos-metal-starting-sha.md`, SHA-256 `d9e39e35afa16b277fe07eef821f501c470c6194bb38e584730c617a78834609`.
- CPU ID-mask oracle: `raw/cpu-id-mask-reference.json`, SHA-256 `7bbdf0b8787a572a469a4f714ad6fcbbf36025444e6ec8846a287401d407d3cd`.
- WebGPU parent field capture: `raw/webgpu-id-mask-parent.json`, SHA-256 `09fe8b7afe7679f8f67189aee5ae8a3c6bed980f5528e4e5881b66085d7b955e`.
- WebGPU mismatch decision: `raw/webgpu-parent-mismatch.json`, SHA-256 `da073e50222e82f94a3a94563ac0198c50b7d83ef3699ec16c23e51eaa0dd117`.

These are starting-context captures, not an A/B performance claim. Instrumentation is the identical existing C00-C03 harness; randomization seed and AB/BA order are not applicable because no candidate comparison is being accepted in C03.

Environment and controls:

- Mac14,6; Apple M2 Max 38-core GPU; 96 GB; macOS 26.5.2 build 25F84; Metal 4.
- Rust/Cargo 1.86.0, Node 22.19.0, wasm-bindgen 0.2.121, Chrome 150.0.7871.114 arm64, ANGLE Metal.
- AC power connected, battery 100%, system Low Power Mode reported enabled, built-in 3456x2234 XDR display, automatic brightness disabled.
- WebGPU viewport 500x232 CSS pixels, DPR 1, cross-origin isolated, warm resources after 64 RAF warmups; startup uses 25 fresh browser profiles.
- Workspace uses locked debug artifacts; the macOS Metal diagnostic and Web package use locked release builds. Official `latest.*` files remain unchanged until C62.

Exact baseline commands:

- `PERF_REPORT_DATE=2026-07-12 cargo run --locked -p oxide-perf-runner -- --run-suite --json-out benchmarks/experiments/c03-renderer-parity/raw/workspace-starting-sha.json --markdown-out benchmarks/experiments/c03-renderer-parity/raw/workspace-starting-sha.md`
- `node scripts/check_webgpu_browser_golden.mjs --report-only --architecture-matrix --chrome-arch arm64 --frame-samples 2 --frames-per-sample 4 --raf-frames 2000 --id-mask-samples 8 --id-mask-frames 8 --upload-samples 2 --upload-frames 4 --scene3d-samples 2 --scene3d-frames 4 --mixed-samples 2 --mixed-frames 4 --report-timeout-ms 240000 --raw-report benchmarks/experiments/c03-renderer-parity/raw/webgpu-starting-sha.json`
- `node scripts/check_webgpu_browser_golden.mjs --chrome-arch arm64 --startup-report benchmarks/experiments/c03-renderer-parity/raw/browser-startup-starting-sha.json --startup-repeats 25 --report-timeout-ms 240000`
- `PERF_REPORT_DATE=2026-07-12 OXIDE_PERF_RUNNER_FILTER='gpu.architecture.' cargo run --release --locked -p oxide-perf-runner -- --run-suite --smoke --json-out benchmarks/experiments/c03-renderer-parity/raw/macos-metal-starting-sha.json --markdown-out benchmarks/experiments/c03-renderer-parity/raw/macos-metal-starting-sha.md`

Physical-iPhone status:

- The repository harness resolved, built, signed, installed, and ran the parked on-screen Oxide host on a paired iPhone 17 Pro Max (`iPhone18,2`, iOS 26.5.1 build 23F81) at native refresh and 1170x2532 physical pixels, DPR 3.
- `raw/iphone-starting-sha.json`, SHA-256 `4339a4eff41cde8cb8c813ea9eef18f04afc935825935fbfd65be0bd7bd764a8`, preserves the device-emitted case report: four 96-frame samples, 387 CADisplayLink intervals, 96 in-app Metal command-buffer GPU samples, peak memory 44,256 KB, missed frames, and hitch metrics.
- In-app GPU total p50/p95/p99/peak was 0.873396/0.877781/0.883500/0.885875 ms. This is the repository-mandated first source of truth and does not depend on Instruments hardware-counter availability.
- Exact successful on-device command: `PERF_REPORT_DATE=2026-07-12 cargo run --locked -p xtask -- ios oxide-device-perf --smoke --device "Victor’s iPhone" --trace-seconds 0 --json-out benchmarks/experiments/c03-renderer-parity/raw/iphone-starting-sha.json --markdown-out benchmarks/experiments/c03-renderer-parity/raw/iphone-starting-sha.md --result-root /tmp/oxide-c03-iphone`; the harness completed the case but correctly declined to emit an official report because external `gpu_time_s`/`gpu_latency_s` aliases were absent.
- A process-scoped 15-second Metal System Trace cross-check was attempted with the same command and `--trace-seconds 15`. The harness rejected it before capture because Xcode DeviceSupport is 26.5 build 23F77 while the phone is build 23F81; installing updated system-wide platform support is outside this commit. No proxy was substituted for the unavailable external trace.
