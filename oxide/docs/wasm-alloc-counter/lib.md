# oxide-wasm-alloc-counter::lib

## Intention and purpose

`oxide-wasm-alloc-counter` provides a small global-allocator wrapper for Oxide browser benchmark builds. It exists to count Rust/WASM allocation, deallocation, and reallocation activity inside measured WebGPU frame loops without adding browser JavaScript heap attribution to renderer hot-path counters.

## Relation to the rest of the code

`oxide-host-web` installs `CountingAllocator<std::alloc::System>` only for `target_arch = "wasm32"`. Browser benchmark exports snapshot the counters before and after each measured frame, then the report script persists the deltas in `benchmarks/web/latest.json` and `benchmarks/web/latest.md`.

Call flow:

- wasm global allocator
- `oxide_wasm_alloc_counter::CountingAllocator`
- `oxide_host_web` benchmark frame loop
- `scripts/check_webgpu_browser_golden.mjs`
- `benchmarks/web/latest.*`

## Entry points list

- `oxide_wasm_alloc_counter::CountingAllocator::new(inner) -> CountingAllocator<A>`: wraps an existing `GlobalAlloc` implementation.
- `oxide_wasm_alloc_counter::snapshot() -> AllocationSnapshot`: returns point-in-time aggregate counters.
- `oxide_wasm_alloc_counter::AllocationSnapshot`: stores allocation, byte, reallocation, live-byte, and peak-live-byte counters.

## Logic narrative

The wrapper forwards every allocator operation to the inner allocator. Successful `alloc` and `alloc_zeroed` calls increment allocation counters and live bytes. `dealloc` increments deallocation counters and subtracts live bytes with saturation. Successful `realloc` calls increment reallocation counters and record only the grow or shrink byte delta, which keeps realloc activity separate from fresh allocation activity in the WebGPU report.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The wrapper preserves the `GlobalAlloc` contract of the wrapped allocator: pointers, layouts, alignment, ownership, and lifetimes are forwarded unchanged. Counters use relaxed atomics because they are benchmark diagnostics and do not synchronize memory ownership.

## Edge cases and failure modes

Failed allocation and reallocation calls return null and are not counted as successful allocation activity. Live-byte subtraction saturates to avoid underflow if an allocator or caller reports an inconsistent size.

## Concurrency and memory behavior

Counters are process-global atomics. `snapshot()` performs only atomic loads and does not allocate.

## Performance notes

The crate is intentionally tiny and dependency-free. It is used for benchmark diagnostics, not as a production allocator policy. Browser report gates currently require bounded current-row per-frame allocation counts/bytes and zero current-row reallocations after warmup.

## Feature flags and cfgs

There are no feature flags. The host-web dependency is target-gated to wasm32.

## Testing and benchmarks

`oxide/crates/wasm-alloc-counter/tests/allocation_counter_tests.rs` directly calls the allocator wrapper with `System` to verify allocation, deallocation, grow-reallocation, and shrink-reallocation deltas.

## Examples

```rust
#[global_allocator]
static ALLOCATOR: oxide_wasm_alloc_counter::CountingAllocator<std::alloc::System> =
   oxide_wasm_alloc_counter::CountingAllocator::new(std::alloc::System);
```

## Changelog

- 2026-06-02: added the allocator counter crate for browser WebGPU Rust/WASM frame allocation audits.
