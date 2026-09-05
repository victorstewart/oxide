use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

use crate::acquisition::{ApplePrAcquisitionSpec, APPLE_PR_SCENARIO_IDS};
use crate::comparator_acceptance::ComparatorIdentity;
use crate::scenario::ArtifactIdentity;
use crate::schema::{BudgetSpec, Platform, Tier, BENCHMARK_SPEC_SCHEMA_VERSION};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApplePrPlanSpec
{
   pub schema_version: u32,
   pub id: String,
   pub platform: Platform,
   pub tier: Tier,
   pub acquisition: ArtifactIdentity,
   pub budget: ArtifactIdentity,
   pub comparator_audits: Vec<ComparatorAuditBinding>,
   pub scenarios: Vec<ApplePrPlanScenario>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorAuditBinding
{
   pub identity: ComparatorIdentity,
   pub audit: ArtifactIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApplePrPlanScenario
{
   pub id: String,
   pub artifact: ArtifactIdentity,
}

pub fn canonical_apple_pr_plan_json(plan: &ApplePrPlanSpec) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(plan).context("serializing Apple PR plan")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn apple_pr_plan_sha256(plan: &ApplePrPlanSpec) -> Result<String>
{
   Ok(format!("{:x}", Sha256::digest(canonical_apple_pr_plan_json(plan)?)))
}

pub fn validate_apple_pr_plan(spec_root: &Path, plan: &ApplePrPlanSpec, acquisition: &ApplePrAcquisitionSpec, budget: &BudgetSpec) -> Result<()>
{
   ensure!(plan.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "Apple PR plan has unsupported schema version {}", plan.schema_version);
   ensure!(plan.id == "apple-pr" && plan.platform == Platform::Apple && plan.tier == Tier::Pr, "Apple PR plan identity is not canonical");
   ensure!(acquisition.id == plan.id && acquisition.platform == plan.platform && acquisition.tier == plan.tier, "Apple PR acquisition identity differs from the plan");
   ensure!(budget.id == plan.id && budget.platform == plan.platform && budget.tier == plan.tier, "Apple PR budget identity differs from the plan");
   validate_exact_artifact(spec_root, &plan.acquisition, "acquisition/apple-pr.json", "Apple PR acquisition")?;
   validate_exact_artifact(spec_root, &plan.budget, "budgets/apple-pr.json", "Apple PR budget")?;
   ensure!(!plan.comparator_audits.is_empty(), "Apple PR plan has no comparator audit bindings");
   for binding in &plan.comparator_audits
   {
      ensure!(
         !binding.identity.platform.is_empty()
            && !binding.identity.framework.is_empty()
            && !binding.identity.implementation.is_empty()
            && !binding.identity.variant.is_empty(),
         "Apple PR comparator audit identity is incomplete",
      );
      ensure!(binding.audit.path.starts_with("audits/") && binding.audit.path.ends_with(".json"), "Apple PR comparator audit path is not under the canonical audit root");
      validate_exact_artifact(spec_root, &binding.audit, &binding.audit.path, "Apple PR comparator audit")?;
   }
   ensure!(plan.comparator_audits.windows(2).all(|pair| comparator_identity_key(&pair[0].identity) < comparator_identity_key(&pair[1].identity)), "Apple PR comparator audit bindings are duplicated or not in canonical identity order");
   ensure!(plan.scenarios.len() == APPLE_PR_SCENARIO_IDS.len(), "Apple PR plan must bind exactly the canonical six scenarios");
   ensure!(acquisition.selected_scenario_ids.iter().map(String::as_str).eq(plan.scenarios.iter().map(|scenario| scenario.id.as_str())), "Apple PR plan scenario order differs from the acquisition");
   for (scenario, expected_id) in plan.scenarios.iter().zip(APPLE_PR_SCENARIO_IDS)
   {
      ensure!(scenario.id == *expected_id, "Apple PR plan scenario {} is out of canonical order", scenario.id);
      validate_exact_artifact(
         spec_root,
         &scenario.artifact,
         &format!("scenarios/{}.json", scenario.id),
         &format!("Apple PR scenario {}", scenario.id),
      )?;
   }
   Ok(())
}

fn comparator_identity_key(identity: &ComparatorIdentity) -> (&str, &str, &str, &str)
{
   (&identity.platform, &identity.framework, &identity.implementation, &identity.variant)
}

fn validate_exact_artifact(spec_root: &Path, artifact: &ArtifactIdentity, expected_path: &str, label: &str) -> Result<()>
{
   ensure!(artifact.path == expected_path, "{} path differs from the canonical artifact", label);
   ensure!(artifact.sha256.len() == 64 && artifact.sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "{} SHA-256 must be 64 lowercase hexadecimal characters", label);
   let path = spec_root.join(&artifact.path);
   let bytes = fs::read(&path).with_context(|| format!("reading {} {}", label, path.display()))?;
   let observed = format!("{:x}", Sha256::digest(&bytes));
   ensure!(observed == artifact.sha256, "{} SHA-256 mismatch: expected {}, observed {}", label, artifact.sha256, observed);
   Ok(())
}
