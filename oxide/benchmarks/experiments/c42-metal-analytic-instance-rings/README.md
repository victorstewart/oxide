# C42 — Metal analytic instance frame rings

Status: accepted.

The C41 parent (`5f03fa3`) staged growing RRect, image, nine-slice, spinner, backdrop, and visual-effect arrays through Metal `set*Bytes`, splitting every array at the 4 KiB API ceiling. C42 reserves completion-protected frame-ring slices, uploads each parameter array once, binds offsets, and emits one draw per compatible ordered run. RRect, nine-slice, backdrop, and visual-effect vertices share the fragment record's rectangle; ordinary images retain warmed CPU parameter scratch because a single build plus bulk ring copies beat a second draw-list traversal. Fixed eight-byte viewport records remain inline.

The final physical-Apple-GPU A/B used an Apple M2 Max on macOS 26.5.2, production release Metal binaries, eight warmups to cycle every offscreen frame slot, 72 measured frames per process, and 20 fixed-seed balanced fresh-process pairs. Each of the 24 rows covers one family at 1/64/1,024/10,000 instances and persists frame, encode, completed-command-buffer GPU, p50/p95/p99/peak, upload, draw, bind, and ring-growth evidence. Percentages below are median paired candidate deltas; negative is faster.

| 10,000-instance family | Frame p50 | Encode p50 | GPU p50 | Frame p95 | Frame p99 | Candidate upload | Draws |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| RRect | -4.9% | -41.5% | +11.8% | -4.8% | -3.7% | 480,000 B | 118 → 1 |
| Image | -3.1% | -10.8% | +10.0% | -3.3% | -2.1% | 640,000 B | 118 → 1 |
| Nine-slice | -7.6% | -51.6% | +9.5% | -7.7% | -7.4% | 640,000 B | 157 → 1 |
| Spinner | -3.6% | -29.8% | +4.1% | -5.1% | -4.5% | 400,000 B | 59 → 1 |
| Backdrop | -1.8% | -9.5% | +5.1% | -0.5% | -1.4% | 320,000 B | 79 → 1 |
| Visual effect | -2.2% | -13.2% | +4.0% | -2.2% | -1.8% | 320,000 B | 79 → 1 |

The decision follows whole-frame behavior rather than a single counter. Device-addressed large arrays raise median GPU duration for these synthetic dense rows, but every family improves frame p50, p95, and p99, CPU encoding falls materially, growing `set*Bytes` calls become zero, and every warm candidate row reports zero ring growth. The predeclared dense-family encode metric aggregates 8,640 samples per side: 0.208/0.575/0.706/1.519 ms parent p50/p95/p99/peak became 0.153/0.520/0.679/1.151 ms. The repository's 100,000-resample analyzer accepted the 24.5% median paired speedup with a 13.0%–29.7% confidence interval and 16/20 pair wins.

At 10,000 instances the parent made 118/118/157/59/79/79 draws and twice as many growing inline-array calls for RRect/image/nine-slice/spinner/backdrop/visual-effect. The candidate makes one draw, two buffer binds, and one fixed viewport inline call per run. Shared rectangle records reduce logical upload bytes 25% for RRect, 20% for nine-slice, and 33.3% for both effect families; image and spinner preserve their prior logical byte counts.

Verification observed:

- `cargo test -p oxide-renderer-metal --features snapshot-tests`: 94 tests passed, including 27 Metal pixel-readback snapshots and the cached-layer image/effect regression.
- `cargo test -p oxide-perf-runner`: 119 tests passed.
- The 24-row release smoke and final balanced matrices completed without Metal submission errors; candidate dense rows reported one draw, two analytic buffer binds, exact instance counts/bytes, and zero warm ring growth.
- Identical parent instrumentation SHA-256: `f2c2554c6f237a603d46631c097e8b0d33bb6dc56168182a42be24d982879184`.
- Parent binary SHA-256: `b9af2e2037e8d566e5bbe6e9877c79e0fa8cb5b873b24c411d588b88c8b00c00`.
- Candidate binary SHA-256: `e53350b0b29f4053ebbebe4a9e0cdb8db6eee9df61df7123b01ad3e72f3764fb`.

The ignored `raw/` directory retains all 40 final JSON reports, the paired input, the accepted paired report, and every indexed sample and counter. Aggregate workspace and iPhone baseline promotion remains deferred to C62.
