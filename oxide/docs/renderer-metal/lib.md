# oxide-renderer-metal::lib

## Intention and purpose

`oxide-renderer-metal` is Oxide's Metal backend. It owns the retained GPU resources, frame command buffers, pipeline state, texture management, and encode path that turn Oxide draw data into Metal work on Apple platforms.

## Relation to the rest of the code

- Upstream callers:
  - `oxide_render_metal::MetalRenderer` is constructed by Oxide hosts and by renderer-focused tests and perf cases.
  - Standalone Rust apps can also depend on this crate directly when they need the same Oxide Metal backend without the full UI stack.
- Downstream dependencies:
  - `oxide-renderer-api` provides draw-list and renderer traits.
  - The in-crate Metal shaders under `oxide/crates/renderer-metal/shaders/` back the pipeline state objects created here.

## Entry points list

- `MetalRenderer::new_with_config(config) -> Result<MetalRenderer, MetalInitError>`
  - Builds the Metal device/queue, shader library, pipeline state, and frame resources.
- `MetalRenderer::mesh3d_create(data) -> Result<MeshHandle3d, RenderError>`
  - Uploads a static indexed 3D mesh into persistent Metal buffers for reuse across frames.
- `MetalRenderer::encode_scene3d(pass) -> Result<(), RenderError>`
  - Encodes one retained 3D pass into the current frame before `encode_pass`.
- `MetalRenderer::encode_id_mask_gpu_compositor(pass) -> Result<(), RenderError>`
  - Rasterizes semantic region/subregion ID triangles into renderer-owned R8 targets before running the compositor shader. Implementation lives in `id_mask_gpu.rs`.
- `MetalRenderer::encode_neon_markers(pass) -> Result<(), RenderError>`
  - Encodes bounded neon marker instances over the current color target before `encode_pass`. Implementation lives in `neon_marker_gpu.rs`.
- `MetalRenderer::encode_pass(list)`
  - Encodes the existing 2D Oxide draw list and reuses the same frame command buffer when a scene3d pass already ran.

## Logic narrative

The renderer keeps long-lived GPU resources resident and reuses them across frames. Static textures and scene3d meshes are uploaded once, while frame-local rings handle transient 2D geometry and uniforms. The new scene3d path is intentionally small: position-only indexed meshes, per-instance transforms and colors, depth testing, and either triangle or line topology. That is enough for high-throughput globe-style geometry without expanding the public API prematurely.

Mixed 2D/3D frames share the same frame command buffer and color target. `encode_scene3d` initializes color/depth when needed, then `encode_pass` loads the already-rendered target instead of clearing it again. The supported ordering is 3D first, then 2D overlay, which matches the intended Oxide use case of a 3D scene under author-driven 2D interface chrome.

The 2D encoder validates local or rebased `u16` index spans before upload, then writes normalized indices directly into the frame-local Metal index ring for Solid and GlyphRun draws. This avoids allocating a temporary index `Vec` in the shared renderer hot path while preserving the existing local-index and absolute-index contracts.

Consecutive rounded rectangles are collected into retained scratch buffers and encoded through the instanced UI shader path. The batches preserve draw-list ordering while moving per-rectangle control overhead out of the Metal command stream, with payload chunks kept under Metal's `set*Bytes` limit.

The same retained-scratch discipline is used for the other small instanced UI batches: nine-slice images, argument-buffer images, spinners, backdrop composites, visual effects, and grouped glyph-run command metadata reuse renderer-owned buffers instead of allocating fresh temporary vectors on each encode.

Solid, image-mesh, text, and SDF text pipelines share the same API vertex descriptor because they all consume `oxide_renderer_api::Vertex` layout: position, UV, and normalized color packed at a 20-byte stride.

Inline layer fallbacks encode the original draw-list range directly. That keeps vertex and index spans valid without cloning the layer item slice or duplicating the full vertex/index arrays when a layer is rendered inline for prepass, unsupported commands, disabled layer caching, or a stale cache miss.

Damage prefiltering stays allocation-light. It now builds a compact temporary command list that borrows the original vertex and index backing arrays, so geometry-backed `Solid`, `GlyphRun`, and inline layer ranges can still be culled without cloning the full vertex/index payload just to discard off-scissor commands.

Layer texture sublists share one geometry-span offset/rebase helper for image meshes and glyph runs. That keeps local layer coordinates and rebased index spans consistent across the pre-render cache pass and the inline encode fallback path.

Renderer GPU timing is collected in-app instead of depending on Instruments hardware-counter availability. Completed frame command buffers update renderer stats from Metal's command-buffer GPU start/end timestamps, and iOS devices that expose the common timestamp counter set attach an `MTLCounterSampleBuffer` to the main 2D render pass for vertex/fragment/pass attribution. Those values are read after command-buffer completion and surfaced through `last_stats()` without waiting on the GPU from the frame hot path.

