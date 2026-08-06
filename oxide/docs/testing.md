# Testing Matrix

This document captures the current state of automated testing across the Oxide workspace. It lists every crate that ships in the workspace, the primary modules or responsibilities they own, what automated coverage exists today, and obvious gaps that subsequent phases must close.

## Command Surface

- `cargo test --locked` – tests the production library crates selected by the
  workspace `default-members` list.
- `cargo test --workspace --all-targets --all-features --locked` – runs the
  full local gate, including hosts, test scenes, snapshot/performance tools,
  and allocation instrumentation.
- `cargo xtask ios prepare` – prepares the iOS host project (capabilities, entitlements, shaders).
- `scripts/ios-test.sh` – convenience wrapper that prepares the project, builds iOS staticlibs, and runs the Xcode UI test target.
- `cargo test -p oxide-ui-core` – runs CPU layout/collection simulation property tests.
- `cargo test -p oxide-renderer-metal --features snapshot-tests` – runs GPU readback snapshot tests on macOS.
- `cargo test -p oxide-platform-ios` – exercises the camera capability heuristics.

## Workspace Inventory

| Crate | Path | Type | Primary modules / responsibilities | Existing automation | Gaps & risks |
| --- | --- | --- | --- | --- | --- |
| `oxide-platform-api` | `crates/platform-api` | lib | app/renderer contracts, update contexts, device caps, animation descriptors, pointer & keyboard events, haptics | Unit tests for modifiers, input events, animation descriptors | extend to cover update-context behaviours and serialization for FFI bindings |
| `oxide-renderer-api` | `crates/renderer-api` | lib | draw list types, geometry structs, color math, renderer trait, resource handles | Draw list invariant tests (layer/clip), damage/vertex storage checks | add validation for glyph spans vs vertex storage and error type conversions |
| `oxide-image-store` | `crates/image-store` | lib | decoded-image ownership, residency decisions, atlas publication, and renderer-resource eviction | Deterministic residency, atlas, invalidation, and eviction tests | add device-backed pressure and eviction journeys |
| `oxide-utils` | `crates/utils` | lib | canvas metrics, pixel snapping helpers, stroke width math | Inline unit tests + property tests (canvas/snap) | still need stress tests for complex transforms and device-scale fuzzing |
| `oxide-timing` | `crates/timing` | lib | monotonic clock, timer wheel, global animation manager (`anim` module) | Unit tests for eases, timers, and animation lifecycle/value behavior | add fake-clock harness for long-running animation scenarios and multi-threaded timer stress |
| `oxide-input` | `crates/input` | lib | gesture recognizer (tap/long/drag), gesture config handling | Deterministic gesture unit tests (tap/long/pan/cancel/velocity) | extend to cover multi-touch interactions and scroll accumulator edge cases |
| `oxide-text` | `crates/text` | lib | font DB, glyph atlas packing, shaping (rustybuzz + swash), glyph quad emission | Atlas packing + font DB unit tests | add golden shaping fixtures and fallback coverage when real font assets available |
| `oxide-ui-core` | `crates/ui-core` | lib | draw list builder, animation helpers, collection view, scenes router, widgets | Property tests (`anim_prop`, `collection_prop`, `layout_prop`) + deterministic layout/hit-test & router scene sims | extend to cover widget-specific behaviours and scene router state transitions |
| `oxide-renderer-metal` | `crates/renderer-metal` | lib | Metal command encoding, resource rings, shader PSOs, damage tracking | macOS-only snapshot test (`tests/snapshots.rs`) + CPU-only ring/renderer sanity tests | expand snapshot suite, add headless validation and simulator coverage |
| `oxide-platform-ios` | `crates/platform-ios` | lib | UIKit/AVFoundation shims, camera capability selection, IME bridges | Unit tests around camera capability heuristics (`tests/lib_tests.rs`) | needs lifecycle/IME tests, bridging safety checks, tokio integration gates |
| `oxide-host-ios` | `host/ios-app/oxide-host-ios` | staticlib | UIApplication bridge, renderer/bootstrap glue, callback registries, push/IME hooks | Unit tests for callback bridges plus the actual-app camera frame-driven scheduling hook | add integration tests via simulator harness, renderer lifecycle coverage |
| `oxide/host/ios-app/App` | `oxide/host/ios-app/App` | Xcode proj | Obj-C app shell plus one window-level launch smoke | `OxideHostUITests` window existence and one startup screenshot | scene and gesture behavior belongs in focused OS-level journeys, not one broad traversal battery |
| `oxide-host-macos` | `host/macos-app/oxide-host-macos` | staticlib | CAMetalLayer host, event routing, resource loading, keyboard/mouse bridge | None | add scripted host harness tests, renderer state assertions |
| `host/macos-app/app-runner` | `host/macos-app/app-runner` | bin | launches macOS host staticlib for local smoke | None | add smoke/integration test that validates exit codes, logging |
| `oxide-perf-runner` | `crates/perf-runner` | bin | automated perf sweeps over scenes with configurable thresholds | No automated tests yet | needs CLI arg tests, deterministic stats fixtures |
| `oxide-snapshot-runner` | `crates/snapshot-runner` | bin | offscreen renderer harness, PNG export, golden diffing | No automated tests yet | add CLI smoke tests, golden diff verification, fixture management |
| `oxide-harness-registry` | `crates/harness-registry` | lib | compile-time registry of components and animations | No automated tests yet | add compile-time completeness tests, ensure IDs stay synchronized with scenes |
| `xtask` | `xtask` | bin | iOS project preparation (capabilities, shader bundling) | Unit tests around entitlements merge | extend with workspace test runner, add tests for argument parsing |
| `host/macos-app/Resources` | `host/macos-app/Resources` | assets | fonts/images consumed in tests | N/A | ensure fixture availability documented |
| `tools/*` | `tools/anim_agg`, `tools/snap_agg`, `tools/sweep_agg` | bins | aggregation utilities for perf/snapshot data | Not exercised by the default local matrix | add unit/CLI tests once stabilized |

## Current Coverage Notes

- Property-based tests in `crates/ui-core/tests` run with fixed RNG seeds and persist prior failure cases under `*.proptest-regressions`.
- GPU snapshot coverage is limited to a single rounded-rect draw; there is no end-to-end coverage for full scenes, HDR/MSAA variants, or damage-based rendering.
- The default host UI automation is one bounded XCUITest (`testWindowLaunchSmoke`) that verifies a window launches and retains one startup screenshot. Scene and gesture behavior belongs in focused OS-level journeys rather than a broad repeated traversal battery.
- No automated smoke tests exist for the macOS host runner or the CLI harnesses (`perf-runner`, `snapshot-runner`).

These gaps inform the follow-on phases outlined in the broader test plan.


## Continuous Integration

GitHub Actions CI is intentionally disabled. Verification is run locally before review
and merge using the command surface above and the focused gates required by `AGENTS.md`.
This repository intentionally does not run `cargo fmt` or `cargo clippy`; the project
style is maintained manually.

Device-authoritative Oxide/UIKit performance remains a reviewed physical-iPhone flow
through the `Justfile` device targets and the checked-in `benchmarks/oxide-device` and
`benchmarks/uikit-device` reports.
