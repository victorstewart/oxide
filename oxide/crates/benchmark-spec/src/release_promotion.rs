use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{canonical_apple_campaign_plan_json, canonical_macos_comparator_qualification_plan_json, canonical_macos_comparator_scale_overlay_json, compare_calibrated_static_pngs, decode_macos_correctness_geometry, macos_comparator_scale_contract, validate_macos_correctness_geometry_pair, validate_scenario, AppleCampaignBudgetComponent, AppleCampaignBudgetComponentSpec, AppleCampaignEvidenceRole, AppleCampaignMeasurementTimingSpec, AppleCampaignPackSpec, AppleCampaignPassRole, AppleCampaignPassSpec, AppleCampaignPassTimingSpec, AppleCampaignPlanSpec, AppleCampaignScenarioBinding, AppleCampaignScenarioTimingSpec, AppleCampaignTimingSpec, ArtifactIdentity, CalibratedStaticVisualParityReport, CalibratedStaticVisualThresholds, MacOsComparatorQualificationPlan, MacOsComparatorScale, MacOsComparatorScaleOverlay, MacOsComparatorScaleVariant, MacOsCorrectnessGeometryPairEvidence, Platform, ScenarioSpec, Tier, APPLE_RELEASE_SCENARIO_IDS, MACOS_COMPARATOR_SCALE_SCHEMA_VERSION};

pub const RELEASE_CANDIDATE_IDS: [&str; 5] = ["grid.large-scroll", "effects.layers", "mutation.damage", "text.multilingual", "resize.theme"];

#[derive(Debug, Serialize)]
pub struct ReleasePromotionCheckpointReport
{
   pub id: String,
   pub appkit_png_sha256: String,
   pub oxide_png_sha256: String,
   pub geometry_sha256: String,
   pub visual: CalibratedStaticVisualParityReport,
}

#[derive(Debug, Serialize)]
pub struct ReleasePromotionScenarioReport
{
   pub id: String,
   pub candidate_sha256: String,
   pub manifest_sha256: String,
   pub checkpoints: Vec<ReleasePromotionCheckpointReport>,
}

#[derive(Debug, Serialize)]
pub struct ReleasePromotionReport
{
   pub schema_version: u32,
   pub canonical_scale: u32,
   pub scenarios: Vec<ReleasePromotionScenarioReport>,
   pub qualification: ReleasePromotionQualificationReport,
}

#[derive(Debug, Serialize)]
pub struct ReleasePromotionQualificationReport
{
   pub execution_plan: ArtifactIdentity,
   pub qualification_plan: ArtifactIdentity,
   pub scale_overlays: Vec<ArtifactIdentity>,
}

struct PlannedScenario
{
   id: String,
   manifest: Vec<u8>,
   checkpoint_artifacts: Vec<(PathBuf, Vec<u8>)>,
   report: ReleasePromotionScenarioReport,
}

