# C61 — Current physical-device and browser proof

Status: accepted as current final-tree measurement proof. The complete saved iPhone result root passed; official report promotion remains reserved for C62.

## Scope and decision rule

C61 refreshes measurement and correctness evidence for the completed rendering stack; it does not claim a new renderer speedup. The candidate is acceptable only if the complete workspace report, supported browser matrix, and native-ProMotion physical-iPhone Oxide/UIKit matrix pass from the final tree. Official `latest` and dated report promotion remains reserved for C62.

The rerun found three evidence defects and one stale interaction test rather than a production rendering regression:

- C18's permanent Metal frame-resource rows collected completed command-buffer time but omitted the required GPU distributions from the report. The harness now persists p50/p95/p99/peak for both rows.
- Chrome's outer `--window-size` did not equal its drawable viewport. The browser page now pins the requested canvas CSS dimensions, restores the renderer surface after independently sized microbenchmarks, persists CSS and physical backing dimensions, and rejects CSS×DPR mismatches.
- A successful screenshot could leave Chrome alive and a timeout lacked a retained forced-kill fallback. Capture now terminates the bounded process after a stable artifact and escalates to `SIGKILL` only if it ignores `SIGTERM`.
- Zoom Image gesture tests still inspected the retired `NineSlice` representation and inferred magnification from an intentionally clipped destination. They now verify the current `Image` source crop and destination invariants.

## Optimized workspace battery

The locked release workspace suite completed all 399 current cases and passed the current report contract. The July 9 official baseline predates much of the completed stack: comparison against it reported 238 missing rows and broad thermally sensitive drift, so it is recorded as stale evidence rather than used to accept or reject C61. No official workspace report is promoted here.

## Chrome arm64 WebGPU

The supported automated browser target is native-arm64 desktop Chrome 150 on the Apple M2 Max host. Safari and a mobile/alternate WebGPU browser have no project-supported automation route, so they are recorded as unsupported rather than represented by substituted data. Native Chrome RAF ran at approximately 120 Hz; symmetric 60 and 120 Hz deadline fields are reported, while forced 90 Hz is unsupported.

The final full run used the optimized 3,538,504-byte package (`wasm` 3,376,453 bytes; generated JavaScript 135,522 bytes), an isolated profile, cross-origin isolation, and 2,000 displayed frames. It passed the browser report contract and exact app golden:

| Metric | p50 | p95 | p99 | peak |
| --- | ---: | ---: | ---: | ---: |
| RAF interval | 8.335 ms | 9.585 ms | 10.245 ms | 10.320 ms |
| direct WebGPU command-buffer time | 0.160 ms | 0.202 ms | 0.223 ms | 0.384 ms |

The 60 Hz deadline had zero missed or hitch frames. The strict 120 Hz comparison counted 1,041/2,000 intervals above 8.333 ms but zero hitch frames; this reflects native callback quantization around the exact 120 Hz boundary and is not reported as a 120 Hz pass. All 14 benchmark marks had zero WASM linear-memory growth. Seventeen timestamped rows reconciled 82 passes and 1,973,136 ns of stage attribution. The duplicate Chrome trace retained 1,188,647 events, all 14 benchmark intervals, 56 marks, 587,848 GPU-related events, and 12,892 WebGPU-related events.

The final capture and committed golden share SHA-256 `fec7565f2b8da54045c46976046cde2ff405d59488e710d8312f99775df6408d`; pixel diff, maximum channel error, and MSE are exactly zero.

### Scale and resolution controls

Each control retained exactly 2,000 RAF intervals and 2,000 direct GPU timestamp samples, ended with zero pending submissions, and reported the requested drawable instead of Chrome's outer window:

| CSS canvas | physical canvas | DPR | RAF p50 / p95 | GPU p50 / p95 |
| --- | --- | ---: | ---: | ---: |
| 320×240 | 640×480 | 2 | 8.335 / 9.520 ms | 0.224 / 0.269 ms |
| 320×240 | 960×720 | 3 | 8.335 / 9.560 ms | 0.365 / 0.420 ms |
| 1920×1080 | 1920×1080 | 1 | 8.335 / 9.685 ms | 0.690 / 0.732 ms |
| 3840×2160 | 3840×2160 | 1 | 8.335 / 9.495 ms | 2.362 / 2.636 ms |

