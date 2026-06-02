# oxide-host-ios::tests::camera_benchmark_tests

This integration test suite protects the iOS app-host camera benchmark contract from macOS-side Rust tests. It intentionally avoids device, simulator, `xcodebuild`, `xctrace`, `devicectl`, and `simctl` execution.

## Coverage

- `benchmark_camera_scene_uses_minimal_preview_draw_list()` boots the benchmark camera scene through the Rust host ABI and verifies the custom Oxide preview path emits a minimal draw list plus camera/GPU diagnostic fields.
- `benchmark_camera_preview_plan_requires_first_drawable()` preserves the first-drawable preview planning contract.
- `actual_app_frame_driven_scheduling_installs_callback_before_camera_start()` keeps camera-frame-driven scheduling armed before the real app-host camera starts.
- `avfoundation_preview_layer_transport_stays_benchmark_diagnostic_only()` statically gates `AVCaptureVideoPreviewLayer` usage so the product custom preview path remains Oxide-owned and the preview-layer transport stays behind explicit AVFoundation or hybrid perf modes.
- `uikit_preview_layer_cases_are_labeled_as_baseline_or_diagnostic()` verifies the UIKit perf catalog keeps preview-layer cases labeled as official AVFoundation baselines or diagnostic-only hybrid transports.
- `ios_manual_touch_path_uses_raw_events_and_recognizer_fallback()` keeps raw window-level touch delivery and file-backed diagnostics intact.
- `merge_camera_contract_fields_prefers_backend_contract_over_rotated_preview_stats()` keeps camera contract metadata sourced from backend capture details.

## Contract

The shipping-oriented custom camera preview path must use `AVCaptureVideoDataOutput` plus Oxide-owned Metal composition. `AVCaptureVideoPreviewLayer` is allowed only for the explicit AVFoundation baseline or diagnostic hybrid preview-layer cases.

## Run

```sh
cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-host-ios --test camera_benchmark_tests
```
