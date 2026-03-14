# ui-core animation helper tests

## Intention and purpose
- Document the shared `oxide-ui-core` animation-helper regressions.

## Coverage
- `shake_sequence_has_segments`
  - Confirms the existing transform shake helper still emits the expected segment structure.
- `cubic_bezier_ease_in_out_uses_non_linear_legacy_curve`
  - Locks the shared standard ease-in-out profile to the same non-linear shape Nametag previously carried locally.
- `sample_keyframed_offset_interpolates_and_clamps_to_zero_after_duration`
  - Verifies the generic keyframed offset sampler interpolates between phase targets and returns to rest after the configured duration.
- `required_field_shake_offset_matches_shared_profile_and_clamps_negative_scale`
  - Verifies the named required-field shake profile starts and ends at zero and ignores negative scale.

## Changelog
- 2026-03-13: added regression coverage for the shared cubic-bezier and required-field shake helpers moved out of Nametag.