struct PlannedQualification
{
   files: Vec<(PathBuf, Vec<u8>)>,
   report: ReleasePromotionQualificationReport,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeCheckpointEvidence
{
   actual_state: ArtifactIdentity,
   actual_accessibility: ArtifactIdentity,
   actual_geometry: Option<ArtifactIdentity>,
   actual_screenshot: Option<ArtifactIdentity>,
   validation: String,
}

struct RuntimeCheckpoint
{
   state: Vec<u8>,
   accessibility: Vec<u8>,
   geometry: Vec<u8>,
   screenshot: Vec<u8>,
   validation: String,
}

pub fn promote_release_candidates(spec_root: &Path, evidence_root: &Path, output_root: &Path, canonical_scale: u32) -> Result<ReleasePromotionReport>
{
   ensure!(canonical_scale > 0, "release promotion canonical scale must be greater than zero");
   ensure!(spec_root.is_dir(), "release promotion spec root does not exist: {}", spec_root.display());
   ensure!(evidence_root.is_dir(), "release promotion evidence root does not exist: {}", evidence_root.display());
   ensure!(!output_root.exists(), "release promotion output already exists: {}", output_root.display());
   let spec_root = fs::canonicalize(spec_root).context("canonicalizing release promotion spec root")?;
   let evidence_root = fs::canonicalize(evidence_root).context("canonicalizing release promotion evidence root")?;
   let output_parent = output_root.parent().context("release promotion output has no parent")?;
   let output_parent = fs::canonicalize(output_parent).context("canonicalizing release promotion output parent")?;
   let output_name = output_root.file_name().context("release promotion output has no final component")?;
   let output_root = output_parent.join(output_name);
   ensure!(!output_root.starts_with(&spec_root) && !output_root.starts_with(&evidence_root), "release promotion output must be outside the input roots");

   let mut planned = Vec::with_capacity(RELEASE_CANDIDATE_IDS.len());
   for id in RELEASE_CANDIDATE_IDS
   {
      planned.push(plan_candidate(&spec_root, &evidence_root, id, canonical_scale)?);
   }
   let qualification = plan_qualification(&spec_root, &planned)?;
   let report = ReleasePromotionReport {
      schema_version: 1,
      canonical_scale,
      scenarios: planned.iter().map(|scenario| ReleasePromotionScenarioReport {
         id: scenario.report.id.clone(),
         candidate_sha256: scenario.report.candidate_sha256.clone(),
         manifest_sha256: scenario.report.manifest_sha256.clone(),
         checkpoints: scenario.report.checkpoints.iter().map(|checkpoint| ReleasePromotionCheckpointReport {
            id: checkpoint.id.clone(),
            appkit_png_sha256: checkpoint.appkit_png_sha256.clone(),
            oxide_png_sha256: checkpoint.oxide_png_sha256.clone(),
            geometry_sha256: checkpoint.geometry_sha256.clone(),
            visual: checkpoint.visual.clone(),
         }).collect(),
      }).collect(),
      qualification: ReleasePromotionQualificationReport {
         execution_plan: qualification.report.execution_plan.clone(),
         qualification_plan: qualification.report.qualification_plan.clone(),
         scale_overlays: qualification.report.scale_overlays.clone(),
      },
   };
   let stage = output_parent.join(format!(".{}.release-promotion-{}", output_name.to_string_lossy(), std::process::id()));
   ensure!(!stage.exists(), "release promotion staging path already exists: {}", stage.display());
   fs::create_dir(&stage).context("creating release promotion staging root")?;
   let result = (|| -> Result<()> {
      copy_tree(&spec_root, &stage)?;
      for scenario in &planned
      {
         for (relative, bytes) in &scenario.checkpoint_artifacts
         {
            write_generated(&stage.join(relative), bytes)?;
         }
         write_generated(&stage.join("scenarios").join(format!("{}.json", scenario.id)), &scenario.manifest)?;
      }
      for (relative, bytes) in &qualification.files
      {
         write_generated(&stage.join(relative), bytes)?;
      }
      let mut report_bytes = serde_json::to_vec_pretty(&report).context("encoding release promotion report")?;
      report_bytes.push(b'\n');
      write_generated(&stage.join("release-candidates/promotion-report.json"), &report_bytes)?;
      for scenario in &planned
      {
         let parsed = serde_json::from_slice::<ScenarioSpec>(&scenario.manifest).context("parsing staged promoted scenario")?;
         validate_scenario(&parsed).context("validating staged promoted scenario")?;
      }
      fs::rename(&stage, &output_root).context("atomically publishing release promotion output")?;
      Ok(())
   })();
   if result.is_err()
   {
      let _ = fs::remove_dir_all(&stage);
   }
   result?;
   Ok(report)
}

fn plan_qualification(spec_root: &Path, promoted: &[PlannedScenario]) -> Result<PlannedQualification>
{
   let mut files = Vec::new();
   let mut scenarios = Vec::with_capacity(APPLE_RELEASE_SCENARIO_IDS.len());
   let mut packs = Vec::with_capacity(APPLE_RELEASE_SCENARIO_IDS.len());
   let mut timing = Vec::with_capacity(APPLE_RELEASE_SCENARIO_IDS.len());
   let mut variants = Vec::with_capacity(APPLE_RELEASE_SCENARIO_IDS.len() * 2);
   let mut scale_overlays = Vec::with_capacity(APPLE_RELEASE_SCENARIO_IDS.len() * 2);
   for id in APPLE_RELEASE_SCENARIO_IDS
   {
      let manifest = if let Some(candidate) = promoted.iter().find(|candidate| candidate.id == *id)
      {
         candidate.manifest.clone()
      }
      else
      {
         fs::read(spec_root.join("scenarios").join(format!("{id}.json"))).with_context(|| format!("reading qualification scenario {id}"))?
      };
      let scenario = serde_json::from_slice::<ScenarioSpec>(&manifest).with_context(|| format!("parsing qualification scenario {id}"))?;
      validate_scenario(&scenario).with_context(|| format!("validating qualification scenario {id}"))?;
      let scenario_path = format!("scenarios/{id}.json");
      scenarios.push(AppleCampaignScenarioBinding {
         id: String::from(*id),
         expected_path: scenario_path.clone(),
         artifact: Some(ArtifactIdentity {path: scenario_path, sha256: sha256(&manifest)}),
      });
      packs.push(AppleCampaignPackSpec {
         id: String::from(*id),
         ordered_scenario_ids: vec![String::from(*id)],
         isolated_process: true,
      });
      timing.push(AppleCampaignScenarioTimingSpec {
         scenario_id: String::from(*id),
         setup_seconds: 1,
         warmup_seconds: 1,
         measurement: AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: 28},
      });
      let (dimension, transform, base_cardinality) = macos_comparator_scale_contract(id)?;
      for scale in [MacOsComparatorScale::OneX, MacOsComparatorScale::TwoX]
      {
         let multiplier = if scale == MacOsComparatorScale::OneX {1} else {2};
         let overlay = MacOsComparatorScaleOverlay {
            schema_version: MACOS_COMPARATOR_SCALE_SCHEMA_VERSION,
            scenario_id: String::from(*id),
            scale,
            dimension,
            transform,
            base_cardinality,
            effective_cardinality: base_cardinality.checked_mul(multiplier).context("qualification scale cardinality overflow")?,
            fixture: scenario.fixture.clone(),
         };
         let bytes = canonical_macos_comparator_scale_overlay_json(&overlay)?;
         let suffix = if scale == MacOsComparatorScale::OneX {"1x"} else {"2x"};
         let path = PathBuf::from("qualification/macos-scale").join(format!("{id}.{suffix}.json"));
         let identity = ArtifactIdentity {path: String::from(path_string(&path)?), sha256: sha256(&bytes)};
         variants.push(MacOsComparatorScaleVariant {
            scenario_id: String::from(*id),
            scale,
            overlay: identity.clone(),
         });
         scale_overlays.push(identity);
         files.push((path, bytes));
      }
   }

