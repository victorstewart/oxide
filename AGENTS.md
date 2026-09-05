# Oxide repository instructions

This is a Rust workspace. The current task, global defaults, and this file are
the automatic instruction set; the retired `GPT5_RULES.md` profile adds no
workflow, documentation, test, or performance obligations.

## Formatting

- Never run `cargo fmt`, rustfmt, `cargo clippy`, or another bulk formatter.
  Oxide's manual style cannot be expressed by rustfmt.
- Use three-space indentation, Allman braces, and one-line function and impl
  signatures. Preserve the established import order (standard library, external
  crates, then crate-local imports), grouping, wrapping, and local style.
- Make surgical edits and review the final diff for formatter churn.
- Address a deprecation only when the task or touched API makes it relevant; an
  unrelated warning is a follow-up, not automatic scope.

## Ownership and platform boundaries

- Rust/Oxide owns product state, layout, input interpretation, rendering policy,
  and lifecycle. UIKit, Web, Objective-C, JavaScript, and WASM hosts are the
  smallest platform adapters and must not duplicate those semantics.
- Shared touch comprehension belongs in `oxide-input`. On iOS, the default path
  is an Oxide-owned multi-touch `UIWindow` that forwards every
  `UIEvent.allTouches` identity, phase, coordinate, and timestamp into Rust.
  View-level touch callbacks are fallback diagnostics, not the primary path when
  the window-level stream is available. Inspect the proven standalone `globe`
  host and Rust input model before inventing new iOS gesture plumbing.
- UIKit recognizers may be diagnostic or input-adapter fallbacks only when an
  environment cannot expose raw touch. They must forward measurements into Rust
  immediately and must not own product gesture state, transforms, physics, or
  rendering. Simulator pointer adapters must accept `UITouchTypeIndirectPointer`
  and `UIScrollTypeMaskAll` where applicable.
- For one-finger drag plus two-finger pinch, preserve the established transition:
  cancel drag when pinch begins, derive pinch from the two-touch distance ratio,
  and restart drag from the remaining touch when pinch ends.
- Verify a changed gesture through the real affected host path. Unit-state or
  synthetic environment toggles alone do not prove OS event delivery; use OS-level
  gestures or manual input and capture visible before/after behavior.
- Keep touch diagnostics file-backed and explicitly gated; do not add shipping
  overlays or logging to make automation pass.

## Unsupported accessibility

- Oxide intentionally does not implement or expose OS accessibility trees,
  VoiceOver elements, accessibility roles/actions, dynamic-type policy, or an
  accessibility bridge on any platform.
- Do not add accessibility as a product requirement, authoring recommendation,
  comparison workload, parity criterion, admission gate, benchmark metric, or
  optimization constraint. Do not introduce accessibility-named public API,
  dirty state, or compatibility slots.
- Do not add platform accessibility or automation identifiers. Harnesses should
  use app/window queries, raw coordinates, and non-accessibility lifecycle
  signals. Do not expand legacy identifier-based controls; remove them only when
  their users can migrate atomically.

## Renderer and camera invariants

- Keep the warm frame loop free of avoidable allocation, pipeline/shader
  compilation, resource construction, string formatting, logging, and CPU waits.
  Reuse persistent GPU objects and in-flight-safe buffer/storage ownership.
- Preserve visual equivalence, ordering, clipping, blending, color, text, input,
  resource lifetime, and device-loss behavior. A faster path that draws less or
  changes semantics is not a valid optimization.
- Keep CPU/GPU overlap and deliberate Metal load/store/storage choices. Add heaps,
  argument buffers, indirect commands, caches, atlases, or extra render passes
  only for a measured benefit on the named workload.
- Oxide owns visible camera-preview rendering, composition, pacing, and
  presentation after the minimal iOS acquisition boundary. A system-managed
  visible preview layer is diagnostic/benchmark-only, not a product path, unless
  an explicitly authorized product revision changes this contract.
