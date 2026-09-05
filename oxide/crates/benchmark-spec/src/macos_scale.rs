use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::ArtifactIdentity;

pub const MACOS_COMPARATOR_SCALE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsComparatorScale
{
   OneX,
   TwoX,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsComparatorScaleDimension
{
   DatasetCardinality,
   OperationCardinality,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsComparatorScaleTransform
{
   NamespacedDatasetShards,
   IsolatedOperationShadow,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MacOsComparatorScaleOverlay
{
   pub schema_version: u32,
   pub scenario_id: String,
   pub scale: MacOsComparatorScale,
   pub dimension: MacOsComparatorScaleDimension,
   pub transform: MacOsComparatorScaleTransform,
   pub base_cardinality: u64,
   pub effective_cardinality: u64,
   pub fixture: ArtifactIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MacOsComparatorRuntimeAttestation
{
   pub schema_version: u32,
   pub scenario_id: String,
   pub side: MacOsComparatorSide,
   pub scale: MacOsComparatorScale,
   pub scale_overlay_sha256: String,
   pub application_run_count: u32,
   pub effective_cardinality: u64,
   pub completed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MacOsComparatorScaleVariant
{
   pub scenario_id: String,
   pub scale: MacOsComparatorScale,
   pub overlay: ArtifactIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MacOsComparatorQualificationPlan
{
   pub schema_version: u32,
   pub profile_window_seconds: u32,
   pub refresh_interval_ns: u64,
   pub variants: Vec<MacOsComparatorScaleVariant>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsComparatorSide
{
   AppKit,
   Oxide,
}

pub fn canonical_macos_comparator_scale_overlay_json(overlay: &MacOsComparatorScaleOverlay) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(overlay).context("serializing macOS comparator scale overlay")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn canonical_macos_comparator_runtime_attestation_json(attestation: &MacOsComparatorRuntimeAttestation) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(attestation).context("serializing macOS comparator runtime attestation")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn canonical_macos_comparator_qualification_plan_json(plan: &MacOsComparatorQualificationPlan) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(plan).context("serializing macOS comparator qualification plan")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn validate_macos_comparator_scale_overlay(overlay: &MacOsComparatorScaleOverlay, scenario_id: &str, fixture: &ArtifactIdentity) -> Result<()>
{
   ensure!(overlay.schema_version == MACOS_COMPARATOR_SCALE_SCHEMA_VERSION, "macOS comparator scale overlay has unsupported schema version {}", overlay.schema_version);
   ensure!(overlay.scenario_id == scenario_id, "macOS comparator scale overlay names {}, expected {}", overlay.scenario_id, scenario_id);
   ensure!(&overlay.fixture == fixture, "macOS comparator scale overlay fixture identity differs from the scenario fixture");
   let (dimension, transform, base_cardinality) = macos_comparator_scale_contract(scenario_id)?;
   ensure!(overlay.dimension == dimension, "macOS comparator scale dimension is invalid for {}", scenario_id);
   ensure!(overlay.transform == transform, "macOS comparator scale transform is invalid for {}", scenario_id);
   ensure!(overlay.base_cardinality == base_cardinality, "macOS comparator scale base cardinality is invalid for {}", scenario_id);
   let multiplier = match overlay.scale
   {
      MacOsComparatorScale::OneX => 1,
      MacOsComparatorScale::TwoX => 2,
   };
   let effective = base_cardinality.checked_mul(multiplier).context("macOS comparator scale cardinality overflow")?;
   ensure!(overlay.effective_cardinality == effective, "macOS comparator scale overlay effective cardinality is not exact {:?}", overlay.scale);
   Ok(())
}

pub fn macos_comparator_scale_contract(scenario_id: &str) -> Result<(MacOsComparatorScaleDimension, MacOsComparatorScaleTransform, u64)>
{
   match scenario_id
   {
      "startup.first-screen" => Ok((MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 24 * 1_024)),
      "feed.variable-scroll" => Ok((MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 2_000)),
      "grid.large-scroll" => Ok((MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 10_000)),
      "chat.live-update" => Ok((MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 5_000)),
      "mutation.damage" => Ok((MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 10_000)),
      "text.multilingual" => Ok((MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards, 1_000)),
      "dashboard.mixed-static" => Ok((MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 20)),
      "navigation.modal" => Ok((MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 4)),
      "image.decode-zoom" => Ok((MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 1)),
      "effects.layers" => Ok((MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 3)),
      "resize.theme" => Ok((MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 10)),
      "idle.steady" => Ok((MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 1)),
      "endurance.churn" => Ok((MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow, 1_300)),
      _ => bail!("macOS comparator scaling has no frozen transformation for {}", scenario_id),
   }
}
