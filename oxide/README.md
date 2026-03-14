# Oxide Workspace

Rust‑first UI runtime and renderer with a macOS test host and Metal backend.

This workspace includes:

- `crates/platform-api`: App trait, events, device caps, animation types, permission domains (notifications, location, camera, contacts, bluetooth, motion, microphone, media library), location/motion services (with history + region tracking), Bluetooth scanning with advertisement metadata/cache helpers, and camera/audio stream + recording APIs.
- `crates/renderer-api`: Draw list types and renderer traits.
- `crates/renderer-metal`: Metal renderer (PSOs, shaders, rings, effects).
- `crates/text`: shaping (rustybuzz) + rasterization (swash) + A8 atlas baking.
- `crates/ui-core`: draw builder, node tree (layout + hit-test), elements, collection view, async layout coordinator, animation helpers (shake/wiggle/scatter, shrink/grow), test scenes/router, and camera demo.
- `crates/input`: gesture helpers (tap/long‑press/pan) and scroll accumulator.
- `crates/timing`: monotonic timers and animation curves (ease/spring).
- Host apps: `host/macos-app` (AppKit + CAMetalLayer bridge), iOS host (skeleton).

See `spec.xml` for a high‑level specification of goals and phases.

## Prerequisites

- Rust (see `oxide/rust-toolchain.toml` for pinned version).
- macOS 12+ with Xcode command‑line tools (Metal shader compilation via `xcrun`).

## Build & Run (macOS host)

Dev run (opens a Metal window):

```
cargo run -p oxide-macos-app
```

Bundle into a `.app` (for resource loading):

```
cargo install cargo-bundle
cargo bundle --bin oxide-macos-app --release
```

Place assets under `host/macos-app/Resources/` before bundling. The host copies this
folder into `Contents/Resources/` in the `.app`:

- `fonts/Inter-Regular.ttf` (required for text rendering)
- `images/sample.png` (used by the Zoom Image scene)

Tips:
- Download Inter from https://rsms.me/inter/ (Inter-Regular.ttf) and place it under
  `host/macos-app/Resources/fonts/Inter-Regular.ttf`.
- Use any PNG for the sample image (e.g., `images/sample.png`). If missing, a
  procedural checkerboard is used.

At runtime (macOS), resources can be loaded from the bundle with:

```rust
if let Some(bytes) = oxide_host_macos::macos_resource_get("fonts/Inter-Regular.ttf") {
    // add font into text::FontDb
}
```

More details in `host/macos-app/README.md`.

## UI Test Scenes

`ui-core::scenes` provides self‑contained scenes and a simple router:

- Controls Showcase (label, progress bar, spinner, button, toggle, slider)
- Text Layout (wrapping, alignment; RTL/CJK samples)
- Zoomable Image (fit, pinch‑zoom, pan, double‑tap reset)
- Animations Timeline (shake/wiggle/scatter helper showcase)
- Collection Stress (virtualized grid, focus/hover, GPU camera overlay, shrink/grow transitions)
- Damage Lab (damage visualization, threshold tuning, blur prefilter)
- Input & Haptics Lab (IME proxy, composition, selection, haptic mapping)
- Nine-slice Playground (interactive slice + alpha adjustments)
- SDF Text Demo (distance field text sizing preview)
- Snapshot Viewer (reference image comparison)
- Camera Demo (live NV12 feed, blur toggle, grayscale, metrics overlay, recording status)

Hosts can embed the router and forward inputs. Example flow:

1) Create `Router` with an `ImageUploader` implementation.
2) On each frame: `router.update(now_ms, dt_ms)` then `router.draw(viewport, device_scale, &mut builder)`.
3) Map pointer/gesture events to `router.input_*` as appropriate.

## Renderer Features

- `oxide-renderer-metal` features:
  - `msaa-4x`: enables 4× MSAA path (optional).
  - `ios-edr`: iOS Extended Dynamic Range targets (ignored on macOS).

Enable via cargo features when building the crate or host that depends on it.

## Snapshot Tests (macOS)

GPU snapshot tests render offscreen and read back to CPU for verification (≤ 1 LSB tolerance per channel).

Run on macOS:

```
cargo test -p oxide-renderer-metal --features snapshot-tests
```

## CPU Tests

Run focused unit tests for input and UI core:

```
cargo test -p oxide-input
cargo test -p oxide-ui-core
```

## Architecture Notes

- App lifecycle and input are abstracted in `platform-api` and implemented per host (e.g., `platform-macos`).
- Draws are built in `ui-core` and consumed by a concrete renderer implementing `renderer-api` (Metal here).
- Text integrates via `text` (A8 atlas) and the renderer’s text PSO.
- Animations are computed on CPU (`timing`, `ui-core::anim`) and applied as draw‑time overrides.

## Formatting & Style

Formatting is enforced by `rustfmt` (see `rustfmt.toml`). When in doubt, prefer the repo’s 3‑space indentation and Allman braces.
