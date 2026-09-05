use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::instrumentation_statistics::{instrumentation_exact_sign_test, instrumentation_median, InstrumentationDecisionAlternative, InstrumentationExactSignTest};

pub const INSTRUMENTATION_CALIBRATION_SCHEMA_VERSION: u32 = 1;
pub const INSTRUMENTATION_CALIBRATION_P50_MARGIN_RATIO: f64 = 0.01;
pub const INSTRUMENTATION_CALIBRATION_P95_MARGIN_RATIO: f64 = 0.02;
pub const INSTRUMENTATION_CALIBRATION_CPU_MARGIN_RATIO: f64 = 0.01;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TracePairOrder
{
   TraceOnFirst,
   TraceOffFirst,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InstrumentationCalibrationInput
{
   pub schema_version: u32,
   pub calibration_id: String,
   pub platform_role: String,
   pub template_id: String,
   pub sensor_sample_hz: u32,
   pub alpha: f64,
   pub pairs: Vec<InstrumentationCalibrationPair>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InstrumentationCalibrationPair
{
   pub pair_index: usize,
   pub order: TracePairOrder,
   pub trace_on_sensor_p50_ms: f64,
   pub trace_off_sensor_p50_ms: f64,
   pub trace_on_sensor_p95_ms: f64,
   pub trace_off_sensor_p95_ms: f64,
   pub trace_on_process_cpu_ms: f64,
   pub trace_off_process_cpu_ms: f64,
   pub valid: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InstrumentationEquivalenceDecision
{
   pub margin_ratio: f64,
   pub median_added_ratio: f64,
   pub lower: InstrumentationExactSignTest,
   pub upper: InstrumentationExactSignTest,
   pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InstrumentationCalibrationReport
{
   pub schema_version: u32,
   pub input_sha256: String,
   pub calibration_id: String,
   pub platform_role: String,
   pub template_id: String,
   pub sensor_sample_hz: u32,
   pub alpha: f64,
   pub pair_count: usize,
   pub p50: InstrumentationEquivalenceDecision,
   pub p95: InstrumentationEquivalenceDecision,
   pub process_cpu_upper: InstrumentationExactSignTest,
   pub median_process_cpu_added_ratio: f64,
   pub accepted: bool,
}

pub fn canonical_instrumentation_calibration_input_json(input: &InstrumentationCalibrationInput) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(input)?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn reduce_instrumentation_calibration(input: &InstrumentationCalibrationInput) -> Result<InstrumentationCalibrationReport>
{
   ensure!(input.schema_version == INSTRUMENTATION_CALIBRATION_SCHEMA_VERSION, "unsupported instrumentation-calibration schema version");
   ensure!(!input.calibration_id.is_empty() && !input.platform_role.is_empty() && !input.template_id.is_empty(), "instrumentation-calibration identity is incomplete");
   ensure!(input.sensor_sample_hz >= 1_000, "instrumentation calibration requires an external sensor sampled at 1000 Hz or faster");
   ensure!(input.alpha.is_finite() && input.alpha > 0.0 && input.alpha <= 0.05, "instrumentation-calibration alpha must be no greater than 0.05");
   ensure!((8..=24).contains(&input.pairs.len()) && input.pairs.len() % 4 == 0, "instrumentation calibration requires 8 through 24 pairs in blocks of four");
   let on_first = input.pairs.iter().filter(|pair| pair.order == TracePairOrder::TraceOnFirst).count();
   ensure!(on_first * 2 == input.pairs.len(), "instrumentation calibration pair order is not balanced");
   ensure!(input.pairs.iter().all(|pair| pair.valid), "instrumentation calibration contains an invalid pair");
   for (expected, pair) in input.pairs.iter().enumerate()
   {
      ensure!(pair.pair_index == expected, "instrumentation calibration pair indices are not contiguous");
      for value in [pair.trace_on_sensor_p50_ms, pair.trace_off_sensor_p50_ms, pair.trace_on_sensor_p95_ms, pair.trace_off_sensor_p95_ms, pair.trace_on_process_cpu_ms, pair.trace_off_process_cpu_ms]
      {
         ensure!(value.is_finite() && value > 0.0, "instrumentation calibration has a non-positive observation");
      }
   }
   let p50_effects = input.pairs.iter().map(|pair| pair.trace_on_sensor_p50_ms / pair.trace_off_sensor_p50_ms - 1.0).collect::<Vec<_>>();
   let p95_effects = input.pairs.iter().map(|pair| pair.trace_on_sensor_p95_ms / pair.trace_off_sensor_p95_ms - 1.0).collect::<Vec<_>>();
   let cpu_effects = input.pairs.iter().map(|pair| pair.trace_on_process_cpu_ms / pair.trace_off_process_cpu_ms - 1.0).collect::<Vec<_>>();
   let p50 = equivalence(&p50_effects, INSTRUMENTATION_CALIBRATION_P50_MARGIN_RATIO, input.alpha)?;
   let p95 = equivalence(&p95_effects, INSTRUMENTATION_CALIBRATION_P95_MARGIN_RATIO, input.alpha)?;
   let process_cpu_upper = instrumentation_exact_sign_test(&cpu_effects, INSTRUMENTATION_CALIBRATION_CPU_MARGIN_RATIO, InstrumentationDecisionAlternative::Lower)?;
   let median_process_cpu_added_ratio = instrumentation_median(&cpu_effects);
   let accepted = p50.accepted && p95.accepted && process_cpu_upper.p_value <= input.alpha && median_process_cpu_added_ratio <= INSTRUMENTATION_CALIBRATION_CPU_MARGIN_RATIO;
   Ok(InstrumentationCalibrationReport {
      schema_version: INSTRUMENTATION_CALIBRATION_SCHEMA_VERSION,
      input_sha256: format!("{:x}", Sha256::digest(canonical_instrumentation_calibration_input_json(input)?)),
      calibration_id: input.calibration_id.clone(),
      platform_role: input.platform_role.clone(),
      template_id: input.template_id.clone(),
      sensor_sample_hz: input.sensor_sample_hz,
      alpha: input.alpha,
      pair_count: input.pairs.len(),
      p50,
      p95,
      process_cpu_upper,
      median_process_cpu_added_ratio,
      accepted,
   })
}

fn equivalence(effects: &[f64], margin: f64, alpha: f64) -> Result<InstrumentationEquivalenceDecision>
{
   let lower = instrumentation_exact_sign_test(effects, -margin, InstrumentationDecisionAlternative::Upper)?;
   let upper = instrumentation_exact_sign_test(effects, margin, InstrumentationDecisionAlternative::Lower)?;
   let accepted = lower.p_value <= alpha && upper.p_value <= alpha;
   Ok(InstrumentationEquivalenceDecision {margin_ratio: margin, median_added_ratio: instrumentation_median(effects), lower, upper, accepted})
}
