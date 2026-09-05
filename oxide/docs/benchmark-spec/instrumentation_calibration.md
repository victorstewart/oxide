# oxide-benchmark-spec `instrumentation_calibration`

## Intention and purpose

Defines the shared, harness-only trace-on/off external-sensor calibration contract. It prevents tracing or sensing overhead from becoming an unmeasured bias in native-framework comparisons.

## Relation to the rest of the code

`perf-runner` re-exports this module for compatibility. `xtask` and the Apple comparison controller consume this benchmark-spec implementation directly, so calibration has one reducer and one set of frozen margins.

## Entry points

- `oxide_benchmark_spec::canonical_instrumentation_calibration_input_json(input: &InstrumentationCalibrationInput) -> anyhow::Result<Vec<u8>>` emits deterministic pretty JSON with one trailing newline.
- `oxide_benchmark_spec::reduce_instrumentation_calibration(input: &InstrumentationCalibrationInput) -> anyhow::Result<InstrumentationCalibrationReport>` validates and reduces 8–24 balanced trace-on/off pairs.
- `InstrumentationCalibrationInput`, `InstrumentationCalibrationPair`, `InstrumentationCalibrationReport`, `InstrumentationEquivalenceDecision`, and `TracePairOrder` define the canonical evidence and report schemas.

## Logic narrative

The reducer requires a sensor rate of at least 1 kHz, a significance level no looser than 0.05, contiguous valid pairs in balanced blocks of four, and finite positive observations. It computes paired trace-on versus trace-off ratios, applies exact two-one-sided sign tests at ±1% p50 and ±2% p95 margins, and applies the one-sided exact CPU test plus a 1% CPU median cap. The report binds the canonical input SHA-256.

## Preconditions and postconditions

Input identity fields are nonempty. Pair counts are 8–24 and divisible by four. Accepted output proves every frozen statistical and median margin gate passed; rejected statistical evidence remains a valid report, while malformed evidence returns an error.
