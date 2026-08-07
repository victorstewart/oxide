# oxide-feed-v1-app `tests/lib_tests.rs`

## Intention and purpose

This integration suite protects the feed app as an external caller sees it. It creates the app only through controller environment inputs, drives only the public `oxide_platform_api::App` contract, and observes only `FeedV1App::status`, prepared frames, uploader calls, and terminal files. That boundary prevents tests from legitimizing private state mutation or a hidden host control surface.

## Relation to the rest of the code

- Exercises `src/lib.rs` mount, gesture, render, submit-acknowledgement, completion, and failure paths.
- Uses public fixture constants/functions from [`contract.rs`](../contract.md) to compare externally visible geometry, colors, offsets, and upload bytes.
- Reads terminal JSON written by [`observation.rs`](../observation.md), while focused schema coverage remains in [`observation_tests.md`](observation_tests.md).
- Uses lightweight fake haptics and image uploaders only at the public platform/renderer interfaces.
- Includes the app manifest and XcodeGen source at compile time to freeze the ordinary-build/device-archive boundary.

Call graph:

- test environment -> `FeedV1App::from_environment`
- helper frame/touch/stats -> `App::prepare_frame` / `App::event`
- assertions -> `FeedV1App::status`, `App::prepared_frame`, upload probe, persisted JSON

## Entry points list

- `ordinary_builds_are_rlib_only_and_the_device_build_explicitly_requests_staticlib` prevents normal host builds and tests from aggregating the complete dependency graph into an unused static archive while requiring the arm64 device phase to request that archive explicitly.
- `frozen_app_starts_at_each_exact_contract_offset` covers both controller endpoints.
- `measurement_begins_at_the_first_drag_offset_change` proves touch-down and sub-slop motion remain unmeasured.
- `a_touch_that_never_drags_returns_to_ready` proves a tap does not create a benchmark sample.
- `checker_uploads_use_only_frozen_nearest_sampled_source_bytes` inspects every visible RGBA upload.
- `text_atlas_publication_is_coalesced_once_per_frame` requires one cold A8 atlas creation, no separate cold glyph update, and no unchanged warm-frame republication.
- `runtime_text_adapter_preserves_append_and_release_operations` protects forwarding of the A8 append and release lifecycle through the runtime uploader adapter.
- `rendered_frame_colors_round_trip_the_frozen_srgb_bytes` inspects renderer commands after sRGB-to-linear conversion.
- `ready_frame_exposes_frozen_damage_clip_and_image_geometry` checks full-canvas damage, surface clip, and the first rounded checker bounds.
- `failed_submissions_retry_the_immutable_frame_under_the_latest_frame_id` freezes exact retry ownership and acknowledgement identity.
- `settlement_waits_for_a_closing_frame_and_retries_its_exact_submit` freezes the extra closing callback and completion acknowledgement boundary.
- `completed_inertial_run_requires_zero_environment_transition_deltas` drives real Rust-owned inertia to settlement, then proves `inertia_observed=true` and both required transition fields are present with zero deltas.
- `noninertial_drag_cannot_emit_a_complete_record` proves settled travel without an inertial phase produces a gesture failure rather than timing evidence.
- `render_failures_are_labeled_by_the_phase_that_observed_them` distinguishes initial-admission and gesture failures.
- `lifecycle_exit_is_stage_correct_while_waiting_for_submit_or_drag` distinguishes pre-ready and active-interaction termination.

## Logic narrative

The build-contract source guard first requires an rlib-only crate manifest and an explicit `cargo rustc --crate-type staticlib` Xcode device command. Runtime tests acquire one process-wide environment lock, install valid controller variables, and restore the previous values through RAII. Tests construct `FeedV1App` through `from_environment`, mount it with two public frame calls, and acknowledge the exact second frame to reach `Ready`. Gesture cases send raw start/move/end touch events through `App::event`; frame helpers then advance settlement and submit acknowledgement exactly as the production host does.

`UploadProbe` records caller bytes and sampling without rewriting them while assigning stable fake handles. Its A8 counters prove that frame-scoped text publication folds cold glyphs into one atlas creation and leaves an unchanged warm frame upload-free. A focused adapter regression inspects the bounded `RuntimeTextUploader` implementation and requires both append and release forwarding. Prepared-frame assertions read the borrowed public draw list. The inertial completion helper uses one bounded frame loop and stops at the first completion-submit boundary; the paired noninertial case proves distance cannot fake a fling. Terminal tests use an isolated temporary `HOME`, then parse the same durable file the device harness would consume. Tests do not access private app fields or mutate state outside public interfaces.

## Preconditions and postconditions

- Environment-mutating tests hold `ENVIRONMENT_LOCK` and restore all variables even after ordinary unwinding.
- Temporary homes are unique by process ID and atomic sequence and are removed on drop.
- Fake frame timing is monotonic and uses the frozen `440 x 956 @3x` viewport unless a test intentionally injects mismatch.
- A passing suite means the represented public state transitions, draw commands, uploads, retry semantics, failure stages, and zero-required fields match the contract.
- Passing native tests does not claim UIKit parity, device touch delivery, Metal output, or physical timing.

## Edge cases and failure modes

- Poisoned environment locks are recovered so one earlier panic does not cascade unrelated failures.
- Missing prepared frames, temporary directories, result files, or valid JSON fail with explicit test diagnostics.
- The short drag intentionally leaves no release velocity in the scroll sampling window; the inertial drag supplies multiple recent samples and asserts the app's direct observation before advancing.
- Lifecycle and viewport hostile cases intentionally generate terminal failure files rather than accepting degraded evidence.

## Concurrency and memory behavior

Rust's test runner may schedule cases concurrently, but all environment mutation is serialized by one mutex. Renderer/app objects remain test-local. Temporary-home naming uses `AtomicU64` with relaxed ordering because uniqueness, not cross-thread publication, is the invariant. Probes allocate vectors only in test code; production allocation claims are not inferred from probe behavior.

## Performance notes

Most assertions use one or two visible frames. The text-publication case compares the cold and unchanged warm frames directly without a repeated loop. The single inertia completion case has a 768-frame fail-closed ceiling but exits at actual settlement; it is a deterministic state-machine proof, not a repeated soak battery. Device A/B evidence remains in the pilot harness.

## Feature flags and cfgs

No feature flags or target cfg branches. Native execution intentionally selects observation's deterministic non-iOS state and zero transition counters.

## Testing and benchmarks

Run with:

```sh
CARGO_TARGET_DIR=/tmp/oxide-feed-v1-native-build \
cargo test --locked --offline \
  --manifest-path oxide/benchmarks/pilots/feed-v1/ios/oxide-feed-app/Cargo.toml \
  --test lib_tests
```

The physical-device smoke/full pilot is required for host FFI, raw OS gestures, submitted Metal pixels, and native-refresh evidence.

## Examples

Filter the retry contract while iterating:

```sh
cargo test --manifest-path oxide/benchmarks/pilots/feed-v1/ios/oxide-feed-app/Cargo.toml \
  --test lib_tests failed_submissions_retry_the_immutable_frame
```

## Changelog

- 2026-08-07: Added a source guard for rlib-only ordinary builds and explicit device-only static-archive emission.
- 2026-08-07: Added frame-coalesced text-atlas publication and complete runtime A8 adapter forwarding coverage.
- 2026-08-06: Added real-inertia success, noninertial rejection, and required zero environment-transition coverage.
- 2026-08-06: Replaced inline private-state tests with focused public-API integration coverage.