   let qualification_plan = MacOsComparatorQualificationPlan {
      schema_version: MACOS_COMPARATOR_SCALE_SCHEMA_VERSION,
      profile_window_seconds: 30,
      refresh_interval_ns: 16_666_667,
      variants,
   };
   let qualification_bytes = canonical_macos_comparator_qualification_plan_json(&qualification_plan)?;
   let qualification_path = PathBuf::from("qualification/macos-comparator-qualification.json");
   let qualification_identity = ArtifactIdentity {path: String::from(path_string(&qualification_path)?), sha256: sha256(&qualification_bytes)};
   files.push((qualification_path, qualification_bytes));

   let pass_id = String::from("attribution-time-profiler");
   let scenario_ids = APPLE_RELEASE_SCENARIO_IDS.iter().map(|id| String::from(*id)).collect::<Vec<_>>();
   let execution_plan = AppleCampaignPlanSpec {
      schema_version: 1,
      id: String::from("macos-comparator-qualification"),
      platform: Platform::Apple,
      tier: Tier::Extended,
      budget_id: String::from("macos-comparator-qualification"),
      timing: AppleCampaignTimingSpec {passes: vec![AppleCampaignPassTimingSpec {
         pass_id: pass_id.clone(),
         reset_seconds_per_session: 5,
         readiness_timeout_seconds: 30,
         scenarios: timing,
      }]},
      scenarios,
      packs: packs.clone(),
      passes: vec![AppleCampaignPassSpec {
         id: pass_id.clone(),
         role: AppleCampaignPassRole::Attribution,
         evidence_role: AppleCampaignEvidenceRole::DescriptiveDiagnostic,
         pair_count: 2,
         pack_ids: packs.into_iter().map(|pack| pack.id).collect(),
         scenario_ids,
         launch_classes: Vec::new(),
         collector: Some(String::from("time-profiler")),
      }],
      budget_components: vec![
         AppleCampaignBudgetComponentSpec {component: AppleCampaignBudgetComponent::CorrectnessInstallPulls, occupied_seconds: 0, pass_ids: Vec::new()},
         AppleCampaignBudgetComponentSpec {component: AppleCampaignBudgetComponent::PrimaryDynamicPresentation, occupied_seconds: 0, pass_ids: Vec::new()},
         AppleCampaignBudgetComponentSpec {component: AppleCampaignBudgetComponent::LaunchOrStartupDelivery, occupied_seconds: 0, pass_ids: Vec::new()},
         AppleCampaignBudgetComponentSpec {component: AppleCampaignBudgetComponent::IdleEndurance, occupied_seconds: 0, pass_ids: Vec::new()},
         AppleCampaignBudgetComponentSpec {component: AppleCampaignBudgetComponent::Energy, occupied_seconds: 0, pass_ids: Vec::new()},
         AppleCampaignBudgetComponentSpec {component: AppleCampaignBudgetComponent::Attribution, occupied_seconds: 1_560, pass_ids: vec![pass_id]},
      ],
   };
   let execution_bytes = canonical_apple_campaign_plan_json(&execution_plan)?;
   let execution_path = PathBuf::from("plans/macos-comparator-qualification.json");
   let execution_identity = ArtifactIdentity {path: String::from(path_string(&execution_path)?), sha256: sha256(&execution_bytes)};
   files.push((execution_path, execution_bytes));
   Ok(PlannedQualification {
      files,
      report: ReleasePromotionQualificationReport {
         execution_plan: execution_identity,
         qualification_plan: qualification_identity,
         scale_overlays,
      },
   })
}