- Keep app-owned rendering distinct from system-owned UI such as keyboards,
  picker sheets, maps, web/video surfaces, and permission alerts. System-owned UI
  is bridge overhead, not an Oxide renderer win.

## Performance work

- Ordinary fixes and features do not trigger a renderer-wide research review,
  exhaustive benchmark matrix, UIKit parity campaign, or physical-device seal.
- For an explicit performance task or a change to a measured hot path, define one
  workload, baseline/artifact identity, primary metric, target, sample protocol,
  correctness invariants, and finite mechanism budget before optimizing.
- Add or update the smallest benchmark that exercises the changed hot path. Do not
  add one perf case per acceptance row or expand the canonical battery with dense
  near-duplicates. Prefer explicit touched-case runs.
- Official Oxide/UIKit comparison claims must use equivalent scene specs and
  same-device evidence. A result is invalid if one side draws less, changes
  quality, or uses different state, geometry, cache state, refresh mode, or input.
- Keep cold, warm, and hot cache states separate. Report distributions and the
  user-visible metrics relevant to the named flow; do not require every possible
  metric for unrelated changes.
- If physical hardware, Metal counters, energy traces, or a host runner are
  unavailable, mark only that claim `Verification Pending`. Do not continue an
  unbounded audit or block independent scoped correctness work.
- A performance phase may evaluate at most three prioritized mechanisms before it
  returns the measured win, negative result, or blocker. Do not use `repeat until
  win` as a contract.

## CI and device evidence

- GitHub Actions CI is intentionally disabled. Run focused local checks for the
  touched scope; do not imply that hosted CI ran.
- Official Oxide/UIKit comparison numbers are physical-device-only. Run the
  touched Oxide and UIKit cases on the same iPhone and record them through
  `oxide/benchmarks/oxide-device/*` and
  `oxide/benchmarks/uikit-device/*`. Simulator and workspace measurements are
  diagnostic and must not appear in official comparison tables.
- For scoped device performance work, use in-app completed-command-buffer Metal
  timing as the basic GPU source, retain supported pass/counter evidence when it
  answers the active question, and scope external Metal traces to the launched
  app process. Never substitute a proxy or unsupported all-process trace for
  missing direct GPU or energy evidence.
- Refresh committed device baselines only when the reviewed performance change
  intentionally affects the touched cases. Missing hardware, counters, or energy
  evidence makes only the corresponding claim `Verification Pending`.
- Official scroll and animation evidence uses real ProMotion hardware at native
  refresh. A separate 60 Hz study is opt-in diagnostic work.

<!-- OXIDE CODEX SKILL ROUTING START -->
## Codex skill routing

Repo-scoped skills live under `.agents/skills/`; use them when the task matches
their trigger descriptions.

- Use `$oxide-internals-performance` for changes or reviews involving Oxide
  runtime internals, including `ui-core`, renderer backends/APIs, text, input,
  timing, platform hosts, frame scheduling, damage redraw, Scene3D, snapshots,
  benchmarks, Metal, GPU profiling, or performance-sensitive architecture.
- Use `$oxide-app-authoring-performance` for applications, demos, examples,
  documentation, sample screens, and app-authoring performance advice when the
  work does not modify Oxide internals.
- If app work crosses into internals, switch to the internals skill before that
  edit. If internals work adds public usage examples, also consult the authoring
  skill for that slice. Skill use does not broaden the task into unrelated
  research, auditing, benchmarking, or documentation.

Repository rules remain authoritative over skill guidance.
<!-- OXIDE CODEX SKILL ROUTING END -->

## Verification and handoff

- Run focused checks for the touched crates and host path. Broaden to the workspace
  or device battery only when the frozen acceptance gates or a release claim
  requires it.
- State exactly which evidence is observed and which affected claim remains
  pending. Missing global performance coverage is not `Remaining Work` for a
  bounded task.
