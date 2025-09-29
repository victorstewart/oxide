# UI Automation Plan

This repository ships host applications for iOS (UIKit) and macOS (AppKit). The
GPU-backed renderer and host bridges require real devices or a Metal-capable
macOS host to exercise end-to-end behaviour, so CI only runs CPU-side unit tests.
This document describes the manual automation passes you can execute locally.

## iOS Host (Xcode UITests)

1. Install the workspace toolchain (`rustup toolchain install --profile minimal 1.86.0`).
2. Prepare the iOS bundle (`cargo xtask ios prepare`) which merges entitlements,
   Info.plist usage strings, and builds the Metal shader library.
3. Build the staticlib(s) for device + simulator (`cargo run -p xtask -- ios prepare`
   or the convenience script `scripts/ios-test.sh`).
4. Open `oxideui/host/ios-app/App/OxideHost.xcodeproj` in Xcode (or run
   `xcodebuild -project oxideui/host/ios-app/App/OxideHost.xcodeproj -scheme OxideHostUITests test`).
5. Choose a **device** destination for Metal coverage. UITests now gate on
   `target_abi != sim`; on simulator they build but skip GPU assertions.
6. Run the `OxideHostUITests` target. The suite now:
   - Walks every sample scene via the segmented control (Controls → Text Layout → Zoom Image → Animations → Collection Stress → Damage Lab → **Input & Haptics** → Nine Slice → SDF Text → Snapshot → Camera).
   - Toggles the overlay and reduce-motion switches, verifying state changes.
   - Exercises zoom gestures, collection scrolling, damage sliders, clipboard/IME flows, and camera toggles.
   - Validates the camera metrics overlay (coverage %, bit-depth, matrix, paused state) while flipping the capture switch.
   - Captures annotated screenshots (find them under the test report attachments).
7. Optional: enable the Accessibility Inspector to verify identifiers
   (`sceneControl`, `overlaySwitch`, `reduceMotionSwitch`, `zoomViewport`).

### Extending iOS Automation

- Add additional gestures (pinch/rotate) via `pressForDuration:thenDragToElement:`.
- Capture PNGs and compare against previous runs with tools like `imgdiff`.
- Drive deep links by injecting launch arguments (`self.app.launchArguments`).
- Extend the camera metrics assertions by parsing `cameraMetricsLabel` and persisting the values alongside screenshots.

## macOS Host (`app-runner` smoke path)

1. Build the macOS host: `cargo run -p oxideui-host-macos`.
2. For non-interactive smoke testing, use `cargo run -p app-runner -- --once` to
   execute a single frame and log `PerfStats` to stdout. Redirect into
   `artifacts/app-runner/*.log` for manual diffing.
3. For scripted scene traversal, extend `app-runner` with CLI flags (TODO) to
   iterate scenes similar to the iOS UITest harness.
4. Use Xcode’s GPU Frame Debugger to capture frames when investigating visual
   regressions.

## Simulator limitations

- Metal on iOS simulator is limited; real-device runs are preferred. Tests skip
  gracefully when compiled for `target_abi=sim`.
- macOS host requires Metal. On machines without a GPU the renderer returns
  `MetalInitError::NoDevice`; CPU-only unit tests still pass.

## Reporting

- Store screenshots/logs under `artifacts/` (gitignored) for manual comparison.
- Install the Metal command line tools once per machine with `xcodebuild -downloadComponent MetalToolchain`; without it `cargo test -p oxideui-host-ios --no-run` fails during shader compilation.
- Update this document whenever new automation steps or scripts are added.
