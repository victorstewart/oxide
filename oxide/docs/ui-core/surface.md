# ui-core `surface.rs`

## Intention and purpose

`surface.rs` owns `UiSurface`, retained snapshot composition, dirty-class routing, animation override application, and surface/router integration. C23 makes the surface the public configuration boundary for retained CPU and future prepared-GPU byte budgets.

## Relation to the rest of the code

- App or router -> `UiSurface` mutation/layout -> `NodeTree` retained sequence.
- `UiSurface::render_snapshot_retained` -> `oxide_renderer_api::RenderSnapshot`.
- Backend consumers traverse immutable chunk instances; compatibility encoding explicitly flattens snapshots into a `DrawListBuilder`.

## Entry points list

- `UiSurface::retained_cache_policy(&self) -> RetainedCachePolicy`: returns the active tree-owned policy.
- `UiSurface::set_retained_cache_policy(&mut self, RetainedCachePolicy)`: applies a new policy, enforces reductions immediately, and invalidates the surface-level snapshot cache.
- `UiSurface::render_snapshot_retained(...) -> Result<SurfaceRenderSnapshot, SurfaceRenderSnapshotError>`: returns an immutable mixed UI/content snapshot plus cache diagnostics.
- `SurfaceRenderChunkStats`: keeps the per-snapshot reuse, copy, and retained-byte summary compact. `UiSurface::retained_node_stats` exposes the complete admission, eviction, prepared-byte, build-time, fallback, completeness, and invalidation telemetry on demand so hot snapshot returns do not copy cold diagnostic fields.

## Logic narrative

The surface asks `NodeTree` for the current immutable UI sequence, combines it with caller-owned content sequences, sorted dynamic properties, and damage, then reuses the complete snapshot only when all identities and metadata match. If the node cache is incomplete because of eviction, suppression, or a zero-byte policy, the surface does not retain a whole-snapshot reference that could keep rejected descendants alive indirectly.

## Preconditions and postconditions

- Chunk namespaces must fit the tree's 32-bit namespace encoding.
- Returned snapshots preserve caller content order and identity.
- A completed render reports retained logical bytes at or below the configured CPU budget.

## Edge cases and failure modes

- A zero CPU budget produces a direct immutable UI chunk and never caches the mixed snapshot.
- Chunk or snapshot validation errors propagate through `SurfaceRenderSnapshotError`; compatibility encoding falls back to direct surface encoding.
- Animation changes use the `Animation` invalidation reason and cannot reuse stale node paint.

## Concurrency and memory behavior

`UiSurface` is mutated by its owning UI thread. Immutable chunks and snapshots use shared ownership for backend handoff. Cache LRU links remain inside `NodeTree`; the surface adds no per-entry allocation. Caller-owned content is outside the node-cache budget and is never evicted by `UiSurface`.

## Performance notes

Complete hot snapshots clone shared handles and compare sequence identities. Incomplete snapshots rebuild composition without retaining indirect references. Zero-budget one-use churn avoids thousands of per-node/path allocations while keeping external text/image chunks reusable.

## Feature flags and cfgs

The retained cache policy is backend-independent and has no target-specific feature gate. Prepared-GPU bytes remain zero until a backend installs prepared chunk ownership in C24 or later.

## Testing and benchmarks

- `crates/ui-core/tests/surface.rs` covers exact output, budget enforcement, LRU protection, churn suppression/readmission, and external identity.
- `cpu.architecture.retained.cache_pressure.hot_reuse` and `.one_use_churn` cover hot and direct pressure policies.
- `cpu.authoring.surface_retained.cache_policy` covers the public configuration API.

## Examples

```rust
let policy = RetainedCachePolicy {
   cpu_budget_bytes: 1024 * 1024,
   prepared_gpu_budget_bytes: 2 * 1024 * 1024,
   ..RetainedCachePolicy::default()
};
surface.set_retained_cache_policy(policy);
```

## Changelog

- 2026-07-13: Added public retained cache policy configuration and complete C23 cache statistics.
