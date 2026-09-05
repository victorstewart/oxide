use serde::{Deserialize, Serialize};

pub const BENCHMARK_SPEC_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform
{
   Apple,
   Web,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tier
{
   Pr,
   Nightly,
   ReleaseCore,
   ClaimComplete,
   Extended,
   FullAttribution,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BudgetSpec
{
   pub schema_version: u32,
   pub id: String,
   pub platform: Platform,
   pub tier: Tier,
   pub correctness_install_pulls_seconds: u64,
   pub primary_dynamic_presentation_seconds: u64,
   pub launch_or_startup_delivery_seconds: u64,
   pub idle_endurance_seconds: u64,
   pub energy_seconds: u64,
   pub attribution_seconds: u64,
   pub pre_reserve_seconds: u64,
   pub reserve_seconds: u64,
   pub hard_total_seconds: u64,
   pub campaign_critical_wall_seconds: u64,
   pub campaign_aggregate_seconds: u64,
}

impl BudgetSpec
{
   pub fn computed_pre_reserve_seconds(&self) -> Option<u64>
   {
      self.correctness_install_pulls_seconds
         .checked_add(self.primary_dynamic_presentation_seconds)?
         .checked_add(self.launch_or_startup_delivery_seconds)?
         .checked_add(self.idle_endurance_seconds)?
         .checked_add(self.energy_seconds)?
         .checked_add(self.attribution_seconds)
   }

   pub fn computed_reserve_seconds(&self) -> Option<u64>
   {
      self.computed_pre_reserve_seconds()?.checked_add(4).map(|seconds| seconds / 5)
   }

   pub fn computed_hard_total_seconds(&self) -> Option<u64>
   {
      self.computed_pre_reserve_seconds()?.checked_add(self.computed_reserve_seconds()?)
   }
}
