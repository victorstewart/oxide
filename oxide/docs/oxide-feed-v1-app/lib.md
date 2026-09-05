# oxide-feed-v1-app `src/lib.rs`

## Intention and purpose

This crate is the production-path Oxide treatment for the frozen `feed-v1` physical-iPhone pilot. It renders the same deterministic 2,000-row scene as both UIKit treatments while keeping collection virtualization, raw-touch drag recognition, inertia, frame demand, drawing, settlement, and result emission in Rust. It is deliberately one narrow app, not a reusable benchmark framework or a second test-scene host.

## Relation to the rest of the code

- `oxide-host-ios` injects the app into its production host, forwards raw UIKit touch samples, supplies frame timing and image upload access, and acknowledges only successfully submitted frames.
- `oxide-ui-core::CollectionView` virtualizes the frozen rows; `VerticalScrollSurface` owns drag and inertia; `oxide-text` shapes the two embedded Asap fonts; renderer-api commands are consumed by the production Metal renderer.
- [`contract`](contract.md) independently reconstructs the Swift fixture identity and exact scene geometry.
- [`observation`](observation.md) writes the strict nonce-scoped run/failure records consumed by the feed-v1 reducer.
- The device harness waits for ready/completion notifications and supplies only controller metadata. It never drives app state through the diagnostic API. XCTest, not this crate, owns the two immediate full-screen PNGs attached to each publication smoke tuple; primary retains none.

Call graph:

- native `main` -> `rust_entry` -> `oxide_host_ios::run_app` -> `FeedV1App::from_environment`
- host callbacks -> `App::event` -> raw-touch/submit/lifecycle state transitions
- display link -> `App::prepare_frame` -> scroll advance -> visible collection rows -> text/checker commands -> `PreparedFrame`
- successful warm submit -> device/counter baseline -> ready notification
- successful closing submit -> endpoint/counter delta -> atomic success record -> completion notification

## Entry points list

- `oxide_feed_v1_app::contract` is the public frozen fixture module documented in [`contract.md`](contract.md).
- `oxide_feed_v1_app::observation` is the public strict record module documented in [`observation.md`](observation.md).
- `oxide_feed_v1_app::FeedV1Phase` is the read-only phase enum: `MountCold`, `MountWarm`, `AwaitingReadySubmit`, `Ready`, `TouchPending`, `Gesture`, `SettledFramePrepared`, `AwaitingCompletionSubmit`, and `Finished` describe admission, interaction, acknowledgement, and terminal state.
- `oxide_feed_v1_app::FeedV1Status` is a copyable diagnostic snapshot containing `phase`, `content_offset_points`, `gesture_start_timestamp_ns`, `callback_sample_count`, `inertia_observed`, and `pending_submit_frame_id`.
- `oxide_feed_v1_app::FeedV1App::from_environment() -> FeedV1App` validates controller-owned `OXIDE_FEED_V1_*` inputs and constructs one frozen run.
- `oxide_feed_v1_app::FeedV1App::status(&self) -> FeedV1Status` observes app-owned state without changing frame demand or behavior. Integration tests are its main caller; the iOS host must not use it as a control surface.
- `impl oxide_platform_api::App for FeedV1App` exposes `init`, `event`, `prepare_frame`, and `prepared_frame` to the production iOS host.
- `oxide_feed_v1_app::rust_entry(argc: i32, argv: *mut *mut c_char) -> i32` is the exported C entrypoint. It transfers the app to `oxide_host_ios::run_app`.

## Logic narrative

