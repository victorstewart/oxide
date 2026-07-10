# renderer-web::wasm::image_slots

## Intention and purpose

`ImageSlots` owns the renderer's bounded generation-checked image table. It reuses constant-time
vector slots after explicit release without allowing a stale `ImageHandle` to resolve to a later
GPU image and without retaining one tombstone for every historical image.

## Relation to the rest of the code

The WebGPU renderer checks capacity before GPU creation, inserts each `GpuImage` into this table,
and stores the returned packed `u32` in backend draw packets. Update, draw-state, bind, and release
paths resolve the same table. The flow is:

- WebGPU image creation
- `ImageSlots::has_capacity`
- GPU texture and bind-group creation
- `ImageSlots::insert`
- update/draw/bind through `ImageSlots::get`
- explicit release through `ImageSlots::remove`
- generation advance and free-slot reuse, or retirement at generation exhaustion

## Entry points list

The type is crate-private and introduces no author-facing API.

- `ImageSlots::new() -> Self`: creates an empty table.
- `ImageSlots::has_capacity(&self) -> bool`: reports whether insert can use a free or new slot.
- `ImageSlots::insert(&mut self, value: T) -> Result<u32, T>`: stores one value and returns its
  packed nonzero handle, returning ownership unchanged at the hard live-slot bound.
- `ImageSlots::get(&self, handle: u32) -> Option<&T>`: resolves only an exact live generation.
- `ImageSlots::remove(&mut self, handle: u32) -> Option<T>`: removes one exact live value,
  invalidates its handle, and recycles or retires the slot.
- `ImageSlots::storage_capacity_bytes(&self) -> usize`: reports allocated vector payload capacity.

## Logic narrative

The low 16 handle bits encode `slot index + 1`; zero therefore remains invalid while 65,535 live
slots remain representable. The high 16 bits encode a nonzero generation. Removal takes the live
value exactly once, increments the generation, and places the index on the free list. A later
insert reuses that index but returns a distinct handle. When generation 65,535 is removed,
`checked_add` fails and the empty slot is retired instead of wrapping to a stale generation.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

- Handle zero and handles with either packed half equal to zero never resolve.
- At most 65,535 values are live at once.
- Every successful insert returns a unique live handle.
- Remove returns ownership exactly once and invalidates the removed handle before reuse.
- A free index appears at most once because only a generation-matched successful `take` pushes it.
- Generation exhaustion retires a slot; generation values never wrap.
- The module contains no unsafe code.

## Edge cases and failure modes

Malformed, stale, already removed, future-generation, and out-of-range handles return `None`.
Insert at the hard bound returns the original value without mutation. A retired slot remains empty
and is never selected again.

## Concurrency and memory behavior

The owning renderer mutates the table through `&mut self` on the browser render thread, so no lock
or atomic operation is needed. Lookup is one mask, one shift, a vector bounds check, a generation
comparison, and an `Option` check. The two vectors retain capacity at the warm peak to avoid
allocator churn. Retired slots add at most one small metadata entry per 65,535 reuse generations;
the 16-bit slot field hard-bounds the table.

## Performance notes

Create, lookup, and release are constant time. Draw-time lookup remains cache-local vector access
and avoids the measured hash-table regression. Packed handles and draw packets remain `u32`.
`storage_capacity_bytes` counts the exact vector payload capacities under the renderer's existing
CPU scratch metric convention.

## Feature flags and cfgs

The production consumer is the `wasm32` WebGPU backend. The same source is included directly by a
native external test so the lifecycle algorithm executes without requiring a browser GPU.

## Testing and benchmarks

`oxide/crates/renderer-web/tests/image_slot_tests.rs` executes malformed/stale handles, reuse,
double release, all 65,535 generations, the 65,535-live hard bound, and repeated churn capacity.
Browser A/B runs cover the existing image-upload and 97-image mixed draw workloads.

## Examples

```rust
let mut slots = ImageSlots::new();
let handle = slots.insert(bytes).map_err(|_| "image table full")?;
assert!(slots.get(handle).is_some());
let bytes = slots.remove(handle).ok_or("image missing")?;
```

## Changelog

- 2026-07-10: introduced bounded generation-checked slot reuse for WebGPU images.
