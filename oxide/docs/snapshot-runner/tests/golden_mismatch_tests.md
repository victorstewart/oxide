# oxide-snapshot-runner tests `golden_mismatch_tests.rs`

## Intention and purpose
- Prove snapshot mismatches fail by default.
- Prove explicit mismatch allowance remains diagnostic.
- Enforce that committed renderer goldens exist and match for primitive, glyph, image, nested composition/effect, Scene3D, camera preview, ID-mask compositor, and renderer-mode variants.

## Relation to the rest of the code
- Builds and invokes the `oxide-snapshot-runner` binary.
- Reads committed images under `goldens/snapshots`.
- Produces temporary outputs under the system temp directory for comparison.

## Entry points list
- `existing_golden_pixel_mismatch_fails_by_default`
  Verifies a component mismatch fails.
- `existing_golden_size_mismatch_fails_by_default`
  Verifies a dimension mismatch fails.
- `allow_mismatch_keeps_pixel_and_size_mismatch_explicit`
  Verifies `--allow-mismatch` allows exit success while retaining diagnostics.
- `committed_renderer_goldens_cover_scene3d_damage_camera_and_id_mask`
  Verifies required renderer-family PNG goldens exist and match current output, including exact primitive/glyph/image/nested-composition fixtures, 1x/2x/3x and aspect-specific Scene3D output, Damage Lab and router scenes, deterministic text-input fixtures, camera variants, and 1x/2x/3x square/wide/portrait ID-mask compositor modes with signal checks.
- `committed_webgpu_browser_golden_exists_with_expected_size`
  Verifies the browser WebGPU app goldens are present at 320x240, 640x360, and 360x640 with visible-scene signal, the browser WebGPU Scene3D goldens are present at 512x512, 640x360, and 360x640 with colored-geometry signal, the square browser WebGPU ID-mask compositor golden is present at 512x512 with visible-compositor signal, and the wide and portrait browser WebGPU ID-mask compositor goldens are present at 640x360 and 360x640 with visible-compositor signal.

## Logic narrative
- The test harness resolves the runner binary from the current Cargo profile and builds it if needed.
- Temporary button goldens exercise mismatch behavior without changing the repo.
- The committed renderer test asserts each required golden exists before invoking the runner so missing files cannot be silently generated during tests. It then rerenders every required component through Metal and compares dimensions with an explicit maximum of 16 changed pixels, 3 channel levels, and 0.02 MSE.
- Router-scene coverage uses the draw-list path rather than a direct renderer shim, because these scenes are meant to catch UI scene-state visual drift in ordinary Oxide composition. The snapshot runner disables the router HUD overlay for these cases so the committed images verify scene content instead of debug chrome.
- Scale-specific required goldens pass their expected scale into the runner so logical-coordinate to pixel-coordinate paths are tested instead of only PNG dimensions.
- Aspect-specific required goldens pass landscape and portrait dimensions into the runner so viewport, camera crop, ID-mask projection, and seam-coordinate paths are tested outside square-target assumptions.
- The seam golden check decodes the committed PNGs and requires enough bright seam pixels against a dark background, so a near-black seam artifact cannot satisfy the contract again.
- The non-seam Metal ID-mask golden check decodes committed PNGs and requires enough colored region signal, so blank or monochrome compositor output cannot satisfy the contract.
- The Scene3D golden check decodes committed PNGs and requires enough colored geometry signal, so blank monochrome Scene3D output cannot satisfy the contract.
- The camera golden check decodes committed PNGs and requires visible non-white preview signal, so a blank synthetic preview cannot satisfy the contract.
- The router-scene and text-input signal check decodes the committed PNGs and requires enough non-white pixels, so a fallback-only or blank scene cannot satisfy the contract.
- The WebGPU signal checks decode the committed PNGs and require enough non-white pixels across square, landscape, and portrait captures, plus colored Scene3D geometry signal, so a blank browser app, Scene3D, or ID-mask capture cannot satisfy the size-only contract.

## Preconditions and postconditions
- Tests require a host capable of running the local Metal snapshot runner.
- Passing committed renderer coverage means current output remains within configured pixel, max-error, and MSE tolerances.
- Passing the 2x/3x renderer cases means the local Metal paths still honor high-DPI projection, viewport, camera background, and ID-mask coordinate conversion.

## Edge cases and failure modes
- Missing committed goldens fail before rendering.
- Size drift and pixel drift remain separate diagnostics.
- Temporary test directories are removed after each case.

## Concurrency and memory behavior
- Tests spawn child processes and otherwise keep state in temporary directories.
- The runner binary path is cached with `OnceLock`.

