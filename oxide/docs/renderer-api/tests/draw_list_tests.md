# renderer-api tests `draw_list_tests.rs`

## Intention and purpose
- Prove the renderer-api draw-list structures preserve stack, damage, geometry, and text-atlas revision invariants.

## Relation to the rest of the code
- Protects `oxide-ui-core::DrawListBuilder`, renderer backends, and retained draw-list replay.
- Complements `ui-core` draw-builder tests that exercise compatibility checks while appending cached draw lists.

## Entry points list
- `draw_cmd_taxonomy_is_frozen`
  Freezes the `DrawCmd` variant set and source declaration order for later draw-stream migration evidence.
- `representative_draw_stream_capture_signature_is_frozen`
  Freezes one representative semantic draw stream plus the callback replay trace used as the baseline for future packed draw-stream A/B work.
- `draw_list_api_has_no_native_visible_preview_command`
  Guards the camera-preview contract by keeping native visible preview commands out of the product draw list.
- `balanced_layers_and_clips_validate`
  Builds a mixed draw list and validates stack balance.
- `draw_list_detects_stale_text_atlas_revision`
  Verifies retained text geometry can be rejected when the atlas revision has advanced.
- `detects_unbalanced_layer_stack`
  Verifies layer underflow and incomplete layer stacks are detected by the local validator.
- `damage_rects_round_trip`
  Verifies damage rectangles remain stable.
- `vertex_storage_is_mutable`
  Verifies backing geometry arrays are writable caller-owned storage.

## Logic narrative
- Tests use plain draw-list values so renderer-api invariants stay independent of UI widgets and backend implementations.
- The taxonomy freeze constructs one representative value for every draw command and maps it through an exhaustive match, so new variants or field-shape changes force an intentional test update.
- The same test reads the `DrawCmd` enum declaration order from source to keep the semantic stream order explicit before packed draw-stream work starts.
- The representative capture fixture builds a valid `DrawList` with backing vertices/indices, clips, layer markers, primitive/image/text/effect/camera/spinner commands, then records both capture rows and replay callback rows.
- Text atlas compatibility is checked by matching an atlas handle and revision against cached `GlyphRun` metadata.

## Preconditions and postconditions
- Passing tests mean retained draw caches can distinguish current text geometry from geometry baked before an atlas eviction or reset.
- Passing tests also mean the current draw-command taxonomy and representative capture/replay signature remain the explicit baseline for future backend packet and packed draw-stream freezes.

## Edge cases and failure modes
- A different atlas handle must not make the cached draw list stale.

## Concurrency and memory behavior
- Tests are single-threaded and allocation-light.

## Performance notes
- The revision check scans draw commands without allocating.
- The capture/replay fixture is measurement harness only. It changes no runtime path and does not claim a performance win.

## Feature flags and cfgs
- No feature-specific branches.

## Testing and benchmarks
- Run with `cargo test --locked -j$(sysctl -n hw.ncpu) -p oxide-renderer-api --test draw_list_tests`.

## Changelog
- 2026-06-22: added a representative draw-stream capture/replay signature freeze before backend packet migrations.
- 2026-06-22: froze the `DrawCmd` variant set and source order as measurement harness for architecture densification A/B work.
- 2026-05-31: added stale text-atlas revision coverage for retained draw cache safety.
