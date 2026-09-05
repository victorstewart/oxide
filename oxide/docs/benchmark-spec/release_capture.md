# oxide-benchmark-spec release_capture

## Intention and purpose

`release_capture.rs` defines the hash-bound, untimed macOS bootstrap plan for capturing the five screenshot-blocked release candidates. It keeps screenshot-free inputs outside normal runnable `ScenarioSpec` admission.

## Relation to the rest of the code

The plan binds the exact five candidate documents and all 18 checkpoint identifiers. The macOS comparison controller validates the plan and Release build identity, launches the AppKit and Oxide comparison applications through their explicit capture-only mode, and supplies the paired evidence root to `release_promotion`.

## Entry points list

- `load_release_candidate_capture_plan`
- `validate_release_candidate_capture_plan`
- `canonical_release_candidate_capture_plan_json`
- `ReleaseCandidateCapturePlan`

## Preconditions and postconditions

The plan must be canonical JSON, use the frozen candidate order, bind every candidate file by SHA-256, bind exactly 18 checkpoint identifiers, declare no timing claim, and reference candidates that still contain no screenshots. Validation never promotes or measures a candidate.

## Performance notes

This is benchmark control-plane code. It does not execute in a production or measured frame path.

## Testing

`tests/release_capture_tests.rs` proves the committed plan identity, complete candidate/checkpoint matrix, and fail-closed order and timing-claim checks.

## Changelog

- 2026-07-21: added the explicit screenshot-bootstrap capture plan.
