use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::acquisition::ApplePrAcquisitionSpec;
use crate::apple_plan::ApplePrPlanSpec;
use crate::comparator_acceptance::ComparatorAcceptanceAudit;
use crate::detection_coverage::{DetectionCoverageManifest, MACOS_DETECTION_COVERAGE_RELATIVE_PATH};
use crate::migration::LegacyCaseDisposition;
use crate::scenario::{FontPackManifest, ScenarioSpec};
use crate::schema::BudgetSpec;

pub const BUDGET_RELATIVE_ROOT: &str = "benchmarks/comparative/specs/v1/budgets";
pub const SCENARIO_RELATIVE_ROOT: &str = "benchmarks/comparative/specs/v1/scenarios";
pub const SCENARIO_SCHEMA_FIXTURE: &str = "benchmarks/comparative/specs/v1/fixtures/scenario-v1.json";
pub const LEGACY_CASE_DISPOSITION: &str = "benchmarks/comparative/legacy_case_disposition.json";
pub const APPLE_PR_ACQUISITION: &str = "benchmarks/comparative/specs/v1/acquisition/apple-pr.json";
pub const APPLE_PR_PLAN: &str = "benchmarks/comparative/specs/v1/plans/apple-pr.json";
pub const APPKIT_MACOS_NATIVE_PRODUCTION_AUDIT: &str = "benchmarks/comparative/specs/v1/audits/macos-appkit-native-production.json";
pub const DEFAULT_FONT_PACK: &str = "benchmarks/comparative/specs/v1/font-packs/oxide-bench-fonts-v1.json";
pub const MACOS_DETECTION_COVERAGE: &str = "benchmarks/comparative/specs/v1/detection/macos-v1.json";

const BUDGET_FILE_NAMES: &[&str] = &[
   "apple-pr.json",
   "web-pr.json",
   "nightly-apple.json",
   "nightly-web-engine.json",
   "nightly-web-mobile.json",
   "apple-release-core.json",
   "web-release-core.json",
   "apple-release-claim-complete.json",
   "web-release-claim-complete.json",
];

const PR_VERTICAL_SCENARIO_FILE_NAMES: &[&str] = &[
   "dashboard.mixed-static.json",
   "feed.variable-scroll.json",
   "navigation.modal.json",
];

const APPLE_PR_SCENARIO_FILE_NAMES: &[&str] = &[
   "startup.first-screen.json",
   "dashboard.mixed-static.json",
   "feed.variable-scroll.json",
   "chat.live-update.json",
   "navigation.modal.json",
   "image.decode-zoom.json",
];

pub fn load_default_budgets(workspace_root: &Path) -> Result<Vec<(PathBuf, BudgetSpec)>>
{
   let root = workspace_root.join(BUDGET_RELATIVE_ROOT);
   BUDGET_FILE_NAMES
      .iter()
      .map(|name| {
         let path = root.join(name);
         let bytes = fs::read(&path).with_context(|| format!("reading comparative budget {}", path.display()))?;
         let budget = serde_json::from_slice(&bytes).with_context(|| format!("parsing comparative budget {}", path.display()))?;
         Ok((path, budget))
      })
      .collect()
}

pub fn canonical_budget_json(budget: &BudgetSpec) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(budget).context("serializing comparative budget")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn load_scenario(workspace_root: &Path, name: &str) -> Result<(PathBuf, ScenarioSpec)>
{
   let path = workspace_root.join(SCENARIO_RELATIVE_ROOT).join(name);
   let bytes = fs::read(&path).with_context(|| format!("reading comparative scenario {}", path.display()))?;
   let scenario = serde_json::from_slice(&bytes).with_context(|| format!("parsing comparative scenario {}", path.display()))?;
   Ok((path, scenario))
}

pub fn load_pr_vertical_scenarios(workspace_root: &Path) -> Result<Vec<(PathBuf, ScenarioSpec)>>
{
   PR_VERTICAL_SCENARIO_FILE_NAMES.iter().map(|name| load_scenario(workspace_root, name)).collect()
}

pub fn load_apple_pr_scenarios(workspace_root: &Path) -> Result<Vec<(PathBuf, ScenarioSpec)>>
{
   APPLE_PR_SCENARIO_FILE_NAMES.iter().map(|name| load_scenario(workspace_root, name)).collect()
}

