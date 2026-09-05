# oxide-benchmark-spec tests: macos_attribution_tests

## Intention and purpose

These tests freeze full-attribution coverage and fail-closed capability handling.

## Relation to the rest of the code

The suite exercises only public `oxide-benchmark-spec` APIs and guards the plan consumed by the standalone macOS attribution controller.

## Entry points list

- `full_attribution_schedules_every_available_configuration_as_an_isolated_thirteen_scenario_replay()` validates the successful matrix.
- `full_attribution_rejects_missing_or_combined_replays_and_implicit_counter_availability()` validates failure paths.

## Logic narrative

One available and one unavailable GPU configuration prove exact replay selection. Mutations then remove VM Tracker, combine selectors, remove an unavailability reason, and omit an available selector.

## Preconditions and postconditions

Tests require only repository source and no Instruments process.

## Edge cases and failure modes

The suite covers missing mandatory work, implicit capability, invalid isolation, and malformed unavailability.

## Concurrency and memory behavior

Tests are single-process and allocate only small plan/JSON values.

## Performance notes

No profiler or benchmark is launched.

## Feature flags and cfgs

None.

## Testing and benchmarks

Run `cargo test --locked -p oxide-benchmark-spec --test macos_attribution_tests` with a bounded external Cargo target.

## Examples

Not applicable.

## Changelog

- 2026-07-21: added full-attribution plan coverage.
