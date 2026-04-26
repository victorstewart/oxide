# oxide-renderer-metal::scene3d

## Intention and purpose

`scene3d` defines the small public author-facing 3D API used by the Metal backend. It exists to let Oxide callers upload static meshes and submit simple retained 3D passes without exposing renderer-internal Metal details.

## Public data model

- `Mat4`
  - Column-major 4x4 matrix layout matching the Metal shader contract.
- `Vertex3d`
  - Position-only vertex used for the current 3D path.
- `Mesh3dData`
  - Borrowed upload payload describing vertices, indices, and topology.
- `Instance3d`
  - One draw instance with transform, color, culling, depth flags, blend mode, material id, and material parameters.
- `BloomLayer3d`
  - One emissive bloom layer with a blur sigma and composite strength.
- `Bloom3d`
  - Optional bloom payload containing emissive instances, bloom layers, and the downsample divisor for the offscreen bloom textures.
- `Pass3d`
  - One frame-local 3D pass with an optional clear color, depth-clear flag, view-projection matrix, instance slice, and optional bloom payload.

## Design notes

The API is intentionally narrow for performance and surface-area control:

- Static mesh data is uploaded once and reused by handle.
- Per-frame changes are limited to transforms, colors, and pass parameters.
- There is no CPU-side tessellation or per-frame reprojection in the hot path.
- Only triangle and line topologies are exposed because they cover solid land fills plus wire/grid overlays efficiently.
- Bloom is encoded as a bounded offscreen emissive pass: draw emissive instances into a downsampled RGBA16Float target, run separable blur, then additively composite back into the main scene target.

This is the right shape for the standalone globe app: an ellipsoid depth shell, a triangulated land mesh, and line meshes for borders and grid.

## Changelog

- 2026-04-23: Added the initial retained mesh API used by the standalone globe study and the authoring perf contract.
- 2026-04-25: Documented scene3d material, additive blend, and bounded bloom payloads.
