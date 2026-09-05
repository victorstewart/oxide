use std::fs;
use std::path::{Component, Path};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{ArtifactIdentity, RELEASE_CANDIDATE_IDS};

pub const RELEASE_CANDIDATE_CAPTURE_PLAN_ID: &str = "macos-release-candidate-capture-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReleaseCandidateCaptureBinding
{
   pub id: String,
   pub artifact: ArtifactIdentity,
   pub checkpoint_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReleaseCandidateCapturePlan
{
   pub schema_version: u32,
   pub id: String,
   pub platform: String,
   pub pass_id: String,
   pub timing_claim: String,
   pub candidates: Vec<ReleaseCandidateCaptureBinding>,
}

pub fn canonical_release_candidate_capture_plan_json(plan: &ReleaseCandidateCapturePlan) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(plan).context("encoding release-candidate capture plan")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn load_release_candidate_capture_plan(spec_root: &Path) -> Result<ReleaseCandidateCapturePlan>
{
   let path = spec_root.join("plans/macos-release-candidate-capture.json");
   let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
   let plan = serde_json::from_slice::<ReleaseCandidateCapturePlan>(&bytes).with_context(|| format!("decoding {}", path.display()))?;
   ensure!(canonical_release_candidate_capture_plan_json(&plan)? == bytes, "release-candidate capture plan is not canonical JSON");
   validate_release_candidate_capture_plan(spec_root, &plan)?;
   Ok(plan)
}

pub fn validate_release_candidate_capture_plan(spec_root: &Path, plan: &ReleaseCandidateCapturePlan) -> Result<()>
{
   ensure!(plan.schema_version == 1, "release-candidate capture plan schema is unsupported");
   ensure!(plan.id == RELEASE_CANDIDATE_CAPTURE_PLAN_ID, "release-candidate capture plan id is invalid");
   ensure!(plan.platform == "macos", "release-candidate capture plan platform is invalid");
   ensure!(plan.pass_id == "release-candidate-capture", "release-candidate capture pass is invalid");
   ensure!(plan.timing_claim == "none-correctness-untimed", "release-candidate capture plan must remain untimed");
   ensure!(plan.candidates.len() == RELEASE_CANDIDATE_IDS.len(), "release-candidate capture plan has the wrong candidate count");
   let mut checkpoint_count = 0;
   for (binding, expected_id) in plan.candidates.iter().zip(RELEASE_CANDIDATE_IDS)
   {
      ensure!(binding.id == expected_id, "release-candidate capture order differs at {expected_id}");
      ensure!(binding.artifact.path == format!("release-candidates/{expected_id}.candidate.json"), "release-candidate capture path differs for {expected_id}");
      ensure!(binding.artifact.sha256.len() == 64 && binding.artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()), "release-candidate capture SHA-256 is invalid for {expected_id}");
      let relative = Path::new(&binding.artifact.path);
      ensure!(!relative.is_absolute() && relative.components().all(|component| matches!(component, Component::Normal(_))), "release-candidate capture path is unsafe for {expected_id}");
      let bytes = fs::read(spec_root.join(relative)).with_context(|| format!("reading release candidate {expected_id}"))?;
      ensure!(format!("{:x}", Sha256::digest(&bytes)) == binding.artifact.sha256, "release-candidate capture hash differs for {expected_id}");
      let candidate = serde_json::from_slice::<Value>(&bytes).with_context(|| format!("decoding release candidate {expected_id}"))?;
      let checkpoints = candidate["parity_checkpoints"].as_array().context("release candidate checkpoints are not an array")?;
      let ids = checkpoints.iter().map(|checkpoint| checkpoint["id"].as_str().map(String::from).context("release candidate checkpoint has no id")).collect::<Result<Vec<_>>>()?;
      ensure!(ids == binding.checkpoint_ids, "release-candidate capture checkpoints differ for {expected_id}");
      ensure!(checkpoints.iter().all(|checkpoint| checkpoint.get("screenshot").is_none()), "release-candidate capture input already contains a screenshot for {expected_id}");
      checkpoint_count += checkpoints.len();
   }
   ensure!(checkpoint_count == 18, "release-candidate capture plan must bind exactly 18 checkpoints");
   Ok(())
}
