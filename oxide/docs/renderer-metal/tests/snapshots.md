# oxide-renderer-metal::tests::snapshots

## Intention and purpose

The snapshot tests validate renderer behavior through readback instead of relying only on encode-time invariants. They exist to catch visible regressions in Metal output.

## Notable coverage

- Rounded-rect raster output and antialiasing.
- Clip-stack behavior.
- Invalid solid-mesh index rejection.
- Mixed `scene3d` plus 2D overlay composition in the same frame.
- Optimized NV12 camera preview parity against the synthetic BGRA benchmark reference.

The pure 2D tests assert Oxide's default opaque black Metal clear on untouched pixels. Snapshot-runner component goldens that need white backgrounds draw that background explicitly in their own draw lists.

## Mixed 2D/3D snapshot

`snapshot_scene3d_mixes_with_2d_overlay` proves the new retained 3D path composes correctly with the existing 2D draw-list path:

- a triangle mesh renders through `encode_scene3d()`
- a line mesh overlays it in the same 3D pass
- a 2D rounded rectangle still renders on top through `encode_pass()`
- untouched pixels preserve the scene clear color

That test is the renderer-level guardrail for the globe app's 3D-under-2D design.
