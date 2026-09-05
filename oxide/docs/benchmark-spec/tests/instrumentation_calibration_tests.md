# oxide-benchmark-spec instrumentation calibration tests

## Intention and purpose

Freezes canonical serialization and the accepted/rejected boundaries of the shared instrumentation reducer.

## Relation to the rest of the code

These tests exercise `oxide_benchmark_spec::instrumentation_calibration` directly. The existing perf-runner tests separately protect its compatibility re-export.

## Entry points

- `canonical_balanced_trace_sensor_input_reduces_to_accepted_bound_report` verifies canonical round-trip, 1 kHz coverage, eight balanced pairs, and frozen p50/p95 margins.
- `loose_alpha_or_cpu_median_fails_admission` rejects alpha above 0.05 and CPU median overhead above 1%.

## Logic narrative

Fixtures use eight alternating trace-on-first and trace-off-first pairs. Mutations isolate the significance and CPU guardrails.

## Preconditions and postconditions

Tests perform no platform acquisition and write no persistent artifacts.