fn plan_candidate(spec_root: &Path, evidence_root: &Path, id: &str, canonical_scale: u32) -> Result<PlannedScenario>
{
   let candidate_path = spec_root.join("release-candidates").join(format!("{id}.candidate.json"));
   let candidate_bytes = fs::read(&candidate_path).with_context(|| format!("reading release candidate {}", candidate_path.display()))?;
   let candidate = serde_json::from_slice::<Value>(&candidate_bytes).context("parsing release candidate")?;
   let mut canonical = serde_json::to_vec(&candidate).context("canonicalizing release candidate")?;
   canonical.push(b'\n');
   ensure!(candidate_bytes == canonical, "release candidate {id} is not canonical compact JSON");
   ensure!(candidate["schema_version"] == 1 && candidate["id"] == id, "release candidate {id} identity is invalid");
   ensure!(candidate["candidate_status"] == "blocked-on-canonical-screenshots", "release candidate {id} has an invalid status");
   ensure!(candidate["owning_pass"] == "minimal-presentation", "release candidate {id} has an invalid owning pass");
   ensure!(candidate["screenshot_materialization"]["required"] == true && candidate["screenshot_materialization"]["status"] == "blocked", "release candidate {id} is not blocked on screenshots");
   ensure!(candidate["screenshot_materialization"]["scenario_manifest_path"] == format!("scenarios/{id}.json"), "release candidate {id} has a noncanonical manifest destination");
   for artifact in candidate_artifacts(&candidate)?
   {
      verify_artifact(spec_root, artifact)?;
   }

   let checkpoint_values = candidate["parity_checkpoints"].as_array().context("release candidate checkpoints are not an array")?;
   ensure!(!checkpoint_values.is_empty(), "release candidate {id} has no checkpoints");
   let expected = candidate["expected_visible_role_counts_by_checkpoint"].as_object().context("release candidate role counts are not an object")?;
   let mut promoted_checkpoints = Vec::with_capacity(checkpoint_values.len());
   let mut checkpoint_artifacts = Vec::with_capacity(checkpoint_values.len() * 2);
   let mut checkpoint_reports = Vec::with_capacity(checkpoint_values.len());
   let mut initial_root = None;
   for checkpoint in checkpoint_values
   {
      let checkpoint_id = checkpoint["id"].as_str().context("release candidate checkpoint has no id")?;
      validate_identifier(checkpoint_id, "checkpoint")?;
      let appkit = read_runtime_checkpoint(&evidence_root.join("native.evidence"), id, checkpoint_id)?;
      let oxide = read_runtime_checkpoint(&evidence_root.join("oxide.evidence"), id, checkpoint_id)?;
      ensure!(appkit.validation == "exact-canonical-state-accessibility-and-role-counts" && oxide.validation == appkit.validation, "{id}/{checkpoint_id} runtime evidence has a noncanonical validation contract");
      let appkit_state = exact_json_bytes(&appkit.state)?;
      let oxide_state = exact_json_bytes(&oxide.state)?;
      let expected_state = exact_json(&spec_root.join(artifact_path(&checkpoint["state"])?))?;
      ensure!(appkit_state == oxide_state && appkit_state == expected_state, "{id}/{checkpoint_id} state differs across AppKit, Oxide, or the frozen checkpoint");
      let appkit_accessibility = exact_json_bytes(&appkit.accessibility)?;
      let oxide_accessibility = exact_json_bytes(&oxide.accessibility)?;
      let expected_accessibility = exact_json(&spec_root.join(artifact_path(&checkpoint["accessibility"])?))?;
      ensure!(appkit_accessibility == oxide_accessibility && appkit_accessibility == expected_accessibility, "{id}/{checkpoint_id} accessibility differs across AppKit, Oxide, or the frozen checkpoint");
      let decoded_geometry = decode_macos_correctness_geometry(&appkit.geometry).with_context(|| format!("validating {id}/{checkpoint_id} runtime geometry"))?;
      let decoded_oxide_geometry = decode_macos_correctness_geometry(&oxide.geometry).with_context(|| format!("validating {id}/{checkpoint_id} Oxide runtime geometry"))?;
      validate_macos_correctness_geometry_pair(&decoded_geometry, &decoded_oxide_geometry).with_context(|| format!("matching {id}/{checkpoint_id} runtime geometry"))?;
      ensure!(decoded_geometry.canonical_scale == canonical_scale, "{id}/{checkpoint_id} runtime geometry scale differs from the requested canonical scale");
      if initial_root.is_none()
      {
         initial_root = Some((decoded_geometry.root.width as u32, decoded_geometry.root.height as u32));
      }
      let visual = compare_calibrated_static_pngs(&oxide.screenshot, &appkit.screenshot, &appkit.geometry, decoded_geometry.canonical_scale, CalibratedStaticVisualThresholds::default()).with_context(|| format!("reducing {id}/{checkpoint_id} calibrated visual parity"))?;
      ensure!(visual.accepted, "{id}/{checkpoint_id} calibrated visual parity was rejected");
      let screenshot_path = PathBuf::from("checkpoints").join(id).join(checkpoint_id).join("screenshot.png");
      let geometry_path = PathBuf::from("checkpoints").join(id).join(checkpoint_id).join("geometry.json");
      let screenshot_sha = sha256(&appkit.screenshot);
      let geometry_pair = MacOsCorrectnessGeometryPairEvidence {schema_version: 1, native: decoded_geometry.clone(), oxide: decoded_oxide_geometry};
      let mut geometry_bytes = serde_json::to_vec_pretty(&geometry_pair).context("encoding paired runtime geometry")?;
      geometry_bytes.push(b'\n');
      let geometry_sha = sha256(&geometry_bytes);
      let mut promoted = checkpoint.clone();
      promoted.as_object_mut().context("release candidate checkpoint is not an object")?.insert("screenshot".into(), json!({"path": path_string(&screenshot_path)?, "sha256": screenshot_sha}));
      promoted.as_object_mut().context("release candidate checkpoint is not an object")?.insert("geometry".into(), json!({"path": path_string(&geometry_path)?, "sha256": geometry_sha}));
      promoted.as_object_mut().context("release candidate checkpoint is not an object")?.insert("expected_visible_role_counts".into(), expected.get(checkpoint_id).with_context(|| format!("release candidate {id} has no role counts for {checkpoint_id}"))?.clone());
      promoted_checkpoints.push(promoted);
      checkpoint_artifacts.push((screenshot_path, appkit.screenshot));
      checkpoint_artifacts.push((geometry_path, geometry_bytes));
      checkpoint_reports.push(ReleasePromotionCheckpointReport {
         id: checkpoint_id.into(),
         appkit_png_sha256: screenshot_sha,
         oxide_png_sha256: sha256(&oxide.screenshot),
         geometry_sha256: geometry_sha,
         visual,
      });
   }

   let (root_width, root_height) = initial_root.context("release candidate has no initial geometry root")?;
   let first_counts = promoted_checkpoints[0]["expected_visible_role_counts"].clone();
   let primary = candidate["primary_metric"].as_str().context("release candidate has no primary metric")?;
   let required = required_metrics(primary);
   let phases = candidate["phases"].as_array().context("release candidate phases are not an array")?.iter().map(|phase| {
      let mut value = phase.clone();
      value.as_object_mut().context("release candidate phase is not an object")?.remove("isolation_class");
      Ok(value)
   }).collect::<Result<Vec<_>>>()?;
   let manifest_value = json!({
      "schema_version": 1,
      "id": id,
      "fixture": candidate["fixture"],
      "assets": candidate["assets"],
      "font_pack": candidate["font_pack"],
      "viewport_class": candidate["viewport_classes"][0],
      "scene": candidate["scene"],
      "phases": phases,
      "primary_metric": primary,
      "required_metrics": required,
      "optional_metrics": ["gpu.device_scope_active_ms", "memory.declared_resource_bytes"],
      "parity_checkpoints": promoted_checkpoints,
      "fairness_contract": {
         "locale": "en_US_POSIX",
         "timezone": "UTC",
         "direction": if id == "text.multilingual" {"mixed-ltr-rtl"} else {"ltr"},
         "logical_viewport_width": root_width,
         "logical_viewport_height": root_height,
         "expected_visible_role_counts": first_counts,
         "schedule_tolerance_us": 1000,
         "coordinate_tolerance_microunits": 1000,
         "elapsed_time_driven": true
      }
   });
   let manifest_typed = serde_json::from_value::<ScenarioSpec>(manifest_value).context("materializing promoted scenario")?;
   validate_scenario(&manifest_typed).context("validating promoted scenario")?;
   let mut manifest = serde_json::to_vec_pretty(&manifest_typed).context("encoding promoted scenario")?;
   manifest.push(b'\n');
   let report = ReleasePromotionScenarioReport {
      id: id.into(),
      candidate_sha256: sha256(&candidate_bytes),
      manifest_sha256: sha256(&manifest),
      checkpoints: checkpoint_reports,
   };
   Ok(PlannedScenario {id: id.into(), manifest, checkpoint_artifacts, report})
}

