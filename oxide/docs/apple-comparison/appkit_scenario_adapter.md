# AppKit render-only diagnostic adapter

`host/apple-comparison/AppKit-macOS/AppKitScenarioAdapter.swift` is the legacy render-only AppKit diagnostic. It implements the common scenario, preview, raw-accessibility, quiescence, virtual-clock, and display-link contracts for all six frozen v1 PR scenarios, but it is not eligible as the `native.production` comparator because one custom `NSView` manually paints the scenes, applies model transitions directly, and synthesizes accessibility children.

The adapter remains useful for deterministic pixel attribution while the full-stack production reference is built. The controller's `ComparatorAcceptance` gate intentionally rejects authoritative measured acquisition against this implementation. It must ultimately move to a physically excluded diagnostic target so the accepted AppKit application cannot select it through a runtime flag.

The adapter verifies scenario, fixture, style, layout, asset-manifest, asset-byte, font-manifest, font-byte, and license identities through `BenchmarkSpecLoader` before installing a scene. It receives Latin, Arabic, and CJK fonts from the shared committed-file CoreText catalog, with every used point size and manifest variation axis prepared before the scene is installed. Named-face lookup and draw-time resizing are absent. Graphemes outside that pack use a prepared `AppKitPreparedInlineText` cache backed by every verified raster variant, not platform fallback. The closest physical-em variant is selected against the canonical 3x comparison scale, making 15- and 13-point attachments exact 45- and 39-pixel samples. It also rejects missing fixture fields, unexpected counts, wrong image dimensions, wrong gesture constants, and unsupported trace operations. A fixed AppKit `NSView` owns the 390x844 logical scene; preview capture uses an opaque sRGB Core Graphics bitmap at exactly 1170x2532 pixels.

## Scenario behavior

- Startup validates the 24 cards and 24 KiB payload, renders six initial cards and atlas images, and tracks foreground, background, fresh-install, visibility, and lifecycle state.
- Dashboard retains the 301-node semantic contract and applies leaf and 10% mutation counts while drawing the frozen 32-card, 64-image, 176-label, 24-control, four-backdrop composition.
- Feed retains all 2,000 variable-height fixture rows, computes visible role counts from the actual frozen row geometry, renders cached attributed image runs for pinned symbols/emoji, tracks pointer-derived scroll state and favorite selection, and preserves the visible anchor when the 20 frozen prepend identities are inserted.
- Chat retains all 5,000 initial messages, the 50 prepend messages, pinned avatar atlas, script-specific fonts and inline image runs, append sequence, composer text, UTF-8 focus range, and replacement semantics.
- Navigation applies the same list, detail, modal, dismiss, back, canonical-cycle, and interactive-cancel state machine as the common scenario trace.
- Image retains verified source bytes and thumbnail, decodes at the decode event, promotes the source at first-visible, and applies the shared one-pointer pan and two-pointer pinch state machine.

Reset restores seed state in place, retains prepared assets and fixture arrays, removes layer animations, and forces layout/display completion before another measured segment. Inline raster variants, selected cell crops, and attributed fixture strings are created once during preparation. Teardown releases the scene and prepared image resources. The adapter does not allocate platform controls or images inside its steady draw loop; event-driven state changes only invalidate the fixed scene view.

Core event synchronization is delta-based after preparation: feed favorite and prepend events use indexed row APIs, chat prepend/append/replacement events mutate only the affected native rows, and navigation transitions do not reinstall the immutable list. Image decode remains in the decode resource event; the upload event must successfully prepare the downsampled pixels and retained sRGB Metal texture before the model can enter its uploaded state. This ordering prevents first-visible presentation from absorbing resource preparation while preserving the frozen state and accessibility checkpoints.

The AppKit scene publishes a live `NSAccessibilityElement` hierarchy using explicit canonical-role-to-AppKit-role mappings. Correctness checkpoints retain that queried platform tree as `accessibility.platform.raw.json` and bind its hash separately from the normalized cross-framework fingerprint. The hierarchy is rebuilt only when role counts or focus state change, so repeated visual mutations with unchanged semantics do not add accessibility allocation work to the reference path.

## Diagnostic app boundary

The current `AppKitBenchMacOS` target embeds the same frozen `specs/v1` resource directory and opens a fixed-size macOS window around this diagnostic adapter. This is an explicitly rejected transitional state, not evidence that the target is a production AppKit baseline. The accepted target must install the full-stack scene roots documented in `appkit_production_reference.md` and exclude this file from its source graph.

The target runs the same platform-neutral packed campaign executor as the other Apple comparison apps. Campaign mode validates the bundled plan and all content identities, writes generation-qualified ready/complete artifacts, performs untimed correctness captures separately from timed passes, and auto-terminates after success or failure. macOS acquisition is launched out of process and does not use the iOS controller or device transport.

The shared checkpoint contract currently represents semantic role frames and counts but has no platform-neutral field for observed text-run geometry. AppKit therefore emits exact state/accessibility semantics, a separately retained raw AppKit tree, and deterministic pixels, but reducer-grade cross-platform text-geometry evidence remains pending that shared contract extension.

## Diagnostic verification

The `AppKitComparison` scheme currently runs the render-only adapter tests beside the production-reference architecture tests. The diagnostic tests prepare/reset/capture all six scenarios, replay every scheduled trace and compare every state and accessibility checkpoint to the frozen artifacts, prove feed visibility from fixture geometry, verify opaque 3x PNG dimensions, and prove the synthetic raw navigation tree contains one list plus twelve buttons. None of those facts admits the diagnostic as `native.production`.

```text
xcodebuild -project host/apple-comparison/AppleComparison.xcodeproj -scheme AppKitComparison -configuration Debug -destination 'platform=macOS,arch=arm64' test
xcodebuild -project host/apple-comparison/AppleComparison.xcodeproj -target AppKitBenchMacOS -configuration Release build
```
