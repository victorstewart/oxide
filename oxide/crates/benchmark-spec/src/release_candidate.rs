use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::scenario::{ArtifactIdentity, AssetManifest, FontPackIdentity, FontPackManifest, RoleCount, ScenarioPhase, SceneContract, TraceEvent};
use crate::validate::{validate_artifact_file, validate_asset_manifest_at_root, validate_font_pack_manifest_at_root};
use crate::{validate_trace, RELEASE_CANDIDATE_IDS};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseCandidateCaptureSpec
{
   pub candidate_sha256: String,
   pub id: String,
   pub fixture: ArtifactIdentity,
   pub assets: ArtifactIdentity,
   pub font_pack: FontPackIdentity,
   pub scene: SceneContract,
   pub phases: Vec<ScenarioPhase>,
   pub viewport_classes: Vec<String>,
   pub checkpoint_ids: Vec<String>,
}

#[derive(Deserialize)]
struct ReleaseCandidateDocument
{
   schema_version: u32,
   id: String,
   candidate_status: String,
   owning_pass: String,
   screenshot_materialization: ScreenshotMaterialization,
   fixture: ArtifactIdentity,
   assets: ArtifactIdentity,
   font_pack: FontPackIdentity,
   scene: SceneContract,
   phases: Vec<ScenarioPhase>,
   viewport_classes: Vec<String>,
   parity_checkpoints: Vec<ReleaseCandidateCheckpoint>,
   expected_visible_role_counts_by_checkpoint: BTreeMap<String, Vec<RoleCount>>,
}

#[derive(Deserialize)]
struct ScreenshotMaterialization
{
   required: bool,
   status: String,
   scenario_manifest_path: String,
}

#[derive(Deserialize)]
struct ReleaseCandidateCheckpoint
{
   id: String,
   phase_id: String,
   at_us: Option<u64>,
   state: ArtifactIdentity,
   accessibility: ArtifactIdentity,
}

pub fn load_release_candidate_for_capture(spec_root: &Path, scenario_id: &str) -> Result<ReleaseCandidateCaptureSpec>
{
   ensure!(RELEASE_CANDIDATE_IDS.contains(&scenario_id), "unsupported release candidate {scenario_id}");
   let canonical_root = fs::canonicalize(spec_root).with_context(|| format!("canonicalizing benchmark-spec root {}", spec_root.display()))?;
   let relative = PathBuf::from("release-candidates").join(format!("{scenario_id}.candidate.json"));
   let candidate_path = fs::canonicalize(canonical_root.join(&relative)).with_context(|| format!("resolving release candidate {}", relative.display()))?;
   ensure!(candidate_path.starts_with(&canonical_root), "release candidate path escapes the benchmark-spec root");
   let bytes = fs::read(&candidate_path).with_context(|| format!("reading release candidate {}", candidate_path.display()))?;
   let value = serde_json::from_slice::<Value>(&bytes).context("parsing release candidate JSON")?;
   let mut canonical = serde_json::to_vec(&value).context("canonicalizing release candidate JSON")?;
   canonical.push(b'\n');
   ensure!(bytes == canonical, "release candidate {scenario_id} is not canonical compact JSON");
   let candidate = serde_json::from_value::<ReleaseCandidateDocument>(value.clone()).context("parsing typed release candidate")?;
   validate_capture_contract(&canonical_root, scenario_id, &value, &candidate)?;
   Ok(ReleaseCandidateCaptureSpec {
      candidate_sha256: format!("{:x}", Sha256::digest(&bytes)),
      id: candidate.id,
      fixture: candidate.fixture,
      assets: candidate.assets,
      font_pack: candidate.font_pack,
      scene: candidate.scene,
      phases: candidate.phases,
      viewport_classes: candidate.viewport_classes,
      checkpoint_ids: candidate.parity_checkpoints.into_iter().map(|checkpoint| checkpoint.id).collect(),
   })
}

fn validate_capture_contract(root: &Path, scenario_id: &str, value: &Value, candidate: &ReleaseCandidateDocument) -> Result<()>
{
   ensure!(candidate.schema_version == 1 && candidate.id == scenario_id, "release candidate {scenario_id} identity is invalid");
   ensure!(candidate.candidate_status == "blocked-on-canonical-screenshots", "release candidate {scenario_id} has an invalid status");
   ensure!(candidate.owning_pass == "minimal-presentation", "release candidate {scenario_id} has an invalid owning pass");
   ensure!(candidate.screenshot_materialization.required && candidate.screenshot_materialization.status == "blocked", "release candidate {scenario_id} is not blocked on screenshots");
   ensure!(candidate.screenshot_materialization.scenario_manifest_path == format!("scenarios/{scenario_id}.json"), "release candidate {scenario_id} has a noncanonical manifest destination");
   ensure!(!candidate.viewport_classes.is_empty(), "release candidate {scenario_id} has no viewport classes");
   ensure!(!candidate.scene.roles.is_empty(), "release candidate {scenario_id} has no scene roles");
   validate_artifact_file(root, &candidate.fixture, "release candidate fixture")?;
   let assets = validate_artifact_file(root, &candidate.assets, "release candidate asset manifest")?;
   let assets = serde_json::from_slice::<AssetManifest>(&assets).context("parsing release candidate asset manifest")?;
   validate_asset_manifest_at_root(root, &assets)?;
   let font_pack = ArtifactIdentity {path: candidate.font_pack.manifest.clone(), sha256: candidate.font_pack.sha256.clone()};
   let fonts = validate_artifact_file(root, &font_pack, "release candidate font pack")?;
   let fonts = serde_json::from_slice::<FontPackManifest>(&fonts).context("parsing release candidate font pack")?;
   validate_font_pack_manifest_at_root(root, &fonts, &candidate.font_pack.id)?;
   validate_artifact_file(root, &candidate.scene.style_tokens, "release candidate style tokens")?;
   validate_artifact_file(root, &candidate.scene.layout_assertions, "release candidate layout assertions")?;
   validate_phases(root, scenario_id, &candidate.phases)?;
   validate_checkpoints(root, scenario_id, value, candidate)?;
   Ok(())
}

