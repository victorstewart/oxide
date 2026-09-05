# Oxide Docs

This directory mirrors Rust source units with design notes, entry points, behavior, and verification references.

## Crates

- [oxide-host-web](oxide-host-web/lib.md)
- [oxide-host-ios](oxide-host-ios/lib.md)
- [oxide-host-macos](oxide-host-macos/lib.md)
- [oxide-comparison-runtime](oxide-comparison-runtime/lib.md)
- [oxide-image-sampling-diagnostic](oxide-image-sampling-diagnostic/lib.md)
- [oxide-text-compositing-diagnostic](oxide-text-compositing-diagnostic/lib.md)
- [oxide-apple-comparison-controller](apple-comparison-controller/lib.md), including the [macOS comparator qualification reducer](apple-comparison-controller/comparator_qualification.md), [macOS launch contract](apple-comparison-controller/launch.md), [direct external-meter energy collector](apple-comparison-controller/energy.md), [exact-process resource collector](apple-comparison-controller/resource.md), [standalone System Trace collector](apple-comparison-controller/system_trace.md), and [macOS presentation trace correlator](apple-comparison-controller/trace.md)
- [image-store](image-store/lib.md)
- [benchmark-spec](benchmark-spec/lib.md), including the [scenario and trace schema](benchmark-spec/scenario.md), [typed PR vertical fixtures](benchmark-spec/pr_fixtures.md), [typed release fixtures](benchmark-spec/release_fixtures.md), [release-candidate capture admission](benchmark-spec/release_candidate.md), [macOS higher-tier campaign plans](benchmark-spec/apple_campaign_plan.md), [instrumentation calibration](benchmark-spec/instrumentation_calibration.md), [strict comparator acceptance gate](benchmark-spec/comparator_acceptance.md), and [normalized visual parity reducer](benchmark-spec/visual.md)
- [Apple comparison Phase-0 transport](apple-comparison/phase0_transport.md)
- [AppKit production-reference foundation](apple-comparison/appkit_production_reference.md), [render-only diagnostic adapter](apple-comparison/appkit_scenario_adapter.md), and [macOS canonical launch executor](apple-comparison/macos_canonical_launch_executor.md)
- [perf-runner](perf-runner/lib.md), including the [macOS density-acquisition controller](perf-runner/density_acquisition.md)
- [permissions manager](permissions/lib.md)
- [permissions sensor bridge](permissions/sensors.md)
- [platform-android](platform-android/lib.md)
- [platform-apple](platform-apple/lib.md)
- [platform-api](platform-api/lib.md)
- [platform-ios](platform-ios/lib.md)
- [platform-ios native network ABI](platform-ios/ios/network.h.md)
- [platform-macos](platform-macos/lib.md)
- [platform-web](platform-web/lib.md)
- [oxide-text](oxide-text/lib.md)
- [renderer-api](renderer-api/lib.md)
- [renderer-metal](renderer-metal/lib.md)
- [renderer-metal effects shader](renderer-metal/shaders/effects.md)
- [renderer-metal ID-mask GPU path](renderer-metal/id_mask_gpu.md)
- [renderer-metal prepared chunks](renderer-metal/prepared.md)
- [renderer-web](renderer-web/lib.md)
- [snapshot-runner](snapshot-runner/main.md)
- [test-scenes](test-scenes/lib.md)
- [timing](timing/lib.md)
- [ui-core](ui-core/lib.md)
- [ui-core deterministic bitmap text](ui-core/bitmap_text.md)
- [wasm-alloc-counter](wasm-alloc-counter/lib.md)
- [xtask](xtask/lib.md)

## Supporting Notes

- [testing](testing.md)
