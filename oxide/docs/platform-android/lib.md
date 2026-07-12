# oxide-platform-android — `src/lib.rs`

## Intention and purpose

This crate makes missing production Android host support a compile-time failure instead of silently
selecting an unsupported HTTP client in a shipping build.

## Relation to the rest of the code

- Workspace builds on non-Android targets compile the marker and its contract test.
- Any Android target reaches `compile_error!` until a production asynchronous `HttpClient` adapter
  replaces the gate.

## Entry points

- `oxide_platform_android::AndroidProductionHttpRequired`: non-Android audit marker.

## Logic narrative

Target selection occurs at compilation. Android cannot produce an artifact while the host adapter
is absent, which prevents a runtime-only unsupported failure.

## Preconditions and postconditions

There are no runtime preconditions. A successful Android build postcondition is intentionally
impossible until a real adapter is implemented.

## Edge cases and failure modes

The Android failure message names the missing asynchronous HTTP host requirement directly.

## Concurrency and memory behavior

The marker has no runtime state, allocation, or concurrency.

## Performance notes

The gate generates no runtime code.

## Feature flags and cfgs

`target_os = "android"` selects the compile failure. Other targets expose only the marker.

## Testing and benchmarks

`tests/lib_tests.rs` proves the workspace includes the non-Android side of the target gate.

## Examples

No runtime use is intended.

## Changelog

- 2026-07-11: Added the explicit production Android HTTP selection gate.
