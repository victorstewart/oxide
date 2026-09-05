use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::schema::{BudgetSpec, Platform, Tier, BENCHMARK_SPEC_SCHEMA_VERSION};

pub const APPLE_PR_SCENARIO_IDS: &[&str] = &[
   "startup.first-screen",
   "dashboard.mixed-static",
   "feed.variable-scroll",
   "chat.live-update",
   "navigation.modal",
   "image.decode-zoom",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ApplePrAcquisitionSpec
{
   pub schema_version: u32,
   pub id: String,
   pub platform: Platform,
   pub tier: Tier,
   pub build_for_testing_count: u32,
   pub public_xctest_methods: Vec<String>,
   pub selected_scenario_ids: Vec<String>,
   pub packs: Vec<AcquisitionPackBudget>,
   pub controller_chunks: Vec<AcquisitionChunkBudget>,
   pub install_budget_seconds: u64,
   pub correctness_budget_seconds: u64,
   pub artifact_pull_count: u32,
   pub artifact_pull_seconds_each: u64,
   pub reserve_seconds: u64,
   pub hard_total_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcquisitionPackBudget
{
   pub id: String,
   pub ordered_scenario_ids: Vec<String>,
   pub reset_segments: Vec<AcquisitionResetSegment>,
   pub pair_count: u32,
   pub sides_per_pair: u32,
   pub reset_count_per_side: u32,
   pub reset_seconds: u64,
   pub setup_seconds_per_scenario: u64,
   pub warmup_seconds_per_scenario: u64,
   pub measure_seconds_per_scenario: u64,
   pub side_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcquisitionResetSegment
{
   pub id: String,
   pub ordered_scenario_ids: Vec<String>,
}

impl AcquisitionPackBudget
{
   pub fn computed_side_seconds(&self) -> Option<u64>
   {
      let resets = u64::from(self.reset_count_per_side).checked_mul(self.reset_seconds)?;
      let scenario_count = u64::try_from(self.ordered_scenario_ids.len()).ok()?;
      let scenario_seconds = self.setup_seconds_per_scenario
         .checked_add(self.warmup_seconds_per_scenario)?
         .checked_add(self.measure_seconds_per_scenario)?;
      resets.checked_add(scenario_count.checked_mul(scenario_seconds)?)
   }

   pub fn computed_campaign_seconds(&self) -> Option<u64>
   {
      self.side_seconds
         .checked_mul(u64::from(self.pair_count))?
         .checked_mul(u64::from(self.sides_per_pair))
   }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AcquisitionChunkBudget
{
   pub id: String,
   pub xctest_method: String,
   pub pass_id: String,
   pub pack_ids: Vec<String>,
   pub ordered_pair_indices: Vec<u32>,
   pub max_occupied_seconds: u64,
   pub artifact_pull_after: bool,
}

pub fn validate_apple_pr_acquisition(spec: &ApplePrAcquisitionSpec, budget: &BudgetSpec) -> Result<()>
{
   ensure!(spec.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION, "Apple PR acquisition has unsupported schema version {}", spec.schema_version);
   ensure!(spec.id == "apple-pr" && spec.platform == Platform::Apple && spec.tier == Tier::Pr, "Apple PR acquisition identity is not canonical");
   ensure!(budget.id == spec.id && budget.platform == spec.platform && budget.tier == spec.tier, "Apple PR acquisition budget identity differs from the expansion");
   ensure!(spec.build_for_testing_count == 1, "Apple PR must build-for-testing exactly once");
   ensure!(spec.public_xctest_methods == ["ComparisonControllerUITests.testManifestCampaign", "ComparisonControllerUITests.testLaunchCampaign"], "Apple PR must expose exactly the two manifest-driven XCTest methods");
   ensure!(spec.selected_scenario_ids.iter().map(String::as_str).eq(APPLE_PR_SCENARIO_IDS.iter().copied()), "Apple PR selected scenarios differ from the canonical six-scenario tier");
   ensure!(spec.packs.len() == 2, "Apple PR must contain exactly non-launch and launch packs");

   let dynamic = spec.packs.iter().find(|pack| pack.id == "pr-non-launch").ok_or_else(|| anyhow::anyhow!("Apple PR non-launch pack is missing"))?;
   let launch = spec.packs.iter().find(|pack| pack.id == "pr-launch").ok_or_else(|| anyhow::anyhow!("Apple PR launch pack is missing"))?;
   ensure!(dynamic.ordered_scenario_ids == ["dashboard.mixed-static", "chat.live-update", "navigation.modal", "feed.variable-scroll", "image.decode-zoom"], "Apple PR non-launch pack does not contain the five canonical scenarios in reset-segment order");
   ensure!(launch.ordered_scenario_ids == ["startup.first-screen"], "Apple PR launch pack is not the canonical startup scenario");
   let reset_segments = [
      ("core-interaction", &["dashboard.mixed-static", "chat.live-update", "navigation.modal"][..]),
      ("scroll-damage", &["feed.variable-scroll"][..]),
      ("media-text-warm", &["image.decode-zoom"][..]),
   ];
   ensure!(dynamic.reset_segments.len() == reset_segments.len(), "Apple PR non-launch pack must declare exactly three reset segments");
   for (segment, expected) in dynamic.reset_segments.iter().zip(reset_segments)
   {
      ensure!(segment.id == expected.0 && segment.ordered_scenario_ids.iter().map(String::as_str).eq(expected.1.iter().copied()), "Apple PR reset segment {} differs from the canonical scenario ownership", segment.id);
   }
   ensure!(launch.reset_segments.is_empty(), "Apple PR launch pack must not declare a warm reset segment");
   for pack in &spec.packs
   {
      ensure!(pack.pair_count == 4 && pack.sides_per_pair == 2, "Apple PR pack {} must contain four A/B pairs", pack.id);
      ensure!(pack.reset_segments.len() == pack.reset_count_per_side as usize, "Apple PR pack {} reset segment count differs from its time budget", pack.id);
      ensure!(pack.computed_side_seconds() == Some(pack.side_seconds), "Apple PR pack {} side-time arithmetic is inconsistent", pack.id);
   }
   ensure!(dynamic.computed_campaign_seconds() == Some(budget.primary_dynamic_presentation_seconds), "Apple PR dynamic pack does not expand to the presentation budget");
   ensure!(launch.computed_campaign_seconds() == Some(budget.launch_or_startup_delivery_seconds), "Apple PR launch pack does not expand to the launch budget");

   let expected_chunks = [
      ("correctness", "ComparisonControllerUITests.testManifestCampaign", "correctness", &["pr-non-launch", "pr-launch"][..], &[][..], 90),
      ("presentation-pairs-0-1", "ComparisonControllerUITests.testManifestCampaign", "minimal-presentation", &["pr-non-launch"][..], &[0, 1][..], 240),
      ("presentation-pairs-2-3", "ComparisonControllerUITests.testManifestCampaign", "minimal-presentation", &["pr-non-launch"][..], &[2, 3][..], 240),
      ("launch-pairs-0-3", "ComparisonControllerUITests.testLaunchCampaign", "canonical-launch", &["pr-launch"][..], &[0, 1, 2, 3][..], 120),
   ];
   ensure!(spec.controller_chunks.len() == expected_chunks.len(), "Apple PR must use exactly four controller acquisitions");
   let mut occupied_seconds = 0_u64;
   let mut pull_count = 0_u32;
   for (chunk, expected) in spec.controller_chunks.iter().zip(expected_chunks)
   {
      ensure!(chunk.id == expected.0 && chunk.xctest_method == expected.1 && chunk.pass_id == expected.2, "Apple PR controller chunk {} has a noncanonical identity", chunk.id);
      ensure!(chunk.pack_ids.iter().map(String::as_str).eq(expected.3.iter().copied()), "Apple PR controller chunk {} has noncanonical packs", chunk.id);
      ensure!(chunk.ordered_pair_indices == expected.4, "Apple PR controller chunk {} has noncanonical pair indices", chunk.id);
      ensure!(chunk.max_occupied_seconds == expected.5 && chunk.max_occupied_seconds <= 300, "Apple PR controller chunk {} violates its occupied-time boundary", chunk.id);
      ensure!(chunk.artifact_pull_after, "Apple PR controller chunk {} must be followed by a durable host pull", chunk.id);
      ensure!(chunk.pass_id != "lean", "Apple PR must not add a separate lean replay");
      occupied_seconds = occupied_seconds.checked_add(chunk.max_occupied_seconds).ok_or_else(|| anyhow::anyhow!("Apple PR occupied time overflows u64"))?;
      pull_count += 1;
   }
   ensure!(occupied_seconds <= 1_200, "Apple PR controller acquisitions exceed twenty occupied minutes");
   ensure!(pull_count == spec.artifact_pull_count && spec.artifact_pull_count == 4, "Apple PR must pull artifacts after all four acquisitions");

   let pulls = u64::from(spec.artifact_pull_count).checked_mul(spec.artifact_pull_seconds_each).ok_or_else(|| anyhow::anyhow!("Apple PR pull budget overflows u64"))?;
   let correctness_install_pulls = spec.correctness_budget_seconds
      .checked_add(spec.install_budget_seconds)
      .and_then(|seconds| seconds.checked_add(pulls))
      .ok_or_else(|| anyhow::anyhow!("Apple PR correctness/install/pull budget overflows u64"))?;
   ensure!(correctness_install_pulls == budget.correctness_install_pulls_seconds, "Apple PR correctness/install/pull expansion differs from the committed budget");
   ensure!(spec.correctness_budget_seconds == 90 && spec.install_budget_seconds == 45 && spec.artifact_pull_seconds_each == 5, "Apple PR correctness, install, or pull component changed from the frozen expansion");
   ensure!(spec.reserve_seconds == budget.reserve_seconds && spec.hard_total_seconds == budget.hard_total_seconds, "Apple PR reserve-inclusive ceiling differs from the committed budget");
   ensure!(spec.hard_total_seconds <= 1_200, "Apple PR reserve-inclusive campaign exceeds twenty minutes");

   let chunk_ids = spec.controller_chunks.iter().map(|chunk| chunk.id.as_str()).collect::<BTreeSet<_>>();
   ensure!(chunk_ids.len() == spec.controller_chunks.len(), "Apple PR controller chunk ids are not unique");
   Ok(())
}
