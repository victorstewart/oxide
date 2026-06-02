# oxide-snapshot-runner `main.rs`

## Intention and purpose
- Render deterministic Oxide UI and renderer scenes into PNG files for visual regression testing.
- Compare rendered output against committed goldens and fail mismatches by default.
- Cover renderer-sensitive paths that unit assertions cannot fully protect, including mixed Scene3D, synthetic camera preview, ID-mask compositor output, and the browser WebGPU golden capture.

## Relation to the rest of the code
- Uses `oxide-renderer-metal` for local Metal-backed rendering and readback.
- Uses `oxide-ui-core` and `oxide-test-scenes` to build representative draw lists.
- Writes outputs under caller-provided paths such as `artifacts/snapshots` and compares against `goldens/snapshots`.

## Entry points list
- `oxide-snapshot-runner --suite static --component <name> --out <png> --golden <png>`
  Renders one component and compares it to a golden.
- `--allow-mismatch`
  Keeps mismatch diagnostics visible while allowing the command to exit successfully.
- `--pixel-tolerance`, `--max-error-tolerance`, `--mse-tolerance`
  Bound acceptable image drift for committed goldens.
- Component `scene3d_mixed`
  Encodes a Scene3D pass followed by a 2D overlay.
- Component `scene3d_bloom`
  Encodes a Scene3D emissive bloom pass plus a 2D overlay.
- Component `scene3d_depth_stack`
  Encodes overlapping depth-tested Scene3D triangles plus a 2D overlay.
- Component `scene3d_viewport_clip`
  Encodes an embedded Scene3D pass constrained by the Metal viewport/scissor path plus a 2D border.
- Component `scene3d_material_cull`
  Encodes Scene3D material parameters and cull-mode behavior in one deterministic pass.
- Component `scene3d_blend_modes`
  Encodes overlapping Scene3D alpha and additive blend-mode primitives with depth-disabled compositing.
- Scene3D committed goldens
  Include 1x, 2x scale, landscape, and portrait variants for mixed Scene3D, bloom, depth stacking, viewport clipping, material/cull, and blend modes to catch aspect/projection, overlay, depth, bloom, and render-state drift.
- Component `scene_damage`
  Renders the Damage Lab router scene with deterministic damage options and frame stats through the normal UI draw-list path.
- Components `scene_anim_timeline`, `scene_input_lab`, `scene_nine_slice`, `scene_sdf_text`, and `scene_animation_config`
  Render additional deterministic router scenes through the normal UI draw-list path. The Nine Slice scene is seeded with the same deterministic checker texture used by the zoom-image scene so the committed golden contains real slice/texture signal. The SDF text golden uses the bundled snapshot font and a 256x256 target so the high-size SDF glyph path has deterministic coverage.
- Components `scene_elements_extended`, `scene_orchestration`, `scene_permissions`, `scene_integration`, and `scene_stress`
  Render larger fixed-coordinate router demos at 800x600, matching the static sweep size so dense control groups are visible and not cropped out of the committed goldens.
- Components `scene_snapshot` and `scene_camera`
  Render the remaining special router scenes. `scene_snapshot` covers the readback-scene text path at 256x256, while `scene_camera` forces the Metal renderer's synthetic camera source and optimized NV12 mode before drawing the router-level camera scene at 384x288.
- Components `text_input_ime_composition`, `text_input_grapheme_selection`, and `text_input_fallback_cjk`
  Render deterministic text-input states for IME marked text, Unicode grapheme selection, and Latin-primary/CJK-fallback cursor-width coverage. These fixtures use the bundled snapshot font plus the vendored CJK fixture font so they do not depend on host-installed fonts.
- Component `camera_preview`
  Encodes the Oxide-owned synthetic camera background path.
- Components `camera_preview_legacy` and `camera_preview_bgra`
  Encode the same synthetic camera scene through the legacy NV12 and BGRA benchmark camera modes.
- Component `camera_preview_blur_gray`
  Encodes the optimized synthetic camera path with grayscale, tint, alpha, and blur enabled.
- Component `camera_preview_tint_alpha`
  Encodes the optimized synthetic camera path with tint and alpha compositing but no grayscale or blur.
- Camera committed goldens
  Include 1x, 2x scale, landscape, and portrait variants for optimized, legacy NV12, BGRA, blur/grayscale, and tint/alpha synthetic camera paths to catch crop/aspect and mode-specific composition drift.
- Component `id_mask_compositor`
  Encodes the Metal ID-mask compositor with deterministic city and neighborhood colors; the committed goldens include 2x scale, landscape, and portrait projection targets.
