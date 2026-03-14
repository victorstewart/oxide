# platform-macos `lib.rs`

## Intention and purpose
- Provide the macOS host implementation of Oxide's `Platform` trait for local host apps and smoke environments.

## Relation to the rest of the code
- Supplies a concrete `Platform` implementation to Oxide host code.
- Uses stub service implementations for unsupported host-test features while still satisfying the full `Platform` trait surface.

## Entry points list
- `MacPlatform`
  Concrete macOS `Platform` implementation.
- `MacHaptics`
  Haptics adapter used by `MacPlatform`.

## Logic narrative
- `MacPlatform` forwards redraw, refresh-rate, idle-timer, clipboard, and haptic calls to Objective-C/FFI shims.
- Unsupported services remain explicit no-op or error-returning stub implementations.
- The recent change keeps `device_caps`, `open_system_settings`, and shared `Arc`-backed haptics aligned with the expanded core `Platform` trait.

## Preconditions and postconditions
- Host FFI shims must be available for redraw, refresh-rate, clipboard, free, and haptics calls.

## Edge cases and failure modes
- Unsupported services return `PlatformError::Unsupported` rather than pretending to work.

## Concurrency and memory behavior
- Haptics are shared through one lazily initialized `Arc`.
- Other stub services are static singletons.

## Performance notes
- Clipboard and haptic calls are thin FFI wrappers and not on hot render paths.

## Feature flags and cfgs
- This crate is specific to macOS hosts.

## Testing and benchmarks
- Indirectly compiled by workspace builds that include the expanded `Platform` trait surface.

## Examples
```rust
let platform = oxide_platform_macos::MacPlatform;
platform.request_redraw();
```

## Changelog
- 2026-03-12: aligned the macOS host implementation with the expanded core `Platform` trait by providing `device_caps`, `open_system_settings`, and shared-`Arc` haptics accessors.
