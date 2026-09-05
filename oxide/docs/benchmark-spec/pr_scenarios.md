# oxide-benchmark-spec::pr_scenarios

## Intention and purpose

`pr_scenarios` freezes the first dashboard/feed/navigation vertical slice, the complete six-scenario PR product set, and the separate nightly endurance workload. It validates executable artifact identity, exact typed workload semantics, and timing/phase shape before any comparator adapter runs.

## Relation to the rest of the code

- Materialization creates the three scenario manifests and every referenced artifact beneath `benchmarks/comparative/specs/v1`.
- `validate_pr_vertical_slice` composes the generic scenario/artifact validator with the typed fixture validators and PR-specific identity, phase, viewport, font, timing, and tolerance rules.
- Apple and browser planners consume the six validated manifests; startup remains in the lifecycle acquisition while the five non-launch scenarios form the dynamic pack.

## Entry points list

- `PR_VERTICAL_SCENARIO_IDS` is the exact ordered three-scenario slice.
- `validate_pr_vertical_slice(spec_root, scenarios)` validates the ordered manifest paths and contracts.
- `validate_apple_pr_scenario_set(spec_root, scenarios)` validates all six Apple PR IDs, filenames, headline metrics, shared font pack, viewport, artifact closures, typed fixtures, and exact startup/chat/image phase contracts.
- `validate_nightly_endurance_scenario(spec_root, scenario)` validates the distinct `endurance.churn` identity, exact five-minute measured window, 100 open/close cycles, 500 tab switches, 600 animation frames, recovery, and dashboard-equivalent final checkpoints.

## Logic narrative

The validator requires filenames to match scenario IDs, exact primaries, the shared content-addressed Noto font pack, and a 390x844 phone portrait. The dynamic scenarios use an unmeasured two-second prewarm and exactly six measured seconds. Chat freezes prepend, append, typing, paste, and selection phases; image freezes separately attributable bytes-ready, decode, upload, first-visible, elapsed-time pan, and elapsed-time pinch phases. Startup freezes three lifecycle classes without misrepresenting the 15-second controller readiness timeout as measured phase duration. Every referenced byte and trace passes generic artifact validation before the matching typed fixture validator checks semantic content.

## Preconditions and postconditions

- Inputs are supplied in the frozen dashboard/feed/navigation order.
- The spec root exists and contains every referenced artifact.
- Success proves the vertical slice is structurally executable and budget-shaped; it does not prove an adapter, visual parity, or device performance.

## Edge cases and failure modes

Missing/reordered scenarios, renamed files, altered primary metrics or phases, startup lifecycle-class drift, missing dynamic durations, changed viewport/font/locale/tolerances, changed warmup, a dynamic measured duration other than six seconds, typed fixture drift, and any artifact/trace failure are rejected.

## Concurrency and memory behavior

Validation is synchronous planning-time file I/O and owns only small manifest structures. It runs outside all measured phases.

## Performance notes

The six-second rule is the conservative pre-calibration PR duration, not a performance observation. Density calibration may revise a future frozen design within the governing bounds before contender acquisition.

## Feature flags and cfgs

No feature or target cfg changes this contract.

## Testing and benchmarks

The materialized scenario integration tests validate all three vertical-slice manifests and all six PR manifests, canonical JSON bytes, complete artifact closures, complete layout text masks, opaque 1170x2532 sRGB checkpoint PNGs, and canonical reducer self-comparisons. They also freeze the dashboard's 38pt card/96pt backdrop geometry and navigation's 325,000 through 625,000 microsecond modal checkpoints. No runtime perf case is required for this off-window validator.

## Changelog

- 2026-07-21: added the exact five-minute `endurance.churn` nightly scenario and semantic trace validation.
- 2026-07-18: added canonical golden opacity, text-mask completeness, reducer self-comparison, dashboard geometry, and modal checkpoint assertions.
- 2026-07-18: added exact typed fixture validation to the frozen dashboard/feed/navigation PR vertical-slice validator.
- 2026-07-18: added the frozen dashboard/feed/navigation PR vertical-slice validator.
- 2026-07-18: promoted startup/chat/image to the exact typed six-scenario PR contract.
