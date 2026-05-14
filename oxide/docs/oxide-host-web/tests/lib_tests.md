# oxide-host-web::tests::lib_tests

## Intention and purpose

These tests verify native-testable support code in the WebAssembly host. They exist so the procedural image used by the browser demo has deterministic dimensions and alpha.

## Relation to the rest of the code

The tests call `oxide_host_web::generate_checker_rgba`, which is used by the wasm host to seed the Zoom scene when no external image bundle exists.

Call flow:

- cargo test
- `oxide-host-web/tests/lib_tests.rs`
- `oxide_host_web::generate_checker_rgba`
- web host image upload during browser startup

## Entry points list

- `checker_texture_has_expected_size_and_alpha()`: verifies byte count and full opacity.
- `checker_texture_alternates_tiles()`: verifies the generated image is not a flat color.
- `static_shell_imports_generated_pkg_and_platform_smoke_hook()`: verifies the static page imports `www/pkg`, exposes the browser platform smoke report, and logs platform/render smoke markers for browser verification.

## Logic narrative

The first test checks RGBA buffer shape. The second test samples different tile positions and confirms they differ, which catches accidental one-color placeholder output. The static shell test catches regressions where the HTML page points at the wrong wasm-bindgen output path, stops invoking the backend smoke hook, or stops logging the browser-test markers.

## Preconditions and postconditions; invariants maintained; unsafe invariants if any

The tests require no wasm runtime. Generated images are always RGBA8 and fully opaque.

## Edge cases and failure modes

Small dimensions still allocate a correctly sized buffer. Tile alternation is checked with a width crossing the tile boundary. The static HTML test uses `include_str!` so it fails at compile time if the shell is moved without updating the test.

## Concurrency and memory behavior

The function allocates one vector sized to `width * height * 4`.

## Performance notes

Generation is linear in pixel count and is used only during host startup.

## Feature flags and cfgs

These tests run on native targets. The wasm host entry points are compile-checked with the wasm target and verified through the browser page.

## Testing and benchmarks

Run with `cargo test -p oxide-host-web --tests`.

## Examples

```rust
pub fn texture() -> Vec<u8>
{
   oxide_host_web::generate_checker_rgba(16, 16)
}
```

## Changelog

- Added static shell coverage for the generated package import and platform smoke hook.
