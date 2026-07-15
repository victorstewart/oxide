# oxide-host-macos `tests/display_scheduling_tests.rs`

## Intention and purpose

Verify the source-level ownership boundaries that let a clean macOS host suspend its display link without losing event, lifecycle, resize, memory, animation, or explicit redraw work.

## Relation to the rest of the code

- Inspects `oxide-host-macos/src/lib.rs`, which owns app-state dirtiness, wake generations, and successful-submission acknowledgment.
- Inspects `oxide-host-macos/src/macos/app.m`, which owns `CADisplayLink`/`CVDisplayLink`, the AppKit launch shell, and the benchmark-only process harness.
- Complements the deterministic headless scheduler test and the real-process C55 A/B runs.

## Entry points list

- `macos_display_links_suspend_without_polling_rust_while_idle()`
  Verifies modern and legacy pause paths, lock-free wake publication, persistent main-run-loop-source coalescing, immediate suspended-wake rendering, refresh-delayed settlement, missed-wake accounting, and the AppKit `CAMetalLayer` construction hook.
- `macos_display_scheduler_wakes_all_host_owned_dirty_sources()`
  Verifies the Rust and native sources that can make a static host renderable again.
- `macos_scheduler_benchmark_measures_real_callbacks_and_wake_latency()`
  Verifies the environment-gated process benchmark, retained delegate, nib-less launch path, and CPU/resident-memory instrumentation.

## Logic narrative

1. Each test loads the production Rust and Objective-C sources at compile time.
2. Contract markers prove the wake generation is published in Rust and consumed by the native display-link owner.
3. Lifecycle, input, memory, resize, router animation demand, and explicit redraw markers prevent a future refactor from adding an un-wakeable dirty path.
4. Launch and backing-layer markers protect the real `.app` path that the C55 benchmark uses, because headless rendering cannot prove AppKit constructed a window or `CAMetalLayer`.
5. The modern handler assertion prevents reintroducing a redundant Rust-state lock before a callback whose unpaused state already proves demand; the post-submit check remains the sole modern pause decision.
6. The wake-bridge assertion prevents synchronous main-thread Rust re-entry while a dirty publisher may still own the app-state mutex, preserves immediate rendering after the run-loop handoff, rejects a per-wake dispatch-block path, and keeps the second settlement frame at least one target refresh period behind the first drawable.

## Preconditions and postconditions

- The source paths remain relative to `CARGO_MANIFEST_DIR`.
- A passing suite proves required ownership and bridge calls remain present; runtime behavior is separately demonstrated by the headless and foreground-app tests.

## Edge cases and failure modes

- The tests fail if a display link can pause without a corresponding generation wake bridge.
- The tests fail if the custom Rust runner can lose its delegate, skip explicit host construction, or create AppKit's generic backing layer instead of `CAMetalLayer`.
- Source inspection cannot prove timing distributions, so no performance claim is based on these tests alone.

## Concurrency and memory behavior

The tests perform read-only compile-time string inspection and allocate only ordinary test strings. They verify that production wake coalescing uses atomics and a persistent main-run-loop source, but do not create native display threads themselves.

## Performance notes

The guarded production path performs no callback or Rust-state lock while suspended. Its first published wake is dispatched immediately after the publisher releases Rust state instead of waiting for the resumed display link's next phase; only the required settlement callback is delayed by one target refresh period. Benchmark code is compiled only with `host-testing`; within that build, counters are enabled only when `OXIDE_MACOS_SCHEDULER_BENCH` selects a supported mode.

## Feature flags and cfgs

The integration test compiles with the macOS host package. Runtime host assertions are exercised with the `host-testing` feature in the focused suite.

## Testing and benchmarks

Run:

```text
cargo test --locked -j12 -p oxide-host-macos --features host-testing --test display_scheduling_tests
```

The real-process benchmark builds `oxide-macos-app --features host-testing --release`, launches the packaged foreground app with `OXIDE_MACOS_SCHEDULER_BENCH=idle` or `wake`, and reports warmup callbacks/submissions, measured callbacks, Rust checks, dispatches, render calls, submissions, external native input, pauses, wake transitions, completed/target wake samples, missed wakes, CPU time, resident bytes, and event-to-submission latencies. `OXIDE_MACOS_SCHEDULER_WAKE_SAMPLES` selects 1–256 events while the fixed buffer preserves the 256-event stress mode; C55 paired proof uses 64 events across each of 15 launches for 960 raw events per side. A bounded foreground handshake activates and orders the signed window, then requires one second of continuous active state before capture; lifecycle activation owns display-link restart, so capture never unpauses an already-settled candidate. Failure after 50 attempts emits an invalid row. The wake workload routes every publication through the real Rust pointer callback. During the measured window a host-testing-only view quarantine counts but does not forward unrelated native input, so desktop activity cannot mutate Oxide or pass through the benchmark window; actual foreground loss still invalidates the row.

## Examples

```text
open -W -n --env OXIDE_MACOS_SCHEDULER_BENCH=idle OxideC55.app
```

## Changelog

- 2026-07-15: required persistent main-run-loop-source coalescing, immediate visible rendering, and refresh-delayed settlement for a suspended wake.
- 2026-07-15: added the one-Rust-check-per-modern-callback scheduling contract.
- 2026-07-15: added the C55 demand-driven display scheduler, AppKit boot/layer, and real-process benchmark contracts.
