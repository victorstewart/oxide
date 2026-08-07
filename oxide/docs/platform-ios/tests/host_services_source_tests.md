# platform-ios `tests/host_services_source_tests.rs`

## Intention and purpose

This suite freezes the native host-service boundaries that cannot execute on the macOS Rust test host: external URL admission, sandbox-directory creation order, and optional legacy-symbol ownership.

## Relation to the rest of the code

- The tests inspect `src/ios/host_services.m`, which owns shell-independent UIKit services.
- Runtime forwarding through `IosPlatform` remains covered by `platform_tests.rs` with host stubs.

## Entry points

- `external_urls_require_a_main_thread_handler_check_before_opening()` requires synchronous main-thread `canOpenURL` admission before `openURL`.
- `standard_paths_create_the_exact_returned_oxide_directory()` requires the app-owned `Oxide` component to be appended before that exact URL is created.
- `optional_nametag_symbols_are_compile_time_guarded()` requires legacy permission callbacks to stay behind the explicit bridge macro.

## Performance and memory

The tests scan compile-time source strings only. The covered main-queue work remains confined to infrequent service calls and never enters the frame loop.

## Testing

```sh
cargo test --locked -p oxide-platform-ios --test host_services_source_tests
```

## Changelog

- 2026-08-07: added focused native ABI ownership and fail-closed service regressions.
