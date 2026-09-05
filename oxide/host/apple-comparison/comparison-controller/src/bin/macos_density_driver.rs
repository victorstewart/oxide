use anyhow::{bail, ensure, Context, Result};
use oxide_apple_comparison_controller::{reduce_macos_resource_artifact, run_macos_density_sessions, ComparisonSide, MacOsDensitySession, MacOsDensitySessionConfig, MacOsDensitySessionResult};
use oxide_benchmark_spec::AppleCampaignPlanSpec;
use oxide_perf_runner::density_acquisition::{MacOsDensityDriverRequest, MacOsDensityDriverResult, MacOsDensityRunMode};
use oxide_perf_runner::density_calibration::{Observation, PairEvidence};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const TELEMETRY_HEADER_BYTES: usize = 136;
const TELEMETRY_RECORD_BYTES: usize = 44;
const TELEMETRY_FOOTER_BYTES: usize = 32;
const SCENE_UPDATE_BEGIN: u16 = 12;
const SCENE_UPDATE_END: u16 = 13;

#[derive(Clone)]
struct Arguments
{
   request: PathBuf,
   output: PathBuf,
   build_manifest: PathBuf,
   campaign_plan: PathBuf,
   session_root: PathBuf,
}

#[derive(Clone)]
enum SessionRole
{
   Scenario,
   SentinelFirst,
   SentinelLast,
}

