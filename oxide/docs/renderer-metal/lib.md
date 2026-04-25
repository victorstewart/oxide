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
- `MetalRenderer::encode_pass(list)`
  - Encodes the existing 2D Oxide draw list and reuses the same frame command buffer when a scene3d pass already ran.

## Logic narrative

The renderer keeps long-lived GPU resources resident and reuses them across frames. Static textures and scene3d meshes are uploaded once, while frame-local rings handle transient 2D geometry and uniforms. The new scene3d path is intentionally small: position-only indexed meshes, per-instance transforms and colors, depth testing, and either triangle or line topology. That is enough for high-throughput globe-style geometry without expanding the public API prematurely.

Mixed 2D/3D frames share the same frame command buffer and color target. `encode_scene3d` initializes color/depth when needed, then `encode_pass` loads the already-rendered target instead of clearing it again. The supported ordering is 3D first, then 2D overlay, which matches the intended Oxide use case of a 3D scene under author-driven 2D interface chrome.

## Preconditions and postconditions

- Preconditions:
  - `MetalRenderer` must be resized before encode work.
  - `encode_scene3d()` currently requires `sample_count == 1`.
  - `encode_scene3d()` must run before `encode_pass()` within a frame.
- Postconditions:
  - Uploaded `MeshHandle3d` values stay valid until `mesh3d_release()` or renderer drop.
  - A mixed 3D/2D frame reuses one frame command buffer and one color target initialization path.

## Changelog

- 2026-04-23: Added the reusable retained `scene3d` mesh pass with depth buffering and same-frame 2D overlay interop.
