use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::density_calibration::{reduce_density_calibration, CalibrationTier, DensityCalibrationInput, PackDecision, PackEvidence, PairEvidence, DENSITY_CALIBRATION_SCHEMA_VERSION};

pub const MACOS_DENSITY_ACQUISITION_SCHEMA_VERSION: u32 = 1;
const PAIR_BLOCK_SIZE: usize = 4;
const MAXIMUM_RESULT_BYTES: u64 = 1_048_576;
const MAXIMUM_OUTPUT_BYTES: u64 = 128 * 1_048_576;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsDensityAcquisitionPlan
{
   pub schema_version: u32,
   pub calibration_id: String,
   pub platform_role: String,
   pub invalidation_key: String,
   pub tier: CalibrationTier,
   pub seed: u64,
   pub bootstrap_resamples: usize,
   pub minimum_pair_count: usize,
   pub maximum_pair_count: usize,
   pub sentinel_scenario_id: String,
   pub implementations: Vec<String>,
   pub scenarios: Vec<MacOsDensityScenario>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsDensityScenario
{
   pub id: String,
   pub primary_metric_id: String,
   pub carryover_margin_basis_points: u32,
   pub risk_weight_millionths: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsDensityRunMode
{
   Isolated,
   Packed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsDensityRunRequest
{
   pub treatment_id: String,
   pub implementation_id: String,
   pub mode: MacOsDensityRunMode,
   pub ordered_packs: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsDensityDriverRequest
{
   pub schema_version: u32,
   pub calibration_id: String,
   pub plan_sha256: String,
   pub invalidation_key: String,
   pub platform_role: String,
   pub candidate_id: String,
   pub pack_size: usize,
   pub all_scenario_count: usize,
   pub pair_index: usize,
   pub order_id: String,
   pub sentinel_scenario_id: String,
   pub runs: Vec<MacOsDensityRunRequest>,
   pub observation_contract: Vec<MacOsDensityObservationContract>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsDensityObservationContract
{
   pub scenario_id: String,
   pub implementation_id: String,
   pub primary_metric_id: String,
   pub position_id: String,
   pub carryover_margin_ratio: f64,
   pub sentinel: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsDensityDriverResult
{
   pub schema_version: u32,
   pub calibration_id: String,
   pub plan_sha256: String,
   pub request_sha256: String,
   pub invalidation_key: String,
   pub platform_role: String,
   pub candidate_id: String,
   pub pack_size: usize,
   pub pair_index: usize,
   pub order_id: String,
   pub controller_plan_sha256: String,
   pub build_manifest_sha256: String,
   pub session_manifest_sha256: String,
   pub occupied_seconds: f64,
   pub process_launches: usize,
   pub pair: PairEvidence,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsDensityAcquisitionReport
{
   pub schema_version: u32,
   pub calibration_id: String,
   pub plan_sha256: String,
   pub driver_sha256: String,
   pub platform_role: String,
   pub candidate_sizes: Vec<usize>,
   pub completed_pair_count: usize,
   pub pair_count_per_candidate: usize,
   pub stopped_after_resolved_block: bool,
   pub evidence_sha256: String,
   pub reduction_sha256: String,
   pub selected_pack_id: Option<String>,
   pub selected_pack_size: Option<usize>,
}

pub fn canonical_macos_density_acquisition_plan_json(plan: &MacOsDensityAcquisitionPlan) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(plan).context("encoding macOS density-acquisition plan")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn validate_macos_density_acquisition_plan(plan: &MacOsDensityAcquisitionPlan) -> Result<()>
{
   ensure!(plan.schema_version == MACOS_DENSITY_ACQUISITION_SCHEMA_VERSION, "unsupported macOS density-acquisition schema version");
   ensure!(!plan.calibration_id.is_empty() && !plan.invalidation_key.is_empty(), "macOS density-acquisition identity is incomplete");
   ensure!(plan.platform_role == "macos-apple-silicon", "macOS density acquisition requires platform role macos-apple-silicon");
   ensure!(plan.bootstrap_resamples >= 1_000, "macOS density acquisition requires at least 1000 bootstrap resamples");
   ensure!(plan.minimum_pair_count == 4, "macOS density acquisition must begin with four balanced pairs");
   ensure!(matches!(plan.maximum_pair_count, 4 | 8 | 12 | 16 | 20 | 24), "macOS density-acquisition maximum pairs must be a four-pair block from 4 through 24");
   ensure!(plan.maximum_pair_count >= plan.minimum_pair_count, "macOS density-acquisition maximum pairs precede its minimum");
   ensure!(plan.implementations == ["oxide", "native-production"], "macOS density acquisition requires ordered oxide and native-production implementations");
   let implementations = plan.implementations.iter().map(String::as_str).collect::<BTreeSet<_>>();
   ensure!(implementations.len() == 2, "macOS density-acquisition implementation ids are duplicated");
   ensure!(plan.scenarios.len() > 8 && plan.scenarios.len() <= 64, "macOS density acquisition requires a bounded scenario surface larger than k=8");
   let mut scenario_ids = BTreeSet::new();
   for scenario in &plan.scenarios
   {
      ensure!(!scenario.id.is_empty() && !scenario.primary_metric_id.is_empty(), "macOS density scenario identity is incomplete");
      ensure!(scenario_ids.insert(scenario.id.as_str()), "macOS density scenario {} is duplicated", scenario.id);
      ensure!((1..=10_000).contains(&scenario.carryover_margin_basis_points), "macOS density scenario {} has an invalid carryover margin", scenario.id);
      ensure!(scenario.risk_weight_millionths > 0, "macOS density scenario {} has zero risk weight", scenario.id);
   }
   ensure!(scenario_ids.contains(plan.sentinel_scenario_id.as_str()), "macOS density sentinel is not a selected scenario");
   let sizes = candidate_sizes(plan.scenarios.len());
   ensure!(sizes.len() == 5 && sizes == [1, 2, 4, 8, plan.scenarios.len()], "macOS density acquisition does not expand to k=1,2,4,8,all");
   Ok(())
}

pub fn acquire_macos_density(plan_path: &Path, driver_path: &Path, output_root: &Path, resume: bool) -> Result<MacOsDensityAcquisitionReport>
{
   ensure!(driver_path.is_absolute() && driver_path.is_file(), "macOS density driver must be an absolute existing file");
   let driver_sha256 = sha256(&fs::read(driver_path).with_context(|| format!("reading macOS density driver {}", driver_path.display()))?);
   acquire_macos_density_with_runner(plan_path, &driver_sha256, output_root, resume, |request, request_path, staging_path, timeout| {
      run_density_driver(driver_path, request, request_path, staging_path, timeout)
   })
}

pub fn acquire_macos_density_with_runner<F>(plan_path: &Path, driver_sha256: &str, output_root: &Path, resume: bool, mut runner: F) -> Result<MacOsDensityAcquisitionReport>
where F: FnMut(&MacOsDensityDriverRequest, &Path, &Path, Duration) -> Result<MacOsDensityDriverResult>
{
   validate_sha256(driver_sha256, "macOS density driver")?;
   ensure!(output_root.is_absolute(), "macOS density acquisition requires an absolute output root");
   let plan_bytes = fs::read(plan_path).with_context(|| format!("reading macOS density-acquisition plan {}", plan_path.display()))?;
   let plan: MacOsDensityAcquisitionPlan = serde_json::from_slice(&plan_bytes).with_context(|| format!("decoding macOS density-acquisition plan {}", plan_path.display()))?;
   validate_macos_density_acquisition_plan(&plan)?;
   ensure!(canonical_macos_density_acquisition_plan_json(&plan)? == plan_bytes, "macOS density-acquisition plan is not canonical JSON");
   let plan_sha256 = sha256(&plan_bytes);
   prepare_output_root(output_root, &plan_bytes, resume)?;
   let candidates = candidate_sizes(plan.scenarios.len());
   let timeout = Duration::from_secs(tier_wall_seconds(plan.tier) + 30);
   let mut evidence = BTreeMap::<usize, Vec<MacOsDensityDriverResult>>::new();
   let mut reduction = None;
   let mut stopped_after_resolved_block = false;
   for block_start in (0..plan.maximum_pair_count).step_by(PAIR_BLOCK_SIZE)
   {
      for pack_size in &candidates
      {
         for pair_index in block_start..block_start + PAIR_BLOCK_SIZE
         {
            let request = build_request(&plan, &plan_sha256, *pack_size, pair_index)?;
            let request_bytes = canonical_json(&request, "macOS density driver request")?;
            let request_sha256 = sha256(&request_bytes);
            let directory = output_root.join("pairs").join(candidate_id(*pack_size, plan.scenarios.len()));
            fs::create_dir_all(&directory).with_context(|| format!("creating macOS density pair directory {}", directory.display()))?;
            let request_path = directory.join(format!("pair-{pair_index:02}.request.json"));
            persist_identity_file(&request_path, &request_bytes, resume)?;
            let result_path = directory.join(format!("pair-{pair_index:02}.result.json"));
            let staging_path = directory.join(format!("pair-{pair_index:02}.result.json.tmp"));
            let result = if result_path.exists()
            {
               ensure!(resume, "refusing to overwrite macOS density result {}", result_path.display());
               load_driver_result(&result_path)?
            }
            else
            {
               if staging_path.exists()
               {
                  ensure!(resume, "stale macOS density staging result exists without --resume: {}", staging_path.display());
                  fs::remove_file(&staging_path).with_context(|| format!("removing stale macOS density staging result {}", staging_path.display()))?;
               }
               let result = runner(&request, &request_path, &staging_path, timeout)?;
               validate_driver_result(&plan, &request, &request_sha256, &result)?;
               let result_bytes = canonical_json(&result, "macOS density driver result")?;
               ensure!(result_bytes.len() as u64 <= MAXIMUM_RESULT_BYTES, "macOS density driver result exceeds one MiB");
               write_new_file(&staging_path, &result_bytes)?;
               fs::rename(&staging_path, &result_path).with_context(|| format!("committing macOS density result {}", result_path.display()))?;
               result
            };
            validate_driver_result(&plan, &request, &request_sha256, &result)?;
            evidence.entry(*pack_size).or_default().push(result);
            ensure!(tree_bytes(output_root)? <= MAXIMUM_OUTPUT_BYTES, "macOS density acquisition exceeded its 128 MiB artifact limit");
         }
      }
      let input = build_reducer_input(&plan, &candidates, &evidence)?;
      let report = reduce_density_calibration(&input)?;
      let unresolved = report.candidates.iter().any(|candidate| candidate.decision == PackDecision::Inconclusive);
      reduction = Some((input, report));
      if !unresolved
      {
         stopped_after_resolved_block = true;
         break;
      }
   }
   let (input, density_report) = reduction.context("macOS density acquisition produced no reducer population")?;
   let evidence_bytes = canonical_json(&input, "macOS density calibration evidence")?;
   let reduction_bytes = canonical_json(&density_report, "macOS density calibration report")?;
   let evidence_sha256 = sha256(&evidence_bytes);
   let reduction_sha256 = sha256(&reduction_bytes);
   persist_identity_file(&output_root.join("density.input.json"), &evidence_bytes, resume)?;
   persist_identity_file(&output_root.join("density.report.json"), &reduction_bytes, resume)?;
   let report = MacOsDensityAcquisitionReport {
      schema_version: MACOS_DENSITY_ACQUISITION_SCHEMA_VERSION,
      calibration_id: plan.calibration_id,
      plan_sha256,
      driver_sha256: String::from(driver_sha256),
      platform_role: plan.platform_role,
      candidate_sizes: candidates,
      completed_pair_count: input.candidates.iter().map(|candidate| candidate.pairs.len()).sum(),
      pair_count_per_candidate: input.candidates.first().map(|candidate| candidate.pairs.len()).unwrap_or(0),
      stopped_after_resolved_block,
      evidence_sha256,
      reduction_sha256,
      selected_pack_id: density_report.selected_pack_id,
      selected_pack_size: density_report.selected_pack_size,
   };
   let report_bytes = canonical_json(&report, "macOS density acquisition report")?;
   persist_identity_file(&output_root.join("acquisition.complete.json"), &report_bytes, resume)?;
   Ok(report)
}

fn build_request(plan: &MacOsDensityAcquisitionPlan, plan_sha256: &str, pack_size: usize, pair_index: usize) -> Result<MacOsDensityDriverRequest>
{
   let ordered = ordered_scenarios(plan, pack_size, pair_index);
   let treatments = treatment_order(pair_index);
   let runs = treatments.iter().map(|(implementation_index, mode)| {
      let implementation_id = plan.implementations[*implementation_index].clone();
      let run_pack_size = if *mode == MacOsDensityRunMode::Isolated {1} else {pack_size};
      MacOsDensityRunRequest {
         treatment_id: format!("{}-{}", implementation_id, match mode {MacOsDensityRunMode::Isolated => "isolated", MacOsDensityRunMode::Packed => "packed"}),
         implementation_id,
         mode: *mode,
         ordered_packs: ordered.chunks(run_pack_size).map(|pack| pack.to_vec()).collect(),
      }
   }).collect::<Vec<_>>();
   let positions = ordered.iter().enumerate().map(|(index, id)| (id.as_str(), format!("pack-{:02}-slot-{:02}", index / pack_size, index % pack_size))).collect::<BTreeMap<_, _>>();
   let mut observation_contract = Vec::with_capacity(plan.implementations.len() * (plan.scenarios.len() + 2));
   for implementation_id in &plan.implementations
   {
      for scenario in &plan.scenarios
      {
         observation_contract.push(MacOsDensityObservationContract {
            scenario_id: scenario.id.clone(),
            implementation_id: implementation_id.clone(),
            primary_metric_id: scenario.primary_metric_id.clone(),
            position_id: positions.get(scenario.id.as_str()).context("ordered density scenario has no position")?.clone(),
            carryover_margin_ratio: scenario.carryover_margin_basis_points as f64 / 10_000.0,
            sentinel: false,
         });
      }
      for position in ["sentinel-first", "sentinel-last"]
      {
         observation_contract.push(MacOsDensityObservationContract {
            scenario_id: format!("{}:{}", plan.sentinel_scenario_id, position),
            implementation_id: implementation_id.clone(),
            primary_metric_id: plan.scenarios.iter().find(|scenario| scenario.id == plan.sentinel_scenario_id).context("density sentinel scenario disappeared")?.primary_metric_id.clone(),
            position_id: String::from(position),
            carryover_margin_ratio: 0.02,
            sentinel: true,
         });
      }
   }
   Ok(MacOsDensityDriverRequest {
      schema_version: MACOS_DENSITY_ACQUISITION_SCHEMA_VERSION,
      calibration_id: plan.calibration_id.clone(),
      plan_sha256: String::from(plan_sha256),
      invalidation_key: plan.invalidation_key.clone(),
      platform_role: plan.platform_role.clone(),
      candidate_id: candidate_id(pack_size, plan.scenarios.len()),
      pack_size,
      all_scenario_count: plan.scenarios.len(),
      pair_index,
      order_id: if pair_index % 2 == 0 {String::from("normal")} else {String::from("reverse")},
      sentinel_scenario_id: plan.sentinel_scenario_id.clone(),
      runs,
      observation_contract,
   })
}

fn treatment_order(pair_index: usize) -> [(usize, MacOsDensityRunMode); 4]
{
   let oxide = 0;
   let native = 1;
   match pair_index % PAIR_BLOCK_SIZE
   {
      0 => [(oxide, MacOsDensityRunMode::Isolated), (oxide, MacOsDensityRunMode::Packed), (native, MacOsDensityRunMode::Packed), (native, MacOsDensityRunMode::Isolated)],
      1 => [(native, MacOsDensityRunMode::Packed), (native, MacOsDensityRunMode::Isolated), (oxide, MacOsDensityRunMode::Isolated), (oxide, MacOsDensityRunMode::Packed)],
      2 => [(native, MacOsDensityRunMode::Isolated), (native, MacOsDensityRunMode::Packed), (oxide, MacOsDensityRunMode::Packed), (oxide, MacOsDensityRunMode::Isolated)],
      _ => [(oxide, MacOsDensityRunMode::Packed), (oxide, MacOsDensityRunMode::Isolated), (native, MacOsDensityRunMode::Isolated), (native, MacOsDensityRunMode::Packed)],
   }
}

fn ordered_scenarios(plan: &MacOsDensityAcquisitionPlan, pack_size: usize, pair_index: usize) -> Vec<String>
{
   let count = plan.scenarios.len();
   let offset = ((plan.seed as usize).wrapping_add(pack_size.wrapping_mul(17)).wrapping_add(pair_index.wrapping_mul(7))) % count;
   let mut ordered = (0..count).map(|index| plan.scenarios[(offset + index) % count].id.clone()).collect::<Vec<_>>();
   if pair_index % 2 == 1
   {
      ordered.reverse();
   }
   ordered
}

fn validate_driver_result(plan: &MacOsDensityAcquisitionPlan, request: &MacOsDensityDriverRequest, request_sha256: &str, result: &MacOsDensityDriverResult) -> Result<()>
{
   ensure!(result.schema_version == MACOS_DENSITY_ACQUISITION_SCHEMA_VERSION, "macOS density driver returned an unsupported schema");
   ensure!(result.calibration_id == request.calibration_id && result.plan_sha256 == request.plan_sha256 && result.request_sha256 == request_sha256, "macOS density driver result identity differs from its request");
   ensure!(result.invalidation_key == request.invalidation_key && result.platform_role == request.platform_role, "macOS density driver result environment identity differs from its request");
   ensure!(result.candidate_id == request.candidate_id && result.pack_size == request.pack_size && result.pair_index == request.pair_index && result.order_id == request.order_id, "macOS density driver result schedule identity differs from its request");
   validate_sha256(&result.controller_plan_sha256, "macOS density controller plan")?;
   validate_sha256(&result.build_manifest_sha256, "macOS density build manifest")?;
   validate_sha256(&result.session_manifest_sha256, "macOS density session manifest")?;
   ensure!(result.occupied_seconds.is_finite() && result.occupied_seconds > 0.0 && result.process_launches > 0, "macOS density driver result has invalid occupancy accounting");
   ensure!(result.pair.pair_index == request.pair_index && result.pair.order_id == request.order_id, "macOS density pair identity differs from its request");
   ensure!(result.pair.process_wall_seconds.is_finite() && result.pair.process_wall_seconds > 0.0, "macOS density driver result has invalid process wall time");
   ensure!(result.pair.process_wall_seconds <= tier_wall_seconds(plan.tier) as f64, "macOS density driver result exceeded its tier process wall limit");
   ensure!(result.pair.acquisition_seconds.is_finite() && result.pair.acquisition_seconds > 0.0 && result.pair.reducer_seconds.is_finite() && result.pair.reducer_seconds >= 0.0, "macOS density driver result has invalid acquisition/reducer time");
   ensure!(result.pair.footprint_recovery_ratio.is_finite() && result.pair.footprint_recovery_ratio >= 0.0, "macOS density driver result has an invalid footprint-recovery ratio");
   ensure!(result.pair.trace_capacity_ratio.is_finite() && result.pair.trace_capacity_ratio >= 0.0 && result.pair.ring_capacity_ratio.is_finite() && result.pair.ring_capacity_ratio >= 0.0, "macOS density driver result has an invalid capacity ratio");
   ensure!(result.pair.observations.len() == request.observation_contract.len(), "macOS density driver result has incomplete observations");
   let expected = request.observation_contract.iter().map(|observation| ((observation.scenario_id.as_str(), observation.implementation_id.as_str()), observation)).collect::<BTreeMap<_, _>>();
   let mut observed = BTreeSet::new();
   for observation in &result.pair.observations
   {
      let key = (observation.scenario_id.as_str(), observation.implementation_id.as_str());
      ensure!(observed.insert(key), "macOS density driver duplicated observation {} / {}", observation.scenario_id, observation.implementation_id);
      let contract = expected.get(&key).with_context(|| format!("macOS density driver returned unexpected observation {} / {}", observation.scenario_id, observation.implementation_id))?;
      ensure!(observation.position_id == contract.position_id && observation.sentinel == contract.sentinel, "macOS density observation position/sentinel contract differs for {} / {}", observation.scenario_id, observation.implementation_id);
      ensure!((observation.carryover_margin_ratio - contract.carryover_margin_ratio).abs() <= f64::EPSILON, "macOS density observation margin differs for {} / {}", observation.scenario_id, observation.implementation_id);
      ensure!(observation.isolated_estimator.is_finite() && observation.isolated_estimator > 0.0 && observation.packed_estimator.is_finite() && observation.packed_estimator > 0.0, "macOS density observation has an invalid estimator");
   }
   ensure!(observed.len() == expected.len(), "macOS density driver omitted an observation");
   Ok(())
}

fn build_reducer_input(plan: &MacOsDensityAcquisitionPlan, sizes: &[usize], evidence: &BTreeMap<usize, Vec<MacOsDensityDriverResult>>) -> Result<DensityCalibrationInput>
{
   let weighted_risk_information = plan.scenarios.iter().map(|scenario| scenario.risk_weight_millionths).sum::<u64>() as f64 / 1_000_000.0;
   let candidates = sizes.iter().map(|pack_size| {
      let results = evidence.get(pack_size).with_context(|| format!("macOS density candidate k={} has no evidence", pack_size))?;
      ensure!(!results.is_empty() && results.len() % PAIR_BLOCK_SIZE == 0, "macOS density candidate k={} does not end on a four-pair block", pack_size);
      Ok(PackEvidence {
         pack_id: candidate_id(*pack_size, plan.scenarios.len()),
         pack_size: *pack_size,
         all_scenario_count: plan.scenarios.len(),
         weighted_risk_information,
         occupied_minutes: results.iter().map(|result| result.occupied_seconds).sum::<f64>() / 60.0,
         process_launches: results.iter().map(|result| result.process_launches).sum(),
         pairs: results.iter().map(|result| result.pair.clone()).collect(),
      })
   }).collect::<Result<Vec<_>>>()?;
   Ok(DensityCalibrationInput {
      schema_version: DENSITY_CALIBRATION_SCHEMA_VERSION,
      calibration_id: plan.calibration_id.clone(),
      platform_role: plan.platform_role.clone(),
      invalidation_key: plan.invalidation_key.clone(),
      tier: plan.tier,
      seed: plan.seed,
      bootstrap_resamples: plan.bootstrap_resamples,
      candidates,
   })
}

fn run_density_driver(driver_path: &Path, _request: &MacOsDensityDriverRequest, request_path: &Path, staging_path: &Path, timeout: Duration) -> Result<MacOsDensityDriverResult>
{
   let mut child = Command::new(driver_path)
      .args(["--platform", "macos", "--request"])
      .arg(request_path)
      .arg("--output")
      .arg(staging_path)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .with_context(|| format!("launching macOS density driver {}", driver_path.display()))?;
   let deadline = Instant::now() + timeout;
   loop
   {
      if let Some(status) = child.try_wait().context("polling macOS density driver")?
      {
         if !status.success()
         {
            if staging_path.exists()
            {
               fs::remove_file(staging_path).with_context(|| format!("removing failed macOS density driver staging result {}", staging_path.display()))?;
            }
            bail!("macOS density driver exited with {}", status);
         }
         break;
      }
      if Instant::now() >= deadline
      {
         child.kill().context("terminating timed-out macOS density driver")?;
         child.wait().context("waiting for terminated macOS density driver")?;
         if staging_path.exists()
         {
            fs::remove_file(staging_path).with_context(|| format!("removing timed-out macOS density driver staging result {}", staging_path.display()))?;
         }
         bail!("macOS density driver exceeded its bounded timeout");
      }
      thread::sleep(Duration::from_millis(25));
   }
   let result = (|| {
      let metadata = fs::metadata(staging_path).with_context(|| format!("reading macOS density driver result metadata {}", staging_path.display()))?;
      ensure!(metadata.is_file() && metadata.len() <= MAXIMUM_RESULT_BYTES, "macOS density driver result is missing, non-file, or exceeds one MiB");
      let bytes = fs::read(staging_path).with_context(|| format!("reading macOS density driver result {}", staging_path.display()))?;
      let result: MacOsDensityDriverResult = serde_json::from_slice(&bytes).context("decoding macOS density driver result")?;
      ensure!(canonical_json(&result, "macOS density driver result")? == bytes, "macOS density driver result is not canonical JSON");
      Ok(result)
   })();
   if staging_path.exists()
   {
      fs::remove_file(staging_path).with_context(|| format!("removing consumed macOS density driver staging result {}", staging_path.display()))?;
   }
   result
}

fn prepare_output_root(output_root: &Path, plan_bytes: &[u8], resume: bool) -> Result<()>
{
   if output_root.exists()
   {
      ensure!(resume, "macOS density output already exists; pass --resume to validate and continue");
      ensure!(output_root.is_dir(), "macOS density output root is not a directory");
   }
   else
   {
      ensure!(!resume, "macOS density --resume requires an existing output root");
      fs::create_dir_all(output_root).with_context(|| format!("creating macOS density output root {}", output_root.display()))?;
   }
   persist_identity_file(&output_root.join("acquisition.plan.json"), plan_bytes, resume)
}

fn persist_identity_file(path: &Path, bytes: &[u8], resume: bool) -> Result<()>
{
   if path.exists()
   {
      ensure!(resume, "refusing to overwrite macOS density artifact {}", path.display());
      ensure!(fs::read(path).with_context(|| format!("reading existing macOS density artifact {}", path.display()))? == bytes, "existing macOS density artifact differs from the current identity: {}", path.display());
      return Ok(());
   }
   write_new_file(path, bytes)
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<()>
{
   if let Some(parent) = path.parent()
   {
      fs::create_dir_all(parent).with_context(|| format!("creating macOS density artifact directory {}", parent.display()))?;
   }
   let mut file = OpenOptions::new().create_new(true).write(true).open(path).with_context(|| format!("creating macOS density artifact {}", path.display()))?;
   file.write_all(bytes).with_context(|| format!("writing macOS density artifact {}", path.display()))?;
   file.sync_all().with_context(|| format!("syncing macOS density artifact {}", path.display()))?;
   Ok(())
}

fn load_driver_result(path: &Path) -> Result<MacOsDensityDriverResult>
{
   let metadata = fs::metadata(path).with_context(|| format!("reading macOS density result metadata {}", path.display()))?;
   ensure!(metadata.is_file() && metadata.len() <= MAXIMUM_RESULT_BYTES, "existing macOS density result is non-file or exceeds one MiB");
   let bytes = fs::read(path).with_context(|| format!("reading macOS density result {}", path.display()))?;
   let result = serde_json::from_slice(&bytes).with_context(|| format!("decoding macOS density result {}", path.display()))?;
   ensure!(canonical_json(&result, "macOS density driver result")? == bytes, "existing macOS density result is not canonical JSON");
   Ok(result)
}

fn candidate_sizes(all: usize) -> Vec<usize>
{
   vec![1, 2, 4, 8, all]
}

fn candidate_id(size: usize, all: usize) -> String
{
   if size == all {String::from("k-all")} else {format!("k-{size}")}
}

fn tier_wall_seconds(tier: CalibrationTier) -> u64
{
   match tier
   {
      CalibrationTier::Pr => 90,
      CalibrationTier::Nightly => 180,
      CalibrationTier::Release => 300,
   }
}

fn canonical_json<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(value).with_context(|| format!("encoding {}", label))?;
   bytes.push(b'\n');
   Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn validate_sha256(value: &str, label: &str) -> Result<()>
{
   ensure!(value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "{} SHA-256 must be 64 lowercase hexadecimal characters", label);
   Ok(())
}

fn tree_bytes(path: &Path) -> Result<u64>
{
   let metadata = fs::symlink_metadata(path).with_context(|| format!("reading macOS density artifact metadata {}", path.display()))?;
   ensure!(!metadata.file_type().is_symlink(), "macOS density artifact tree contains a symlink: {}", path.display());
   if metadata.is_file()
   {
      return Ok(metadata.len());
   }
   ensure!(metadata.is_dir(), "macOS density artifact is neither file nor directory: {}", path.display());
   let mut total = 0_u64;
   for entry in fs::read_dir(path).with_context(|| format!("reading macOS density artifact directory {}", path.display()))?
   {
      total = total.checked_add(tree_bytes(&entry?.path())?).context("macOS density artifact byte count overflow")?;
   }
   Ok(total)
}
