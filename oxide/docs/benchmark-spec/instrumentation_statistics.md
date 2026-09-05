# oxide-benchmark-spec `instrumentation_statistics`

## Intention and purpose

Owns the small exact-sign and median primitives required by the shared instrumentation-calibration reducer.

## Relation to the rest of the code

The module is private implementation support for `instrumentation_calibration`; public callers use the calibration reducer and its report types.

## Entry points

- `instrumentation_exact_sign_test(values: &[f64], boundary: f64, alternative: InstrumentationDecisionAlternative) -> anyhow::Result<InstrumentationExactSignTest>` computes the exact binomial tail.
- `instrumentation_median(samples: &[f64]) -> f64` computes the linearly interpolated median.
- `InstrumentationDecisionAlternative` and `InstrumentationExactSignTest` preserve the complete decision evidence.

## Logic narrative

The exact test counts values below, above, and tied at the boundary and evaluates the exact binomial tail for at most 63 non-tied observations. Median reduction sorts a private copy and interpolates the midpoint for even populations.

## Preconditions and postconditions

Boundaries and observations must be finite. The exact test rejects populations too large for its fixed-width denominator. Empty median input returns zero and is never admitted by the calibration contract.
