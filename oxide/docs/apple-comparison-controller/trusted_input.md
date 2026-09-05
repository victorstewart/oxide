# Trusted macOS input preflight

## Intention and purpose

`trusted_input.rs` compiles benchmark trace operations into the subset of public macOS XCUI operations that the comparison controller can synthesize faithfully. It runs before a measured comparison process or collector starts, so an unsupported workload fails without producing partial or ineligible performance evidence.

The compiler is controller-only. It is not linked into Oxide production crates or either comparator application, and it adds no work to a measured frame or input path.

## Supported trusted-input commands

- A paired pointer down/up at one location becomes one click. An immediately following `navigate` event is treated as the declared consequence of that click and is not clicked a second time.
- A nonzero wheel event becomes one wheel command.
- A `commit-text` event targeting `chat:composer` becomes one text command.
- The adjacent frozen `focus(chat:append:16, 0:6)` and `commit-text(chat:append:16, Oxide)` events become one typed select/replace command covering both source indices.
- The exact 18-event image pinch trace becomes one typed, element-relative full-track drag on `image.zoom`; AppKit and Oxide map that real mouse input to the same 1x--2x fixture range.
- The exact five-event navigation cancellation trace becomes one typed application-relative drag whose mouse-up is the symmetric macOS cancellation boundary.
- Adjacent matching key-down/key-up events become one key-pair command.
- One pointer with zero or more moves followed by pointer-up becomes one duration-preserving single-pointer drag.
- Focus, navigation, and favorite mutations become clicks when their target is representable.

The plan retains source event indices, targets, coordinates, deltas, pointer identity, and drag duration. Session preflight also retains the exact ordered command list per scenario, allowing the controller to derive the expected global command count and identity sequence for that selected session pack.

## Declared application stimuli

Resize, orientation, theme, scale, non-favorite mutation, resource-arrival, and pressure operations are classified separately as application stimuli. They are not misreported as trusted OS input. The scenario application remains responsible for executing these declared state stimuli at the trace boundary.

## Fail-closed boundary

Preflight rejects input that public macOS XCUI cannot reproduce faithfully:

- pinch or overlapping multipointer input other than the exact frozen image-zoom substitution;
- pointer cancellation other than the exact frozen navigation duration-preserving mouse click-drag-release substitution;
- IME composition;
- any text selection target, range, replacement, or event adjacency other than the frozen public-XCUI-safe select/replace command;
- background or foreground transitions until the controller owns activation or suspension and persists receipts;
- unpaired pointer or key events, changed pointer identity, missing targets, coordinates, values, or drag duration;
- unsafe artifact paths or scenario/trace bytes whose SHA-256 differs from the plan binding.

These failures are capability findings, not timing results. A scenario remains ineligible until its required operation has a faithful trusted controller implementation.

## Campaign integration

For non-correctness, non-launch sessions, `run_or_resume_session` resolves the session's exact content-addressed scenarios, verifies every scenario and trace artifact, and compiles every phase before resolving or launching the comparator executable. This occurs before process creation, resource sampling, or Instruments collection.

After each Primary session completes, the controller requires exactly one canonical `request.json`, `controller.json`, and `application.json` for every preflight-derived global command sequence. It rejects missing or extra filenames, noncanonical JSON, identity or command drift, broken request/controller SHA-256 links, wrong dispatch path, wrong full trace-event range, wrong ordered raw event families/types, nonforeground or incomplete receipts, non-finite raw coordinates/timestamps, and state generations that do not strictly advance. Select/replace specifically requires mouse-down followed by key-down evidence. Request, controller dispatch, and application receipt timestamps must be ordered on `mach_continuous_time`. Generations may advance by more than one for multi-event commands such as text, remain monotonic within one scenario, and reset only at the next preflight scenario boundary.

Successful validation atomically persists `<side>.trusted-input.manifest.json`. Its artifact identity is relative to the campaign root and is included in the session result, atomic pair checkpoint, acquisition-validity report, and analyzer-side revalidation. Revalidation recomputes the manifest from the canonical raw triplets and never trusts a prior summary alone.

The raw request's `descriptorPreparedTimestamp` is the pre-start time at which the immutable command descriptor became durable. It is not an interaction-latency origin. Validation requires descriptor preparation to precede the controller's live `dispatchStartedTimestamp`; application delivery and controller completion must then follow in monotonic order. The Swift application and XCUI runner preload and validate request descriptors before the controlled start barrier, exchange typed dispatch receipts without scheduled filesystem/JSON/hash work, and persist controller/application receipts only in the post-measurement flush.

Correctness sessions remain untimed and launch sessions use their separate canonical launch interaction contract, so neither is routed through this trace compiler.

## Performance notes

Compilation is bounded by the selected scenario traces and occurs outside the measured application. Its allocations and JSON decoding cannot affect comparator CPU, GPU, memory, interaction, or presentation samples.

## Changelog

- 2026-07-21: added content-addressed trusted-input classification and preflight before measured macOS session startup.
- 2026-07-21: added exact Primary receipt-triplet validation and deterministic session manifests bound through acquisition validity.
- 2026-07-21: added the frozen typed select/replace command plus full event-range and ordered mouse/key raw-evidence validation.
- 2026-07-21: added exact typed image-pinch and navigation-cancellation substitutions backed by real public-XCUI mouse drags and `mouseDragged` application receipts.
- 2026-07-21: distinguished pre-start descriptor preparation from live dispatch timing and documented the preloaded, post-measurement-flush receipt path.
