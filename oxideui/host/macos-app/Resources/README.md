Resources (bundled into .app)

Place runtime assets under this folder so they are copied into the macOS app bundle at
`Contents/Resources/` when using `cargo bundle`.

Expected paths used by the app:
- fonts/Inter-Regular.ttf
- images/sample.png

Notes
- If `images/sample.png` is missing, the Zoom Image scene falls back to a procedural checkerboard.
- If `fonts/Inter-Regular.ttf` is missing, text will not render (labels/overlay). Bundle the font
  for full text rendering.
