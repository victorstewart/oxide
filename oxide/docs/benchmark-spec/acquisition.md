# oxide-benchmark-spec::acquisition

## Intention and purpose

`acquisition` freezes the Apple PR lane expansion that sits between the tier budget and a build-specific `ComparisonPlan`. It makes the promised one-build, two-XCTest-method, four-controller-acquisition campaign and its occupied-time arithmetic machine-checkable before any device work starts.

## Relation to the rest of the code

- `benchmarks/comparative/specs/v1/acquisition/apple-pr.json` is the canonical expansion.
- The nine tier budgets remain the authoritative component and reserve ceilings.
- A build-specific comparison planner will attach executable, source, environment, trace, calibration, and comparator-audit identities without changing this acquisition shape.
- The Apple controller consumes the four frozen chunk identities after the build-specific plan has been bundled and signed.

## Entry points list

- `ApplePrAcquisitionSpec` owns the selected scenarios, public XCTest methods, pack budgets, controller chunks, build/install/pull counts, reserve, and hard total.
- `AcquisitionPackBudget` computes the bounded time for one side and the full paired pack campaign.
- `AcquisitionChunkBudget` binds a controller acquisition to one public XCTest method, pass, packs, pair indices, occupied-time cap, and durable pull requirement.
- `validate_apple_pr_acquisition` proves the exact PR expansion and reconciles it to the committed `apple-pr` budget.
- `load_apple_pr_acquisition` and `canonical_apple_pr_acquisition_json` provide the checked fixture boundary.

## Logic narrative

The non-launch pack contains dashboard, feed, chat, navigation, and image. Its three reset boundaries are explicit and ordered: `core-interaction` owns dashboard/chat/navigation, `scroll-damage` owns feed, and `media-text-warm` owns image. One side costs those three five-second resets plus five scenarios at one-second setup, two-second warmup, and six-second measurement, or 60 seconds. Four pairs and two sides therefore consume the frozen 480-second presentation component. The startup pack uses one isolated 15-second side with no warm reset segment, producing the 120-second launch component across four pairs.

The four controller chunks are correctness, presentation pairs 0–1, presentation pairs 2–3, and launch pairs 0–3. Every chunk is at most five occupied minutes and is followed by one durable host pull. Correctness (90 seconds), one install allowance (45 seconds), and four five-second pulls reconcile to the 155-second correctness/install/pull component. Adding the 151-second reserve produces the 906-second hard ceiling.

## Preconditions and postconditions

- The companion budget must be the canonical Apple PR budget identity.
- Validation succeeds only for one build-for-testing invocation, exactly two public controller methods, the six canonical PR scenarios, four pairs per pack, and four exact chunks.
- A successful expansion contains no separate lean replay and stays below both the five-minute chunk cap and twenty-minute lane cap.

## Edge cases and failure modes

Changing a scenario, pair index, method, pass, pack, component duration, pull count, reserve, or hard ceiling fails validation. Arithmetic uses checked operations. Duplicate controller chunk IDs fail. Build-specific identity or calibration omissions remain the responsibility of `ComparisonPlan` validation and prevent acquisition even when this tier expansion is valid.

## Concurrency and memory behavior

The schema and validator are immutable planning-time data. They perform no device I/O and allocate only small vectors and sets outside measured work.

## Performance notes

This contract bounds controller occupancy; it is not a measured result. The 480- and 120-second components are conservative caps and cannot be reported as performance observations.

## Feature flags and cfgs

No feature or target cfg changes the expansion.

## Testing and benchmarks

`tests/acquisition_tests.rs` validates the committed canonical JSON, exact budget reconciliation, rejection of a lean replay, and rejection of missing pair coverage. No runtime performance case is required because this module runs only during off-device planning.

## Changelog

- 2026-07-17: added the canonical Apple PR acquisition expansion and exact budget reconciliation.