## Performance notes
- These are visual correctness tests, not performance measurements.

## Feature flags and cfgs
- No feature-specific branches in the test harness.

## Testing and benchmarks
- Run with `cargo test --locked -p oxide-snapshot-runner --test golden_mismatch_tests`.

## Examples
```rust
cargo test --locked -p oxide-snapshot-runner --test golden_mismatch_tests
```

## Changelog
- 2026-07-12: added primitive, A8/Unicode glyph, crop/zoom, nested layer/transform/opacity/effect, Scene3D 3x, and ID-mask square/wide/portrait 3x goldens.
- 2026-06-01: expanded WebGPU browser golden enforcement to include a 512x512 Scene3D capture with colored-geometry signal checks.
- 2026-06-01: expanded WebGPU browser Scene3D golden enforcement to include wide and portrait captures.
- 2026-06-01: expanded committed Metal golden enforcement to include portrait Scene3D, synthetic camera, and ID-mask captures.
- 2026-06-01: expanded WebGPU browser golden enforcement to include portrait app and ID-mask captures at 360x640.
- 2026-06-01: added colored-region signal checks for committed Metal ID-mask beauty, city-id, and neighborhood-id goldens.
- 2026-06-01: added colored-geometry signal checks for committed Scene3D goldens.
- 2026-06-01: expanded committed Scene3D golden enforcement to non-square mixed, bloom, and depth-stack captures.
- 2026-06-01: expanded committed camera golden enforcement to non-square legacy NV12, BGRA, blur/grayscale, and tint/alpha captures, with visible-signal checks for camera goldens.
- 2026-06-01: expanded WebGPU browser golden enforcement to include visible-compositor signal checks for ID-mask captures.
- 2026-06-01: expanded committed golden enforcement to non-square ID-mask city-id and neighborhood-id visualization captures.
- 2026-06-01: expanded WebGPU browser golden size enforcement to include the 640x360 app capture and visible-signal checks for app captures.
- 2026-06-01: expanded committed golden enforcement to non-square Scene3D viewport, camera preview, ID-mask beauty/seam, and browser WebGPU ID-mask captures.
- 2026-06-01: expanded committed golden enforcement to deterministic text-input IME composition, grapheme selection, and CJK fallback-font fixtures.
- 2026-06-01: expanded committed router-scene golden enforcement to snapshot/readback and synthetic camera scenes.
- 2026-06-01: expanded committed router-scene golden enforcement to elements-extended, orchestration, permissions, integration, and stress scenes at 800x600.
- 2026-06-01: expanded committed router-scene golden enforcement to animation timeline, input lab, nine-slice, SDF text, and animation-config scenes with visible-signal checks.
- 2026-06-01: expanded 2x committed renderer golden enforcement to ID-mask seams and added high-contrast seam-signal checks.
- 2026-06-01: added committed wide Scene3D material/cull and blend-mode golden enforcement.
- 2026-06-01: expanded 2x committed renderer golden enforcement to synthetic camera legacy NV12 and BGRA paths.
- 2026-06-01: expanded 2x committed renderer golden enforcement to Scene3D blend modes, synthetic camera blur/grayscale, and ID-mask neighborhood-id visualization.
- 2026-06-01: expanded committed renderer golden enforcement to cover the deterministic Damage Lab router scene.
- 2026-06-01: expanded committed renderer golden enforcement to cover 2x Scene3D viewport clipping, camera preview, and ID-mask compositor output.
- 2026-06-01: expanded committed renderer golden enforcement to cover Scene3D alpha/additive blend-mode behavior.
- 2026-06-01: expanded committed WebGPU browser golden size enforcement to include the 512x512 ID-mask compositor capture.
- 2026-05-31: expanded committed renderer golden enforcement to cover Scene3D material parameter and cull-mode behavior.
- 2026-05-31: expanded committed renderer golden enforcement to cover Scene3D viewport clipping and optimized synthetic camera tint/alpha compositing.
- 2026-05-31: expanded committed renderer golden enforcement to cover Scene3D depth stacking, blurred/grayscale synthetic camera preview, and ID-mask neighborhood/seam visualizations.
- 2026-05-31: added committed WebGPU browser golden presence and size enforcement.
- 2026-05-31: expanded committed renderer golden enforcement to cover Scene3D bloom, legacy/BGRA synthetic camera preview, and ID-mask city-id visualization.
- 2026-05-31: added committed renderer golden enforcement for Scene3D, synthetic camera preview, and ID-mask compositor outputs.
