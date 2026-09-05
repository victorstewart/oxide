use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{Platform, Tier, APPLE_RELEASE_SCENARIO_IDS, BENCHMARK_SPEC_SCHEMA_VERSION};

pub const MACOS_FULL_ATTRIBUTION_PLAN_ID: &str = "macos-full-attribution-v1";
pub const MACOS_FULL_ATTRIBUTION_PAIR_COUNT: u32 = 12;
pub const MACOS_FULL_ATTRIBUTION_RESET_SECONDS: u64 = 5;
pub const MACOS_FULL_ATTRIBUTION_SETUP_SECONDS: u64 = 1;
pub const MACOS_FULL_ATTRIBUTION_WARMUP_SECONDS: u64 = 5;
pub const MACOS_FULL_ATTRIBUTION_MEASUREMENT_SECONDS: u64 = 20;
pub const MACOS_FULL_ATTRIBUTION_MAX_HARD_SECONDS: u64 = 24 * 60 * 60;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsAttributionCollectorKind
{
   Allocations,
   VmTracker,
   MetalSystemTrace,
   GpuCounters,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MacOsAttributionAvailability
{
   Available,
   UnavailableDevice,
   UnavailableToolchain,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsAttributionTraceSelector
{
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub template: Option<String>,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub instrument: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsGpuCounterConfiguration
{
   pub id: String,
   pub availability: MacOsAttributionAvailability,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub selector: Option<MacOsAttributionTraceSelector>,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsAttributionReplaySpec
{
   pub id: String,
   pub collector: MacOsAttributionCollectorKind,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub configuration_id: Option<String>,
   pub selector: MacOsAttributionTraceSelector,
   pub pair_count: u32,
   pub reset_seconds_per_session: u64,
   pub setup_seconds_per_scenario: u64,
   pub warmup_seconds_per_scenario: u64,
   pub measurement_seconds_per_scenario: u64,
   pub scenario_ids: Vec<String>,
   pub evidence_role: String,
   pub combination_calibration: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsAttributionBudget
{
   pub occupied_seconds_per_replay: u64,
   pub available_replay_count: u64,
   pub occupied_seconds: u64,
   pub reserve_seconds: u64,
   pub hard_total_seconds: u64,
   pub hard_ceiling_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacOsFullAttributionPlan
{
   pub schema_version: u32,
   pub id: String,
   pub platform: Platform,
   pub tier: Tier,
   pub scenario_ids: Vec<String>,
   pub replays: Vec<MacOsAttributionReplaySpec>,
   pub gpu_counter_configurations: Vec<MacOsGpuCounterConfiguration>,
   pub budget: MacOsAttributionBudget,
}

pub fn materialize_macos_full_attribution_plan(gpu_counter_configurations: Vec<MacOsGpuCounterConfiguration>) -> Result<MacOsFullAttributionPlan>
{
   let scenarios = APPLE_RELEASE_SCENARIO_IDS.iter().map(|id| String::from(*id)).collect::<Vec<_>>();
   let mut replays = vec![
      replay("allocations", MacOsAttributionCollectorKind::Allocations, None, selector_template("Allocations"), &scenarios),
      replay("vm-tracker", MacOsAttributionCollectorKind::VmTracker, None, selector_instrument("VM Tracker"), &scenarios),
      replay("metal-system-trace", MacOsAttributionCollectorKind::MetalSystemTrace, None, selector_template("Metal System Trace"), &scenarios),
   ];
   for configuration in &gpu_counter_configurations
   {
      if configuration.availability == MacOsAttributionAvailability::Available
      {
         let selector = configuration.selector.clone().with_context(|| format!("available GPU-counter configuration {} has no trace selector", configuration.id))?;
         replays.push(replay(
            &format!("gpu-counters-{}", configuration.id),
            MacOsAttributionCollectorKind::GpuCounters,
            Some(configuration.id.clone()),
            selector,
            &scenarios,
         ));
      }
   }
   let occupied_seconds_per_replay = replay_occupied_seconds()?;
   let available_replay_count = replays.len() as u64;
   let occupied_seconds = occupied_seconds_per_replay.checked_mul(available_replay_count).context("macOS full-attribution occupied-time overflow")?;
   let reserve_seconds = occupied_seconds.checked_add(4).context("macOS full-attribution reserve rounding overflow")? / 5;
   let hard_total_seconds = occupied_seconds.checked_add(reserve_seconds).context("macOS full-attribution hard-total overflow")?;
   let plan = MacOsFullAttributionPlan {
      schema_version: BENCHMARK_SPEC_SCHEMA_VERSION,
      id: String::from(MACOS_FULL_ATTRIBUTION_PLAN_ID),
      platform: Platform::Apple,
      tier: Tier::FullAttribution,
      scenario_ids: scenarios,
      replays,
      gpu_counter_configurations,
      budget: MacOsAttributionBudget {
         occupied_seconds_per_replay,
         available_replay_count,
         occupied_seconds,
         reserve_seconds,
         hard_total_seconds,
         hard_ceiling_seconds: MACOS_FULL_ATTRIBUTION_MAX_HARD_SECONDS,
      },
   };
   validate_macos_full_attribution_plan(&plan)?;
   Ok(plan)
}

pub fn validate_macos_full_attribution_plan(plan: &MacOsFullAttributionPlan) -> Result<()>
{
   ensure!(plan.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "macOS full-attribution plan has unsupported schema version {}", plan.schema_version);
   ensure!(plan.id == MACOS_FULL_ATTRIBUTION_PLAN_ID && plan.platform == Platform::Apple && plan.tier == Tier::FullAttribution, "macOS full-attribution plan identity is not canonical");
   ensure!(plan.scenario_ids.iter().map(String::as_str).eq(APPLE_RELEASE_SCENARIO_IDS.iter().copied()), "macOS full-attribution plan must cover the exact 13-scenario release matrix");
   ensure!(plan.replays.len() >= 3, "macOS full-attribution plan omits a mandatory replay");
   let replay_ids = unique(plan.replays.iter().map(|replay| replay.id.as_str()), "macOS attribution replay")?;
   ensure!(replay_ids.contains("allocations") && replay_ids.contains("vm-tracker") && replay_ids.contains("metal-system-trace"), "macOS full-attribution plan omits Allocations, VM Tracker, or Metal System Trace");
   let mandatory = [
      ("allocations", MacOsAttributionCollectorKind::Allocations),
      ("vm-tracker", MacOsAttributionCollectorKind::VmTracker),
      ("metal-system-trace", MacOsAttributionCollectorKind::MetalSystemTrace),
   ];
   for (id, collector) in mandatory
   {
      let replay = plan.replays.iter().find(|replay| replay.id == id).with_context(|| format!("missing mandatory macOS attribution replay {}", id))?;
      ensure!(replay.collector == collector && replay.configuration_id.is_none(), "mandatory macOS attribution replay {} has a changed collector or configuration", id);
   }
   let configuration_ids = unique(plan.gpu_counter_configurations.iter().map(|configuration| configuration.id.as_str()), "macOS GPU-counter configuration")?;
   ensure!(!configuration_ids.is_empty(), "macOS full-attribution plan must record at least one available or explicitly unavailable GPU-counter configuration");
   for configuration in &plan.gpu_counter_configurations
   {
      ensure!(!configuration.id.is_empty() && configuration.id.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'), "macOS GPU-counter configuration id is not path-safe lowercase kebab-case");
      match configuration.availability
      {
         MacOsAttributionAvailability::Available =>
         {
            ensure!(configuration.selector.is_some() && configuration.unavailable_reason.is_none(), "available GPU-counter configuration {} must have exactly one selector and no unavailability reason", configuration.id);
         }
         MacOsAttributionAvailability::UnavailableDevice | MacOsAttributionAvailability::UnavailableToolchain =>
         {
            ensure!(configuration.selector.is_none() && configuration.unavailable_reason.as_ref().is_some_and(|reason| !reason.trim().is_empty()), "unavailable GPU-counter configuration {} must have one nonempty reason and no selector", configuration.id);
         }
      }
   }
   let available_ids = plan.gpu_counter_configurations.iter().filter(|configuration| configuration.availability == MacOsAttributionAvailability::Available).map(|configuration| configuration.id.as_str()).collect::<BTreeSet<_>>();
   let replay_configuration_ids = plan.replays.iter().filter(|replay| replay.collector == MacOsAttributionCollectorKind::GpuCounters).map(|replay| replay.configuration_id.as_deref().unwrap_or("")).collect::<BTreeSet<_>>();
   ensure!(replay_configuration_ids == available_ids, "macOS full-attribution replays must cover each and only each available GPU-counter configuration");
   ensure!(replay_configuration_ids.is_subset(&configuration_ids), "macOS full-attribution replay refers to an unknown GPU-counter configuration");
   for replay in &plan.replays
   {
      validate_selector(&replay.selector)?;
      ensure!(replay.pair_count == MACOS_FULL_ATTRIBUTION_PAIR_COUNT, "macOS full-attribution replay {} must use release pair count {}", replay.id, MACOS_FULL_ATTRIBUTION_PAIR_COUNT);
      ensure!(replay.reset_seconds_per_session == MACOS_FULL_ATTRIBUTION_RESET_SECONDS
         && replay.setup_seconds_per_scenario == MACOS_FULL_ATTRIBUTION_SETUP_SECONDS
         && replay.warmup_seconds_per_scenario == MACOS_FULL_ATTRIBUTION_WARMUP_SECONDS
         && replay.measurement_seconds_per_scenario == MACOS_FULL_ATTRIBUTION_MEASUREMENT_SECONDS,
         "macOS full-attribution replay {} timing differs from the frozen contract", replay.id);
      ensure!(replay.scenario_ids == plan.scenario_ids, "macOS full-attribution replay {} does not cover all 13 scenarios in canonical order", replay.id);
      ensure!(replay.evidence_role == "descriptive-diagnostic" && replay.combination_calibration == "none-isolated-replay", "macOS full-attribution replay {} attempts a claim or uncalibrated collector combination", replay.id);
   }
   let occupied_seconds_per_replay = replay_occupied_seconds()?;
   let available_replay_count = plan.replays.len() as u64;
   let occupied_seconds = occupied_seconds_per_replay.checked_mul(available_replay_count).context("macOS full-attribution occupied-time overflow")?;
   let reserve_seconds = occupied_seconds.checked_add(4).context("macOS full-attribution reserve rounding overflow")? / 5;
   let hard_total_seconds = occupied_seconds.checked_add(reserve_seconds).context("macOS full-attribution hard-total overflow")?;
   ensure!(plan.budget == MacOsAttributionBudget {
      occupied_seconds_per_replay,
      available_replay_count,
      occupied_seconds,
      reserve_seconds,
      hard_total_seconds,
      hard_ceiling_seconds: MACOS_FULL_ATTRIBUTION_MAX_HARD_SECONDS,
   }, "macOS full-attribution budget arithmetic differs from the exact replay expansion");
   ensure!(hard_total_seconds <= MACOS_FULL_ATTRIBUTION_MAX_HARD_SECONDS, "macOS full-attribution plan exceeds its 24-hour hard ceiling");
   Ok(())
}

pub fn canonical_macos_full_attribution_plan_json(plan: &MacOsFullAttributionPlan) -> Result<Vec<u8>>
{
   validate_macos_full_attribution_plan(plan)?;
   let mut bytes = serde_json::to_vec_pretty(plan).context("serializing macOS full-attribution plan")?;
   bytes.push(b'\n');
   Ok(bytes)
}

fn replay(id: &str, collector: MacOsAttributionCollectorKind, configuration_id: Option<String>, selector: MacOsAttributionTraceSelector, scenario_ids: &[String]) -> MacOsAttributionReplaySpec
{
   MacOsAttributionReplaySpec {
      id: String::from(id),
      collector,
      configuration_id,
      selector,
      pair_count: MACOS_FULL_ATTRIBUTION_PAIR_COUNT,
      reset_seconds_per_session: MACOS_FULL_ATTRIBUTION_RESET_SECONDS,
      setup_seconds_per_scenario: MACOS_FULL_ATTRIBUTION_SETUP_SECONDS,
      warmup_seconds_per_scenario: MACOS_FULL_ATTRIBUTION_WARMUP_SECONDS,
      measurement_seconds_per_scenario: MACOS_FULL_ATTRIBUTION_MEASUREMENT_SECONDS,
      scenario_ids: scenario_ids.to_vec(),
      evidence_role: String::from("descriptive-diagnostic"),
      combination_calibration: String::from("none-isolated-replay"),
   }
}

fn selector_template(template: &str) -> MacOsAttributionTraceSelector
{
   MacOsAttributionTraceSelector {template: Some(String::from(template)), instrument: None}
}

fn selector_instrument(instrument: &str) -> MacOsAttributionTraceSelector
{
   MacOsAttributionTraceSelector {template: None, instrument: Some(String::from(instrument))}
}

fn validate_selector(selector: &MacOsAttributionTraceSelector) -> Result<()>
{
   match (&selector.template, &selector.instrument)
   {
      (Some(template), None) if !template.trim().is_empty() => Ok(()),
      (None, Some(instrument)) if !instrument.trim().is_empty() => Ok(()),
      _ => bail!("macOS attribution trace selector must name exactly one nonempty template or instrument"),
   }
}

fn replay_occupied_seconds() -> Result<u64>
{
   let scenario_seconds = MACOS_FULL_ATTRIBUTION_SETUP_SECONDS
      .checked_add(MACOS_FULL_ATTRIBUTION_WARMUP_SECONDS)
      .and_then(|seconds| seconds.checked_add(MACOS_FULL_ATTRIBUTION_MEASUREMENT_SECONDS))
      .context("macOS attribution scenario-time overflow")?;
   let side_seconds = scenario_seconds.checked_mul(APPLE_RELEASE_SCENARIO_IDS.len() as u64)
      .and_then(|seconds| seconds.checked_add(MACOS_FULL_ATTRIBUTION_RESET_SECONDS))
      .context("macOS attribution side-time overflow")?;
   side_seconds.checked_mul(MACOS_FULL_ATTRIBUTION_PAIR_COUNT as u64)
      .and_then(|seconds| seconds.checked_mul(2))
      .context("macOS attribution replay-time overflow")
}

fn unique<'a>(values: impl Iterator<Item = &'a str>, label: &str) -> Result<BTreeSet<&'a str>>
{
   let mut set = BTreeSet::new();
   for value in values
   {
      ensure!(!value.is_empty(), "{} id is empty", label);
      ensure!(set.insert(value), "{} id {} is duplicated", label, value);
   }
   Ok(set)
}
