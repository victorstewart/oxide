# ui-core::layout_async

## Intention and purpose

`layout_async` coordinates layout jobs that may be expensive enough to run away from the caller. It exists so retained surfaces and stress scenes can request layout work, poll for the newest result, and drop stale intermediate results.

## Relation to the rest of the code

The module is re-exported by `oxide_ui_core` and used by `SurfaceRouter` plus the test scenes. Native targets run jobs on a background worker thread. Browser WebAssembly targets execute the job immediately and store it as the pending result because `wasm32-unknown-unknown` does not provide native threads in this host configuration.

Call flow:

- caller creates `AsyncLayoutCoordinator`
- caller invokes `request`
- native target sends work to the worker thread
- wasm target computes synchronously
- caller invokes `poll_latest`
- caller applies the newest result

## Entry points list

- `oxide_ui_core::layout_async::AsyncLayoutCoordinator<T>::new() -> Self`: creates a coordinator with a native worker or wasm synchronous fallback.
- `oxide_ui_core::layout_async::AsyncLayoutCoordinator<T>::request<F>(&mut self, job: F) -> u64`: queues or computes a layout job and returns its sequence id.
- `oxide_ui_core::layout_async::AsyncLayoutCoordinator<T>::poll_latest(&mut self) -> Option<(u64, T)>`: returns the newest unapplied result.
- `oxide_ui_core::layout_async::AsyncLayoutCoordinator<T>::has_inflight(&self) -> bool`: reports whether a requested result has not been applied.
- `impl Drop for AsyncLayoutCoordinator<T>`: shuts down the native worker or clears the wasm pending result.

## Logic narrative

Native builds allocate two channels and spawn an `oxide-layout-worker` thread. Requests send `Command::Compute` to that worker, and polling drains all available results while keeping only the highest sequence id. WebAssembly builds cannot use this native worker path, so `request` computes the closure synchronously and stores one pending result. A later request replaces the pending result, preserving the same coalescing semantics.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

Jobs and results must be `Send + 'static` because the native path crosses a worker-thread boundary. Sequence ids monotonically increase with saturation. `last_applied` never moves backward. The module uses no unsafe code.

## Edge cases and failure modes

If the native worker receiver is gone, send failures are ignored and polling simply yields no result. Dropping the coordinator sends shutdown and joins the worker. On wasm, shutdown just clears the pending result. Multiple queued native results coalesce to the newest visible result.

## Concurrency and memory behavior

Native targets allocate a worker thread and channel messages. Wasm targets allocate only the computed result storage. No test code is intermingled with production logic.

## Performance notes

Native request cost is one boxed closure plus a channel send. Wasm request cost is the layout computation itself, which can block the browser frame; this is a deliberate fallback for correctness until a browser scheduler or worker-backed wasm path is introduced.

## Feature flags and cfgs

`target_arch = "wasm32"` selects synchronous coalescing. Other targets use `std::thread` and `std::sync::mpsc`.

## Testing and benchmarks

`oxide/crates/ui-core/tests/layout_async.rs` covers request, polling, and coalescing behavior on native targets. The wasm path is compile-checked through `oxide-host-web` and exercised by the browser boot test.

## Examples

```rust
pub fn compute_once()
{
   let mut coordinator = oxide_ui_core::layout_async::AsyncLayoutCoordinator::new();
   let seq = coordinator.request(|| vec![1, 2, 3]);
   assert_eq!(coordinator.poll_latest().map(|item| item.0), Some(seq));
}
```

## Changelog

- 2026-05-18: Compacted native async-layout coalescing tests while keeping channel-gated worker ordering.
- Added a wasm32 synchronous fallback so the WebAssembly host can construct scenes without native thread support.
