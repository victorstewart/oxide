# platform-api app frame tests

## Purpose

Protect the source-compatible two-stage `App` frame contract.

## Coverage

- Prepared apps receive display-link timing, viewport, scale, and a real runtime image uploader.
- `PreparedFrame` borrows app-owned draw-list and damage storage.
- `FrameDemand` reports continuous versus idle scheduling.
- Apps that only implement `draw` keep selecting the compatibility path.

## Command

`cargo test -p oxide-platform-api --test app_frame_tests`