- Component `id_mask_compositor_city_ids`
  Encodes the Metal ID-mask compositor city-id visualization path without glow; the committed goldens include 2x scale, landscape, and portrait projection variants.
- Components `id_mask_compositor_neighborhood_ids` and `id_mask_compositor_seams`
  Encode the remaining ID-mask visualization modes without glow; the neighborhood-id and seam visualizations include 2x scale and non-square projection variants.
- WebGPU browser golden
  Is captured by `scripts/check_webgpu_browser_golden.mjs`; Rust tests enforce that the committed app, wide app, portrait app, square ID-mask compositor, wide ID-mask compositor, and portrait ID-mask compositor PNGs exist at the expected capture sizes with visible-signal checks, while browser execution remains an opt-in local check. The same script can also write `benchmarks/web/latest.json` and `benchmarks/web/latest.md` with the browser frame-loop and WebGPU ID-mask current/legacy A/B rows.

## Logic narrative
- Standard components are converted into a `DrawList`, encoded through `MetalRenderer::encode_pass`, submitted, and read back as BGRA.
- Direct renderer components bypass the draw-list-only path when the renderer API requires a dedicated encode call. The direct family now includes mixed Scene3D, Scene3D bloom, Scene3D depth stacking, Scene3D viewport clipping, Scene3D material/cull coverage, Scene3D blend-mode coverage, ID-mask beauty mode, and all three ID-mask visualization modes. Non-square direct goldens keep viewport, camera crop, ID-mask projection, and seam coordinates covered outside the square-target fast path. Router-scene components such as `scene_damage`, `scene_anim_timeline`, `scene_input_lab`, `scene_nine_slice`, `scene_sdf_text`, `scene_snapshot`, `scene_camera`, `scene_elements_extended`, `scene_animation_config`, `scene_orchestration`, `scene_permissions`, `scene_integration`, and `scene_stress` intentionally use the ordinary UI draw-list path so scene labels, panels, seeded images, text, camera background, and stats are covered without a renderer-only shim. Text-input fixture components use the ordinary UI element encode path with vendored fonts to cover IME composition, grapheme selection, fallback-font shaping, glyph atlas publication, selection, composition underline, and caret rendering. The snapshot runner disables the router HUD overlay for these scene goldens so committed images cover scene content rather than debug chrome.
- Browser WebGPU execution can vary by a few pixels between Chrome/ANGLE runs, so the Rust test gates committed WebGPU PNG dimensions plus app and ID-mask visible-scene signal while exact browser recapture remains an explicit local check through the script.
- The runner writes the current PNG output, then compares it with the requested golden if one exists.
- Missing goldens are created by the runner, while integration tests assert committed required goldens already exist.
- If no explicit snapshot font path or resource font is present, the runner loads `crates/ui-core/assets/Asap-Regular.ttf` as a deterministic bundled fallback for text-bearing snapshots. CJK text-input fixtures additionally register `crates/text/tests/fixtures/test_text_cjk.ttf` as a fallback font.

## Preconditions and postconditions
- The host must support the local Metal renderer for image generation.
- A successful strict run means dimensions and pixel tolerances match the committed golden.
- A successful `--allow-mismatch` run still prints mismatch diagnostics.

## Edge cases and failure modes
- Size mismatch fails unless `--allow-mismatch` is explicit.
- Pixel mismatch fails unless all configured tolerances are satisfied or `--allow-mismatch` is explicit.
- Unknown components render an empty stable surface and print a diagnostic.

## Concurrency and memory behavior
- Each run owns one `MetalRenderer` instance.
- Direct components create transient mesh or compositor data for the single frame. Synthetic camera components use the same draw-list camera command but switch camera render mode before encoding.
- The runner writes PNGs through buffered file output.

## Performance notes
- The runner is correctness-first and not a timing benchmark.
- Direct renderer components keep Scene3D and ID-mask passes out of fake draw-list shims, so snapshots exercise the real encode path.

## Feature flags and cfgs
- No feature-specific runner behavior.

