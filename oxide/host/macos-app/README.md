# macOS Test Host

Minimal AppKit application that hosts a Metal-backed view for Oxide UI testing.

- Staticlib: `oxide-host-macos` exposes `rust_entry()` that starts the app.
- Runner: `oxide-macos-app` binary calls `rust_entry()`.
- View: `MetalView` uses `CAMetalLayer` and configures drawable size on layout.

Build and run:

```
cargo run -p oxide-macos-app
```

This opens a white window with a Metal surface and logs lifecycle events.

## Packaging (.app bundle)

You can create a macOS app bundle using `cargo-bundle`:

1) Install: `cargo install cargo-bundle`

2) Bundle the app:

```
cargo bundle --bin oxide-macos-app --release
```

This produces `target/release/bundle/osx/oxide-macos-app.app` containing
`Contents/Info.plist` and `Contents/Resources/`. Place fonts and images under
`host/macos-app/Resources/` before bundling; they will be available via the
bundle at runtime (see `macos_resource_get()` helper).

## Resources

- Place assets under `host/macos-app/Resources/`. Example:
  - `fonts/Inter-Regular.ttf` (OFL)
  - `images/sample.png`

At runtime, you can load a resource by name (relative to `Resources/`) using the
macOS host helper:

```rust
// macOS only
if let Some(bytes) = oxide_host_macos::macos_resource_get("fonts/Inter-Regular.ttf") {
    // use bytes
}
```

Note: When running as a plain cargo binary (not a .app bundle), bundle resources
are not automatically found; prefer bundling for resource loading.

## Hotkeys

- 1 / 2 / 3 / 4 / 5: switch scene (Controls / Text Layout / Zoom Image / Animations / Collection Stress)
- Space: press/release button (Controls)
- Left / Right: adjust slider (Controls); move focus left/right (Collection)
- Up / Down: move focus up/down (Collection)
- Z: reset zoom/pan (Zoom Image)
- F: toggle on‑screen overlay (fps, draws, anims, reduce‑motion flag)
- R: toggle high refresh (display sync enabled)
- M: toggle Reduce Motion (snaps animations where supported)
- I: toggle idle sleep prevention

## Manual QA Checklist

- Launch window renders live content without flicker; FPS overlay appears.
- Press 1–5 to switch scenes; each renders expected content:
  - Controls: label, progress, spinner, button, toggle, slider
  - Text Layout: multi‑language samples wrap and align correctly
  - Zoom Image: sample PNG if bundled, otherwise checkerboard; pinch/drag works; Z resets
  - Animations: bars animate; RM toggle snaps
  - Collection: large virtualized grid scrolls smoothly; focus moves with arrows
- Press F to hide/show overlay; overlay reports fps, draw count, active animations, RM:on/off.
- Press Space on Controls: button animates down (80 ms) and up (120 ms); press via mouse also works.
- Press Left/Right on Controls: slider value moves with step; dragging the thumb also works.
- Press R to toggle high refresh; FPS should reflect device’s rate when enabled.
- Press M to toggle Reduce Motion; Animations scene snaps; (optional) verify other animations adopt policy if extended.
- Press I to toggle idle sleep prevention; screen should remain responsive; no OS sleep while enabled.

## Notes

- Presentation path is offscreen render + blit to the window drawable (late acquire) with triple buffering.
- Resources are optional at dev time; if `images/sample.png` is missing, a procedural checkerboard is used; if `fonts/Inter-Regular.ttf` is missing, labels may fall back depending on your font setup.
