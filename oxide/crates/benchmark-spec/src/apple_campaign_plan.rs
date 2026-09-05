use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::scenario::ArtifactIdentity;
use crate::schema::{BudgetSpec, Platform, Tier, BENCHMARK_SPEC_SCHEMA_VERSION};
use crate::validate::{validate_budget, validate_scenario, validate_scenario_artifacts};
use crate::ScenarioSpec;

pub const APPLE_NIGHTLY_SCENARIO_IDS: &[&str] = &[
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
   "idle.steady",
   "endurance.churn",
];

pub const APPLE_RELEASE_SCENARIO_IDS: &[&str] = &[
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "grid.large-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
   "effects.layers",
   "mutation.damage",
   "text.multilingual",
   "resize.theme",
   "idle.steady",
   "endurance.churn",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppleCampaignPassRole
{
   Correctness,
   Primary,
   Launch,
   Idle,
   Endurance,
   Energy,
   Attribution,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppleCampaignBudgetComponent
{
   CorrectnessInstallPulls,
   PrimaryDynamicPresentation,
   LaunchOrStartupDelivery,
   IdleEndurance,
   Energy,
   Attribution,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppleCampaignEvidenceRole
{
   CorrectnessOnly,
   ClaimBearing,
   DescriptiveDiagnostic,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppleCampaignPlanSpec
{
   pub schema_version: u32,
   pub id: String,
   pub platform: Platform,
   pub tier: Tier,
   pub budget_id: String,
   pub timing: AppleCampaignTimingSpec,
   pub scenarios: Vec<AppleCampaignScenarioBinding>,
   pub packs: Vec<AppleCampaignPackSpec>,
   pub passes: Vec<AppleCampaignPassSpec>,
   pub budget_components: Vec<AppleCampaignBudgetComponentSpec>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppleCampaignTimingSpec
{
   pub passes: Vec<AppleCampaignPassTimingSpec>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppleCampaignPassTimingSpec
{
   pub pass_id: String,
   pub reset_seconds_per_session: u64,
   pub readiness_timeout_seconds: u64,
   pub scenarios: Vec<AppleCampaignScenarioTimingSpec>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppleCampaignScenarioTimingSpec
{
   pub scenario_id: String,
   pub setup_seconds: u64,
   pub warmup_seconds: u64,
   pub measurement: AppleCampaignMeasurementTimingSpec,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum AppleCampaignMeasurementTimingSpec
{
   Duration
   {
      duration_seconds: u64,
   },
   Iterations
   {
      phase_id: String,
      source_iteration_count: u32,
      iteration_count: u32,
      occupied_seconds: u64,
   },
}

impl AppleCampaignMeasurementTimingSpec
{
   pub fn occupied_seconds(&self) -> u64
   {
      match self
      {
         Self::Duration {duration_seconds} => *duration_seconds,
         Self::Iterations {occupied_seconds, ..} => *occupied_seconds,
      }
   }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppleCampaignScenarioBinding
{
   pub id: String,
   pub expected_path: String,
   pub artifact: Option<ArtifactIdentity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppleCampaignPackSpec
{
   pub id: String,
   pub ordered_scenario_ids: Vec<String>,
   pub isolated_process: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppleCampaignPassSpec
{
   pub id: String,
   pub role: AppleCampaignPassRole,
   pub evidence_role: AppleCampaignEvidenceRole,
   pub pair_count: u32,
   pub pack_ids: Vec<String>,
   pub scenario_ids: Vec<String>,
   pub launch_classes: Vec<String>,
   pub collector: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppleCampaignBudgetComponentSpec
{
   pub component: AppleCampaignBudgetComponent,
   pub occupied_seconds: u64,
   pub pass_ids: Vec<String>,
}

pub fn canonical_apple_campaign_plan_json(plan: &AppleCampaignPlanSpec) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(plan).context("serializing Apple campaign plan")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn apple_campaign_plan_sha256(plan: &AppleCampaignPlanSpec) -> Result<String>
{
   Ok(format!("{:x}", Sha256::digest(canonical_apple_campaign_plan_json(plan)?)))
}

pub fn materialize_default_macos_campaign_plan(spec_root: &Path, budget: &BudgetSpec) -> Result<AppleCampaignPlanSpec>
{
   validate_budget(budget)?;
   ensure!(budget.platform == Platform::Apple, "macOS campaign budget {} is not an Apple budget", budget.id);
   let contract = match budget.tier
   {
      Tier::Nightly => nightly_contract(),
      Tier::ReleaseCore => release_contract(false),
      Tier::ClaimComplete => release_contract(true),
      Tier::Pr => bail!("Apple PR retains its frozen ApplePrPlanSpec and acquisition contract"),
      Tier::Extended | Tier::FullAttribution => bail!("tier {:?} has no benchmark-spec v1 default; provide an explicit plan and budget", budget.tier),
   };
   ensure!(budget.id == contract.id, "macOS {:?} budget id must be {}, observed {}", budget.tier, contract.id, budget.id);

   let scenarios = contract.scenario_ids.iter().map(|id| bind_scenario(spec_root, id)).collect::<Result<Vec<_>>>()?;
   let plan = AppleCampaignPlanSpec {
      schema_version: BENCHMARK_SPEC_SCHEMA_VERSION,
      id: String::from(contract.id),
      platform: Platform::Apple,
      tier: budget.tier,
      budget_id: budget.id.clone(),
      timing: contract.timing,
      scenarios,
      packs: contract.packs,
      passes: contract.passes,
      budget_components: budget_components(budget, &contract.component_pass_ids),
   };
   validate_apple_campaign_contract(&plan, budget)?;
   Ok(plan)
}

pub fn validate_apple_campaign_contract(plan: &AppleCampaignPlanSpec, budget: &BudgetSpec) -> Result<()>
{
   validate_budget(budget)?;
   ensure!(plan.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "Apple campaign plan has unsupported schema version {}", plan.schema_version);
   ensure!(plan.platform == Platform::Apple && budget.platform == Platform::Apple, "Apple campaign plan and budget must use the Apple platform");
   ensure!(plan.id == budget.id && plan.budget_id == budget.id && plan.tier == budget.tier, "Apple campaign plan identity differs from its budget");
   ensure!(!plan.scenarios.is_empty() && !plan.packs.is_empty() && !plan.passes.is_empty(), "Apple campaign plan has an empty scenario, pack, or pass surface");

   let scenario_ids = unique_strings(plan.scenarios.iter().map(|scenario| scenario.id.as_str()), "Apple campaign scenario")?;
   for scenario in &plan.scenarios
   {
      ensure!(scenario.expected_path == format!("scenarios/{}.json", scenario.id), "Apple campaign scenario {} path is not canonical", scenario.id);
      if let Some(artifact) = &scenario.artifact
      {
         ensure!(artifact.path == scenario.expected_path, "Apple campaign scenario {} artifact path differs from its binding", scenario.id);
         validate_sha256(&artifact.sha256, &format!("Apple campaign scenario {}", scenario.id))?;
      }
   }

   let pack_ids = unique_strings(plan.packs.iter().map(|pack| pack.id.as_str()), "Apple campaign pack")?;
   for pack in &plan.packs
   {
      ensure!(!pack.ordered_scenario_ids.is_empty(), "Apple campaign pack {} is empty", pack.id);
      let members = unique_strings(pack.ordered_scenario_ids.iter().map(String::as_str), &format!("Apple campaign pack {} scenario", pack.id))?;
      ensure!(members.is_subset(&scenario_ids), "Apple campaign pack {} refers to an unbound scenario", pack.id);
   }

   let pass_ids = unique_strings(plan.passes.iter().map(|pass| pass.id.as_str()), "Apple campaign pass")?;
   for pass in &plan.passes
   {
      ensure!(pass.pair_count > 0 || pass.role == AppleCampaignPassRole::Correctness, "Apple campaign pass {} has zero pairs", pass.id);
      ensure!(pass.pack_ids.iter().all(|id| pack_ids.contains(id.as_str())), "Apple campaign pass {} refers to an unknown pack", pass.id);
      ensure!(pass.scenario_ids.iter().all(|id| scenario_ids.contains(id.as_str())), "Apple campaign pass {} refers to an unbound scenario", pass.id);
      ensure!(pass.role == AppleCampaignPassRole::Attribution || pass.collector.is_none(), "non-attribution pass {} declares a collector", pass.id);
      ensure!(pass.role != AppleCampaignPassRole::Attribution || pass.collector.as_ref().is_some_and(|collector| !collector.is_empty()), "attribution pass {} has no collector", pass.id);
      ensure!(pass.role == AppleCampaignPassRole::Launch || pass.launch_classes.is_empty(), "non-launch pass {} declares launch classes", pass.id);
      ensure!(pass.role != AppleCampaignPassRole::Launch || !pass.launch_classes.is_empty(), "launch pass {} has no launch classes", pass.id);
   }
   validate_timing_contract(plan, &pass_ids)?;

   ensure!(plan.budget_components.len() == 6, "Apple campaign must bind all six budget components");
   let mut observed_components = BTreeMap::new();
   let mut owned_passes = BTreeSet::new();
   for component in &plan.budget_components
   {
      ensure!(observed_components.insert(component.component, component.occupied_seconds).is_none(), "Apple campaign repeats a budget component");
      for pass_id in &component.pass_ids
      {
         ensure!(pass_ids.contains(pass_id.as_str()), "Apple campaign budget component refers to unknown pass {}", pass_id);
         ensure!(owned_passes.insert(pass_id.as_str()), "Apple campaign pass {} belongs to more than one budget component", pass_id);
      }
      ensure!(component.occupied_seconds > 0 || component.pass_ids.is_empty(), "zero-second Apple campaign budget component owns a pass");
   }
   ensure!(owned_passes.len() == pass_ids.len(), "Apple campaign has a pass without budget-component ownership");
   ensure!(observed_components == expected_budget_components(budget), "Apple campaign budget-component bindings differ from budget {}", budget.id);
   validate_tier_contract(plan)?;
   Ok(())
}

pub fn validate_runnable_apple_campaign_plan(spec_root: &Path, plan: &AppleCampaignPlanSpec, budget: &BudgetSpec) -> Result<()>
{
   validate_apple_campaign_contract(plan, budget)?;
   let missing = plan.scenarios.iter().filter(|scenario| scenario.artifact.is_none()).map(|scenario| scenario.expected_path.as_str()).collect::<Vec<_>>();
   if !missing.is_empty()
   {
      bail!("Apple {:?} campaign is not runnable; missing scenario artifacts: {}", plan.tier, missing.join(", "));
   }
   for binding in &plan.scenarios
   {
      let artifact = binding.artifact.as_ref().expect("missing scenario artifacts rejected above");
      let path = spec_root.join(&artifact.path);
      let bytes = fs::read(&path).with_context(|| format!("reading Apple campaign scenario {}", path.display()))?;
      ensure!(format!("{:x}", Sha256::digest(&bytes)) == artifact.sha256, "Apple campaign scenario {} SHA-256 mismatch", binding.id);
      let scenario = serde_json::from_slice::<ScenarioSpec>(&bytes).with_context(|| format!("parsing Apple campaign scenario {}", path.display()))?;
      ensure!(scenario.id == binding.id, "Apple campaign scenario {} artifact declares id {}", binding.id, scenario.id);
      validate_scenario_artifacts(spec_root, &scenario).with_context(|| format!("validating Apple campaign scenario {} artifact closure", binding.id))?;
   }
   Ok(())
}

struct DefaultContract
{
   id: &'static str,
   scenario_ids: &'static [&'static str],
   timing: AppleCampaignTimingSpec,
   packs: Vec<AppleCampaignPackSpec>,
   passes: Vec<AppleCampaignPassSpec>,
   component_pass_ids: [Vec<&'static str>; 6],
}

fn nightly_contract() -> DefaultContract
{
   let mut timing = tier_timing(
      20,
      3,
      12,
      10,
      &[
         ("primary-presentation", &APPLE_NIGHTLY_SCENARIO_IDS[1..6]),
         ("attribution-time-profiler", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
         ("attribution-physical-footprint", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
      ],
   );
   timing.passes.extend(soak_timing(20));
   DefaultContract {
      id: "nightly-apple",
      scenario_ids: APPLE_NIGHTLY_SCENARIO_IDS,
      timing,
      packs: vec![
         pack("launch", &["startup.first-screen"], true),
         pack("core-interaction", &["dashboard.mixed-static", "chat.live-update", "navigation.modal"], false),
         pack("scroll-damage", &["feed.variable-scroll"], false),
         pack("media-text-warm", &["image.decode-zoom"], false),
         pack("soak-idle", &["idle.steady"], true),
         pack("soak-endurance", &["endurance.churn"], true),
      ],
      passes: vec![
         pass("correctness", AppleCampaignPassRole::Correctness, AppleCampaignEvidenceRole::CorrectnessOnly, 0, &[], APPLE_NIGHTLY_SCENARIO_IDS),
         pass("primary-presentation", AppleCampaignPassRole::Primary, AppleCampaignEvidenceRole::ClaimBearing, 6, &["core-interaction", "scroll-damage", "media-text-warm"], &APPLE_NIGHTLY_SCENARIO_IDS[1..6]),
         launch_pass("canonical-launch", 10, &["terminated-warm-system-cache:8", "fresh-install-first-launch:2"]),
         pass("idle", AppleCampaignPassRole::Idle, AppleCampaignEvidenceRole::DescriptiveDiagnostic, 2, &["soak-idle"], &["idle.steady"]),
         pass("endurance", AppleCampaignPassRole::Endurance, AppleCampaignEvidenceRole::DescriptiveDiagnostic, 2, &["soak-endurance"], &["endurance.churn"]),
         attribution_pass("attribution-time-profiler", "time-profiler", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
         attribution_pass("attribution-physical-footprint", "physical-footprint", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
      ],
      component_pass_ids: [
         vec!["correctness"],
         vec!["primary-presentation"],
         vec!["canonical-launch"],
         vec!["idle", "endurance"],
         vec![],
         vec!["attribution-time-profiler", "attribution-physical-footprint"],
      ],
   }
}

fn release_contract(claim_complete: bool) -> DefaultContract
{
   let id = if claim_complete { "apple-release-claim-complete" } else { "apple-release-core" };
   let resource_evidence = if claim_complete { AppleCampaignEvidenceRole::ClaimBearing } else { AppleCampaignEvidenceRole::DescriptiveDiagnostic };
   let resource_pairs = if claim_complete { 12 } else { 2 };
   let energy_pairs = if claim_complete { 12 } else { 5 };
   let mut gpu = attribution_pass("common-gpu", "common-gpu", &["dashboard.mixed-static", "feed.variable-scroll", "image.decode-zoom", "effects.layers"]);
   if claim_complete
   {
      gpu.pair_count = 12;
   }
   let mut timing = tier_timing(
      30,
      5,
      20,
      20,
      &[
         ("primary-presentation", &APPLE_RELEASE_SCENARIO_IDS[1..11]),
         ("attribution-time-profiler", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
         ("attribution-system-trace", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
         ("common-gpu", &["dashboard.mixed-static", "feed.variable-scroll", "image.decode-zoom", "effects.layers"]),
         ("attribution-physical-footprint", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
      ],
   );
   timing.passes.extend(soak_timing(30));
   timing.passes.push(energy_timing());
   DefaultContract {
      id,
      scenario_ids: APPLE_RELEASE_SCENARIO_IDS,
      timing,
      packs: vec![
         pack("launch", &["startup.first-screen"], true),
         pack("core-interaction", &["dashboard.mixed-static", "chat.live-update", "navigation.modal", "resize.theme"], false),
         pack("scroll-damage", &["feed.variable-scroll", "grid.large-scroll", "mutation.damage"], false),
         pack("media-text-warm", &["image.decode-zoom", "effects.layers", "text.multilingual"], false),
         pack("soak-idle", &["idle.steady"], true),
         pack("soak-endurance", &["endurance.churn"], true),
      ],
      passes: vec![
         pass("correctness", AppleCampaignPassRole::Correctness, AppleCampaignEvidenceRole::CorrectnessOnly, 0, &[], APPLE_RELEASE_SCENARIO_IDS),
         pass("primary-presentation", AppleCampaignPassRole::Primary, AppleCampaignEvidenceRole::ClaimBearing, 12, &["core-interaction", "scroll-damage", "media-text-warm"], &APPLE_RELEASE_SCENARIO_IDS[1..11]),
         launch_pass("canonical-launch", if claim_complete { 36 } else { 14 }, if claim_complete { &["terminated-warm-system-cache:12", "fresh-install-first-launch:12", "warm-resume:12"] } else { &["terminated-warm-system-cache:12", "fresh-install-first-launch:2"] }),
         pass("idle", AppleCampaignPassRole::Idle, resource_evidence, resource_pairs, &["soak-idle"], &["idle.steady"]),
         pass("endurance", AppleCampaignPassRole::Endurance, resource_evidence, resource_pairs, &["soak-endurance"], &["endurance.churn"]),
         pass("energy", AppleCampaignPassRole::Energy, resource_evidence, energy_pairs, &[], &["dashboard.mixed-static"]),
         attribution_pass("attribution-time-profiler", "time-profiler", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
         attribution_pass("attribution-system-trace", "system-trace", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
         gpu,
         attribution_pass("attribution-physical-footprint", "physical-footprint", &["dashboard.mixed-static", "feed.variable-scroll", "navigation.modal", "image.decode-zoom"]),
      ],
      component_pass_ids: [
         vec!["correctness"],
         vec!["primary-presentation"],
         vec!["canonical-launch"],
         vec!["idle", "endurance"],
         vec!["energy"],
         vec!["attribution-time-profiler", "attribution-system-trace", "common-gpu", "attribution-physical-footprint"],
      ],
   }
}

fn tier_timing(readiness_timeout_seconds: u64, warmup_seconds: u64, measured_seconds: u64, navigation_iterations: u32, passes: &[(&str, &[&str])]) -> AppleCampaignTimingSpec
{
   AppleCampaignTimingSpec {
      passes: passes.iter().map(|(pass_id, scenario_ids)| AppleCampaignPassTimingSpec {
         pass_id: String::from(*pass_id),
         reset_seconds_per_session: 5,
         readiness_timeout_seconds,
         scenarios: scenario_ids.iter().map(|scenario_id| AppleCampaignScenarioTimingSpec {
            scenario_id: String::from(*scenario_id),
            setup_seconds: 1,
            warmup_seconds,
            measurement: if *scenario_id == "navigation.modal"
            {
               AppleCampaignMeasurementTimingSpec::Iterations {
                  phase_id: String::from("canonical-cycles"),
                  source_iteration_count: 4,
                  iteration_count: navigation_iterations,
                  occupied_seconds: measured_seconds,
               }
            }
            else if *scenario_id == "resize.theme"
            {
               AppleCampaignMeasurementTimingSpec::Iterations {
                  phase_id: String::from("ten-changes"),
                  source_iteration_count: 10,
                  iteration_count: 10,
                  occupied_seconds: measured_seconds,
               }
            }
            else
            {
               AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: measured_seconds}
            },
         }).collect(),
      }).collect(),
   }
}

fn soak_timing(readiness_timeout_seconds: u64) -> Vec<AppleCampaignPassTimingSpec>
{
   [("idle", "idle.steady", 3, 60), ("endurance", "endurance.churn", 2, 300)].into_iter().map(|(pass_id, scenario_id, warmup_seconds, duration_seconds)| AppleCampaignPassTimingSpec {
      pass_id: String::from(pass_id),
      reset_seconds_per_session: 5,
      readiness_timeout_seconds,
      scenarios: vec![AppleCampaignScenarioTimingSpec {
         scenario_id: String::from(scenario_id),
         setup_seconds: 1,
         warmup_seconds,
         measurement: AppleCampaignMeasurementTimingSpec::Duration {duration_seconds},
      }],
   }).collect()
}

fn energy_timing() -> AppleCampaignPassTimingSpec
{
   AppleCampaignPassTimingSpec {
      pass_id: String::from("energy"),
      reset_seconds_per_session: 5,
      readiness_timeout_seconds: 30,
      scenarios: vec![AppleCampaignScenarioTimingSpec {
         scenario_id: String::from("dashboard.mixed-static"),
         setup_seconds: 1,
         warmup_seconds: 120,
         measurement: AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: 120},
      }],
   }
}

fn validate_timing_contract(plan: &AppleCampaignPlanSpec, pass_ids: &BTreeSet<&str>) -> Result<()>
{
   let timed_passes = plan.passes.iter().filter(|pass| matches!(pass.role, AppleCampaignPassRole::Primary | AppleCampaignPassRole::Attribution | AppleCampaignPassRole::Idle | AppleCampaignPassRole::Endurance | AppleCampaignPassRole::Energy)).collect::<Vec<_>>();
   ensure!(plan.timing.passes.len() == timed_passes.len(), "Apple campaign timing overlay does not cover every primary, attribution, idle, endurance, and energy pass exactly once");
   let timing_pass_ids = unique_strings(plan.timing.passes.iter().map(|timing| timing.pass_id.as_str()), "Apple campaign timing pass")?;
   ensure!(timing_pass_ids.iter().all(|id| pass_ids.contains(id)), "Apple campaign timing overlay refers to an unknown pass");

   for pass in timed_passes
   {
      let timing = plan.timing.passes.iter().find(|timing| timing.pass_id == pass.id).with_context(|| format!("Apple campaign pass {} has no timing overlay", pass.id))?;
      ensure!(timing.reset_seconds_per_session > 0 && timing.readiness_timeout_seconds > 0, "Apple campaign pass {} has a zero reset or readiness timeout", pass.id);
      ensure!(timing.scenarios.iter().map(|scenario| scenario.scenario_id.as_str()).eq(pass.scenario_ids.iter().map(String::as_str)), "Apple campaign pass {} timing scenario order differs from its scenario selection", pass.id);
      unique_strings(timing.scenarios.iter().map(|scenario| scenario.scenario_id.as_str()), &format!("Apple campaign pass {} timing scenario", pass.id))?;
      for scenario in &timing.scenarios
      {
         ensure!(scenario.setup_seconds > 0 && scenario.warmup_seconds > 0 && scenario.measurement.occupied_seconds() > 0, "Apple campaign pass {} scenario {} has a zero setup, warmup, or measurement bound", pass.id, scenario.scenario_id);
         if let AppleCampaignMeasurementTimingSpec::Iterations {phase_id, source_iteration_count, iteration_count, ..} = &scenario.measurement
         {
            ensure!(!phase_id.is_empty() && *source_iteration_count > 0 && *iteration_count > 0, "Apple campaign pass {} scenario {} has an invalid iteration phase or count", pass.id, scenario.scenario_id);
         }
      }
   }

   match plan.tier
   {
      Tier::Nightly => validate_default_timing(plan, 20, 3, 12, 10),
      Tier::ReleaseCore | Tier::ClaimComplete => validate_default_timing(plan, 30, 5, 20, 20),
      Tier::Extended | Tier::FullAttribution => Ok(()),
      Tier::Pr => bail!("generic Apple campaign timing cannot replace the frozen Apple PR timing contract"),
   }
}

fn validate_default_timing(plan: &AppleCampaignPlanSpec, readiness_timeout_seconds: u64, warmup_seconds: u64, measured_seconds: u64, navigation_iterations: u32) -> Result<()>
{
   for pass in &plan.timing.passes
   {
      ensure!(pass.reset_seconds_per_session == 5, "Apple {:?} pass {} reset must be five seconds", plan.tier, pass.pass_id);
      ensure!(pass.readiness_timeout_seconds == readiness_timeout_seconds, "Apple {:?} pass {} readiness timeout differs from Section 15", plan.tier, pass.pass_id);
      for scenario in &pass.scenarios
      {
         let (expected_warmup, expected_duration) = match pass.pass_id.as_str()
         {
            "idle" => (3, 60),
            "endurance" => (2, 300),
            "energy" => (120, 120),
            _ => (warmup_seconds, measured_seconds),
         };
         ensure!(scenario.setup_seconds == 1 && scenario.warmup_seconds == expected_warmup, "Apple {:?} pass {} scenario {} setup or warmup differs from Section 15", plan.tier, pass.pass_id, scenario.scenario_id);
         let expected = if scenario.scenario_id == "navigation.modal"
         {
            AppleCampaignMeasurementTimingSpec::Iterations {
               phase_id: String::from("canonical-cycles"),
               source_iteration_count: 4,
               iteration_count: navigation_iterations,
               occupied_seconds: measured_seconds,
            }
         }
         else if scenario.scenario_id == "resize.theme"
         {
            AppleCampaignMeasurementTimingSpec::Iterations {
               phase_id: String::from("ten-changes"),
               source_iteration_count: 10,
               iteration_count: 10,
               occupied_seconds: measured_seconds,
            }
         }
         else
         {
            AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: expected_duration}
         };
         ensure!(scenario.measurement == expected, "Apple {:?} pass {} scenario {} measurement mode differs from Section 15", plan.tier, pass.pass_id, scenario.scenario_id);
      }
   }
   Ok(())
}

fn bind_scenario(spec_root: &Path, id: &str) -> Result<AppleCampaignScenarioBinding>
{
   let expected_path = format!("scenarios/{}.json", id);
   let path = spec_root.join(&expected_path);
   let artifact = match fs::read(&path)
   {
      Ok(bytes) =>
      {
         let scenario = serde_json::from_slice::<ScenarioSpec>(&bytes).with_context(|| format!("parsing available Apple campaign scenario {}", path.display()))?;
         ensure!(scenario.id == id, "Apple campaign scenario path {} declares id {}", expected_path, scenario.id);
         validate_scenario(&scenario).with_context(|| format!("validating available Apple campaign scenario {}", id))?;
         Some(ArtifactIdentity {
            path: expected_path.clone(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
         })
      }
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
      Err(error) => return Err(error).with_context(|| format!("reading Apple campaign scenario {}", path.display())),
   };
   Ok(AppleCampaignScenarioBinding {
      id: String::from(id),
      expected_path,
      artifact,
   })
}

fn pack(id: &str, scenario_ids: &[&str], isolated_process: bool) -> AppleCampaignPackSpec
{
   AppleCampaignPackSpec {
      id: String::from(id),
      ordered_scenario_ids: strings(scenario_ids),
      isolated_process,
   }
}

fn pass(id: &str, role: AppleCampaignPassRole, evidence_role: AppleCampaignEvidenceRole, pair_count: u32, pack_ids: &[&str], scenario_ids: &[&str]) -> AppleCampaignPassSpec
{
   AppleCampaignPassSpec {
      id: String::from(id),
      role,
      evidence_role,
      pair_count,
      pack_ids: strings(pack_ids),
      scenario_ids: strings(scenario_ids),
      launch_classes: vec![],
      collector: None,
   }
}

fn launch_pass(id: &str, pair_count: u32, launch_classes: &[&str]) -> AppleCampaignPassSpec
{
   let mut pass = pass(id, AppleCampaignPassRole::Launch, AppleCampaignEvidenceRole::ClaimBearing, pair_count, &["launch"], &["startup.first-screen"]);
   pass.launch_classes = strings(launch_classes);
   pass
}

fn attribution_pass(id: &str, collector: &str, scenario_ids: &[&str]) -> AppleCampaignPassSpec
{
   let mut pass = pass(id, AppleCampaignPassRole::Attribution, AppleCampaignEvidenceRole::DescriptiveDiagnostic, 3, &[], scenario_ids);
   pass.collector = Some(String::from(collector));
   pass
}

fn strings(values: &[&str]) -> Vec<String>
{
   values.iter().map(|value| String::from(*value)).collect()
}

fn budget_components(budget: &BudgetSpec, pass_ids: &[Vec<&str>; 6]) -> Vec<AppleCampaignBudgetComponentSpec>
{
   [
      (AppleCampaignBudgetComponent::CorrectnessInstallPulls, budget.correctness_install_pulls_seconds),
      (AppleCampaignBudgetComponent::PrimaryDynamicPresentation, budget.primary_dynamic_presentation_seconds),
      (AppleCampaignBudgetComponent::LaunchOrStartupDelivery, budget.launch_or_startup_delivery_seconds),
      (AppleCampaignBudgetComponent::IdleEndurance, budget.idle_endurance_seconds),
      (AppleCampaignBudgetComponent::Energy, budget.energy_seconds),
      (AppleCampaignBudgetComponent::Attribution, budget.attribution_seconds),
   ].into_iter().zip(pass_ids).map(|((component, occupied_seconds), pass_ids)| AppleCampaignBudgetComponentSpec {
      component,
      occupied_seconds,
      pass_ids: strings(pass_ids),
   }).collect()
}

fn expected_budget_components(budget: &BudgetSpec) -> BTreeMap<AppleCampaignBudgetComponent, u64>
{
   [
      (AppleCampaignBudgetComponent::CorrectnessInstallPulls, budget.correctness_install_pulls_seconds),
      (AppleCampaignBudgetComponent::PrimaryDynamicPresentation, budget.primary_dynamic_presentation_seconds),
      (AppleCampaignBudgetComponent::LaunchOrStartupDelivery, budget.launch_or_startup_delivery_seconds),
      (AppleCampaignBudgetComponent::IdleEndurance, budget.idle_endurance_seconds),
      (AppleCampaignBudgetComponent::Energy, budget.energy_seconds),
      (AppleCampaignBudgetComponent::Attribution, budget.attribution_seconds),
   ].into_iter().collect()
}

fn validate_tier_contract(plan: &AppleCampaignPlanSpec) -> Result<()>
{
   match plan.tier
   {
      Tier::Nightly =>
      {
         ensure!(plan.scenarios.iter().map(|scenario| scenario.id.as_str()).eq(APPLE_NIGHTLY_SCENARIO_IDS.iter().copied()), "Apple nightly scenario bindings differ from the canonical eight");
         ensure!(passes_with_role(plan, AppleCampaignPassRole::Primary) == ["primary-presentation"], "Apple nightly must have exactly one primary pass");
         ensure!(passes_with_role(plan, AppleCampaignPassRole::Launch) == ["canonical-launch"], "Apple nightly must have exactly one launch pass");
         ensure!(passes_with_role(plan, AppleCampaignPassRole::Attribution) == ["attribution-time-profiler", "attribution-physical-footprint"], "Apple nightly must have exactly the two declared attribution acquisitions");
      }
      Tier::ReleaseCore | Tier::ClaimComplete =>
      {
         ensure!(plan.scenarios.iter().map(|scenario| scenario.id.as_str()).eq(APPLE_RELEASE_SCENARIO_IDS.iter().copied()), "Apple release scenario bindings differ from the 13-candidate matrix");
         ensure!(plan.packs.iter().map(|pack| pack.id.as_str()).eq(["launch", "core-interaction", "scroll-damage", "media-text-warm", "soak-idle", "soak-endurance"]), "Apple release pack order differs from the frozen release matrix");
         ensure!(passes_with_role(plan, AppleCampaignPassRole::Attribution) == ["attribution-time-profiler", "attribution-system-trace", "common-gpu", "attribution-physical-footprint"], "Apple release must bind the four declared attribution collectors");
      }
      Tier::Extended | Tier::FullAttribution => validate_explicit_tier_contract(plan)?,
      Tier::Pr => bail!("generic Apple campaign contract cannot replace the frozen Apple PR contract"),
   }
   Ok(())
}

fn validate_explicit_tier_contract(plan: &AppleCampaignPlanSpec) -> Result<()>
{
   ensure!(plan.scenarios.iter().map(|scenario| scenario.id.as_str()).eq(APPLE_RELEASE_SCENARIO_IDS.iter().copied()), "explicit Apple {:?} plan must bind the full 13-scenario release matrix", plan.tier);
   if plan.tier == Tier::FullAttribution
   {
      let attribution = plan.passes.iter().filter(|pass| pass.role == AppleCampaignPassRole::Attribution).collect::<Vec<_>>();
      ensure!(attribution.iter().filter(|pass| pass.id == "full-attribution").count() == 1, "full-attribution audit must use exactly one `full-attribution` GPU/counter pass");
      for pass in attribution
      {
         ensure!(pass.scenario_ids.iter().map(String::as_str).eq(APPLE_RELEASE_SCENARIO_IDS.iter().copied()), "full-attribution pass {} does not cover every release scenario", pass.id);
      }
   }
   Ok(())
}

fn passes_with_role(plan: &AppleCampaignPlanSpec, role: AppleCampaignPassRole) -> Vec<&str>
{
   plan.passes.iter().filter(|pass| pass.role == role).map(|pass| pass.id.as_str()).collect()
}

fn unique_strings<'a>(values: impl Iterator<Item = &'a str>, label: &str) -> Result<BTreeSet<&'a str>>
{
   let mut observed = BTreeSet::new();
   for value in values
   {
      ensure!(!value.is_empty(), "{} id is empty", label);
      ensure!(observed.insert(value), "{} id {} is duplicated", label, value);
   }
   Ok(observed)
}

fn validate_sha256(value: &str, label: &str) -> Result<()>
{
   ensure!(value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "{} SHA-256 must be 64 lowercase hexadecimal characters", label);
   Ok(())
}
