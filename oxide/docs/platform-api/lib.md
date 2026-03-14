# platform-api `lib.rs`

## Intention and purpose
- Define Oxide's platform contract and the shared process-global platform registry used by host and app code.
- Provide one source of truth for generic OS services such as redraw, permissions, sensors, haptics, IME, clipboard, telephony, and media-library access.

## Relation to the rest of the code
- Host crates install concrete platform implementations through `set_current_platform`.
- App crates such as Nametag resolve services through `current_platform()` and `request_redraw_if_registered()`.
- `SharedPlatform` keeps older boxed-`Platform` APIs working without reintroducing parallel registries.
- Module-specific adapters such as [`secure_storage`](./secure_storage.md) can also expose process-global callback registries when synchronous compatibility shims still need generic OS services before a full app runtime is available.
- Shared normalization/utilities for service adapters live with the corresponding module docs, for example [`telephony`](./telephony.md).

## Entry points list
- `Platform`
  Core trait implemented by each host runtime.
- `set_current_platform(platform)`
  Installs the process-global shared platform.
- `clear_current_platform_for_tests()`
  Clears the registry for deterministic test teardown.
- `current_platform_if_registered() -> Option<Arc<dyn Platform + Send + Sync>>`
  Returns the installed platform without aborting.
- `current_platform() -> Arc<dyn Platform + Send + Sync>`
  Returns the installed platform or aborts with a diagnostic.
- `request_redraw_if_registered() -> bool`
  Issues a redraw through the installed platform when available.
- `SharedPlatform`
  Adapter that forwards boxed `Platform` calls to a shared `Arc`.

## Logic narrative
- The new registry keeps one `Arc<dyn Platform + Send + Sync>` behind an `RwLock`.
- Hosts install a concrete implementation once per process.
- Callers that still need a boxed trait object wrap the shared instance in `SharedPlatform`, avoiding duplicate host-bridge graphs while preserving existing constructor signatures.

## Preconditions and postconditions
- A host must install the current platform before app code calls `current_platform()`.
- After installation, redraw requests and service lookups all target the same host object.

## Edge cases and failure modes
- `current_platform()` aborts loudly when called before registration because there is no safe fallback for missing host services.
- `request_redraw_if_registered()` returns `false` instead of aborting so callers can use it opportunistically during early boot/teardown.

## Concurrency and memory behavior
- Registry state is guarded by an `RwLock`.
- Callers share the platform through cloned `Arc`s; service lifetimes remain owned by the host implementation.

## Performance notes
- Generic service lookups are one `RwLock` read plus an `Arc` clone.
- This replaces duplicate app-local singleton lookups with one framework-owned registry.

## Feature flags and cfgs
- Registry behavior is target-agnostic and always enabled.

## Testing and benchmarks
- Covered by `docs/platform-api/tests/current_platform_tests.md`.

## Examples
```rust
let platform = oxide_platform_api::current_platform();
platform.request_redraw();
```

## Changelog
- 2026-03-12: added the current-platform registry helpers and `SharedPlatform` adapter so apps can consume a single Oxide-owned platform instance instead of maintaining duplicate bridge registries.