1. The authoritative device runner first executes independent Swift and Rust host checks against the frozen source snapshot; both derive and verify the complete 717,745-byte canonical fixture. Runtime construction then parses the treatment, endpoint, phase, nonce, and sampling indices, builds only the 2,001-entry height prefix, registers the embedded Asap fonts, and preallocates frame and observation scratch. Offscreen row strings remain cold until collection virtualization requests them. Phase and indices affect record identity only; smoke and primary runs share identical scene and interaction behavior.
2. Every composition starts text collection with `begin_frame_at_scale(frame.scale)`, emits visible glyph commands, and calls `finish_frame` through `RuntimeTextUploader` before the prepared draw list is published. That frame scope coalesces glyph-atlas publication instead of uploading each glyph mutation independently. The cold mount composes the visible viewport, admits the static collection content extent once, and schedules one natural warm frame. The warm mount freezes actual visible component geometry and then waits for the exact frame's successful-submit acknowledgement.
3. Ready admission first snapshots cumulative thermal/low-power transition counters, then samples device/display-link state and posts the nonce-scoped ready notification. Counter-first ordering closes the gap in which a transient state change and return could otherwise disappear before the baseline.
4. A raw touch inside the feed becomes `TouchPending`. Only the first move that crosses Rust-owned drag slop starts measurement, records the gesture timestamp/offset, and requests continuous frames.
5. Touch release marks `inertia_observed` only when `VerticalScrollSurface::wants_next_frame` proves Rust-owned inertial motion is active. Gesture frames append bounded display-link timestamp pairs, advance that motion, and render only visible rows. When motion settles, one additional callback composes the exact closing frame.
6. Submission failure leaves the prepared frame immutable and allows the host to associate it with a newer retry frame ID. Readiness or completion occurs only for the latest matching successful-submit acknowledgement.
7. Completion captures device state and cumulative transition counters again. Ready-to-completion deltas use saturating subtraction and must fit `u32`; then the observation layer atomically persists the run before posting completion.
8. Any validation, rendering, lifecycle, capacity, acknowledgement, or persistence error emits the separate non-measurable failure schema and enters `Finished`.

## Preconditions and postconditions

- The host canvas is exactly `440 x 956 pt @3x`; the feed viewport is `390 x 844 pt` at `(25, 56)`.
- The deterministic prefix must end at `237460 pt`; top starts at zero and bottom at `236616 pt`.
- Both authoritative host checks must reproduce the frozen canonical byte count and SHA-256 before either device app is built; matching only prefix geometry is insufficient.
- The two embedded fonts must retain their frozen byte counts. Checker images use exact RGBA8 source bytes and nearest sampling.
- UIKit supplies raw OS events only. Gesture state, offsets, inertia, draw commands, and settlement remain Oxide-owned.
- The pilot retains six diagnostic smoke records and exactly one 54-run primary block; publication timing rows come only from the 54 primary records.
- A ready or complete transition requires acknowledgement of the exact prepared frame ID that established the corresponding visual state.
- Complete records contain at least two callback samples, `gesture.inertia_observed=true`, and required zero-or-greater `thermal_state_change_count` and `low_power_mode_change_count` deltas.
- `FeedV1Status` is observational: reading it has no postcondition beyond returning current app state.

### Unsafe contracts

`rust_entry` forwards `argc/argv` without dereferencing them; their validity remains the native host's C-entry contract. iOS-only environment queries live in `observation.rs`. The app itself contains no pointer arithmetic or unsafe renderer access.

## Edge cases and failure modes

- A failed Swift or Rust canonical preflight stops the runner before device build or launch. Missing/foreign runtime environment values, an unsafe nonce, or negative/non-numeric indices fail at launch.
- A touch outside the surface is ignored; a tap that never crosses drag slop returns to `Ready` and produces no measurement.
- Touch cancellation, no real inertial phase, failure to settle within six seconds, fewer than two callbacks, or bounded-array exhaustion marks the gesture non-measurable.
- A viewport, content extent, visible component, font, or sampled-image mismatch fails closed instead of producing a faster but visually different result.
- Backgrounding or termination before protocol completion records the phase-correct failure stage.
- A transition delta larger than `u32::MAX` is rejected; cumulative counter regression yields zero through the specified saturating subtraction.
- A record is never announced before its file and containing directory are synchronized.

## Concurrency and memory behavior

`FeedV1App` is owned and driven on the host app thread; it does not create threads or locks. Prepared draw-list storage, damage, callback samples, visible-component arrays, strings, checker bytes/handles, collection caches, and text state are retained by the app. `RuntimeTextUploader` preserves the renderer's complete A8 atlas lifecycle by forwarding create, update, append, and release operations to the host uploader. Borrowed `PreparedFrame` data cannot outlive the app. Runtime construction retains prefix geometry but performs no full-feed canonical pass. The host transition counters are atomics, but this crate only takes coherent cumulative snapshots through the C ABI.

