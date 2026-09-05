use anyhow::{bail, Context, Result};
use oxide_benchmark_spec::{
   compare_calibrated_static_pngs, decode_macos_correctness_geometry, decode_macos_correctness_geometry_pair,
   validate_macos_correctness_geometry_pair, validate_scenario, AppleCampaignPlanSpec, ApplePrAcquisitionSpec, ApplePrPlanSpec,
   ArtifactIdentity, CalibratedStaticVisualParityReport, CalibratedStaticVisualThresholds,
   MacOsCorrectnessGeometryEvidence, RoleCount, ScenarioSpec,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppleCorrectnessVisualCheckpoint
{
   pub scenario_id: String,
   pub checkpoint_id: String,
   pub oxide_screenshot_sha256: String,
   pub native_screenshot_sha256: String,
   pub oxide_state_sha256: String,
   pub native_state_sha256: String,
   pub oxide_accessibility_sha256: String,
   pub native_accessibility_sha256: String,
   pub oxide_geometry_sha256: String,
   pub native_geometry_sha256: String,
   pub geometry_capture_profile: String,
   pub canonical_scale: u32,
   pub geometry_accepted: bool,
   pub structural_accepted: bool,
   pub visual: CalibratedStaticVisualParityReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppleCorrectnessVisualReport
{
   pub schema_version: u64,
   pub algorithm: String,
   pub plan_sha256: String,
   pub pack_id: Option<String>,
   pub oxide_evidence_root: String,
   pub native_evidence_root: String,
   pub checkpoints: Vec<AppleCorrectnessVisualCheckpoint>,
   pub accepted_checkpoint_count: u64,
   pub rejected_checkpoint_count: u64,
   pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppleRapidVisualCheckpointReport
{
   pub schema_version: u64,
   pub algorithm: String,
   pub scenario_id: String,
   pub checkpoint_id: String,
   pub oxide_screenshot_sha256: String,
   pub native_screenshot_sha256: String,
   pub oxide_state_sha256: String,
   pub native_state_sha256: String,
   pub oxide_geometry_sha256: String,
   pub native_geometry_sha256: String,
   pub geometry_capture_profile: String,
   pub canonical_scale: u32,
   pub geometry_accepted: bool,
   pub structural_accepted: bool,
   pub visual: CalibratedStaticVisualParityReport,
   pub accepted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorrectnessCheckpointEvidence
{
   actual_state: ArtifactIdentity,
   actual_accessibility: ArtifactIdentity,
   actual_geometry: Option<ArtifactIdentity>,
   actual_screenshot: Option<ArtifactIdentity>,
   validation: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RapidCheckpointEvidence
{
   schema_version: u32,
   side: String,
   mode: String,
   #[serde(rename = "scenarioID")]
   scenario_id: String,
   #[serde(rename = "checkpointID")]
   checkpoint_id: String,
   actual_state: ArtifactIdentity,
   actual_geometry: Option<ArtifactIdentity>,
   actual_screenshot: Option<ArtifactIdentity>,
   visible_role_counts: Vec<RoleCount>,
   validation: String,
}

pub fn reduce_apple_correctness_evidence(oxide_root: &Path, native_root: &Path, spec_root: &Path, pack_id: Option<&str>) -> Result<AppleCorrectnessVisualReport>
{
   let plan_path = spec_root.join("plans/apple-pr.json");
   let plan_bytes = fs::read(&plan_path).with_context(|| format!("reading {}", plan_path.display()))?;
   let plan: ApplePrPlanSpec = serde_json::from_slice(&plan_bytes).with_context(|| format!("decoding {}", plan_path.display()))?;
   let plan_sha256 = sha256(&plan_bytes);
   let selected_scenarios = if let Some(pack_id) = pack_id
   {
      let acquisition_path = spec_root.join("acquisition/apple-pr.json");
      let acquisition_bytes = fs::read(&acquisition_path).with_context(|| format!("reading {}", acquisition_path.display()))?;
      let acquisition: ApplePrAcquisitionSpec = serde_json::from_slice(&acquisition_bytes).with_context(|| format!("decoding {}", acquisition_path.display()))?;
      Some(acquisition.packs.iter().find(|pack| pack.id == pack_id).with_context(|| format!("unknown Apple PR pack {}", pack_id))?.ordered_scenario_ids.clone())
   }
   else
   {
      None
   };
   let scenarios = plan.scenarios.iter().filter(|entry| {
      !selected_scenarios.as_ref().is_some_and(|selected| !selected.contains(&entry.id))
   }).map(|entry| CorrectnessScenarioBinding {
      id: entry.id.clone(),
      artifact: entry.artifact.clone(),
   }).collect::<Vec<_>>();
   reduce_bound_correctness_evidence(
      oxide_root,
      native_root,
      spec_root,
      &scenarios,
      &plan_sha256,
      pack_id.map(String::from),
   )
}

pub fn reduce_generic_apple_correctness_evidence(oxide_root: &Path, native_root: &Path, spec_root: &Path, plan_path: &Path) -> Result<AppleCorrectnessVisualReport>
{
   let plan_bytes = fs::read(plan_path).with_context(|| format!("reading {}", plan_path.display()))?;
   let plan: AppleCampaignPlanSpec = serde_json::from_slice(&plan_bytes).with_context(|| format!("decoding {}", plan_path.display()))?;
   let scenarios = plan.scenarios.iter().map(|entry| Ok(CorrectnessScenarioBinding {
      id: entry.id.clone(),
      artifact: entry.artifact.clone().with_context(|| format!("generic correctness scenario {} has no artifact", entry.id))?,
   })).collect::<Result<Vec<_>>>()?;
   reduce_bound_correctness_evidence(
      oxide_root,
      native_root,
      spec_root,
      &scenarios,
      &sha256(&plan_bytes),
      Some(String::from("direct")),
   )
}

#[derive(Clone)]
struct CorrectnessScenarioBinding
{
   id: String,
   artifact: ArtifactIdentity,
}

fn reduce_bound_correctness_evidence(oxide_root: &Path, native_root: &Path, spec_root: &Path, scenarios: &[CorrectnessScenarioBinding], plan_sha256: &str, pack_id: Option<String>) -> Result<AppleCorrectnessVisualReport>
{
   let mut checkpoints = Vec::new();
   for scenario_entry in scenarios
   {
      validate_path_component(&scenario_entry.id, "scenario")?;
      let scenario_bytes = read_verified_artifact(spec_root, &scenario_entry.artifact)?;
      let scenario: ScenarioSpec = serde_json::from_slice(&scenario_bytes).with_context(|| format!("decoding scenario {}", scenario_entry.id))?;
      if scenario.id != scenario_entry.id
      {
         bail!("scenario identity differs from Apple PR plan: {}", scenario_entry.id);
      }
      for checkpoint in &scenario.parity_checkpoints
      {
         validate_path_component(&checkpoint.id, "checkpoint")?;
         let oxide = read_verified_correctness_checkpoint(oxide_root, &scenario.id, &checkpoint.id)?;
         let native = read_verified_correctness_checkpoint(native_root, &scenario.id, &checkpoint.id)?;
         let runtime_geometry_accepted = validate_macos_correctness_geometry_pair(&native.decoded_geometry, &oxide.decoded_geometry).is_ok();
         let frozen_geometry_accepted = if let Some(identity) = &checkpoint.geometry
         {
            let expected = decode_macos_correctness_geometry_pair(&read_verified_artifact(spec_root, identity)?)?;
            expected.native == native.decoded_geometry && expected.oxide == oxide.decoded_geometry
         }
         else
         {
            true
         };
         let geometry_accepted = runtime_geometry_accepted && frozen_geometry_accepted;
         let structural_accepted = oxide.state == native.state
            && oxide.accessibility == native.accessibility
            && geometry_accepted
            && oxide.validation == "exact-canonical-state-accessibility-and-role-counts"
            && native.validation == "exact-canonical-state-accessibility-and-role-counts";
         let visual = compare_calibrated_static_pngs(
            &oxide.screenshot,
            &native.screenshot,
            &oxide.geometry,
            oxide.decoded_geometry.canonical_scale,
            CalibratedStaticVisualThresholds::default(),
         )?;
         checkpoints.push(AppleCorrectnessVisualCheckpoint {
            scenario_id: scenario.id.clone(),
            checkpoint_id: checkpoint.id.clone(),
            oxide_screenshot_sha256: sha256(&oxide.screenshot),
            native_screenshot_sha256: sha256(&native.screenshot),
            oxide_state_sha256: sha256(&oxide.state),
            native_state_sha256: sha256(&native.state),
            oxide_accessibility_sha256: sha256(&oxide.accessibility),
            native_accessibility_sha256: sha256(&native.accessibility),
            oxide_geometry_sha256: sha256(&oxide.geometry),
            native_geometry_sha256: sha256(&native.geometry),
            geometry_capture_profile: oxide.decoded_geometry.capture_profile.clone(),
            canonical_scale: oxide.decoded_geometry.canonical_scale,
            geometry_accepted,
            structural_accepted,
            visual,
         });
      }
   }
   let accepted_checkpoint_count = checkpoints.iter().filter(|checkpoint| checkpoint.structural_accepted && checkpoint.visual.accepted).count() as u64;
   let rejected_checkpoint_count = checkpoints.len() as u64 - accepted_checkpoint_count;
   Ok(AppleCorrectnessVisualReport {
      schema_version: 5,
      algorithm: String::from("apple-correctness-semantic-region-static-matrix-v5"),
      plan_sha256: String::from(plan_sha256),
      pack_id,
      oxide_evidence_root: oxide_root.to_string_lossy().into_owned(),
      native_evidence_root: native_root.to_string_lossy().into_owned(),
      checkpoints,
      accepted_checkpoint_count,
      rejected_checkpoint_count,
      accepted: rejected_checkpoint_count == 0,
   })
}

struct VerifiedCorrectnessCheckpoint
{
   state: Vec<u8>,
   accessibility: Vec<u8>,
   geometry: Vec<u8>,
   decoded_geometry: MacOsCorrectnessGeometryEvidence,
   screenshot: Vec<u8>,
   validation: String,
}

pub fn compare_apple_correctness_evidence(oxide_root: &Path, native_root: &Path, spec_root: &Path, pack_id: Option<&str>, output_path: &Path) -> Result<AppleCorrectnessVisualReport>
{
   let report = reduce_apple_correctness_evidence(oxide_root, native_root, spec_root, pack_id)?;
   durable_json(&report, output_path)?;
   Ok(report)
}

pub fn compare_apple_rapid_visual_checkpoint(oxide_root: &Path, native_root: &Path, spec_root: &Path, scenario_id: &str, checkpoint_id: &str, output_path: &Path) -> Result<AppleRapidVisualCheckpointReport>
{
   validate_path_component(scenario_id, "scenario")?;
   validate_path_component(checkpoint_id, "checkpoint")?;
   let scenario_path = spec_root.join("scenarios").join(format!("{scenario_id}.json"));
   let scenario_bytes = fs::read(&scenario_path).with_context(|| format!("reading {}", scenario_path.display()))?;
   let scenario: ScenarioSpec = serde_json::from_slice(&scenario_bytes).with_context(|| format!("decoding {}", scenario_path.display()))?;
   validate_scenario(&scenario)?;
   if scenario.id != scenario_id
   {
      bail!("targeted scenario identity differs from its path: {}", scenario_id);
   }
   let checkpoint = scenario.parity_checkpoints.iter().find(|checkpoint| checkpoint.id == checkpoint_id)
      .with_context(|| format!("targeted scenario {} has no checkpoint {}", scenario_id, checkpoint_id))?;
   let oxide = read_verified_rapid_checkpoint(oxide_root, scenario_id, checkpoint_id, "oxide")?;
   let native = read_verified_rapid_checkpoint(native_root, scenario_id, checkpoint_id, "native")?;
   let runtime_geometry_accepted = validate_macos_correctness_geometry_pair(&native.decoded_geometry, &oxide.decoded_geometry).is_ok();
   let frozen_geometry_accepted = if let Some(identity) = &checkpoint.geometry
   {
      let expected = decode_macos_correctness_geometry_pair(&read_verified_artifact(spec_root, identity)?)?;
      expected.native == native.decoded_geometry && expected.oxide == oxide.decoded_geometry
   }
   else
   {
      true
   };
   let expected_state = read_verified_artifact(spec_root, &checkpoint.state)?;
   let structural_accepted = runtime_geometry_accepted
      && frozen_geometry_accepted
      && json_equal(&oxide.state, &expected_state)?
      && json_equal(&native.state, &expected_state)?
      && oxide.roles == checkpoint.expected_visible_role_counts
      && native.roles == checkpoint.expected_visible_role_counts;
   let visual = compare_calibrated_static_pngs(
      &oxide.screenshot,
      &native.screenshot,
      &oxide.geometry,
      oxide.decoded_geometry.canonical_scale,
      CalibratedStaticVisualThresholds::default(),
   )?;
   let accepted = structural_accepted && visual.accepted;
   let report = AppleRapidVisualCheckpointReport {
      schema_version: 1,
      algorithm: String::from("apple-rapid-targeted-visual-v1"),
      scenario_id: String::from(scenario_id),
      checkpoint_id: String::from(checkpoint_id),
      oxide_screenshot_sha256: sha256(&oxide.screenshot),
      native_screenshot_sha256: sha256(&native.screenshot),
      oxide_state_sha256: sha256(&oxide.state),
      native_state_sha256: sha256(&native.state),
      oxide_geometry_sha256: sha256(&oxide.geometry),
      native_geometry_sha256: sha256(&native.geometry),
      geometry_capture_profile: oxide.decoded_geometry.capture_profile.clone(),
      canonical_scale: oxide.decoded_geometry.canonical_scale,
      geometry_accepted: runtime_geometry_accepted && frozen_geometry_accepted,
      structural_accepted,
      visual,
      accepted,
   };
   durable_json(&report, output_path)?;
   Ok(report)
}

struct VerifiedRapidCheckpoint
{
   state: Vec<u8>,
   roles: Vec<RoleCount>,
   geometry: Vec<u8>,
   decoded_geometry: MacOsCorrectnessGeometryEvidence,
   screenshot: Vec<u8>,
}

fn read_verified_rapid_checkpoint(root: &Path, scenario_id: &str, checkpoint_id: &str, side: &str) -> Result<VerifiedRapidCheckpoint>
{
   let directory = root.join(scenario_id).join(checkpoint_id);
   let evidence_path = directory.join("visual.evidence.json");
   let evidence_bytes = fs::read(&evidence_path).with_context(|| format!("reading {}", evidence_path.display()))?;
   let evidence: RapidCheckpointEvidence = serde_json::from_slice(&evidence_bytes).with_context(|| format!("decoding {}", evidence_path.display()))?;
   if evidence.schema_version != 1
      || evidence.side != side
      || evidence.mode != "visual"
      || evidence.scenario_id != scenario_id
      || evidence.checkpoint_id != checkpoint_id
      || evidence.validation != "exact-canonical-state-role-counts-geometry-and-png"
   {
      bail!("targeted visual evidence identity differs from the requested checkpoint");
   }
   let state = read_verified_checkpoint_artifact(root, scenario_id, checkpoint_id, &directory, &evidence.actual_state, "state.actual.json", "state")?;
   let geometry_identity = evidence.actual_geometry.context("targeted visual checkpoint has no geometry")?;
   let geometry = read_verified_checkpoint_artifact(root, scenario_id, checkpoint_id, &directory, &geometry_identity, "geometry.actual.json", "geometry")?;
   let screenshot_identity = evidence.actual_screenshot.context("targeted visual checkpoint has no screenshot")?;
   let screenshot = read_verified_checkpoint_artifact(root, scenario_id, checkpoint_id, &directory, &screenshot_identity, "screenshot.actual.png", "screenshot")?;
   let decoded_geometry = decode_macos_correctness_geometry(&geometry)?;
   Ok(VerifiedRapidCheckpoint {
      state,
      roles: evidence.visible_role_counts,
      geometry,
      decoded_geometry,
      screenshot,
   })
}

fn json_equal(left: &[u8], right: &[u8]) -> Result<bool>
{
   let left: serde_json::Value = serde_json::from_slice(left).context("decoding targeted actual state")?;
   let right: serde_json::Value = serde_json::from_slice(right).context("decoding targeted expected state")?;
   Ok(left == right)
}

fn read_verified_artifact(root: &Path, identity: &ArtifactIdentity) -> Result<Vec<u8>>
{
   let path = safe_relative_join(root, &identity.path)?;
   let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
   if sha256(&bytes) != identity.sha256
   {
      bail!("artifact hash mismatch: {}", identity.path);
   }
   Ok(bytes)
}

fn read_verified_correctness_checkpoint(root: &Path, scenario_id: &str, checkpoint_id: &str) -> Result<VerifiedCorrectnessCheckpoint>
{
   validate_path_component(scenario_id, "scenario")?;
   validate_path_component(checkpoint_id, "checkpoint")?;
   let directory = root.join(scenario_id).join(checkpoint_id);
   let evidence_path = directory.join("evidence.json");
   let evidence_bytes = fs::read(&evidence_path).with_context(|| format!("reading {}", evidence_path.display()))?;
   let evidence: CorrectnessCheckpointEvidence = serde_json::from_slice(&evidence_bytes).with_context(|| format!("decoding {}", evidence_path.display()))?;
   let screenshot = read_verified_checkpoint_artifact(root, scenario_id, checkpoint_id, &directory, &evidence.actual_screenshot.context("correctness checkpoint has no screenshot")?, "screenshot.actual.png", "screenshot")?;
   let state = read_verified_checkpoint_artifact(root, scenario_id, checkpoint_id, &directory, &evidence.actual_state, "state.actual.json", "state")?;
   let accessibility = read_verified_checkpoint_artifact(root, scenario_id, checkpoint_id, &directory, &evidence.actual_accessibility, "accessibility.actual.json", "accessibility")?;
   let geometry_identity = evidence.actual_geometry.context("correctness checkpoint has no runtime geometry")?;
   let geometry = read_verified_checkpoint_artifact(root, scenario_id, checkpoint_id, &directory, &geometry_identity, "geometry.actual.json", "geometry")?;
   let decoded_geometry = decode_macos_correctness_geometry(&geometry)?;
   Ok(VerifiedCorrectnessCheckpoint {state, accessibility, geometry, decoded_geometry, screenshot, validation: evidence.validation})
}

fn read_verified_checkpoint_artifact(root: &Path, scenario_id: &str, checkpoint_id: &str, directory: &Path, identity: &ArtifactIdentity, file_name: &str, label: &str) -> Result<Vec<u8>>
{
   let relative = normalized_relative_path(&identity.path)?;
   let root_name = root.file_name().and_then(|name| name.to_str()).context("correctness evidence root has no UTF-8 terminal component")?;
   let suffix = Path::new(root_name).join(scenario_id).join(checkpoint_id).join(file_name);
   if !relative.ends_with(&suffix)
   {
      bail!("correctness {} identity path does not bind the evidence root and checkpoint: {}", label, identity.path);
   }
   let path = directory.join(file_name);
   let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
   if sha256(&bytes) != identity.sha256
   {
      bail!("correctness {} hash mismatch: {}:{}", label, scenario_id, checkpoint_id);
   }
   Ok(bytes)
}

fn safe_relative_join(root: &Path, relative: &str) -> Result<PathBuf>
{
   Ok(root.join(normalized_relative_path(relative)?))
}

fn normalized_relative_path(value: &str) -> Result<&Path>
{
   let path = Path::new(value);
   if path.as_os_str().is_empty() || path.is_absolute() || path.components().any(|component| !matches!(component, Component::Normal(_)))
   {
      bail!("artifact path is not normalized and relative: {}", path.display());
   }
   Ok(path)
}

fn validate_path_component(value: &str, kind: &str) -> Result<()>
{
   let path = normalized_relative_path(value)?;
   if path.components().count() != 1
   {
      bail!("{} identity is not a single path component: {}", kind, value);
   }
   Ok(())
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn durable_json<T: Serialize>(value: &T, destination: &Path) -> Result<()>
{
   let parent = destination.parent().context("correctness report destination has no parent")?;
   fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
   let bytes = serde_json::to_vec_pretty(value).context("encoding Apple correctness report")?;
   let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).context("system clock precedes Unix epoch")?.as_nanos();
   let temporary = parent.join(format!(".correctness.{}.{}.tmp", std::process::id(), timestamp));
   {
      let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary).with_context(|| format!("creating {}", temporary.display()))?;
      file.write_all(&bytes).with_context(|| format!("writing {}", temporary.display()))?;
      file.write_all(b"\n").with_context(|| format!("terminating {}", temporary.display()))?;
      file.sync_all().with_context(|| format!("synchronizing {}", temporary.display()))?;
   }
   fs::rename(&temporary, destination).with_context(|| format!("renaming {} to {}", temporary.display(), destination.display()))?;
   File::open(parent).with_context(|| format!("opening {}", parent.display()))?.sync_all().with_context(|| format!("synchronizing {}", parent.display()))?;
   Ok(())
}
