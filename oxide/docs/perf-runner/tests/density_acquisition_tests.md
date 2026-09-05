# oxide-perf-runner::tests::density_acquisition_tests

## Intention and purpose

These public-API tests prove that macOS density calibration is an acquisition protocol rather than only a reducer.

## Relation to the rest of the code

The tests inject a deterministic runner into `density_acquisition` and inspect persisted control-plane artifacts without launching AppKit, Oxide, Instruments, or an iPhone.

## Entry points list

- `acquisition_executes_all_five_candidates_in_balanced_four_pair_blocks` verifies candidate, treatment-order, singleton-isolated versus candidate-packed mode, observation, and selection contracts.
- `acquisition_resume_reuses_exact_results_without_reinvoking_runner` verifies byte-exact resumability.
- `acquisition_rejects_missing_observation_before_committing_result` verifies fail-before-commit behavior.
- `plan_rejects_non_macos_role_and_incomplete_candidate_sequence` verifies platform and five-size admission.

## Logic narrative

Fixtures declare ten scenarios, making the required candidate sizes `1, 2, 4, 8, 10`. The injected runner maps each request contract to equal isolated/packed estimators. Equal estimators admit all candidates, while lower occupied time and fewer launches select `all`.

## Preconditions and postconditions

Each test uses an isolated temporary directory and a canonical plan. Passing tests leave no workspace artifacts.

## Edge cases and failure modes

The tests cover unsupported roles, too-small scenario inventories, missing observations, and attempts to resume by reacquiring completed evidence.

## Concurrency and memory behavior

Tests are deterministic and perform no concurrent acquisition. Temporary artifacts remain small and are deleted with their temporary directories.

## Performance notes

No measured production path runs. The suite validates scheduling semantics only.

## Feature flags and cfgs

No feature flags or target cfgs apply.

## Testing and benchmarks

Run `cargo test --locked -p oxide-perf-runner --test density_acquisition_tests` with a bounded external `CARGO_TARGET_DIR`.

## Examples

The happy-path fixture supplies a closure returning typed `MacOsDensityDriverResult` values for each request.

## Changelog

- 2026-07-21: Added explicit singleton-isolated versus candidate-packed partition coverage.
- 2026-07-21: Added macOS density acquisition and resume coverage.