fn required_metrics(primary: &str) -> Vec<&'static str>
{
   [
      "frame.present_ms", "first.interactive_ms", "input.event_to_visible_response_ms",
      "frame.missed_display_opportunities_per_1000", "cpu.main_thread_ms", "memory.resident_bytes",
      "oxide.dirty_node_count", "oxide.layout_pass_count", "oxide.draw_call_count",
      "oxide.encoded_bytes", "oxide.texture_bytes",
   ].into_iter().filter(|metric| *metric != primary).collect()
}

fn candidate_artifacts(candidate: &Value) -> Result<Vec<&Value>>
{
   let mut artifacts = vec![&candidate["fixture"], &candidate["assets"], &candidate["scene"]["style_tokens"], &candidate["scene"]["layout_assertions"]];
   artifacts.push(&candidate["font_pack"]);
   for phase in candidate["phases"].as_array().context("candidate phases are not an array")?
   {
      if phase.get("trace").is_some()
      {
         artifacts.push(&phase["trace"]);
      }
   }
   for checkpoint in candidate["parity_checkpoints"].as_array().context("candidate checkpoints are not an array")?
   {
      artifacts.push(&checkpoint["state"]);
      artifacts.push(&checkpoint["accessibility"]);
   }
   Ok(artifacts)
}