Frame-level camera/effect metadata is gathered in one draw-list scan. Camera coverage, camera-blur sigma, backdrop presence, and the strongest visual-effect blur plan are reused by the later policy and prepass blocks instead of rediscovering the same facts with separate passes.

Native camera preview commands are treated as compositor-plane markers. The Metal renderer uses them to keep the drawable clear alpha transparent for the frame and otherwise performs no camera-frame texture work, leaving preview presentation to the host layer below the Metal layer.

Synthetic camera benchmark textures keep the BGRA reference and optimized NV12 shader on the same BT.709 full-range contract. The optimized shader uses normalized chroma offsets directly, while the legacy shader intentionally preserves its older divergent full-range conversion so the snapshot benchmark can detect regressions against the BGRA reference.

Scene3D bloom uses the same persistent-object discipline: additive bloom PSOs are created once, bloom textures are reused across frames at a bounded downsample size, and `encode_scene3d()` routes `Pass3d::bloom` through the dedicated blur/composite encoder after the main 3D pass has initialized the target.

ID-mask composition is GPU-owned. Semantic region/subregion triangles are rasterized into private R8 render targets and then sampled by the compositor. The renderer keeps those render targets and the raster vertex upload buffer in the frame ring, so repeated mask composition does not allocate fresh textures and buffers every frame. ID-mask and neon-marker internals are split out of this file into focused renderer modules while keeping the public `MetalRenderer` API unchanged.

## Preconditions and postconditions

- Preconditions:
  - `MetalRenderer` must be resized before encode work.
  - `encode_scene3d()` currently requires `sample_count == 1`.
  - `encode_scene3d()` must run before `encode_pass()` within a frame.
  - ID-mask compositor dimensions must be non-zero, and GPU raster input must be a non-empty triangle list.
- Postconditions:
  - Uploaded `MeshHandle3d` values stay valid until `mesh3d_release()` or renderer drop.
  - A mixed 3D/2D frame reuses one frame command buffer and one color target initialization path.

## Changelog

- 2026-05-25: shared layer-sublist geometry offset/rebase handling between image meshes and glyph runs.
- 2026-05-25: reused the existing unindexed-vertex primitive selector for Metal image meshes so four-vertex quads encode as triangle strips instead of incomplete triangle lists.
- 2026-05-30: Aligned optimized full-range NV12 camera shader chroma handling with the BGRA benchmark reference.
- 2026-05-22: Shared the Metal API vertex descriptor across solid, image-mesh, text, and SDF text PSO setup.
- 2026-05-18: Compact ID-mask render-target reuse and shared the clear/store setup used by raster and field passes.
- 2026-05-15: made `NativeCameraPreview` a no-op Metal draw marker that requests transparent clear so host compositor camera layers can show through under Oxide UI.
- 2026-05-15: Shared overlay color-target attachment setup between ID-mask and neon-marker encoders.
- 2026-05-14: Shared scene3d mesh validation, buffer upload, and handle insertion between position-only and colored mesh uploads.
- 2026-04-23: Added the reusable retained `scene3d` mesh pass with depth buffering and same-frame 2D overlay interop.
- 2026-04-25: Removed the temporary normalized-index allocation from Solid and GlyphRun Metal uploads.
- 2026-04-25: Wired scene3d bloom PSO initialization and documented the dedicated bloom composite path.
- 2026-04-25: Batched consecutive rounded rectangles through the existing instanced UI shader path.
- 2026-04-25: Replaced inline layer fallback sublist cloning with range-aware draw-list encoding.
- 2026-04-25: Collapsed repeated frame metadata scans for camera and visual-effect decisions.
- 2026-04-25: Reused renderer-retained scratch across remaining small batch encode paths and made damage prefiltering borrow geometry backing storage instead of cloning it.
- 2026-04-26: Generalized in-app Metal GPU timing from camera direct preview to normal renderer frame submissions.
- 2026-04-30: Routed Scene3D bloom payloads through the offscreen blur/composite encoder and fixed the Scene3D material shader padding ABI.
- 2026-05-13: Rejected empty ID-mask dimensions and invalid GPU raster input before Metal texture work.
- 2026-05-13: Reused the shared scene3d matrix multiply and collapsed repeated Metal alpha/additive blend-state setup plus duplicate ID-mask halo scans.
- 2026-05-13: Removed the legacy CPU ID-mask upload path and pooled GPU ID-mask render targets plus raster vertex uploads in the renderer frame ring.
- 2026-05-13: Moved ID-mask GPU compositor and neon-marker encode internals into focused modules without changing the public renderer API.
