use oxide_perf_runner::density_acquisition::{acquire_macos_density_with_runner, canonical_macos_density_acquisition_plan_json, validate_macos_density_acquisition_plan, MacOsDensityAcquisitionPlan, MacOsDensityDriverRequest, MacOsDensityDriverResult, MacOsDensityObservationContract, MacOsDensityRunMode, MacOsDensityScenario};
use oxide_perf_runner::density_calibration::{CalibrationTier, Observation, PairEvidence};
use std::collections::BTreeSet;
use std::fs;

#[test]
fn acquisition_executes_all_five_candidates_in_balanced_four_pair_blocks()
{
   let plan = plan();
   let temporary = tempfile::tempdir().expect("density acquisition tempdir");
   let plan_path = temporary.path().join("plan.json");
   fs::write(&plan_path, canonical_macos_density_acquisition_plan_json(&plan).expect("canonical density plan")).expect("write density plan");
   let output = temporary.path().join("evidence");
   let mut requests = Vec::new();
   let report = acquire_macos_density_with_runner(&plan_path, &"a".repeat(64), &output, false, |request, _, _, _| {
      requests.push(request.clone());
      Ok(result(request, 0.0))
   }).expect("acquire macOS density calibration");

   assert_eq!(report.candidate_sizes, [1, 2, 4, 8, 10]);
   assert_eq!(report.completed_pair_count, 20);
   assert_eq!(report.pair_count_per_candidate, 4);
   assert!(report.stopped_after_resolved_block);
   assert_eq!(report.selected_pack_size, Some(10));
   assert_eq!(requests.len(), 20);
   for pack_size in [1, 2, 4, 8, 10]
   {
      let candidate = requests.iter().filter(|request| request.pack_size == pack_size).collect::<Vec<_>>();
      assert_eq!(candidate.len(), 4);
      let treatment_ids = candidate[0].runs.iter().map(|run| run.treatment_id.clone()).collect::<BTreeSet<_>>();
      assert_eq!(treatment_ids.len(), 4);
      for position in 0..4
      {
         let at_position = candidate.iter().map(|request| request.runs[position].treatment_id.as_str()).collect::<BTreeSet<_>>();
         assert_eq!(at_position.len(), 4);
      }
      for request in candidate
      {
         assert_eq!(request.runs.iter().filter(|run| run.mode == MacOsDensityRunMode::Isolated).count(), 2);
         assert_eq!(request.runs.iter().filter(|run| run.mode == MacOsDensityRunMode::Packed).count(), 2);
         assert!(request.runs.iter().filter(|run| run.mode == MacOsDensityRunMode::Isolated).all(|run| run.ordered_packs.iter().all(|pack| pack.len() == 1)));
         assert!(request.runs.iter().filter(|run| run.mode == MacOsDensityRunMode::Packed).all(|run| run.ordered_packs.iter().all(|pack| pack.len() <= pack_size)));
         assert_eq!(request.observation_contract.len(), 24);
      }
   }
   assert!(output.join("density.input.json").is_file());
   assert!(output.join("density.report.json").is_file());
   assert!(output.join("acquisition.complete.json").is_file());
}

#[test]
fn acquisition_resume_reuses_exact_results_without_reinvoking_runner()
{
   let plan = plan();
   let temporary = tempfile::tempdir().expect("density acquisition tempdir");
   let plan_path = temporary.path().join("plan.json");
   fs::write(&plan_path, canonical_macos_density_acquisition_plan_json(&plan).expect("canonical density plan")).expect("write density plan");
   let output = temporary.path().join("evidence");
   acquire_macos_density_with_runner(&plan_path, &"b".repeat(64), &output, false, |request, _, _, _| Ok(result(request, 0.0))).expect("initial density acquisition");
   let report = acquire_macos_density_with_runner(&plan_path, &"b".repeat(64), &output, true, |_, _, _, _| panic!("resume reran completed density evidence")).expect("resume density acquisition");
   assert_eq!(report.completed_pair_count, 20);
   assert_eq!(report.selected_pack_size, Some(10));
}

