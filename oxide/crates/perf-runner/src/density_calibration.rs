use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::paired_statistics::{median, percentile_sorted};

pub const DENSITY_CALIBRATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CalibrationTier
{
   Pr,
   Nightly,
   Release,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DensityCalibrationInput
{
   pub schema_version: u32,
   pub calibration_id: String,
   pub platform_role: String,
   pub invalidation_key: String,
   pub tier: CalibrationTier,
   pub seed: u64,
   pub bootstrap_resamples: usize,
   pub candidates: Vec<PackEvidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PackEvidence
{
   pub pack_id: String,
   pub pack_size: usize,
   pub all_scenario_count: usize,
   pub weighted_risk_information: f64,
   pub occupied_minutes: f64,
   pub process_launches: usize,
   pub pairs: Vec<PairEvidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PairEvidence
{
   pub pair_index: usize,
   pub order_id: String,
   pub valid: bool,
   pub reset_complete: bool,
   pub terminators_complete: bool,
   pub event_loss_count: u64,
   pub footprint_recovery_ratio: f64,
   pub retained_slope_within_guardrail: bool,
   pub thermal_transition_before_final: bool,
   pub trace_capacity_ratio: f64,
   pub ring_capacity_ratio: f64,
   pub process_wall_seconds: f64,
   pub acquisition_seconds: f64,
   pub reducer_seconds: f64,
   pub observations: Vec<Observation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Observation
{
   pub scenario_id: String,
   pub implementation_id: String,
   pub position_id: String,
   pub isolated_estimator: f64,
   pub packed_estimator: f64,
   pub carryover_margin_ratio: f64,
   pub sentinel: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackDecision
{
   Admissible,
   Rejected,
   Inconclusive,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PackReport
{
   pub pack_id: String,
   pub pack_size: usize,
   pub pair_count: usize,
   pub valid_pair_count: usize,
   pub simultaneous_max_shift: f64,
   pub simultaneous_max_shift_upper_90: f64,
   pub maximum_position_order_shift: f64,
   pub maximum_sentinel_shift: f64,
   pub density_per_occupied_minute: f64,
   pub decision: PackDecision,
   pub failures: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DensityCalibrationReport
{
   pub schema_version: u32,
   pub calibration_id: String,
   pub platform_role: String,
   pub invalidation_key: String,
   pub selected_pack_id: Option<String>,
   pub selected_pack_size: Option<usize>,
   pub candidates: Vec<PackReport>,
}

pub fn reduce_density_calibration(input: &DensityCalibrationInput) -> Result<DensityCalibrationReport>
{
   validate_input(input)?;
   let mut reports = input.candidates.iter().map(|candidate| reduce_candidate(input, candidate)).collect::<Result<Vec<_>>>()?;
   reports.sort_by_key(|report| report.pack_size);
   let best_density = reports.iter().filter(|report| report.decision == PackDecision::Admissible).map(|report| report.density_per_occupied_minute).max_by(f64::total_cmp);
   let selected = best_density.and_then(|best| reports.iter().filter(|report| report.decision == PackDecision::Admissible && report.density_per_occupied_minute >= best * 0.95).min_by_key(|report| {
      let source = input.candidates.iter().find(|candidate| candidate.pack_id == report.pack_id).expect("validated density candidate");
      (source.process_launches, std::cmp::Reverse(source.pack_size))
   }));
   Ok(DensityCalibrationReport {
      schema_version: DENSITY_CALIBRATION_SCHEMA_VERSION,
      calibration_id: input.calibration_id.clone(),
      platform_role: input.platform_role.clone(),
      invalidation_key: input.invalidation_key.clone(),
      selected_pack_id: selected.map(|report| report.pack_id.clone()),
      selected_pack_size: selected.map(|report| report.pack_size),
      candidates: reports,
   })
}

fn validate_input(input: &DensityCalibrationInput) -> Result<()>
{
   ensure!(input.schema_version == DENSITY_CALIBRATION_SCHEMA_VERSION, "unsupported density-calibration schema version");
   ensure!(!input.calibration_id.is_empty() && !input.platform_role.is_empty() && !input.invalidation_key.is_empty(), "density-calibration identity is incomplete");
   ensure!(input.bootstrap_resamples >= 1_000, "density calibration requires at least 1000 bootstrap resamples");
   ensure!(!input.candidates.is_empty(), "density calibration has no candidates");
   let all = input.candidates[0].all_scenario_count;
   let mut sizes = BTreeSet::new();
   for candidate in &input.candidates
   {
      ensure!(!candidate.pack_id.is_empty() && candidate.all_scenario_count == all && all > 0, "density candidate identity is invalid");
      ensure!(candidate.pack_size > 0 && candidate.pack_size <= all && (candidate.pack_size == 1 || candidate.pack_size.is_power_of_two() || candidate.pack_size == all), "density candidate is outside the sequential-doubling sequence");
      ensure!(sizes.insert(candidate.pack_size), "density calibration repeats a pack size");
      ensure!(candidate.weighted_risk_information.is_finite() && candidate.weighted_risk_information >= 0.0 && candidate.occupied_minutes.is_finite() && candidate.occupied_minutes > 0.0 && candidate.process_launches > 0, "density candidate has invalid density inputs");
      ensure!((4..=24).contains(&candidate.pairs.len()) && candidate.pairs.len() % 4 == 0, "density candidate pair count is not a block of four from 4 through 24");
      let mut indices = BTreeSet::new();
      for pair in &candidate.pairs
      {
         ensure!(indices.insert(pair.pair_index) && !pair.order_id.is_empty(), "density pair identity is invalid");
         ensure!(pair.acquisition_seconds.is_finite() && pair.acquisition_seconds > 0.0, "density pair acquisition time is invalid");
         ensure!(!pair.observations.is_empty(), "density pair has no observations");
         let implementations = pair.observations.iter().map(|observation| observation.implementation_id.as_str()).collect::<BTreeSet<_>>();
         ensure!(implementations.len() == 2, "density pair must contain both implementations");
         for observation in &pair.observations
         {
            ensure!(!observation.scenario_id.is_empty() && !observation.position_id.is_empty(), "density observation identity is incomplete");
            ensure!(observation.isolated_estimator.is_finite() && observation.isolated_estimator > 0.0 && observation.packed_estimator.is_finite() && observation.packed_estimator > 0.0 && observation.carryover_margin_ratio.is_finite() && observation.carryover_margin_ratio > 0.0, "density observation values are invalid");
         }
      }
   }
   Ok(())
}

fn reduce_candidate(input: &DensityCalibrationInput, candidate: &PackEvidence) -> Result<PackReport>
{
   let valid = candidate.pairs.iter().filter(|pair| pair.valid).collect::<Vec<_>>();
   let simultaneous = point_max(&valid, false, false);
   let sentinel = point_max(&valid, true, false);
   let position_order = point_max(&valid, false, true);
   let mut bootstrap = Vec::with_capacity(input.bootstrap_resamples);
   let mut state = (input.seed ^ candidate.pack_size as u64).max(1);
   if valid.is_empty()
   {
      bootstrap.push(f64::INFINITY);
   }
   else
   {
      for _ in 0..input.bootstrap_resamples
      {
         let mut sample = Vec::with_capacity(valid.len());
         for _ in 0..valid.len()
         {
            state = random(state);
            sample.push(valid[(state as usize) % valid.len()]);
         }
         bootstrap.push(point_max(&sample, false, false));
      }
   }
   bootstrap.sort_unstable_by(f64::total_cmp);
   let upper = percentile_sorted(&bootstrap, 0.90);
   let mut failures = Vec::new();
   let wall_limit = match input.tier { CalibrationTier::Pr => 90.0, CalibrationTier::Nightly => 180.0, CalibrationTier::Release => 300.0 };
   if valid.len() as f64 / (candidate.pairs.len() as f64) < 0.95 { failures.push("valid pack ratio below 95%".into()); }
   for pair in &candidate.pairs
   {
      if !pair.reset_complete { failures.push(format!("pair {} reset incomplete", pair.pair_index)); }
      if !pair.terminators_complete || pair.event_loss_count > 0 { failures.push(format!("pair {} trace integrity failed", pair.pair_index)); }
      if pair.footprint_recovery_ratio > 1.05 || !pair.retained_slope_within_guardrail { failures.push(format!("pair {} memory recovery failed", pair.pair_index)); }
      if pair.thermal_transition_before_final { failures.push(format!("pair {} thermal transition", pair.pair_index)); }
      if pair.trace_capacity_ratio >= 0.70 || pair.ring_capacity_ratio >= 0.70 { failures.push(format!("pair {} capacity reached 70%", pair.pair_index)); }
      if pair.reducer_seconds >= pair.acquisition_seconds * 0.25 { failures.push(format!("pair {} reducer reached 25% of acquisition time", pair.pair_index)); }
      if pair.process_wall_seconds > wall_limit { failures.push(format!("pair {} wall limit exceeded", pair.pair_index)); }
   }
   if simultaneous >= 1.0 { failures.push("simultaneous carryover reached its margin".into()); }
   if position_order >= 1.0 { failures.push("position/order interaction reached its margin".into()); }
   if sentinel >= 1.0 { failures.push("first-to-last sentinel reached its margin".into()); }
   let decision = if !failures.is_empty() { PackDecision::Rejected } else if upper < 1.0 { PackDecision::Admissible } else { PackDecision::Inconclusive };
   Ok(PackReport {
      pack_id: candidate.pack_id.clone(), pack_size: candidate.pack_size, pair_count: candidate.pairs.len(), valid_pair_count: valid.len(),
      simultaneous_max_shift: simultaneous, simultaneous_max_shift_upper_90: upper, maximum_position_order_shift: position_order,
      maximum_sentinel_shift: sentinel, density_per_occupied_minute: candidate.weighted_risk_information / candidate.occupied_minutes,
      decision, failures,
   })
}

fn point_max(pairs: &[&PairEvidence], sentinel: bool, position_order: bool) -> f64
{
   let mut groups = BTreeMap::<String, Vec<f64>>::new();
   for pair in pairs
   {
      for observation in pair.observations.iter().filter(|observation| observation.sentinel == sentinel)
      {
         let position = if position_order { format!(":{}:{}", pair.order_id, observation.position_id) } else { String::new() };
         let key = format!("{}:{}{}", observation.scenario_id, observation.implementation_id, position);
         groups.entry(key).or_default().push((observation.packed_estimator / observation.isolated_estimator - 1.0) / observation.carryover_margin_ratio);
      }
   }
   groups.values().map(|values| median(values).abs()).max_by(f64::total_cmp).unwrap_or(f64::INFINITY)
}

fn random(mut value: u64) -> u64
{
   value ^= value << 13;
   value ^= value >> 7;
   value ^= value << 17;
   value
}
