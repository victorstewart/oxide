# renderer-metal tests `solid_vertex_color_tests.rs`

## Intention and purpose

Freeze the CPU/shader binding contract where snapshot execution is unavailable.

## Relation to the rest of the code

Reads `shaders/solid.metal` and verifies assumptions made by `renderer-metal/src/lib.rs`.

## Entry points list

- `solid_shader_inherits_zero_and_interpolates_nonzero_vertex_color()`: verifies the solid shader source contract.

## Logic narrative

Whitespace-normalized assertions require vertex buffer 1, exact-zero selection, a color varying, and fragment return of that varying.

## Preconditions and postconditions

The shader must remain at its mapped repository path. Passing means its source interface matches CPU binding.

## Edge cases and failure modes

Renaming or changing a binding fails before runtime pipeline use.

## Concurrency and memory behavior

The test performs bounded local string processing with no external state.

## Performance notes

Source normalization is test-only.

## Feature flags and cfgs

No feature or target gate.

## Testing and benchmarks

Run with `cargo test --locked -p oxide-renderer-metal --test solid_vertex_color_tests`.

## Examples

The test rejects a regression back to fragment buffer 0.

## Changelog

- 2026-07-12: added portable solid vertex-color shader coverage.
