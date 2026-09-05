use oxide_benchmark_spec::{canonical_instrumentation_calibration_input_json, reduce_instrumentation_calibration, InstrumentationCalibrationInput, InstrumentationCalibrationPair, TracePairOrder};

#[test]
fn canonical_balanced_trace_sensor_input_reduces_to_accepted_bound_report()
{
   let input = fixture();
   let bytes = canonical_instrumentation_calibration_input_json(&input).expect("canonical calibration input");
   assert_eq!(serde_json::from_slice::<InstrumentationCalibrationInput>(&bytes).expect("calibration input JSON"), input);
   let report = reduce_instrumentation_calibration(&input).expect("instrumentation calibration report");
   assert!(report.accepted);
   assert_eq!(report.sensor_sample_hz, 1_000);
   assert_eq!(report.pair_count, 8);
   assert_eq!(report.p50.margin_ratio, 0.01);
   assert_eq!(report.p95.margin_ratio, 0.02);
}

#[test]
fn loose_alpha_or_cpu_median_fails_admission()
{
   let mut loose = fixture();
   loose.alpha = 0.051;
   assert!(reduce_instrumentation_calibration(&loose).is_err());

   let mut cpu = fixture();
   for pair in &mut cpu.pairs
   {
      pair.trace_on_process_cpu_ms = 101.1;
   }
   assert!(!reduce_instrumentation_calibration(&cpu).expect("CPU calibration decision").accepted);
}

fn fixture() -> InstrumentationCalibrationInput
{
   InstrumentationCalibrationInput {
      schema_version: 1,
      calibration_id: String::from("macos-animation-hitches-v1"),
      platform_role: String::from("macos-apple-silicon"),
      template_id: String::from("Animation Hitches"),
      sensor_sample_hz: 1_000,
      alpha: 0.05,
      pairs: (0..8).map(|pair_index| InstrumentationCalibrationPair {
         pair_index,
         order: if pair_index % 2 == 0 {TracePairOrder::TraceOnFirst} else {TracePairOrder::TraceOffFirst},
         trace_on_sensor_p50_ms: 10.0,
         trace_off_sensor_p50_ms: 10.0,
         trace_on_sensor_p95_ms: 20.0,
         trace_off_sensor_p95_ms: 20.0,
         trace_on_process_cpu_ms: 100.0,
         trace_off_process_cpu_ms: 100.0,
         valid: true,
      }).collect(),
   }
}