#[derive(Clone)]
struct ScheduledSession
{
   implementation_id: String,
   mode: MacOsDensityRunMode,
   role: SessionRole,
   scenarios: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteEnvelope
{
   telemetry_sha256: String,
   telemetry_byte_count: u64,
   scenarios: Vec<ScenarioEnvelope>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioEnvelope
{
   #[serde(rename = "scenarioID")]
   scenario_id: String,
   first_telemetry_sequence: u64,
   last_telemetry_sequence: u64,
}

#[derive(Serialize)]
struct DensitySessionManifest<'a>
{
   schema_version: u32,
   request_sha256: &'a str,
   controller_plan_sha256: &'a str,
   build_manifest_sha256: &'a str,
   sessions: &'a [MacOsDensitySessionResult],
}

struct TelemetryEvidence
{
   estimators: BTreeMap<String, f64>,
   capacity_ratio: f64,
}

fn main()
{
   if let Err(error) = run()
   {
      eprintln!("macOS density driver failed: {error:#}");
      std::process::exit(1);
   }
}

fn run() -> Result<()>
{
   let arguments = arguments()?;
   let request_bytes = fs::read(&arguments.request).with_context(|| format!("reading {}", arguments.request.display()))?;
   let request: MacOsDensityDriverRequest = serde_json::from_slice(&request_bytes).context("decoding macOS density request")?;
   ensure!(canonical_json(&request)? == request_bytes, "macOS density request is not canonical JSON");
   let request_sha256 = sha256(&request_bytes);
   let plan_bytes = fs::read(&arguments.campaign_plan).with_context(|| format!("reading {}", arguments.campaign_plan.display()))?;
   let plan: AppleCampaignPlanSpec = serde_json::from_slice(&plan_bytes).context("decoding macOS density campaign plan")?;
   let controller_plan_sha256 = sha256(&plan_bytes);
   let build_manifest_sha256 = sha256(&fs::read(&arguments.build_manifest).with_context(|| format!("reading {}", arguments.build_manifest.display()))?);
   let thermal_before = thermal_nominal()?;
   let acquisition_started = Instant::now();
   let (controller_sessions, scheduled) = schedule(&request, &plan)?;
   let run_id = format!("density-{}-{}-{}", request.candidate_id, request.pair_index, &request_sha256[..12]);
   let results = run_macos_density_sessions(&MacOsDensitySessionConfig {
      build_manifest_path: arguments.build_manifest.clone(),
      plan_path: arguments.campaign_plan.clone(),
      output_root: arguments.session_root.clone(),
      run_id,
      sessions: controller_sessions,
   })?;
   ensure!(results.len() == scheduled.len(), "macOS density controller omitted a scheduled session");
   let acquisition_seconds = acquisition_started.elapsed().as_secs_f64();
   let reducer_started = Instant::now();
   let mut estimates = BTreeMap::new();
   let mut maximum_ring_ratio = 0.0_f64;
   let mut maximum_process_seconds = 0.0_f64;
   let mut maximum_footprint_ratio = 0.0_f64;
   let mut retained_slope_within_guardrail = true;
   for (result, scheduled) in results.iter().zip(&scheduled)
   {
      let evidence = telemetry_evidence(result, &controller_plan_sha256)?;
      maximum_ring_ratio = maximum_ring_ratio.max(evidence.capacity_ratio);
      for scenario in &scheduled.scenarios
      {
         let estimator = *evidence.estimators.get(scenario).with_context(|| format!("density session omitted estimator for {}", scenario))?;
         let role = match scheduled.role {SessionRole::Scenario => "scenario", SessionRole::SentinelFirst => "sentinel-first", SessionRole::SentinelLast => "sentinel-last"};
         ensure!(estimates.insert((scheduled.implementation_id.clone(), scheduled.mode, String::from(role), scenario.clone()), estimator).is_none(), "density session duplicated estimator identity");
      }
      let resource_bytes = fs::read(&result.campaign.resource_path).with_context(|| format!("reading {}", result.campaign.resource_path))?;
      let resource = reduce_macos_resource_artifact(&resource_bytes, None, None)?;
      maximum_process_seconds = maximum_process_seconds.max(resource.wall_time_ns as f64 / 1_000_000_000.0);
      if resource.physical_footprint_start_bytes > 0
      {
         maximum_footprint_ratio = maximum_footprint_ratio.max(resource.physical_footprint_end_bytes as f64 / resource.physical_footprint_start_bytes as f64);
      }
      retained_slope_within_guardrail &= resource.retained_slope_bytes_per_min <= 0.0;
   }
   let observations = request.observation_contract.iter().map(|contract| {
      ensure!(contract.primary_metric_id == "scene-update-p50-ns", "macOS density driver does not support primary metric {}", contract.primary_metric_id);
      let (role, scenario) = if let Some(scenario) = contract.scenario_id.strip_suffix(":sentinel-first")
      {
         ("sentinel-first", scenario)
      }
      else if let Some(scenario) = contract.scenario_id.strip_suffix(":sentinel-last")
      {
         ("sentinel-last", scenario)
      }
      else
      {
         ("scenario", contract.scenario_id.as_str())
      };
      let isolated = estimates.get(&(contract.implementation_id.clone(), MacOsDensityRunMode::Isolated, String::from(role), String::from(scenario))).with_context(|| format!("missing isolated density estimator for {} / {}", contract.scenario_id, contract.implementation_id))?;
      let packed = estimates.get(&(contract.implementation_id.clone(), MacOsDensityRunMode::Packed, String::from(role), String::from(scenario))).with_context(|| format!("missing packed density estimator for {} / {}", contract.scenario_id, contract.implementation_id))?;
      Ok(Observation {
         scenario_id: contract.scenario_id.clone(),
         implementation_id: contract.implementation_id.clone(),
         position_id: contract.position_id.clone(),
         isolated_estimator: *isolated,
         packed_estimator: *packed,
         carryover_margin_ratio: contract.carryover_margin_ratio,
         sentinel: contract.sentinel,
      })
   }).collect::<Result<Vec<_>>>()?;
   let manifest = DensitySessionManifest {
      schema_version: 1,
      request_sha256: &request_sha256,
      controller_plan_sha256: &controller_plan_sha256,
      build_manifest_sha256: &build_manifest_sha256,
      sessions: &results,
   };
   let manifest_bytes = canonical_json(&manifest)?;
   let session_manifest_sha256 = sha256(&manifest_bytes);
   write_new(&session_manifest_path(&arguments.output), &manifest_bytes)?;
   let reducer_seconds = reducer_started.elapsed().as_secs_f64();
   let thermal_after = thermal_nominal()?;
   let result = MacOsDensityDriverResult {
      schema_version: request.schema_version,
      calibration_id: request.calibration_id,
      plan_sha256: request.plan_sha256,
      request_sha256,
      invalidation_key: request.invalidation_key,
      platform_role: request.platform_role,
      candidate_id: request.candidate_id,
      pack_size: request.pack_size,
      pair_index: request.pair_index,
      order_id: request.order_id.clone(),
      controller_plan_sha256,
      build_manifest_sha256,
      session_manifest_sha256,
      occupied_seconds: acquisition_seconds,
      process_launches: results.len(),
      pair: PairEvidence {
         pair_index: request.pair_index,
         order_id: request.order_id,
         valid: true,
         reset_complete: true,
         terminators_complete: true,
         event_loss_count: 0,
         footprint_recovery_ratio: maximum_footprint_ratio,
         retained_slope_within_guardrail,
         thermal_transition_before_final: !thermal_before || !thermal_after,
         trace_capacity_ratio: 0.0,
         ring_capacity_ratio: maximum_ring_ratio,
         process_wall_seconds: maximum_process_seconds,
         acquisition_seconds,
         reducer_seconds,
         observations,
      },
   };
   write_new(&arguments.output, &canonical_json(&result)?)
}

fn arguments() -> Result<Arguments>
{
   let mut values = env::args().skip(1);
   let mut platform = None;
   let mut request = None;
   let mut output = None;
   while let Some(flag) = values.next()
   {
      let value = values.next().with_context(|| format!("missing value for {flag}"))?;
      match flag.as_str()
      {
         "--platform" => platform = Some(value),
         "--request" => request = Some(PathBuf::from(value)),
         "--output" => output = Some(PathBuf::from(value)),
         other => bail!("unknown macOS density driver argument {other}"),
      }
   }
   ensure!(platform.as_deref() == Some("macos"), "macOS density driver requires --platform macos");
   let build_manifest = absolute_environment_path("OXIDE_MACOS_DENSITY_BUILD_MANIFEST")?;
   let campaign_plan = absolute_environment_path("OXIDE_MACOS_DENSITY_CAMPAIGN_PLAN")?;
   let session_root = absolute_environment_path("OXIDE_MACOS_DENSITY_SESSION_ROOT")?;
   Ok(Arguments {
      request: request.context("macOS density driver requires --request")?,
      output: output.context("macOS density driver requires --output")?,
      build_manifest,
      campaign_plan,
      session_root,
   })
}

fn absolute_environment_path(name: &str) -> Result<PathBuf>
{
   let path = PathBuf::from(env::var(name).with_context(|| format!("missing {name}"))?);
   ensure!(path.is_absolute(), "{name} must be absolute");
   Ok(path)
}

fn schedule(request: &MacOsDensityDriverRequest, plan: &AppleCampaignPlanSpec) -> Result<(Vec<MacOsDensitySession>, Vec<ScheduledSession>)>
{
   let pass = plan.passes.iter().find(|pass| pass.id == "primary-presentation").context("density campaign has no primary-presentation pass")?;
   let sentinel = plan.packs.iter().find(|pack| pack.isolated_process && pack.ordered_scenario_ids == [request.sentinel_scenario_id.as_str()]).context("density campaign has no isolated sentinel pack")?;
   let mut controller = Vec::new();
   let mut scheduled = Vec::new();
   let mut sequence = 0_u32;
   for (treatment_index, run) in request.runs.iter().enumerate()
   {
      let side = match run.implementation_id.as_str()
      {
         "oxide" => ComparisonSide::Oxide,
         "native-production" => ComparisonSide::Native,
         other => bail!("unsupported density implementation {other}"),
      };
      append_session(request, pass.id.as_str(), sentinel.id.as_str(), side, treatment_index, sequence, run, SessionRole::SentinelFirst, sentinel.ordered_scenario_ids.clone(), &mut controller, &mut scheduled)?;
      sequence += 1;
      for pack in &run.ordered_packs
      {
         let isolated = run.mode == MacOsDensityRunMode::Isolated;
         let selected = plan.packs.iter().find(|candidate| candidate.isolated_process == isolated && candidate.ordered_scenario_ids == *pack)
            .with_context(|| format!("density campaign has no {} pack for {:?}", if isolated {"isolated"} else {"packed"}, pack))?;
         ensure!(pass.pack_ids.contains(&selected.id), "density pack {} is not selected by primary-presentation", selected.id);
         append_session(request, pass.id.as_str(), selected.id.as_str(), side, treatment_index, sequence, run, SessionRole::Scenario, pack.clone(), &mut controller, &mut scheduled)?;
         sequence += 1;
      }
      append_session(request, pass.id.as_str(), sentinel.id.as_str(), side, treatment_index, sequence, run, SessionRole::SentinelLast, sentinel.ordered_scenario_ids.clone(), &mut controller, &mut scheduled)?;
      sequence += 1;
   }
   Ok((controller, scheduled))
}

#[allow(clippy::too_many_arguments)]
fn append_session(request: &MacOsDensityDriverRequest, pass_id: &str, pack_id: &str, side: ComparisonSide, treatment_index: usize, sequence: u32, run: &oxide_perf_runner::density_acquisition::MacOsDensityRunRequest, role: SessionRole, scenarios: Vec<String>, controller: &mut Vec<MacOsDensitySession>, scheduled: &mut Vec<ScheduledSession>) -> Result<()>
{
   let session_id = format!("density-{}-p{:02}-t{:02}-s{:03}", request.candidate_id, request.pair_index, treatment_index, sequence);
   controller.push(MacOsDensitySession {session_id, pass_id: String::from(pass_id), pack_id: String::from(pack_id), pair_index: sequence, side});
   scheduled.push(ScheduledSession {implementation_id: run.implementation_id.clone(), mode: run.mode, role, scenarios});
   Ok(())
}

fn telemetry_evidence(result: &MacOsDensitySessionResult, plan_sha256: &str) -> Result<TelemetryEvidence>
{
   let telemetry = fs::read(&result.telemetry_path).with_context(|| format!("reading {}", result.telemetry_path))?;
   ensure!(sha256(&telemetry) == result.telemetry_sha256, "density telemetry hash differs from session receipt");
   ensure!(telemetry.len() >= TELEMETRY_HEADER_BYTES + TELEMETRY_FOOTER_BYTES && &telemetry[..8] == b"OXBTEL02", "density telemetry header is invalid");
   ensure!(u32_at(&telemetry, 8)? == 2 && u32_at(&telemetry, 12)? as usize == TELEMETRY_HEADER_BYTES && u32_at(&telemetry, 16)? as usize == TELEMETRY_RECORD_BYTES, "density telemetry schema is unsupported");
   let count = usize::try_from(u64_at(&telemetry, 24)?).context("density telemetry count exceeds usize")?;
   let capacity = usize::try_from(u64_at(&telemetry, 32)?).context("density telemetry capacity exceeds usize")?;
   ensure!(count > 0 && count <= capacity, "density telemetry count/capacity is invalid");
   let expected_len = TELEMETRY_HEADER_BYTES.checked_add(count.checked_mul(TELEMETRY_RECORD_BYTES).context("density telemetry size overflow")?).and_then(|size| size.checked_add(TELEMETRY_FOOTER_BYTES)).context("density telemetry size overflow")?;
   ensure!(telemetry.len() == expected_len, "density telemetry byte count differs from its header");
   ensure!(hex(&telemetry[40..72]) == plan_sha256, "density telemetry plan hash differs from controller plan");
   let footer = Sha256::digest(&telemetry[..telemetry.len() - TELEMETRY_FOOTER_BYTES]);
   ensure!(footer[..] == telemetry[telemetry.len() - TELEMETRY_FOOTER_BYTES..], "density telemetry footer hash is invalid");
   let complete_path = Path::new(&result.telemetry_path).with_file_name(Path::new(&result.telemetry_path).file_name().context("density telemetry has no filename")?.to_string_lossy().replace(".telemetry.bin", ".complete.json"));
   let complete_bytes = fs::read(&complete_path).with_context(|| format!("reading {}", complete_path.display()))?;
   ensure!(sha256(&complete_bytes) == result.campaign.artifact_sha256, "density complete envelope hash differs from session receipt");
   let complete: CompleteEnvelope = serde_json::from_slice(&complete_bytes).context("decoding density complete envelope")?;
   ensure!(complete.telemetry_sha256 == result.telemetry_sha256 && complete.telemetry_byte_count == telemetry.len() as u64, "density complete envelope telemetry identity differs");
   let numerator = u64::from(u32_at(&telemetry, 128)?);
   let denominator = u64::from(u32_at(&telemetry, 132)?);
   ensure!(numerator > 0 && denominator > 0, "density telemetry timebase is invalid");
   let mut estimators = BTreeMap::new();
   for scenario in complete.scenarios
   {
      let first = usize::try_from(scenario.first_telemetry_sequence).context("density scenario first sequence exceeds usize")?;
      let last = usize::try_from(scenario.last_telemetry_sequence).context("density scenario last sequence exceeds usize")?;
      ensure!(first <= last && last < count, "density scenario telemetry range is invalid");
      let mut begins = BTreeMap::<u64, u64>::new();
      let mut durations = Vec::new();
      for index in first..=last
      {
         let offset = TELEMETRY_HEADER_BYTES + index * TELEMETRY_RECORD_BYTES;
         ensure!(u64_at(&telemetry, offset)? == index as u64, "density telemetry sequence is noncontiguous");
         let timestamp = u64_at(&telemetry, offset + 8)?;
         let kind = u16_at(&telemetry, offset + 16)?;
         let identifier = u64_at(&telemetry, offset + 20)?;
         if kind == SCENE_UPDATE_BEGIN
         {
            ensure!(begins.insert(identifier, timestamp).is_none(), "density telemetry overlaps scene updates");
         }
         else if kind == SCENE_UPDATE_END
         {
            let begin = begins.remove(&identifier).context("density telemetry scene update has no begin")?;
            ensure!(timestamp >= begin, "density telemetry scene update time reversed");
            durations.push((timestamp - begin) as f64 * numerator as f64 / denominator as f64);
         }
      }
      ensure!(begins.is_empty() && !durations.is_empty(), "density scenario has incomplete or empty scene-update evidence");
      durations.sort_unstable_by(f64::total_cmp);
      let estimator = if durations.len() % 2 == 0 {(durations[durations.len() / 2 - 1] + durations[durations.len() / 2]) * 0.5} else {durations[durations.len() / 2]};
      ensure!(estimator.is_finite() && estimator > 0.0, "density scenario estimator is invalid");
      ensure!(estimators.insert(scenario.scenario_id, estimator).is_none(), "density complete envelope duplicated a scenario");
   }
   Ok(TelemetryEvidence {estimators, capacity_ratio: count as f64 / capacity as f64})
}

fn thermal_nominal() -> Result<bool>
{
   let output = Command::new("/usr/bin/pmset").args(["-g", "therm"]).output().context("reading macOS thermal state")?;
   ensure!(output.status.success(), "pmset thermal query failed");
   let value = String::from_utf8(output.stdout).context("pmset thermal output is not UTF-8")?.to_ascii_lowercase();
   Ok(value.contains("no thermal warning level") && value.contains("no performance warning level"))
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(value).context("encoding canonical JSON")?;
   bytes.push(b'\n');
   Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()>
{
   if let Some(parent) = path.parent()
   {
      fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
   }
   let mut file = OpenOptions::new().create_new(true).write(true).open(path).with_context(|| format!("creating {}", path.display()))?;
   file.write_all(bytes).with_context(|| format!("writing {}", path.display()))?;
   file.sync_all().with_context(|| format!("syncing {}", path.display()))?;
   Ok(())
}

fn session_manifest_path(output: &Path) -> PathBuf
{
   PathBuf::from(format!("{}.sessions.json", output.display()))
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16>
{
   let value = bytes.get(offset..offset + 2).context("density telemetry u16 exceeds input")?;
   Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32>
{
   let value = bytes.get(offset..offset + 4).context("density telemetry u32 exceeds input")?;
   Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn u64_at(bytes: &[u8], offset: usize) -> Result<u64>
{
   let value = bytes.get(offset..offset + 8).context("density telemetry u64 exceeds input")?;
   Ok(u64::from_le_bytes([value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7]]))
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String
{
   bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
