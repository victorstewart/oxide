# oxide-renderer-metal::tests::snapshots

## Intention and purpose

The snapshot tests validate renderer behavior through readback instead of relying only on encode-time invariants. They exist to catch visible regressions in Metal output.

## Notable coverage

- Rounded-rect raster output and antialiasing.
- Clip-stack behavior.
- Invalid solid-mesh index rejection.
- Solid packed red/blue endpoints, midpoint interpolation, and byte-identical zero-rgba uniform inheritance.
- Mixed `scene3d` plus 2D overlay composition in the same frame.
- Optimized NV12 camera preview parity against the synthetic BGRA benchmark reference.
- Persistent prepared-snapshot parity across RRects, images, A8 glyphs, solids, clips, dynamic transform/opacity through the frame uniform ring, opaque fractional raster edges, one-dirty rebuilding, byte-budget eviction, resource generations, explicit purge, and flat fallback accounting.

The pure 2D tests assert Oxide's default opaque black Metal clear on untouched pixels. Snapshot-runner component goldens that need white backgrounds draw that background explicitly in their own draw lists.

## Mixed 2D/3D snapshot

`snapshot_scene3d_mixes_with_2d_overlay` proves the new retained 3D path composes correctly with the existing 2D draw-list path:

- a triangle mesh renders through `encode_scene3d()`
- a line mesh overlays it in the same 3D pass
- a 2D rounded rectangle still renders on top through `encode_pass()`
- untouched pixels preserve the scene clear color

That test is the renderer-level guardrail for the globe app's 3D-under-2D design.

## Solid vertex-color snapshot

`snapshot_solid_vertex_color_interpolates_and_zero_inherits_uniform` renders one packed red-to-blue quad and one zero-rgba green-uniform quad. It asserts both endpoints, the mixed midpoint, and exact interior BGRA `[0, 255, 0, 255]` for the legacy uniform path.

## Preconditions and postconditions

The test requires a readback-capable Metal target. Passing proves the existing single-draw solid path resolves zero before raster interpolation.

## Edge cases and failure modes

Unavailable Metal fails construction; mismatched colors report the observed pixel.

## Concurrency and memory behavior

The test submits bounded frames and uses synchronous readback only in the test path. The C26 property-ring case completes each submission while cycling every physical frame slot, then proves unchanged reuse uploads zero bytes and one changed record uploads exactly 48 bytes without relying on callback timing.

## Performance notes

Readback is verification-only and never enters the production frame loop. Prepared clean replay assertions require zero command traversal, geometry copy, buffer upload, and frame-ring VB/IB/UB use; the one-dirty case requires three hits, one miss, and only the changed chunk's upload.

## Feature flags and cfgs

The snapshot file requires `snapshot-tests` and macOS or physical iOS Metal support.

## Testing and benchmarks

Run the named test with `cargo test --locked -p oxide-renderer-metal --features snapshot-tests --test snapshots`.

## Changelog
- 2026-07-13: added C26 property-ring warmup, unchanged-zero-upload, and one-record-update coverage.

- 2026-07-13: added exact prepared-versus-flat mixed and fractional opaque pixels, dynamic-property reuse, one-dirty, LRU/purge, generation invalidation, and fallback-work coverage.
- 2026-07-12: added packed endpoint/interpolation and zero-rgba byte-identity coverage.
