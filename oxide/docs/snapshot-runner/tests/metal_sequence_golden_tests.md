# oxide-snapshot-runner tests `metal_sequence_golden_tests.rs`

## Intention and purpose

These macOS tests freeze visible multi-frame Metal behavior for every sequence required by C03.

## Relation to the rest of the code

They use the production `MetalRenderer` API and committed images under `goldens/sequences`.

## Entry points list

- `full_direct_then_partial_damage_freezes_parent_defect_and_reference`
- `memory_warning_recreation_rebuilds_identical_visible_state`
- `resize_sequence_matches_fresh_target_goldens`
- `device_loss_recreation_preserves_visible_golden`
- `atlas_eviction_and_recreation_preserve_glyph_golden`
- `effect_target_plans_preserve_direct_prepass_quarter_and_eighth_goldens`

## Logic narrative

The damage case freezes both the retained full-direct then partial-damage parent output and a fresh complete-scene reference. It explicitly proves the known parent mismatch that C10 must remove. Resize compares a retained resized target with a fresh target. Memory-warning and device-loss controls destroy and recreate the renderer, preserving visible state before later work adds narrower production recovery hooks. Atlas eviction releases and recreates the A8 atlas texture, then verifies rebuilt and warm glyph frames. Effect-target coverage renders the direct, zero-blur prepass, quarter-resolution blur, and eighth-resolution blur plans on fresh renderers so target-lifetime changes cannot alter the visible result.

## Preconditions and postconditions

The tests require macOS Metal. All simple-scene and A8 sequence images compare exactly, with no tolerance.

## Edge cases and failure modes

Unexpected parent damage changes, loss of the known C10 defect signal, resize residue, recreate drift, stale atlas handles, or incomplete atlas upload fail byte-for-byte. C10 must replace the damage inequality with exact equality and intentionally refresh the defective parent golden.

## Concurrency and memory behavior

Each test owns its renderer and performs explicit blocking readback only after submission.

## Performance notes

These are correctness sequences, not timing benchmarks.

## Feature flags and cfgs

The integration test is macOS-only.

## Testing and benchmarks

Run `cargo test --locked -p oxide-snapshot-runner --test metal_sequence_golden_tests`.

## Examples

Set `UPDATE_GOLDENS=1` only for intentional reviewed golden updates.

## Changelog

- 2026-07-12: added direct/prepass/quarter/eighth effect-target plan goldens.
- 2026-07-12: added exact damage, recreate, resize, device-loss, and atlas-eviction sequence goldens.
