use std::collections::{BTreeMap, BTreeSet};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::schema::{Tier, BENCHMARK_SPEC_SCHEMA_VERSION};

pub const MACOS_DETECTION_COVERAGE_RELATIVE_PATH: &str = "detection/macos-v1.json";
pub const MACOS_DETECTION_FAULT_FAMILY_COUNT: usize = 14;
pub const MACOS_DETECTION_SEVERITY_COUNT: usize = 3;
pub const MACOS_DETECTION_SEEDS_PER_EXPECTATION: usize = 3;
pub const MACOS_DETECTION_CASE_COUNT: usize = 126;

const REQUIRED_RISK_DIMENSION_IDS: &[&str] = &[
   "startup",
   "layout",
   "text-shaping",
   "images-decode-upload",
   "clipping-effects",
   "scrolling-virtualization",
   "mutation-damage",
   "animation",
   "discrete-input",
   "focus-ime",
   "allocation-teardown",
   "idle-scheduling",
];

const REQUIRED_FAULT_FAMILY_IDS: &[&str] = &[
   "startup-delay-payload-bloat",
   "main-thread-layout-stall",
   "text-fallback-shaping-slowdown",
   "image-decode-upload-delay",
   "list-devirtualization",
   "dirty-region-expansion",
   "extra-offscreen-effect-pass",
   "gpu-upload-churn",
   "input-handler-delay",
   "focus-ime-breakage",
   "mount-teardown-leak",
   "idle-timer-wakeup",
   "retained-resource-slope",
   "visual-fidelity-reduction",
];

