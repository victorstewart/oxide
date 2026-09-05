# `oxide-apple-comparison-controller` `src/trace.rs`

## Intention and purpose

This comparison-only module converts exported macOS Instruments XML into deterministic visual-generation-to-compositor-frame-lifetime evidence. It is not linked into Oxide, AppKit, or any production host.

## Relation to the rest of the code

`src/lib.rs` exports `os-signpost`, `hitches-updates`, and `hitches-frame-lifetimes` from the exact-process trace. This module parses those exports and returns `MacOsPresentationCorrelationArtifact`; the controller durably writes and hashes that JSON before a traced session can resume. Resume validation recomputes the artifact from the exports and checks every process string against the controller PID.

## Entry points list

- `correlate_macos_presentation_trace(signposts_xml, updates_xml, frame_lifetimes_xml)` parses and correlates the three required exported tables.
- `MacOsPresentationCorrelationArtifact` carries correlated rows, explicit uncorrelated visual generations, and calibration status.
- `MacOsPresentationCorrelation` binds a scenario/generation to input and visual markers, one process update, one display/swap ID, and one compositor frame-lifetime interval.
- `MacOsUncorrelatedVisualGeneration` preserves a visual generation superseded before a unique process update instead of silently excluding it.

## Logic narrative

The controller launches each `xctrace` process with a session-owned `TMPDIR`/`TMP`/`TEMP` under the durable session directory. It counts that isolated Instruments scratch tree together with the final `.trace` bundle throughout startup, recording, and finalization, and terminates the trace when the combined working set exceeds 512 MiB. Dropping the trace owner kills and waits for a still-running child before the scratch owner removes its directory, including ordinary error unwinding.

The XML reader uses schema mnemonics rather than display column names and resolves both top-level and nested `id`/`ref` values emitted by `xctrace`. Exact duplicate Hitches rows are normalized because Instruments may expose the same update at multiple containment levels.

For visual generation `g`, the executor contract assigns the originating input and display-opportunity generation `g - 1`. Input must exist exactly once and precede the visual marker. A same-scenario/generation display opportunity is the latest marker no later than input; it remains `null` when the executor emitted none for that generation.

The correlator first uses a unique exact-process update containing the visual marker. Otherwise it uses the earliest exact-process update after the marker. When another visual generation arrives before that candidate update, the earlier generation becomes an explicit `superseded-before-next-exact-process-update` row and the later generation remains eligible. The selected update must join to exactly one frame-lifetime row with the same display and swap ID. Candidate input/visual latency ends at the frame-lifetime interval end.

## Preconditions and postconditions

All three exports must contain their expected schema and at least one row. Marker subsystem/category/event type must be `com.oxide.comparison`, `Presentation`, and `Event`. Numeric timestamps, durations, generations, and swap IDs must parse without overflow. Success guarantees every emitted visual marker is either correlated or explicitly uncorrelated; it does not promote the candidate interval end to authoritative presentation.

## Edge cases and failure modes

Malformed XML, wrong schemas, unresolved references, generation zero, missing or duplicate input markers, reversed input/visual order, multiple containing updates, multiple different updates at the first eligible timestamp, missing frame lifetimes, duplicate non-identical swap joins, zero durations, and interval overflow fail closed. Missing per-generation display-opportunity markers are retained as `null` because current real trace evidence contains that condition.

## Concurrency and memory behavior

Parsing is synchronous in the host controller after trace finalization. Documents and typed rows are held in controller memory. No work or allocation is added to either measured application.

## Performance notes

The artifact availability is `available-correlated-frame-lifetime-calibration-pending`. The following gaps must be closed before Stage-2 authoritative presentation claims:

- prove, on the supported macOS/toolchain/display matrix, what the exported frame-lifetime end means and whether it equals or bounds visible/on-glass presentation;
- calibrate the inferred signpost-to-first-process-update edge because the export has no explicit signpost-to-swap ID;
- measure Instruments/signpost/Hitches capture overhead with matched traced and untraced campaigns;
- classify explicit superseded/uncorrelated generations as missed, coalesced, or otherwise presented using independent display evidence;
- make per-action display-opportunity coverage complete, or define and validate why `null` is acceptable;
- prove the same schemas and joins on complete Oxide and AppKit traces; the inspected AppKit bundle was incomplete and `xctrace export` returned `Document Missing Template Error`;
- extend the already trusted out-of-process `XCUIElement.click` launch probe to every non-launch interaction whose response latency is claim-bearing.

Existing Oxide evidence parsed successfully with 84 correlated rows, 121 explicit superseded/uncorrelated rows, and 15 correlated rows without a same-generation display-opportunity marker. These counts diagnose the parser and evidence gaps; they are not benchmark results.

## Feature flags and cfgs

None.

## Testing and benchmarks

`tests/trace_tests.rs` uses bounded XML fixtures. A read-only parser proof also succeeded against the existing exported Oxide trace without launching either application.

## Examples

The controller invokes the parser automatically after `xctrace export`; callers normally consume the durable `*.presentation.correlation.json` artifact.

## Changelog

- 2026-07-19: introduced comparison-only macOS marker/update/swap/frame-lifetime correlation and explicit superseded-generation evidence.
- 2026-07-20: isolated Instruments scratch per session, added a live 512 MiB bundle-plus-scratch fuse, and made scratch cleanup controller-owned.
- 2026-07-20: added behavioral regression coverage that drops a live trace stand-in and proves both process termination and scratch removal.