fn validate_phases(root: &Path, scenario_id: &str, phases: &[ScenarioPhase]) -> Result<()>
{
   ensure!(!phases.is_empty(), "release candidate {scenario_id} has no phases");
   let mut phase_ids = BTreeSet::new();
   for phase in phases
   {
      ensure!(!phase.id.is_empty() && phase_ids.insert(phase.id.as_str()), "release candidate {scenario_id} phase ids must be nonempty and unique");
      if let Some(trace) = &phase.trace
      {
         let bytes = validate_artifact_file(root, trace, "release candidate trace")?;
         let events = serde_json::from_slice::<Vec<TraceEvent>>(&bytes).with_context(|| format!("parsing release candidate {scenario_id} phase {} trace", phase.id))?;
         validate_trace(&events).with_context(|| format!("validating release candidate {scenario_id} phase {} trace", phase.id))?;
         if let Some(duration_ms) = phase.duration_ms
         {
            let duration_us = duration_ms.checked_mul(1_000).context("release candidate phase duration overflows microseconds")?;
            ensure!(events.last().is_some_and(|event| event.at_us <= duration_us), "release candidate {scenario_id} phase {} trace exceeds its duration", phase.id);
         }
      }
   }
   Ok(())
}

fn validate_checkpoints(root: &Path, scenario_id: &str, value: &Value, candidate: &ReleaseCandidateDocument) -> Result<()>
{
   ensure!(!candidate.parity_checkpoints.is_empty(), "release candidate {scenario_id} has no checkpoints");
   let raw = value["parity_checkpoints"].as_array().context("release candidate checkpoints are not an array")?;
   ensure!(raw.len() == candidate.parity_checkpoints.len(), "release candidate {scenario_id} checkpoint decoding changed cardinality");
   let phases = candidate.phases.iter().map(|phase| (phase.id.as_str(), phase)).collect::<BTreeMap<_, _>>();
   let mut checkpoint_ids = BTreeSet::new();
   for (raw, checkpoint) in raw.iter().zip(&candidate.parity_checkpoints)
   {
      ensure!(raw.get("screenshot").is_none() && raw.get("geometry").is_none(), "release candidate {scenario_id}/{} binds pre-promotion visual artifacts", checkpoint.id);
      ensure!(valid_identifier(&checkpoint.id) && checkpoint_ids.insert(checkpoint.id.as_str()), "release candidate {scenario_id} checkpoint ids must be canonical and unique");
      let phase = phases.get(checkpoint.phase_id.as_str()).with_context(|| format!("release candidate {scenario_id}/{} refers to unknown phase {}", checkpoint.id, checkpoint.phase_id))?;
      if let (Some(at_us), Some(duration_ms)) = (checkpoint.at_us, phase.duration_ms)
      {
         ensure!(at_us <= duration_ms.checked_mul(1_000).context("release candidate checkpoint duration overflows microseconds")?, "release candidate {scenario_id}/{} lies beyond its phase", checkpoint.id);
      }
      let state = validate_artifact_file(root, &checkpoint.state, "release candidate checkpoint state")?;
      let accessibility = validate_artifact_file(root, &checkpoint.accessibility, "release candidate checkpoint accessibility")?;
      let expected = candidate.expected_visible_role_counts_by_checkpoint.get(&checkpoint.id).with_context(|| format!("release candidate {scenario_id}/{} has no visible-role contract", checkpoint.id))?;
      validate_checkpoint_json(scenario_id, &checkpoint.id, expected, &state, &accessibility)?;
   }
   ensure!(checkpoint_ids.len() == candidate.expected_visible_role_counts_by_checkpoint.len(), "release candidate {scenario_id} visible-role map differs from its checkpoints");
   Ok(())
}

fn validate_checkpoint_json(scenario_id: &str, checkpoint_id: &str, expected: &[RoleCount], state: &[u8], accessibility: &[u8]) -> Result<()>
{
   ensure!(!expected.is_empty(), "release candidate {scenario_id}/{checkpoint_id} has no visible roles");
   let state = serde_json::from_slice::<Value>(state).context("parsing release candidate checkpoint state")?;
   let accessibility = serde_json::from_slice::<Value>(accessibility).context("parsing release candidate checkpoint accessibility")?;
   ensure!(state["scenario_id"] == scenario_id && state["checkpoint_id"] == checkpoint_id, "release candidate {scenario_id}/{checkpoint_id} state identity differs");
   ensure!(accessibility["scenario_id"] == scenario_id && accessibility["checkpoint_id"] == checkpoint_id, "release candidate {scenario_id}/{checkpoint_id} accessibility identity differs");
   let expected = serde_json::to_value(expected).context("encoding release candidate visible-role contract")?;
   ensure!(state["visible_role_counts"] == expected, "release candidate {scenario_id}/{checkpoint_id} state role counts differ");
   let accessibility_roles = accessibility["nodes"].as_array().context("release candidate accessibility nodes are not an array")?.iter().map(|node| serde_json::json!({"count": node["count"], "role": node["role"]})).collect::<Vec<_>>();
   ensure!(Value::Array(accessibility_roles) == expected, "release candidate {scenario_id}/{checkpoint_id} accessibility role counts differ");
   Ok(())
}

fn valid_identifier(id: &str) -> bool
{
   !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
}
