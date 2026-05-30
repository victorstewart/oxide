# ui-core `tests/layout_async.rs`

## Intention and purpose

These integration tests verify the native `AsyncLayoutCoordinator` worker contract without timing sleeps. They exist to keep queued layout ordering deterministic while protecting the test process from blocked worker jobs during assertion failures.

## Relation to the rest of the code

The tests exercise `oxide_ui_core::layout_async::AsyncLayoutCoordinator` through its public `request` and `poll_latest` API. Channel gates hold worker jobs at known points so the test can prove how completed and inflight results interact.

Call flow:

- test creates an `AsyncLayoutCoordinator`
- test queues one or more channel-gated jobs
- worker reports that a gated job started
- test releases selected jobs
- test polls and checks the surfaced sequence/value pair

## Entry points list

- `async_coordinator_returns_latest()`: verifies a completed newer job wins over an older completed job while a later job is still inflight.
- `async_coordinator_applies_intermediate_when_no_newer_ready()`: verifies a completed result can be applied while a newer request remains inflight.
- `ReleaseOnDrop::new(sender: std::sync::mpsc::Sender<()>) -> Self`: arms a release signal for a blocked worker job.
- `ReleaseOnDrop::release(&mut self)`: sends the release signal early and disarms the drop path.
- `impl Drop for ReleaseOnDrop`: releases blocked worker jobs during unwinding so coordinator teardown can join the worker.

## Logic narrative

The tests avoid wall-clock sleeps by blocking worker closures on channels. The first test holds the first job, queues a fast second job, then queues a sentinel third job that remains inflight while the second result is polled. The second test queues one completed job and one sentinel job, then proves the completed intermediate result is still visible. `ReleaseOnDrop` owns every blocking release sender so any panic before the explicit release still unblocks the worker before the coordinator is dropped.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests assume native threading is available for the integration-test target. They use only safe Rust and public API calls. On success or panic, every blocked worker job receives a release signal before the coordinator joins the worker thread.

## Edge cases and failure modes

The release guard handles assertion failures, timeout failures, and early returns in the test body. If a worker job has not reached its receive call yet, the channel stores the release signal until it does.

## Concurrency and memory behavior

Each test creates one coordinator worker thread and several one-shot channels. The release guard is intentionally stack-owned and relies on normal reverse drop order: it is declared after the coordinator, so it drops before the coordinator joins the worker.

## Performance notes

The tests are not performance benchmarks. They avoid sleeps, so runtime depends on channel handoff rather than arbitrary delay.

## Feature flags and cfgs

The file has no explicit feature flags. It covers the native non-wasm path used by normal cargo test runs.

## Testing and benchmarks

Run with:

```bash
cargo test -j$(sysctl -n hw.ncpu) -p oxide-ui-core --test layout_async
```

## Examples

```rust
let mut coordinator = oxide_ui_core::layout_async::AsyncLayoutCoordinator::new();
let seq = coordinator.request(|| 1_u32);
assert_eq!(coordinator.poll_latest().map(|item| item.0), Some(seq));
```

## Changelog

- 2026-05-22: Added release guards so failed assertions cannot leave blocked worker jobs waiting during coordinator teardown.
