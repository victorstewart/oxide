# platform-api tests `current_platform_tests.rs`

## Intention and purpose
- Verify the new current-platform registry contract and `SharedPlatform` adapter behavior.

## Relation to the rest of the code
- Exercises the public `set_current_platform`, `current_platform_if_registered`, `current_platform`, `request_redraw_if_registered`, and `SharedPlatform` APIs.

## Entry points list
- `current_platform_registry_tracks_shared_platform_instance`
  Regression test for registry install, redraw forwarding, and shared adapter delegation.

## Logic narrative
- The test installs a recording platform, verifies both accessor shapes, issues a redraw through the helper, and confirms the same shared instance is still observed through the boxed adapter path.

## Preconditions and postconditions
- Test setup clears the registry before install.
- Test teardown clears the registry after validation.

## Edge cases and failure modes
- Missing registry synchronization or a broken adapter would fail the redraw count or accessor assertions.

## Concurrency and memory behavior
- Uses the same `Arc` + `RwLock` path as production.

## Performance notes
- Smoke regression only.

## Feature flags and cfgs
- Standard test target.

## Testing and benchmarks
- Run with `cargo test --locked -p oxide-platform-api --test current_platform_tests`.

## Examples
```rust
// Regression test only.
```

## Changelog
- 2026-03-12: added coverage for the process-global current-platform registry and the `SharedPlatform` forwarding adapter.
