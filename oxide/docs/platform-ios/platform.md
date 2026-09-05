# platform-ios `platform.rs`

## Intention and purpose

This module supplies the production iOS implementation of `oxide_platform_api::Platform`. It gives a Rust app one coherent service object while keeping UIKit limited to lifecycle, responder, sandbox, display, and URL host operations.

## Relation to the rest of the code

- `oxide-platform-api` defines `Platform`, the service traits, device capabilities, and process-global registries.
- `oxide-platform-apple` owns reusable HTTP, sockets, secure storage, camera, Bluetooth, location, motion, push, and media-library implementations.
- `oxide-networking::ReachabilityManager` owns the latest network path snapshot and subscription fanout.
- UIKit hosts supply narrow `oxide_host_*` symbols and own app lifecycle independently of `Platform::run_app`.

Call flow:

```text
iOS host -> install_current_platform -> platform and clipboard registries
Rust app -> IosPlatform -> shared Apple service or narrow host FFI
NWPathMonitor -> reachability trampoline -> ReachabilityManager -> NetworkStatus subscriber
```

## Entry points list

- `oxide_platform_ios::IosPlatform`: copyable, stateless handle to process-owned iOS services.
- `oxide_platform_ios::IosPlatform::new() -> IosPlatform`: constructs an unregistered handle without starting services.
- `oxide_platform_ios::install_current_platform() -> Arc<IosPlatform>`: installs one shared handle in both the platform and clipboard registries.
- `oxide_platform_ios::platform() -> IosPlatform`: returns an unregistered handle.
- `impl oxide_platform_api::clipboard::ClipboardProvider for IosPlatform`: reads and writes UTF-8 pasteboard text through the shared native clipboard bridge.
- `impl oxide_platform_api::Platform for IosPlatform`: forwards redraw, refresh, idle-timer, settings, URL, IME, device, and simulation queries to the host; returns stable shared Apple services; exposes sandbox paths and network status; parks the caller in `run_app` because UIKit already owns the application loop.

## Logic narrative

1. `install_current_platform` allocates one `Arc<IosPlatform>`, coerces it into the platform and clipboard trait objects, and registers both before app initialization.
2. Control methods convert booleans to the one-byte host ABI and otherwise forward borrowed UTF-8 bytes without allocation.
3. Device capabilities sanitize invalid native scale values and retain a conservative 60 Hz minimum when no refresh rate is reported.
4. Service accessors return process-stable instances. Haptics uses one lazily allocated `Arc`; the remaining stateless Apple adapters are static values.
5. Standard-path lookup asks the host for an existing sandbox directory, copies the returned bytes once, and releases the native allocation. A host failure falls back to an ensured temporary directory.
6. Network status constructs one lazy `ReachabilityManager`, installs the existing native reachability callback, and starts monitoring only when queried or subscribed. Each public subscription is retained because `NetworkStatusService` has no unsubscribe handle.
7. `run_app` retains the supplied app and parks indefinitely as a fallback for hosts that do not own app installation and lifecycle externally.

## Preconditions and postconditions

- The iOS host must export every declared `oxide_host_*` symbol with the exact integer widths and pointer ownership described below.
- UI-affecting host functions must marshal onto the UIKit main thread when invoked from a Rust worker.
- Successful standard-path calls return a nonempty UTF-8 absolute path to an existing directory.
- `install_current_platform` completes before the host initializes the Rust app.
- No platform accessor creates per-frame state or enters the renderer command path.

## Edge cases and failure modes

- Empty external URLs fail locally as `PlatformError::Invalid`; native URL rejection becomes `PlatformError::Unsupported`.
- A non-finite or nonpositive native scale becomes `1.0`; a reported refresh rate below 60 becomes 60.
- Failed or malformed native path results use an ensured temporary directory rather than panicking.
- Reachability begins offline until the native monitor produces a path. Unknown path kinds remain connected but report no known interface bit.
- A poisoned callback/subscription lock is recovered with its inner value so one panic does not permanently disable platform delivery.
- `run_app` can wake spuriously, so it parks in a loop; the loop performs work only after a wake and immediately parks again.

## Concurrency and memory behavior

- `IosPlatform` is a zero-sized `Copy + Send + Sync` handle.
- Shared Apple services and the path/time adapters are static and allocate nothing per lookup.
- Haptics allocates its shared `Arc` once.
- Network monitoring allocates its manager once and one retained record per subscription. Native updates copy the callback list inside `ReachabilityManager`, then invoke callbacks without holding its state lock.
- Control, IME, haptic, clipboard-write, and device calls cross FFI once per explicit app request; none runs in the renderer hot path.
- Standard-path and clipboard reads allocate only for their returned owned Rust strings.

### Unsafe contracts

- Borrowed string pointers passed to the host remain valid only for the duration of the call; the host must copy any bytes retained asynchronously.
- `oxide_host_standard_path` writes either a null/zero failure result or a malloc-owned buffer and exact byte length. Rust copies before calling `oxide_host_string_free` exactly once.
- The reachability callback uses the C ABI `(u32 status, u32 path_kind, u8 expensive)` and may run on a native monitor queue; the Rust trampoline is thread-safe.
- Device and control functions do not retain pointers and must not unwind across the FFI boundary.

## Performance notes

- Service lookup is constant time and returns existing objects.
- `run_app` consumes no CPU while parked.
- Reachability is event-driven; it does not poll during frames or at idle.
- No platform control adds work to draw-list construction, Metal encoding, or presentation unless the app explicitly requests that control.

## Feature flags and cfgs

- `tokio-runtime` remains independent and configures the crate's existing task-spawn adapter.
- `native-camera-bridge` and `nametag-host-bridge` affect native service compilation but do not change the `IosPlatform` aggregate shape.
- Production use targets arm64 iOS device and arm64 iOS Simulator builds.

## Testing and benchmarks

- `cargo check --locked -p oxide-platform-ios` verifies the aggregate and service trait surface.
- `cargo test --locked -p oxide-platform-ios --test platform_tests` supplies native host stubs and verifies exact forwarding, error mapping, buffer ownership, path kinds, and reachability fanout.
- This service wiring is not a renderer hot-path change and does not create a standalone performance result. Host/device journeys must separately prove redraw, touch, frame scheduling, and presentation.

## Examples

```rust
use oxide_platform_api::Platform;

let platform = oxide_platform_ios::install_current_platform();
platform.request_redraw();
platform.set_high_refresh(true);
```

## Changelog

- 2026-08-06: added the production iOS platform aggregate, global installation, host controls and device capabilities, native sandbox paths, and event-driven network status.