const ALL_SCENARIO_IDS: &[&str] = &[
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
pub enum DetectionSeverity
{
   Small,
   Medium,
   Large,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DetectionExpectedDirection
{
   HigherIsWorse,
   LowerIsWorse,
   NamedValidator,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DetectionCoverageManifest
{
   pub schema_version: u32,
   pub id: String,
   pub platform: String,
   pub scoring_regime: String,
   pub required_must_detect_rate_basis_points: u32,
   pub required_weighted_detection_rate_basis_points: u32,
   pub maximum_null_false_positive_rate_basis_points: u32,
   pub expected_case_count: u32,
   pub risk_dimensions: Vec<DetectionRiskDimension>,
   pub metric_ids: Vec<String>,
   pub fault_families: Vec<DetectionFaultFamily>,
   pub expectations: Vec<DetectionExpectation>,
   pub tier_selections: Vec<DetectionTierSelection>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DetectionRiskDimension
{
   pub id: String,
   pub description: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DetectionFaultFamily
{
   pub id: String,
   pub injection: String,
   pub risk_dimension_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DetectionExpectation
{
   pub fault_id: String,
   pub severity: DetectionSeverity,
   pub must_detect: bool,
   pub reviewed_non_must_detect_reason: Option<String>,
   pub weight_basis_points: u32,
   pub seeds: Vec<u64>,
   pub eligible_scenario_ids: Vec<String>,
   pub expected_metric_ids: Vec<String>,
   pub expected_direction: DetectionExpectedDirection,
   pub native_unit_injected_effect: String,
   pub minimum_detection_boundary: String,
   pub terminal_validator_id: Option<String>,
   pub required_seed_count: u32,
   pub null_control_id: String,
   pub maximum_false_positive_rate_basis_points: u32,
}

impl DetectionExpectation
{
   pub fn id(&self) -> String
   {
      format!("{}:{:?}", self.fault_id, self.severity).to_lowercase()
   }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DetectionTierSelection
{
   pub tier: Tier,
   pub selected_scenario_ids: Vec<String>,
   pub omitted_scenarios: Vec<DetectionOmittedScenario>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DetectionOmittedScenario
{
   pub scenario_id: String,
   pub reason: String,
   pub marginal_risk_dimension_ids: Vec<String>,
   pub marginal_expectation_count: u32,
   pub marginal_occupied_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectionCoverageExplanation
{
   pub manifest_id: String,
   pub tier: Tier,
   pub selected_scenario_ids: Vec<String>,
   pub risk_coverage: Vec<DetectionRiskCoverage>,
   pub omitted_scenarios: Vec<DetectionOmittedScenario>,
   pub expectation_count: usize,
   pub injected_case_count: usize,
   pub must_detect_expectation_count: usize,
   pub planned_must_detect_rate_basis_points: u32,
   pub planned_weighted_detection_rate_basis_points: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectionRiskCoverage
{
   pub risk_dimension_id: String,
   pub selected_scenario_ids: Vec<String>,
}

pub fn canonical_detection_coverage_json(manifest: &DetectionCoverageManifest) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(manifest).context("serializing detection coverage manifest")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn detection_coverage_sha256(manifest: &DetectionCoverageManifest) -> Result<String>
{
   Ok(format!("{:x}", Sha256::digest(canonical_detection_coverage_json(manifest)?)))
}

pub fn validate_macos_detection_coverage(manifest: &DetectionCoverageManifest) -> Result<()>
{
   ensure!(manifest.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "macOS detection coverage has unsupported schema version {}", manifest.schema_version);
   ensure!(manifest.id == "macos-v1" && manifest.platform == "macos", "macOS detection coverage identity is not canonical");
   ensure!(manifest.scoring_regime == "deterministic-must-cover-leave-one-family-out-v1", "macOS detection coverage scoring regime is not canonical");
   ensure!(manifest.required_must_detect_rate_basis_points == 10_000, "macOS detection coverage must require 100% must-detect coverage");
   ensure!(manifest.required_weighted_detection_rate_basis_points == 9_500, "macOS detection coverage must require at least 95% weighted detection");
   ensure!(manifest.maximum_null_false_positive_rate_basis_points == 500, "macOS detection coverage null-control ceiling must be 5%");
   ensure!(manifest.expected_case_count as usize == MACOS_DETECTION_CASE_COUNT, "macOS detection coverage expected case count must be {}", MACOS_DETECTION_CASE_COUNT);

   ensure!(manifest.risk_dimensions.iter().map(|risk| risk.id.as_str()).eq(REQUIRED_RISK_DIMENSION_IDS.iter().copied()), "macOS detection coverage risk dimensions differ from the authoritative inventory");
   let risk_ids = ordered_unique(manifest.risk_dimensions.iter().map(|risk| risk.id.as_str()), "risk dimension")?;
   ensure!(manifest.risk_dimensions.iter().all(|risk| !risk.description.trim().is_empty()), "macOS detection coverage has an undescribed risk dimension");
   let metric_ids = ordered_unique(manifest.metric_ids.iter().map(String::as_str), "metric")?;
   ensure!(!metric_ids.is_empty(), "macOS detection coverage has no eligible metrics");

   ensure!(manifest.fault_families.len() == MACOS_DETECTION_FAULT_FAMILY_COUNT, "macOS detection coverage has {} fault families, expected {}", manifest.fault_families.len(), MACOS_DETECTION_FAULT_FAMILY_COUNT);
   ensure!(manifest.fault_families.iter().map(|fault| fault.id.as_str()).eq(REQUIRED_FAULT_FAMILY_IDS.iter().copied()), "macOS detection fault families differ from the authoritative v1 corpus");
   let fault_ids = ordered_unique(manifest.fault_families.iter().map(|fault| fault.id.as_str()), "fault family")?;
   let mut risks_with_faults = BTreeSet::new();
   for fault in &manifest.fault_families
   {
      ensure!(!fault.injection.trim().is_empty(), "fault family {} has no injection contract", fault.id);
      let fault_risks = unique_nonempty(fault.risk_dimension_ids.iter().map(String::as_str), "fault risk dimension")?;
      ensure!(!fault_risks.is_empty() && fault_risks.iter().all(|id| risk_ids.contains(id)), "fault family {} has missing or unknown risk dimensions", fault.id);
      risks_with_faults.extend(fault_risks);
   }
   ensure!(risks_with_faults == risk_ids, "macOS detection fault corpus does not cover every production risk dimension");

   ensure!(manifest.expectations.len() == MACOS_DETECTION_FAULT_FAMILY_COUNT * MACOS_DETECTION_SEVERITY_COUNT, "macOS detection coverage has {} expectations, expected {}", manifest.expectations.len(), MACOS_DETECTION_FAULT_FAMILY_COUNT * MACOS_DETECTION_SEVERITY_COUNT);
   let all_scenarios = ALL_SCENARIO_IDS.iter().copied().collect::<BTreeSet<_>>();
   let mut expectation_keys = BTreeSet::new();
   let mut expectations_per_fault = BTreeMap::<&str, BTreeSet<DetectionSeverity>>::new();
   for expectation in &manifest.expectations
   {
      ensure!(fault_ids.contains(&expectation.fault_id), "detection expectation names unknown fault family {}", expectation.fault_id);
      ensure!(expectation_keys.insert((expectation.fault_id.as_str(), expectation.severity)), "duplicate detection expectation {} {:?}", expectation.fault_id, expectation.severity);
      expectations_per_fault.entry(&expectation.fault_id).or_default().insert(expectation.severity);
      ensure!(expectation.weight_basis_points > 0, "detection expectation {} has zero weight", expectation.id());
      ensure!(expectation.seeds.len() == MACOS_DETECTION_SEEDS_PER_EXPECTATION, "detection expectation {} must have exactly three seeds", expectation.id());
      ensure!(unique_nonzero_u64(&expectation.seeds), "detection expectation {} seeds must be nonzero and unique", expectation.id());
      let eligible = unique_nonempty(expectation.eligible_scenario_ids.iter().map(String::as_str), "eligible scenario")?;
      ensure!(!eligible.is_empty() && eligible.iter().all(|id| all_scenarios.contains(id.as_str())), "detection expectation {} has missing or unknown eligible scenarios", expectation.id());
      ensure!(!expectation.native_unit_injected_effect.trim().is_empty() && !expectation.minimum_detection_boundary.trim().is_empty(), "detection expectation {} has no injected effect or boundary", expectation.id());
      ensure!(!expectation.null_control_id.trim().is_empty(), "detection expectation {} has no null control", expectation.id());
      ensure!(expectation.maximum_false_positive_rate_basis_points <= manifest.maximum_null_false_positive_rate_basis_points, "detection expectation {} weakens the canonical null-control ceiling", expectation.id());
      match expectation.severity
      {
         DetectionSeverity::Small => ensure!(!expectation.must_detect && expectation.reviewed_non_must_detect_reason.as_ref().is_some_and(|reason| !reason.trim().is_empty()), "small expectation {} must be reviewed weighted-sensitivity evidence", expectation.id()),
         DetectionSeverity::Medium | DetectionSeverity::Large => ensure!(expectation.must_detect && expectation.reviewed_non_must_detect_reason.is_none(), "medium/large expectation {} must be must-detect", expectation.id()),
      }
      match expectation.expected_direction
      {
         DetectionExpectedDirection::NamedValidator =>
         {
            ensure!(expectation.expected_metric_ids.is_empty(), "validator expectation {} must not accept a correlated metric", expectation.id());
            ensure!(expectation.terminal_validator_id.as_ref().is_some_and(|id| !id.trim().is_empty()), "validator expectation {} has no terminal validator", expectation.id());
            ensure!(expectation.required_seed_count as usize == MACOS_DETECTION_SEEDS_PER_EXPECTATION, "validator expectation {} must fire on all three seeds", expectation.id());
            ensure!(expectation.maximum_false_positive_rate_basis_points == 0, "validator expectation {} must permit no false positives", expectation.id());
         }
         DetectionExpectedDirection::HigherIsWorse | DetectionExpectedDirection::LowerIsWorse =>
         {
            let expected_metrics = unique_nonempty(expectation.expected_metric_ids.iter().map(String::as_str), "expected metric")?;
            ensure!(!expected_metrics.is_empty() && expected_metrics.iter().all(|id| metric_ids.contains(id)), "performance expectation {} has missing or unknown metrics", expectation.id());
            ensure!(expectation.terminal_validator_id.is_none(), "performance expectation {} cannot substitute a terminal validator", expectation.id());
            ensure!(expectation.required_seed_count == 2, "performance expectation {} must require two of three seeds plus the median rule", expectation.id());
         }
      }
   }
   let expected_severities = [DetectionSeverity::Small, DetectionSeverity::Medium, DetectionSeverity::Large].into_iter().collect::<BTreeSet<_>>();
   ensure!(expectations_per_fault.len() == MACOS_DETECTION_FAULT_FAMILY_COUNT && expectations_per_fault.values().all(|severities| *severities == expected_severities), "every fault family must have exactly small, medium, and large expectations");
   ensure!(manifest.expectations.len() * MACOS_DETECTION_SEEDS_PER_EXPECTATION == manifest.expected_case_count as usize, "macOS detection expectation expansion does not produce the declared 126 cases");

   validate_tier_selections(manifest, &risk_ids, &all_scenarios)?;
   Ok(())
}

pub fn explain_macos_detection_coverage(manifest: &DetectionCoverageManifest, tier: Tier, selected_scenario_ids: &[String]) -> Result<DetectionCoverageExplanation>
{
   validate_macos_detection_coverage(manifest)?;
   let selection_tier = match tier
   {
      Tier::Extended | Tier::FullAttribution => Tier::ClaimComplete,
      other => other,
   };
   let selection = manifest.tier_selections.iter().find(|selection| selection.tier == selection_tier).with_context(|| format!("macOS detection coverage has no {:?} selection", selection_tier))?;
   ensure!(selection.selected_scenario_ids == selected_scenario_ids, "macOS {:?} plan scenarios differ from the canonical detection selection", tier);
   let selected = selected_scenario_ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
   let fault_risks = manifest.fault_families.iter().map(|fault| (fault.id.as_str(), fault.risk_dimension_ids.as_slice())).collect::<BTreeMap<_, _>>();
   let mut risk_scenarios = BTreeMap::<&str, BTreeSet<String>>::new();
   let mut must_total = 0u64;
   let mut must_covered = 0u64;
   let mut weight_total = 0u64;
   let mut weight_covered = 0u64;
   for expectation in &manifest.expectations
   {
      let covering = expectation.eligible_scenario_ids.iter().filter(|scenario| selected.contains(scenario.as_str())).cloned().collect::<Vec<_>>();
      let covered = !covering.is_empty();
      if expectation.must_detect
      {
         must_total += 1;
         must_covered += u64::from(covered);
      }
      weight_total += u64::from(expectation.weight_basis_points);
      if covered
      {
         weight_covered += u64::from(expectation.weight_basis_points);
         for risk in fault_risks[expectation.fault_id.as_str()]
         {
            risk_scenarios.entry(risk).or_default().extend(covering.iter().cloned());
         }
      }
   }
   let must_rate = rate_basis_points(must_covered, must_total)?;
   let weighted_rate = rate_basis_points(weight_covered, weight_total)?;
   ensure!(must_rate >= manifest.required_must_detect_rate_basis_points, "macOS {:?} selection covers only {} basis points of must-detect expectations", tier, must_rate);
   ensure!(weighted_rate >= manifest.required_weighted_detection_rate_basis_points, "macOS {:?} selection covers only {} basis points of weighted expectations", tier, weighted_rate);
   let risk_coverage = manifest.risk_dimensions.iter().map(|risk| DetectionRiskCoverage {
      risk_dimension_id: risk.id.clone(),
      selected_scenario_ids: risk_scenarios.remove(risk.id.as_str()).unwrap_or_default().into_iter().collect(),
   }).collect::<Vec<_>>();
   ensure!(risk_coverage.iter().all(|risk| !risk.selected_scenario_ids.is_empty()), "macOS {:?} selection leaves a production risk dimension uncovered", tier);
   Ok(DetectionCoverageExplanation {
      manifest_id: manifest.id.clone(),
      tier,
      selected_scenario_ids: selected_scenario_ids.to_vec(),
      risk_coverage,
      omitted_scenarios: selection.omitted_scenarios.clone(),
      expectation_count: manifest.expectations.len(),
      injected_case_count: manifest.expectations.len() * MACOS_DETECTION_SEEDS_PER_EXPECTATION,
      must_detect_expectation_count: must_total as usize,
      planned_must_detect_rate_basis_points: must_rate,
      planned_weighted_detection_rate_basis_points: weighted_rate,
   })
}

fn validate_tier_selections(manifest: &DetectionCoverageManifest, risk_ids: &BTreeSet<String>, all_scenarios: &BTreeSet<&str>) -> Result<()>
{
   let expected_tiers = [Tier::Pr, Tier::Nightly, Tier::ReleaseCore, Tier::ClaimComplete];
   ensure!(manifest.tier_selections.iter().map(|selection| selection.tier).eq(expected_tiers), "macOS detection tier selections must be PR, nightly, release-core, and claim-complete in order");
   for selection in &manifest.tier_selections
   {
      let selected = unique_nonempty(selection.selected_scenario_ids.iter().map(String::as_str), "selected scenario")?;
      ensure!(!selected.is_empty() && selected.iter().all(|id| all_scenarios.contains(id.as_str())), "macOS {:?} detection selection has missing or unknown scenarios", selection.tier);
      let omitted = unique_nonempty(selection.omitted_scenarios.iter().map(|scenario| scenario.scenario_id.as_str()), "omitted scenario")?;
      ensure!(selected.is_disjoint(&omitted), "macOS {:?} detection selection both selects and omits a scenario", selection.tier);
      let complete = selected.iter().chain(omitted.iter()).map(String::as_str).collect::<BTreeSet<_>>();
      ensure!(complete == *all_scenarios, "macOS {:?} detection selection does not disposition all thirteen scenarios", selection.tier);
      for scenario in &selection.omitted_scenarios
      {
         ensure!(!scenario.reason.trim().is_empty(), "macOS {:?} omitted scenario {} has no reason", selection.tier, scenario.scenario_id);
         ensure!(scenario.marginal_occupied_seconds > 0, "macOS {:?} omitted scenario {} has no marginal-time estimate", selection.tier, scenario.scenario_id);
         ensure!(scenario.marginal_expectation_count > 0, "macOS {:?} omitted scenario {} has no marginal expectation count", selection.tier, scenario.scenario_id);
         let risks = unique_nonempty(scenario.marginal_risk_dimension_ids.iter().map(String::as_str), "marginal risk dimension")?;
         ensure!(!risks.is_empty() && risks.iter().all(|id| risk_ids.contains(id)), "macOS {:?} omitted scenario {} has missing or unknown marginal risks", selection.tier, scenario.scenario_id);
         let eligible_count = manifest.expectations.iter().filter(|expectation| expectation.eligible_scenario_ids.contains(&scenario.scenario_id)).count() as u32;
         ensure!(scenario.marginal_expectation_count == eligible_count, "macOS {:?} omitted scenario {} marginal expectation count is stale", selection.tier, scenario.scenario_id);
      }
   }
   Ok(())
}

fn ordered_unique<'a>(values: impl Iterator<Item = &'a str>, label: &str) -> Result<BTreeSet<String>>
{
   let values = values.collect::<Vec<_>>();
   let unique = unique_nonempty(values.iter().copied(), label)?;
   ensure!(values.len() == unique.len(), "{} ids must be unique", label);
   Ok(unique)
}

fn unique_nonempty<'a>(values: impl Iterator<Item = &'a str>, label: &str) -> Result<BTreeSet<String>>
{
   let mut unique = BTreeSet::new();
   for value in values
   {
      ensure!(!value.trim().is_empty(), "{} id is empty", label);
      ensure!(unique.insert(value.to_string()), "duplicate {} id {}", label, value);
   }
   Ok(unique)
}

fn unique_nonzero_u64(values: &[u64]) -> bool
{
   values.iter().all(|value| *value > 0) && values.iter().copied().collect::<BTreeSet<_>>().len() == values.len()
}

fn rate_basis_points(numerator: u64, denominator: u64) -> Result<u32>
{
   ensure!(denominator > 0, "detection coverage rate has a zero denominator");
   Ok((numerator.saturating_mul(10_000) / denominator) as u32)
}
