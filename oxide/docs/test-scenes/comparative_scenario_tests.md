# oxide-test-scenes::tests::comparative_scenario_tests

## Intention and purpose

These integration tests prove the real Oxide scene adapters consume all six committed PR fixtures/traces and all five frozen release-candidate fixtures/traces with exact semantic state, bounded visible work, and reset-compatible state.

## Relation to the rest of the code

- Loads and validates committed scenarios through `oxide-benchmark-spec`.
- Drives scenes only through public `Router` comparison methods.
- Uses a null glyph uploader and sentinel image handles to inspect semantic draw-list output without a renderer shortcut.

## Entry points list

The test binary exports no public API. Cargo invokes cases covering initial six-scenario contracts, the nightly idle contract, dashboard/feed mutations, navigation timing, startup/chat mutation bounds, image resource/gesture behavior, complete checkpoint replay, and semantic-fault observability.

## Logic narrative

The initial case prepares every scenario at 390x844, binds pinned resources with distinct thumbnail and inline-text variant sentinel handles, compares role counts to the manifest, and checks exact image work/source cells. Dashboard must emit its 24 hard controls as one accent-colored `Solid` command with 96 vertices and 144 indices and no matching zero-radius `RRect`. The idle case reuses that complete dashboard output byte-for-byte, rejects logical mutation, and reports no continuing animation request. Feed must emit nine benchmark-owned thumbnail meshes plus the four declared inline image runs; chat must emit ten avatar meshes plus its visible pinned graphemes. Canonical 15-point inline draws select the 45-pixel variant, use exact 45x45 grid cells, have physical-pixel-aligned destination origins, remain wholly inside the feed title line, occur inside a balanced label clip, and never masquerade as thumbnail draws. Feed thumbnail mesh counts remain exact after every canonical scroll, favorite, and prepend phase. Mutation cases replay committed traces in phase order and redraw after each phase. The image case first validates committed PNG IHDR dimensions, rejects a mismatched SHA-256 handoff, walks the four ordered resource stages, proves thumbnail-to-source handle cutover, and compares destination geometry after pan and 2x pinch. Reprepare checks reset behavior. Checkpoint replay merges trace events and checkpoints by scheduler timestamp and compares all 32 runtime state/accessibility snapshots to the committed artifacts. The fault case proves missing mutation, active focus, and background visibility remain observable.

The release case uses the explicit identity-only scene preparation API after asserting the candidates remain non-loadable as runnable manifests. It decodes their strict fixtures, replays every grid/effects/mutation/text/resize trace at the frozen checkpoint boundary, and compares all 18 runtime state/accessibility JSON objects with their hash-bound candidate artifacts. It also requires every resulting scene to emit nonempty standard draw-list work and rejects an unknown identity.

The macOS XCUI geometry case proves the live Rust tree exposes every supported controller target in the state where it is actionable. The feed favorite target is a genuinely visible row at the established 150,000-millionths scroll position; navigation's host-only modal-action alias is added by the AppKit shell without changing shared semantic geometry.

## Preconditions and postconditions

The committed benchmark-spec v1 tree exists beneath the workspace. Success proves adapter state and draw-list contracts; it does not prove Metal pixel parity or physical-device performance.

## Edge cases and failure modes

Stale artifacts, invalid role counts, unbounded chat/avatar draws, substituted image handles, wrong source rectangles, hash mismatches, rejected canonical events, incorrect elapsed transform geometry, or state leakage across prepare fail the suite.

## Concurrency and memory behavior

Each test owns its router. Committed assets are read-only. The null uploader holds no resources, and tests do not share mutable state.

## Performance notes

Assertions cap feed at nine rows and chat at ten rows after every mutation. This is structural performance evidence, not timing evidence; device campaigns own latency and resource distributions.

## Feature flags and cfgs

No feature flag or target cfg changes the tests.

## Testing and benchmarks

Run `cargo test --locked -p oxide-test-scenes --features comparison-geometry --test comparative_scenario_tests` or the complete crate suite with the same feature.

## Examples

The tests show the required host sequence: validate, prepare, bind shared resources, bind hash-matched standalone images when applicable, apply traces, draw, and checkpoint roles.

## Changelog

- 2026-07-26: required all initial feed inline-image destinations to stay inside the canonical title-line bounds.
- 2026-07-21: added exact semantic replay coverage for all 18 checkpoints across the five release candidates.
- 2026-07-21: replaced partial synthetic release scenario shells with the explicit identity-only capture preparation API and unknown-ID rejection coverage.
- 2026-07-21: added live Rust-geometry coverage for every supported macOS XCUI target and corrected the favorite workload to visible row 300.
- 2026-07-21: added full-dashboard idle reuse, zero-logical-work rejection, and quiescent-frame assertions.
- 2026-07-19: froze the single-batch solid geometry and absence of zero-radius rounded commands for dashboard controls.
- 2026-07-18: asserted retained inline-text raster selection, counts, exact source cells, clipping, and physical-pixel-aligned destinations in feed and chat.
- 2026-07-18: added exact canonical feed-thumbnail radius assertions across initial and traced states.
- 2026-07-18: expanded coverage to startup, chat, and image, including reset and exact image resource handoff.
- 2026-07-18: added dashboard, feed, and navigation vertical-slice coverage.