The separate architecture report passed all 18 cases and reconciled the final full-run trace. A 25-process startup population kept cold, warm, and hot observations separate: the first cold run reached report-ready in 73.840 ms, the second warm run in 65.830 ms, and the remaining hot population reported p50/p95 of 67.395/70.970 ms. Hot first-frame p50/p95 was 15.105/15.415 ms. Startup WASM memory was a stable 3,473,408 bytes.

## Physical iPhone

The complete same-root matrix passed on the attached iPhone 17 Pro Max (`iPhone18,2`, iOS 26.5.1) with Xcode 26.6 at native refresh. Its build stamp records destination `platform=iOS,id=00008150-001529C434F8401C`, development team `6GQ7T2VDQ5`, and source fingerprint `15434600775279868670`. The watchable smoke contained 9 UIKit and 5 unique Oxide cases. The family proof then completed 38 UIKit and 23 Oxide cases: component 10/9, animation 14/7, navigation 4/2, journey 8/4, and camera 2/1. Every family is marked `watchable_smoke_passed=true` and `family_proof_passed=true` in `proof-status.json`.

The unchanged signed build produced application timing reports and process-scoped Metal System Trace evidence for every case. The result root retains 75 trace bundles. Xcode's optional Metal GPU Counters profile was unsupported or timed out for several launches; the harness therefore retained the required in-app command-buffer/pass timing and process-scoped Metal System Trace fallback instead of inventing counter values. One optional counter launch for the Oxide input-form case posted a transient benchmark-failure notification; the normal console run passed, exact-root resume rejected the corrupt trace, and the no-counter fallback passed without a source or build change.

The camera family preserved the required separate buckets. The pure-custom UIKit NV12 path measured 1.0201/1.0410/1.0429 s clock p50/p95/p99, 47.67 ms CPU p50, 227.33 ms bounded process GPU time, zero missed frames, and 86,229.84 KiB peak memory. The diagnostic `AVCaptureVideoPreviewLayer` baseline measured 1.0515/1.0516/1.0516 s, 26.53 ms CPU p50, 218.09 ms bounded process GPU time, zero missed frames, and 14,189.30 KiB peak memory. Oxide's custom camera path reported direct in-app renderer GPU p95/p99/peak of 1.3499/1.4380/1.4382 ms across 120 frames, capture-total p95 0.0747 ms, and host-tick p95 0.2830 ms. These measurements do not change the product camera architecture and do not turn the preview-layer diagnostic into a release path.

Automatic direct energy is unavailable in the current toolchain path. Per the device contract and the user's instruction, energy remains manual-pending and no proxy is substituted.

## Evidence and verification

- Workspace: `/tmp/oxide-c61/workspace/current.json`, `current.md`, `current-compare.json`, and `current-compare.md`.
- Full browser: `/tmp/oxide-c61/browser/full-final7/`.
- Browser architecture: `/tmp/oxide-c61/browser/architecture-final/`.
- DPR/resolution controls: `/tmp/oxide-c61/browser/matrix-{dpr2,dpr3,1080p,4k}-final/`.
- Startup: `/tmp/oxide-c61/browser/startup-final/startup.json`.
- Physical device: `/tmp/oxide-c61/device/`, including `proof-status.json`, family reports, application logs, and 75 process trace bundles.
- `node --check scripts/check_webgpu_browser_golden.mjs`
- `node scripts/check_webgpu_browser_golden.mjs --self-test-measurement`
- `cargo test --locked -p oxide-host-web --test lib_tests`: 28 passed.
- `cargo test --locked -p oxide-test-scenes --test onscreen_benchmark_tests`: 4 passed after correcting the stale draw-representation assertion.
- Required renderer-api, ui-core, text, and Metal snapshot-feature suites passed.
- The required combined 8-package `--all-targets` gate passed, including 114 perf-runner report tests and 129 xtask tests.

## Decision

Accept C61 as a measurement-foundation and final-tree proof commit with no renderer speedup claim. The workspace, supported browser, golden, and complete same-build physical-iPhone gates pass. Unsupported browser/refresh/counter configurations and manual-pending direct energy remain explicit rather than being replaced by synthetic data. C62 alone owns baseline promotion.