## Performance notes

- After natural warmup, the steady frame path is designed to avoid heap allocation and capacity growth.
- Frame-scoped text publication creates the cold atlas once, coalesces same-frame glyph writes, and does not republish unchanged warm glyph pixels.
- Static content extent admission runs only on the cold mount; it is not recomputed or checked on every measured frame.
- `CollectionView` visits visible rows only. Each row retains a cheap out-of-range guard, but no measured-frame fixture-vs-recipe height recheck remains.
- Checker source scratch is fixed at 576 bytes; upload handles are cached by 64 deterministic variants. Observation uses fixed arrays of 1,024 callbacks and 96 visible components.
- The app samples environment counters only at ready and completion, outside per-frame draw encoding.
- Full canonical hashing runs only in the runner's pre-build host check, never in app construction or a measured process.

## Feature flags and cfgs

The app defines no crate features. The library compiles on the native host for deterministic integration tests. The root-owned `feed-v1-device` profile preserves the pilot's frozen release codegen, including `opt-level = 3` for `oxide-host-ios`, instead of inheriting the product host's size-oriented package override. `observation` uses iOS FFI for live device/display-link/transition state and Darwin notifications; non-iOS tests receive a deterministic nominal 120 Hz state, zero transition counters, and no-op notifications.

## Testing and benchmarks

- [`tests/lib_tests.md`](tests/lib_tests.md) drives the public `App` and diagnostic APIs through mount, raw touch, rendering, retry, completion, and failure paths.
- [`tests/contract_tests.md`](tests/contract_tests.md) exercises the production canonical streamer used by the runner's Rust host preflight and freezes the deterministic recipe.
- [`tests/observation_tests.md`](tests/observation_tests.md) validates exact success/failure JSON, geometry translation, transition fields, and nonce rejection.
- The physical-device harness remains the authoritative proof for UIKit/Oxide visual comparison and native-refresh callback pacing. Host-side tests do not claim device performance.

Build the arm64 device archive with an explicit device-only `staticlib` output and an external target directory:

```sh
cd oxide
IPHONEOS_DEPLOYMENT_TARGET=18.0 \
CARGO_TARGET_DIR=/tmp/oxide-feed-v1-device-build \
cargo rustc --locked --offline --profile feed-v1-device \
  --target aarch64-apple-ios -p oxide-feed-v1-app --lib --crate-type staticlib
```

Run the host-side contract and state-machine tests with:

```sh
cd oxide
cargo test --locked --offline -p oxide-feed-v1-app
```

## Examples

```rust
let app = oxide_feed_v1_app::FeedV1App::from_environment();
let status = app.status();
assert_eq!(status.callback_sample_count, 0);
```

## Changelog

- 2026-08-07: Documented the 12-image publication smoke gate and attachment-free primary population.
- 2026-08-07: Moved complete canonical derivation from every app startup into paired authoritative pre-build host checks so offscreen row content remains runtime-cold.
- 2026-08-07: Joined the non-default root workspace graph and replaced the nested lock/profile boundary with root-owned resolution and target reuse.
- 2026-08-07: Made ordinary host builds rlib-only and moved static-archive emission to the explicit arm64 device build.
- 2026-08-07: Scoped text collection/publication to each rendered frame and preserved append/release operations through the runtime A8 uploader adapter.
- 2026-08-06: Added complete allocation-bounded canonical identity admission at startup.
- 2026-08-06: Consolidated standalone build and verification commands into the required mapped crate documentation.
- 2026-08-06: Required direct observation of Rust-owned inertia and ready-to-completion thermal/low-power transition deltas.
- 2026-08-06: Moved tests to public-API integration suites, added read-only status diagnostics, removed obsolete overflow state, and limited static extent admission to cold mount.
- 2026-08-06: Added the production-path frozen feed-v1 Oxide treatment.
