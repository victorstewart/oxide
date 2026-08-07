# oxide-host-ios injected app tests

## Purpose

Protect the runtime-image adapter used by injected apps without launching UIKit.

## Coverage

The upload wiring test freezes direct forwarding of the caller's RGBA slice, row stride, and explicit sampling mode into `MetalRenderer`. It rejects host-side byte copies or channel swaps, requires both create paths to translate the invalid zero sentinel into `None`, and requires successful handles to release through `MetalRenderer::image_release`.

## Command

`cargo test -p oxide-host-ios --test injected_app_tests`