#[test]
fn acquisition_rejects_missing_observation_before_committing_result()
{
   let plan = plan();
   let temporary = tempfile::tempdir().expect("density acquisition tempdir");
   let plan_path = temporary.path().join("plan.json");
   fs::write(&plan_path, canonical_macos_density_acquisition_plan_json(&plan).expect("canonical density plan")).expect("write density plan");
   let output = temporary.path().join("evidence");
   let error = acquire_macos_density_with_runner(&plan_path, &"c".repeat(64), &output, false, |request, _, _, _| {
      let mut result = result(request, 0.0);
      result.pair.observations.pop();
      Ok(result)
   }).expect_err("missing observation must fail density acquisition");
   assert!(error.to_string().contains("incomplete observations"));
   assert!(!output.join("pairs/k-1/pair-00.result.json").exists());
}

#[test]
fn plan_rejects_non_macos_role_and_incomplete_candidate_sequence()
{
   let mut invalid = plan();
   invalid.platform_role = "ios-iphone".into();
   assert!(validate_macos_density_acquisition_plan(&invalid).is_err());
   invalid = plan();
   invalid.scenarios.truncate(8);
   invalid.sentinel_scenario_id = invalid.scenarios[0].id.clone();
   assert!(validate_macos_density_acquisition_plan(&invalid).is_err());
}

fn plan() -> MacOsDensityAcquisitionPlan
{
   MacOsDensityAcquisitionPlan {
      schema_version: 1,
      calibration_id: "macos-density-fixture".into(),
      platform_role: "macos-apple-silicon".into(),
      invalidation_key: "fixture-instrumentation-lifecycle-order-role".into(),
      tier: CalibrationTier::Pr,
      seed: 0x5eed,
      bootstrap_resamples: 1_000,
      minimum_pair_count: 4,
      maximum_pair_count: 4,
      sentinel_scenario_id: "scenario-0".into(),
      implementations: vec!["oxide".into(), "native-production".into()],
      scenarios: (0..10).map(|index| MacOsDensityScenario {
         id: format!("scenario-{index}"),
         primary_metric_id: "event-to-visible-p50".into(),
         carryover_margin_basis_points: 200,
         risk_weight_millionths: 1_000_000,
      }).collect(),
   }
}

fn result(request: &MacOsDensityDriverRequest, carryover: f64) -> MacOsDensityDriverResult
{
   let request_bytes = canonical_request(request);
   MacOsDensityDriverResult {
      schema_version: 1,
      calibration_id: request.calibration_id.clone(),
      plan_sha256: request.plan_sha256.clone(),
      request_sha256: sha256(&request_bytes),
      invalidation_key: request.invalidation_key.clone(),
      platform_role: request.platform_role.clone(),
      candidate_id: request.candidate_id.clone(),
      pack_size: request.pack_size,
      pair_index: request.pair_index,
      order_id: request.order_id.clone(),
      controller_plan_sha256: "d".repeat(64),
      build_manifest_sha256: "e".repeat(64),
      session_manifest_sha256: "f".repeat(64),
      occupied_seconds: 100.0 / request.pack_size as f64,
      process_launches: request.all_scenario_count * 2 + request.all_scenario_count.div_ceil(request.pack_size) * 2,
      pair: PairEvidence {
         pair_index: request.pair_index,
         order_id: request.order_id.clone(),
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
         observations: request.observation_contract.iter().map(|contract| observation(contract, carryover)).collect(),
      },
   }
}

fn observation(contract: &MacOsDensityObservationContract, carryover: f64) -> Observation
{
   Observation {
      scenario_id: contract.scenario_id.clone(),
      implementation_id: contract.implementation_id.clone(),
      position_id: contract.position_id.clone(),
      isolated_estimator: 100.0,
      packed_estimator: 100.0 * (1.0 + carryover),
      carryover_margin_ratio: contract.carryover_margin_ratio,
      sentinel: contract.sentinel,
   }
}

fn canonical_request(request: &MacOsDensityDriverRequest) -> Vec<u8>
{
   let mut bytes = serde_json::to_vec_pretty(request).expect("encode density request");
   bytes.push(b'\n');
   bytes
}

fn sha256(bytes: &[u8]) -> String
{
   use sha2::{Digest, Sha256};
   format!("{:x}", Sha256::digest(bytes))
}
