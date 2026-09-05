use oxide_perf_runner::density_calibration::{
   reduce_density_calibration, CalibrationTier, DensityCalibrationInput, Observation,
   PackDecision, PackEvidence, PairEvidence,
};

#[test]
fn zero_carryover_is_admissible_and_highest_density_pack_is_selected()
{
   let mut input = calibration(&[1, 2, 4], 0.0);
   input.candidates[0].weighted_risk_information = 1.0;
   input.candidates[1].weighted_risk_information = 3.0;
   input.candidates[2].weighted_risk_information = 4.0;
   let report = reduce_density_calibration(&input).expect("reduce zero-carryover calibration");
   assert_eq!(report.selected_pack_size, Some(4));
   assert!(report.candidates.iter().all(|candidate| candidate.decision == PackDecision::Admissible));
}

#[test]
fn injected_carryover_selects_the_prior_packing_knee()
{
   let mut input = calibration(&[1, 2, 4], 0.0);
   for pair in &mut input.candidates[2].pairs
   {
      for observation in pair.observations.iter_mut().filter(|observation| !observation.sentinel)
      {
         observation.packed_estimator = observation.isolated_estimator * 1.03;
      }
   }
   let report = reduce_density_calibration(&input).expect("reduce injected carryover calibration");
   assert_eq!(report.selected_pack_size, Some(2));
   assert_eq!(report.candidates[2].decision, PackDecision::Rejected);
}

#[test]
fn integrity_reset_memory_thermal_and_capacity_fail_closed()
{
   let mut input = calibration(&[1], 0.0);
   let pair = &mut input.candidates[0].pairs[0];
   pair.reset_complete = false;
   pair.terminators_complete = false;
   pair.footprint_recovery_ratio = 1.06;
   pair.thermal_transition_before_final = true;
   pair.trace_capacity_ratio = 0.70;
   let report = reduce_density_calibration(&input).expect("reduce broken calibration");
   assert_eq!(report.selected_pack_size, None);
   assert_eq!(report.candidates[0].decision, PackDecision::Rejected);
   assert!(report.candidates[0].failures.len() >= 5);
}

fn calibration(sizes: &[usize], carryover: f64) -> DensityCalibrationInput
{
   let all = *sizes.iter().max().expect("candidate sizes");
   DensityCalibrationInput {
      schema_version: 1,
      calibration_id: "fixture".into(),
      platform_role: "macos-apple-silicon".into(),
      invalidation_key: "fixture-key".into(),
      tier: CalibrationTier::Pr,
      seed: 0x5eed,
      bootstrap_resamples: 1_000,
      candidates: sizes.iter().map(|size| PackEvidence {
         pack_id: format!("pack-{size}"),
         pack_size: *size,
         all_scenario_count: all,
         weighted_risk_information: *size as f64,
         occupied_minutes: 1.0,
         process_launches: all + 1 - size,
         pairs: (0..4).map(|pair_index| pair(pair_index, *size, carryover)).collect(),
      }).collect(),
   }
}

fn pair(pair_index: usize, pack_size: usize, carryover: f64) -> PairEvidence
{
   let mut observations = Vec::new();
   for implementation in ["oxide", "native"]
   {
      for scenario in 0..pack_size
      {
         observations.push(observation(format!("scenario-{scenario}"), implementation, false, pair_index, carryover));
      }
      observations.push(observation("sentinel-first".into(), implementation, true, pair_index, 0.0));
      observations.push(observation("sentinel-last".into(), implementation, true, pair_index, 0.0));
   }
   PairEvidence {
      pair_index,
      order_id: if pair_index % 2 == 0 { "normal".into() } else { "reverse".into() },
      valid: true,
      reset_complete: true,
      terminators_complete: true,
      event_loss_count: 0,
      footprint_recovery_ratio: 1.0,
      retained_slope_within_guardrail: true,
      thermal_transition_before_final: false,
      trace_capacity_ratio: 0.20,
      ring_capacity_ratio: 0.20,
      process_wall_seconds: 30.0,
      acquisition_seconds: 20.0,
      reducer_seconds: 1.0,
      observations,
   }
}

fn observation(scenario_id: String, implementation_id: &str, sentinel: bool, position: usize, carryover: f64) -> Observation
{
   Observation {
      scenario_id,
      implementation_id: implementation_id.into(),
      position_id: position.to_string(),
      isolated_estimator: 100.0,
      packed_estimator: 100.0 * (1.0 + carryover),
      carryover_margin_ratio: 0.02,
      sentinel,
   }
}