## Testing and benchmarks
- `crates/snapshot-runner/tests/golden_mismatch_tests.rs` covers mismatch failure, explicit mismatch allowance, committed renderer goldens including 1x/2x/landscape/portrait Scene3D mixed/bloom/depth/viewport/material-cull/blend variants with colored-geometry signal checks, Damage Lab plus additional router-scene output including SDF text, snapshot/readback, camera, and extended fixed-coordinate scenes, deterministic text-input IME/grapheme/CJK-fallback fixtures, 1x/2x/landscape/portrait optimized/legacy/BGRA/blur/tint-alpha camera paths with visible-preview signal checks, ID-mask visualization variants, 2x/landscape/portrait ID-mask compositor/city/neighborhood/seam output, colored-region ID-mask signal checks, high-contrast seam-signal enforcement, router-scene visible-signal enforcement, and committed WebGPU browser app plus square/wide/portrait ID-mask golden presence and visible-signal enforcement.
- `crates/renderer-metal/tests/snapshots.rs` covers lower-level Metal readback invariants for the same renderer families.

## Examples
```rust
cargo run --locked -p oxide-snapshot-runner -- --suite static --component scene3d_mixed --width 192 --height 192 --scale 1 --out artifacts/snapshots/scene3d_mixed.png --golden goldens/snapshots/scene3d_mixed.png
```

## Changelog
- 2026-06-01: added committed portrait Metal Scene3D, synthetic camera, and ID-mask goldens with visible-signal enforcement.
- 2026-06-01: added committed portrait WebGPU app and ID-mask browser goldens at 360x640 with size and visible-signal enforcement.
- 2026-06-01: added colored-region signal enforcement for committed Metal ID-mask beauty, city-id, and neighborhood-id goldens.
- 2026-06-01: added colored-geometry signal enforcement for committed Scene3D goldens.
- 2026-06-01: added non-square committed Scene3D mixed, bloom, and depth-stack goldens.
- 2026-06-01: added non-square committed synthetic camera preview goldens for legacy NV12, BGRA, blur/grayscale, and tint/alpha modes, with visible-signal enforcement across camera goldens.
- 2026-06-01: added visible-signal enforcement for committed browser WebGPU ID-mask goldens.
- 2026-06-01: added non-square committed ID-mask city-id and neighborhood-id visualization goldens.
- 2026-06-01: added a committed wide WebGPU app golden at 640x360 with size and visible-signal enforcement.
- 2026-06-01: added committed wide Scene3D material/cull and blend-mode goldens at 320x192.
- 2026-06-01: added non-square committed goldens for Scene3D viewport clipping, optimized camera preview, ID-mask beauty/seam paths, and browser WebGPU ID-mask capture.
- 2026-06-01: added deterministic text-input snapshot fixtures for IME composition, grapheme selection, and CJK fallback-font cursor-width coverage.
- 2026-06-01: added committed router-level snapshot/readback and synthetic camera scene goldens.
- 2026-06-01: expanded committed router-scene goldens to elements-extended, orchestration, permissions, integration, and stress scenes at 800x600.
- 2026-06-01: added a bundled snapshot font fallback, disabled router HUD overlay for scene snapshots, and added committed router-scene goldens for animation timeline, input lab, nine-slice, SDF text, and animation-config scenes, including visible-signal enforcement.
- 2026-06-01: strengthened ID-mask seam snapshot topology and added 2x seam golden enforcement.
- 2026-06-01: expanded 2x committed renderer golden enforcement to synthetic camera legacy NV12 and BGRA paths.
- 2026-06-01: added committed 2x-scale renderer goldens for Scene3D blend modes, synthetic camera blur/grayscale, and ID-mask neighborhood-id visualization.
- 2026-06-01: added a deterministic Damage Lab router-scene component and committed golden so damage-scene visuals are covered by the macOS Metal snapshot contract.
- 2026-06-01: added committed 2x-scale renderer goldens for Scene3D viewport clipping, synthetic camera preview, and ID-mask compositor output.
- 2026-06-01: added committed Scene3D blend-mode snapshot coverage for alpha/additive no-depth composition.
- 2026-06-01: expanded committed WebGPU browser golden enforcement to include the 512x512 ID-mask compositor capture.
- 2026-05-31: added a committed variant golden for Scene3D material parameter and cull-mode behavior.
- 2026-05-31: added committed variant goldens for Scene3D viewport clipping and optimized synthetic camera tint/alpha compositing.
- 2026-05-31: added committed variant goldens for Scene3D depth stacking, blurred/grayscale synthetic camera preview, and ID-mask neighborhood/seam visualizations.
- 2026-05-31: documented and enforced the committed WebGPU browser golden presence/size contract.
- 2026-05-31: documented browser WebGPU baseline writes from the golden recapture script.
- 2026-05-31: added committed variant goldens for Scene3D bloom, legacy/BGRA synthetic camera preview, and ID-mask city-id visualization.
- 2026-05-31: added direct Scene3D, camera preview, and ID-mask compositor snapshot components for committed Metal-backed goldens.
