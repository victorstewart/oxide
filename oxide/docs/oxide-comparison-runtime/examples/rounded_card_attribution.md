# oxide-comparison-runtime / examples / rounded_card_attribution

## Intention and purpose

This comparison-only command folds the 32 canonical `dashboard.mixed-static` cards onto one device-pixel coordinate system and attributes exact AppKit-versus-Oxide RGB differences to rounded-surface and shifted-shadow composition. It is diagnostic evidence for evaluating analytic shader candidates; it cannot accept a static checkpoint, modify a capture, or participate in a production application.

## Relation to the rest of the code

The command belongs to the isolated `oxide-comparison-runtime` package. It reads already-acquired AppKit and Oxide PNGs, reconstructs the frozen 390×844 dashboard geometry at the supplied integral scale, excludes image, label, and control content, and emits JSON to stdout. It does not depend on or alter production renderer code.

The four reported underlays are:

- `background`;
- `material_over_background`;
- `shadow_over_background`;
- `shadow_over_material_over_background`.

Each mismatch also records one of `surface_over_base`, `surface_over_shadow`, `shadow_only`, or `outside_analytic_support`. The last role exposes pixels reached by native antialiasing but outside the current half-pixel analytic support model.

## Entry points

- `rounded_card_attribution::main() -> ExitCode`: parses capture paths, validates the canonical dimensions, performs exact RGB attribution, and writes one compact JSON object.

Run the v22 evidence without launching either application:

```sh
cargo run --locked -p oxide-comparison-runtime --example rounded_card_attribution -- \
  --native /private/tmp/oxide-mac-v22-combined.27wZO1/native/dashboard.mixed-static/idle/screenshot.actual.png \
  --oxide /private/tmp/oxide-mac-v22-combined.27wZO1/oxide/dashboard.mixed-static/idle/screenshot.actual.png \
  --scale 3 > /private/tmp/oxide-mac-v22-rounded-card-attribution.json
```

## Logic narrative

The command decodes opaque PNG input to RGB8 and rejects images that differ in dimensions or do not equal `390×scale` by `844×scale`. It first records exact full-frame differences. For every canonical card, it then visits the card plus the two-point shifted-shadow extent, excludes content-owned rectangles, and evaluates the current branchless rounded-rectangle signed-distance coverage at each device-pixel center.

Material membership and nonzero analytic shadow coverage select one of the four underlays. Surface and shadow coverage select the edge role. Mismatching pixels are folded to card-relative device coordinates, preserving the number of matching card occurrences, the signed native-minus-Oxide RGB sum, absolute channel-delta histogram, and packed native/Oxide color-pair populations. This is enough to distinguish a repeated coverage-ramp error from background-dependent two-stage composition without storing 32 redundant card images.

`relative_position_repetition` folds those class-specific rows once more by device-pixel coordinate. Each row records how many cards differ at that coordinate, how many one-card positions have that repetition count, and their total contribution to the residue.

## Preconditions and postconditions

Both inputs must be opaque, same-sized dashboard captures at a positive integral scale. Geometry is intentionally frozen to the version-one dashboard contract. Successful output is deterministic for identical PNG bytes and arguments because all attribution maps use sorted keys.

The output is not a visual acceptance result. Static parity continues to require zero differing RGB channels across the full normalized frame.

## Concurrency and memory behavior

The command is single-threaded. PNGs and decoded RGB planes are held in memory once. Relative output stores only mismatching samples rather than full per-card pixel planes.

## Error handling

Malformed arguments, PNG decode failures, non-opaque captures, dimension mismatches, and output failures return a nonzero exit status with a concise stderr diagnostic. No partial JSON object is emitted before validation completes.

## Testing and benchmarks

Focused example tests cover analytic inside/edge/outside samples, presence of all four underlays in canonical geometry, signed channel-delta accounting, and exclusion of labels, icons, and controls:

```sh
cargo test --locked -p oxide-comparison-runtime --example rounded_card_attribution
```

This is offline diagnostic tooling, so it has no production performance row. Any renderer candidate informed by the report still requires unchanged-workload visual proof and balanced p50/p95/p99 CPU/GPU A/B evidence.

## Changelog

- 2026-07-19: added deterministic one-card rounded-surface/shadow attribution for paired macOS dashboard captures.
