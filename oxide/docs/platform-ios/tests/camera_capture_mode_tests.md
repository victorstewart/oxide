# platform-ios `tests/camera_capture_mode_tests.rs`

## Intention and purpose
- Keep the camera capture start-mode invariant in the test tree after removing in-source unit tests from `platform-ios`.

## Relation to the rest of the code
- Checks `crates/platform-apple/src/lib.rs` source text for the shared private `camera_capture_start_mode` helper that `IosCameraManager` re-exports through `AppleCameraManager`.
- Preserves the expectation that app-visible streams without audio subscribers use preview-only camera startup, while audio subscribers use the default capture path.

## Testing and benchmarks
- Run with `cargo test -p oxide-platform-ios --tests --locked`.

## Changelog
- 2026-05-19: moved the camera capture start-mode test out of production source.
