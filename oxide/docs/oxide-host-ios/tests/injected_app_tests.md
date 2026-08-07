# oxide-host-ios injected app tests

## Purpose

Protect explicit single-owner installation and the runtime-image adapter used by injected apps without launching UIKit.

## Coverage

The installation tests verify that the first production app claims the process-global slot, a second app cannot replace it, the installed flag is published only after ownership is stored, and the mutually exclusive legacy test host rejects installation.

The upload wiring test freezes direct forwarding of A8 append/release operations plus the caller's RGBA slice, row stride, and explicit sampling mode into `MetalRenderer`. It rejects host-side byte copies or channel swaps, requires both RGBA create paths to translate the invalid zero sentinel into `None`, routes append-only A8 writes through `image_append_a8`, and releases owned A8/RGBA handles through `image_release`.

## Changelog

- 2026-08-07: required production uploader forwarding for append-only A8 publication and page release.

## Command

`cargo test --locked -p oxide-host-ios --no-default-features --test injected_app_tests`

`cargo test --locked -p oxide-host-ios --features test-scenes-entrypoint --test injected_app_tests`
