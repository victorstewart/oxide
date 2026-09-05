use std::collections::BTreeSet;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::schema::BENCHMARK_SPEC_SCHEMA_VERSION;

pub const COMPARISON_SCENARIO_IDS: &[&str] = &[
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyDisposition
{
   ComparativePhase,
   InternalMicrobench,
   FastCorrectness,
   SpecializedSuite,
   NativeCeilingOnly,
   RemoveDuplicate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LegacyDispositionGroup
{
   pub id: String,
   pub disposition: LegacyDisposition,
   pub target_id: String,
   pub covered_risk_dimensions: Vec<String>,
   pub methods: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LegacyCoverageGap
{
   pub risk_dimension: String,
   pub required_replacement_target: String,
   pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LegacyCaseDisposition
{
   pub schema_version: u32,
   pub source_revision: String,
   pub source_files: Vec<String>,
   pub required_risk_dimensions: Vec<String>,
   pub known_coverage_gaps: Vec<LegacyCoverageGap>,
   pub groups: Vec<LegacyDispositionGroup>,
}

pub fn validate_legacy_case_disposition(manifest: &LegacyCaseDisposition, expected_methods: &BTreeSet<String>) -> Result<()>
{
   if manifest.schema_version != BENCHMARK_SPEC_SCHEMA_VERSION
   {
      bail!("legacy disposition schema_version must be {}", BENCHMARK_SPEC_SCHEMA_VERSION);
   }
   if manifest.source_revision.trim().is_empty()
   {
      bail!("legacy disposition source_revision must not be empty");
   }
   if manifest.source_files.len() != 2 || manifest.source_files.iter().any(|path| path.trim().is_empty())
   {
      bail!("legacy disposition must name exactly the two reviewed XCTest source files");
   }
   ensure_unique_nonempty("legacy source file", manifest.source_files.iter().map(String::as_str))?;
   ensure_unique_nonempty("required risk dimension", manifest.required_risk_dimensions.iter().map(String::as_str))?;
   if manifest.required_risk_dimensions.is_empty()
   {
      bail!("legacy disposition must preserve at least one required risk dimension");
   }
   if manifest.groups.is_empty()
   {
      bail!("legacy disposition must contain reviewed groups");
   }

   let required_risks = manifest.required_risk_dimensions.iter().cloned().collect::<BTreeSet<_>>();
   let mut gap_risks = BTreeSet::new();
   for gap in &manifest.known_coverage_gaps
   {
      if gap.risk_dimension.trim().is_empty()
         || gap.required_replacement_target.trim().is_empty()
         || gap.reason.trim().is_empty()
         || !gap_risks.insert(gap.risk_dimension.clone())
      {
         bail!("legacy coverage gaps must have unique risks, replacement targets, and reasons");
      }
      if required_risks.contains(&gap.risk_dimension)
      {
         bail!("legacy risk `{}` cannot be both covered and a known gap", gap.risk_dimension);
      }
   }
   let mut covered_risks = BTreeSet::new();
   let mut group_ids = BTreeSet::new();
   let mut mapped_methods = BTreeSet::new();
   for group in &manifest.groups
   {
      if group.id.trim().is_empty() || !group_ids.insert(group.id.clone())
      {
         bail!("legacy disposition group ids must be non-empty and unique: `{}`", group.id);
      }
      if group.target_id.trim().is_empty()
      {
         bail!("legacy disposition group `{}` must name a target", group.id);
      }
      if group.disposition == LegacyDisposition::ComparativePhase && !COMPARISON_SCENARIO_IDS.contains(&group.target_id.as_str())
      {
         bail!("legacy comparative group `{}` targets unknown scenario `{}`", group.id, group.target_id);
      }
      if group.methods.is_empty()
      {
         bail!("legacy disposition group `{}` must contain at least one method", group.id);
      }
      for risk in &group.covered_risk_dimensions
      {
         if !required_risks.contains(risk)
         {
            bail!("legacy disposition group `{}` names unknown risk dimension `{}`", group.id, risk);
         }
         if group.disposition != LegacyDisposition::RemoveDuplicate
         {
            covered_risks.insert(risk.clone());
         }
      }
      for method in &group.methods
      {
         if method.trim().is_empty() || !mapped_methods.insert(method.clone())
         {
            bail!("legacy performance method is empty or mapped more than once: `{}`", method);
         }
      }
   }

   let missing_methods = expected_methods.difference(&mapped_methods).cloned().collect::<Vec<_>>();
   let unexpected_methods = mapped_methods.difference(expected_methods).cloned().collect::<Vec<_>>();
   if !missing_methods.is_empty() || !unexpected_methods.is_empty()
   {
      bail!("legacy disposition source mismatch; missing={:?} unexpected={:?}", missing_methods, unexpected_methods);
   }
   let uncovered_risks = required_risks.difference(&covered_risks).cloned().collect::<Vec<_>>();
   if !uncovered_risks.is_empty()
   {
      bail!("legacy disposition loses required risk coverage: {:?}", uncovered_risks);
   }
   Ok(())
}

fn ensure_unique_nonempty<'a>(label: &str, values: impl Iterator<Item = &'a str>) -> Result<()>
{
   let mut unique = BTreeSet::new();
   for value in values
   {
      if value.trim().is_empty() || !unique.insert(value)
      {
         bail!("{} values must be non-empty and unique: `{}`", label, value);
      }
   }
   Ok(())
}
