use oxide_perf_runner::instrumentation_calibration::{
   reduce_instrumentation_calibration, InstrumentationCalibrationInput,
   InstrumentationCalibrationPair, TracePairOrder,
};

#[test]
fn balanced_external_sensor_calibration_accepts_negligible_overhead()
{
   let report = reduce_instrumentation_calibration(&fixture(1.002, 1.005, 1.003)).expect("reduce accepted calibration");
   assert!(report.accepted);
   assert_eq!(report.pair_count, 8);
}

#[test]
fn sensor_or_cpu_overhead_at_the_boundary_rejects()
{
   assert!(!reduce_instrumentation_calibration(&fixture(1.011, 1.005, 1.003)).expect("reduce p50 failure").accepted);
   assert!(!reduce_instrumentation_calibration(&fixture(1.002, 1.021, 1.003)).expect("reduce p95 failure").accepted);
   assert!(!reduce_instrumentation_calibration(&fixture(1.002, 1.005, 1.011)).expect("reduce CPU failure").accepted);
}

#[test]
fn missing_apparatus_or_unbalanced_pairs_fail_closed()
{
   let mut input = fixture(1.0, 1.0, 1.0);
   input.sensor_sample_hz = 999;
   assert!(reduce_instrumentation_calibration(&input).is_err());
   input.sensor_sample_hz = 1_000;
   input.pairs[0].order = TracePairOrder::TraceOffFirst;
   assert!(reduce_instrumentation_calibration(&input).is_err());
   input = fixture(1.0, 1.0, 1.0);
   input.alpha = 0.051;
   assert!(reduce_instrumentation_calibration(&input).is_err());
}

fn fixture(p50_ratio: f64, p95_ratio: f64, cpu_ratio: f64) -> InstrumentationCalibrationInput
{
   InstrumentationCalibrationInput {
      schema_version: 1,
      calibration_id: "macos-animation-hitches-v1".into(),
      platform_role: "macos-apple-silicon".into(),
      template_id: "Animation Hitches".into(),
      sensor_sample_hz: 1_000,
      alpha: 0.05,
      pairs: (0..8).map(|pair_index| InstrumentationCalibrationPair {
         pair_index,
         order: if pair_index % 2 == 0 { TracePairOrder::TraceOnFirst } else { TracePairOrder::TraceOffFirst },
         trace_on_sensor_p50_ms: 10.0 * p50_ratio,
         trace_off_sensor_p50_ms: 10.0,
         trace_on_sensor_p95_ms: 20.0 * p95_ratio,
         trace_off_sensor_p95_ms: 20.0,
         trace_on_process_cpu_ms: 100.0 * cpu_ratio,
         trace_off_process_cpu_ms: 100.0,
         valid: true,
      }).collect(),
   }
}