pub fn load_scenario_schema_fixture(workspace_root: &Path) -> Result<(PathBuf, ScenarioSpec)>
{
   let path = workspace_root.join(SCENARIO_SCHEMA_FIXTURE);
   let bytes = fs::read(&path).with_context(|| format!("reading comparative scenario fixture {}", path.display()))?;
   let scenario = serde_json::from_slice(&bytes).with_context(|| format!("parsing comparative scenario fixture {}", path.display()))?;
   Ok((path, scenario))
}

pub fn canonical_scenario_json(scenario: &ScenarioSpec) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(scenario).context("serializing comparative scenario")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn load_legacy_case_disposition(workspace_root: &Path) -> Result<(PathBuf, LegacyCaseDisposition)>
{
   let path = workspace_root.join(LEGACY_CASE_DISPOSITION);
   let bytes = fs::read(&path).with_context(|| format!("reading legacy disposition {}", path.display()))?;
   let manifest = serde_json::from_slice(&bytes).with_context(|| format!("parsing legacy disposition {}", path.display()))?;
   Ok((path, manifest))
}

pub fn canonical_legacy_case_disposition_json(manifest: &LegacyCaseDisposition) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(manifest).context("serializing legacy disposition")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn load_apple_pr_acquisition(workspace_root: &Path) -> Result<(PathBuf, ApplePrAcquisitionSpec)>
{
   let path = workspace_root.join(APPLE_PR_ACQUISITION);
   let bytes = fs::read(&path).with_context(|| format!("reading Apple PR acquisition expansion {}", path.display()))?;
   let spec = serde_json::from_slice(&bytes).with_context(|| format!("parsing Apple PR acquisition expansion {}", path.display()))?;
   Ok((path, spec))
}

pub fn canonical_apple_pr_acquisition_json(spec: &ApplePrAcquisitionSpec) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(spec).context("serializing Apple PR acquisition expansion")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn load_apple_pr_plan(workspace_root: &Path) -> Result<(PathBuf, ApplePrPlanSpec)>
{
   let path = workspace_root.join(APPLE_PR_PLAN);
   let bytes = fs::read(&path).with_context(|| format!("reading Apple PR plan {}", path.display()))?;
   let plan = serde_json::from_slice(&bytes).with_context(|| format!("parsing Apple PR plan {}", path.display()))?;
   Ok((path, plan))
}

pub fn load_appkit_macos_native_production_audit(workspace_root: &Path) -> Result<(PathBuf, ComparatorAcceptanceAudit)>
{
   let path = workspace_root.join(APPKIT_MACOS_NATIVE_PRODUCTION_AUDIT);
   let bytes = fs::read(&path).with_context(|| format!("reading AppKit macOS comparator audit {}", path.display()))?;
   let audit = serde_json::from_slice(&bytes).with_context(|| format!("parsing AppKit macOS comparator audit {}", path.display()))?;
   Ok((path, audit))
}

pub fn load_default_font_pack(workspace_root: &Path) -> Result<(PathBuf, FontPackManifest)>
{
   let path = workspace_root.join(DEFAULT_FONT_PACK);
   let bytes = fs::read(&path).with_context(|| format!("reading default comparison font pack {}", path.display()))?;
   let manifest = serde_json::from_slice(&bytes).with_context(|| format!("parsing default comparison font pack {}", path.display()))?;
   Ok((path, manifest))
}

pub fn load_macos_detection_coverage(workspace_root: &Path) -> Result<(PathBuf, DetectionCoverageManifest)>
{
   let path = workspace_root.join("benchmarks/comparative/specs/v1").join(MACOS_DETECTION_COVERAGE_RELATIVE_PATH);
   let bytes = fs::read(&path).with_context(|| format!("reading macOS detection coverage {}", path.display()))?;
   let manifest = serde_json::from_slice(&bytes).with_context(|| format!("parsing macOS detection coverage {}", path.display()))?;
   Ok((path, manifest))
}

pub fn canonical_font_pack_json(manifest: &FontPackManifest) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(manifest).context("serializing comparison font pack")?;
   bytes.push(b'\n');
   Ok(bytes)
}
