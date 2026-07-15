# oxide-image-store::tests::image_store_tests

## Intention and purpose

These integration tests freeze C60 image identity, decode, residency, paging, cancellation, budget, and recovery behavior through a deterministic in-memory backend.

## Relation to the rest of the code

The mock backend implements the same `ImageResidencyBackend` contract as Metal and WebGPU. It records exact pixels, mip requests, releases, device generation, and invalidated chunk identities without adding test behavior to production code.

## Entry points list

- `invalid_store_configuration_returns_typed_errors`
- `decode_is_display_sized_and_variant_hits_keep_identity`
- `completed_variants_do_not_retain_the_encoded_source_allocation`
- `canceled_and_stale_completions_never_publish`
- `malformed_decode_completion_never_reaches_gpu_publication`
- `atlas_gutters_repeat_only_the_owning_image_edges`
- `atlas_reuse_invalidates_only_referencing_chunks_and_checks_generations`
- `unsuitable_images_stay_standalone_and_minified_images_request_mips`
- `memory_pressure_and_device_loss_purge_exact_residency`
- `memory_warning_cancels_queued_decode_and_allows_explicit_restart`
- `invalidation_deduplicates_a_chunk_shared_by_multiple_images`
- `release_and_reuse_changes_the_logical_generation`
- `scrolling_release_and_reuse_invalidates_only_visible_chunk_owners`
- `native_pool_decodes_off_the_requesting_thread`
- `ten_thousand_requests_remain_within_hard_cpu_and_gpu_budgets`

## Logic narrative

Tests generate deterministic PNGs, request variants, run either inline or worker decode, upload through the mock backend, and assert both public status and exact backing storage. Churn cases release and replace entries while checking logical and atlas-slot generations. Pressure cases compare every live byte/counter with configured limits before and after purge or simulated device replacement.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

All fixtures use valid RGBA PNGs unless a failure is the subject under test. Successful tests leave no stale identity capable of resolving, no cross-slot gutter pixel, and no current residency counter inconsistent with the mock textures. No unsafe code is used.

## Edge cases and failure modes

Coverage includes cancellation before completion and before upload, stale completion, repeated request hits, unsuitable image policies, empty-page release, queued-work cancellation and explicit restart after memory warning, device generation loss, logical slot reuse, visible-owner-only invalidation, and populations much larger than the configured working set.

## Concurrency and memory behavior

The native pool test proves completion arrives through the worker channel while store mutation remains on the test thread. Weak `Arc` coverage proves encoded bytes are dropped after completion. The 10,000-request test exercises exact same-class slot-LRU reuse, retains the newest full-budget working set, and requires texture creation to stop at the budget-sized page count.

## Performance notes

These are invariant tests, not timing benchmarks. They freeze the work and memory counters needed to interpret the C60 A/B reports and guard against hidden full-page uploads or broad invalidation.

## Feature flags and cfgs

The native worker test runs only on non-wasm targets with the crate's native decode path.

## Testing and benchmarks

Run `cargo test --locked -p oxide-image-store` from the Rust workspace root. Device and browser performance evidence is acquired through `oxide-perf-runner`, `xtask`, and `oxide-host-web` rather than this mock.

## Examples

The `populate` helper demonstrates the test lifecycle: request, decode, upload, then resolve.

## Changelog

- 2026-07-15: added typed invalid-configuration coverage for the non-panicking public constructor.
- 2026-07-15: added the complete C60 logical identity, decode, atlas, fallback, invalidation, pressure, device-loss, worker, and 10,000-request contract.