fn verify_artifact(root: &Path, artifact: &Value) -> Result<()>
{
   let relative = if let Some(path) = artifact.get("path") {path.as_str()} else {artifact.get("manifest").and_then(Value::as_str)}.context("candidate artifact has no path")?;
   let expected = artifact["sha256"].as_str().context("candidate artifact has no SHA-256")?;
   ensure!(expected.len() == 64 && expected.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()), "candidate artifact SHA-256 is invalid");
   let relative = safe_relative(relative)?;
   let bytes = fs::read(root.join(relative)).with_context(|| format!("reading candidate artifact {}", relative.display()))?;
   ensure!(sha256(&bytes) == expected, "candidate artifact {} SHA-256 mismatch", relative.display());
   Ok(())
}

fn artifact_path(artifact: &Value) -> Result<&Path>
{
   safe_relative(artifact["path"].as_str().context("checkpoint artifact has no path")?)
}

fn safe_relative(path: &str) -> Result<&Path>
{
   let path = Path::new(path);
   ensure!(!path.is_absolute() && path.components().all(|component| matches!(component, Component::Normal(_))), "artifact path is not a safe relative path");
   Ok(path)
}

fn validate_identifier(id: &str, label: &str) -> Result<()>
{
   ensure!(!id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-')), "{label} id is invalid");
   Ok(())
}

fn exact_json(path: &Path) -> Result<Value>
{
   serde_json::from_slice(&fs::read(path).with_context(|| format!("reading exact JSON {}", path.display()))?).with_context(|| format!("parsing exact JSON {}", path.display()))
}

fn exact_json_bytes(bytes: &[u8]) -> Result<Value>
{
   serde_json::from_slice(bytes).context("parsing exact runtime JSON")
}

fn read_runtime_checkpoint(root: &Path, scenario_id: &str, checkpoint_id: &str) -> Result<RuntimeCheckpoint>
{
   let directory = root.join(scenario_id).join(checkpoint_id);
   let evidence_path = directory.join("evidence.json");
   let evidence: RuntimeCheckpointEvidence = serde_json::from_slice(&fs::read(&evidence_path).with_context(|| format!("reading runtime evidence {}", evidence_path.display()))?).with_context(|| format!("parsing runtime evidence {}", evidence_path.display()))?;
   let state = read_runtime_artifact(root, scenario_id, checkpoint_id, &directory, &evidence.actual_state, "state.actual.json", "state")?;
   let accessibility = read_runtime_artifact(root, scenario_id, checkpoint_id, &directory, &evidence.actual_accessibility, "accessibility.actual.json", "accessibility")?;
   let geometry = read_runtime_artifact(root, scenario_id, checkpoint_id, &directory, &evidence.actual_geometry.context("runtime checkpoint has no geometry artifact")?, "geometry.actual.json", "geometry")?;
   let screenshot = read_runtime_artifact(root, scenario_id, checkpoint_id, &directory, &evidence.actual_screenshot.context("runtime checkpoint has no screenshot artifact")?, "screenshot.actual.png", "screenshot")?;
   Ok(RuntimeCheckpoint {state, accessibility, geometry, screenshot, validation: evidence.validation})
}

fn read_runtime_artifact(root: &Path, scenario_id: &str, checkpoint_id: &str, directory: &Path, identity: &ArtifactIdentity, file_name: &str, label: &str) -> Result<Vec<u8>>
{
   let relative = safe_relative(&identity.path)?;
   let root_name = root.file_name().and_then(|name| name.to_str()).context("runtime evidence root has no UTF-8 terminal component")?;
   let suffix = Path::new(root_name).join(scenario_id).join(checkpoint_id).join(file_name);
   ensure!(relative.ends_with(&suffix), "runtime {label} identity does not bind its evidence root and checkpoint: {}", identity.path);
   let path = directory.join(file_name);
   let bytes = fs::read(&path).with_context(|| format!("reading runtime {label} {}", path.display()))?;
   ensure!(sha256(&bytes) == identity.sha256, "runtime {label} hash mismatch: {scenario_id}:{checkpoint_id}");
   Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn path_string(path: &Path) -> Result<&str>
{
   path.to_str().context("promotion path is not UTF-8")
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()>
{
   for entry in fs::read_dir(source).with_context(|| format!("reading source tree {}", source.display()))?
   {
      let entry = entry.context("reading source tree entry")?;
      let file_type = entry.file_type().context("reading source tree entry type")?;
      let target = destination.join(entry.file_name());
      if file_type.is_dir()
      {
         fs::create_dir(&target).with_context(|| format!("creating staged directory {}", target.display()))?;
         copy_tree(&entry.path(), &target)?;
      }
      else if file_type.is_file()
      {
         fs::copy(entry.path(), &target).with_context(|| format!("copying staged file {}", target.display()))?;
      }
      else
      {
         bail!("release promotion source contains unsupported link or special file: {}", entry.path().display());
      }
   }
   Ok(())
}

fn write_generated(path: &Path, bytes: &[u8]) -> Result<()>
{
   ensure!(!path.exists() || path.is_file(), "release promotion destination is not a regular file: {}", path.display());
   if let Some(parent) = path.parent()
   {
      fs::create_dir_all(parent).with_context(|| format!("creating release promotion destination {}", parent.display()))?;
   }
   fs::write(path, bytes).with_context(|| format!("writing release promotion destination {}", path.display()))
}
