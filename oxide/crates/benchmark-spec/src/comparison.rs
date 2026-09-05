use std::collections::BTreeMap;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::scenario::ArtifactIdentity;
use crate::schema::{Platform, Tier};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DecimalU64(pub u64);

impl Serialize for DecimalU64
{
   fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer
   {
      serializer.collect_str(&self.0)
   }
}

impl<'de> Deserialize<'de> for DecimalU64
{
   fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: Deserializer<'de>
   {
      let value = String::deserialize(deserializer)?;
      value.parse().map(Self).map_err(serde::de::Error::custom)
   }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRole
{
   RequiredClaim,
   DescriptiveDiagnostic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionClaimKind
{
   OxideSuperiority,
   ReferenceSuperiority,
   EquivalenceLower,
   EquivalenceUpper,
   RequiredGuardrailNoninferiority,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAlternative
{
   Lower,
   Upper,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection
{
   LowerIsBetter,
   HigherIsBetter,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOrder
{
   Ab,
   Ba,
}

pub const BALANCED_PAIR_ORDER_ALGORITHM: &str = "sha256-prefix-be64-xorshift64-abba-baab-v1";

pub fn comparison_seed_from_content_sha256(content_sha256: &str) -> Result<DecimalU64>
{
   ensure!(content_sha256.len() == 64 && content_sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "comparison content SHA-256 must be 64 lowercase hexadecimal characters");
   let seed = u64::from_str_radix(&content_sha256[..16], 16).context("decoding comparison content SHA-256 seed prefix")?;
   Ok(DecimalU64(seed))
}

pub fn balanced_comparison_order(seed: u64, pair_count: usize) -> Vec<ComparisonOrder>
{
   let mut state = seed.max(1);
   let mut orders = Vec::with_capacity(pair_count);
   while orders.len() < pair_count
   {
      state = xorshift64(state);
      let block = if state & 1 == 0
      {
         [ComparisonOrder::Ab, ComparisonOrder::Ba, ComparisonOrder::Ba, ComparisonOrder::Ab]
      }
      else
      {
         [ComparisonOrder::Ba, ComparisonOrder::Ab, ComparisonOrder::Ab, ComparisonOrder::Ba]
      };
      let remaining = pair_count - orders.len();
      orders.extend_from_slice(&block[..remaining.min(block.len())]);
   }
   orders
}

fn xorshift64(mut value: u64) -> u64
{
   value ^= value << 13;
   value ^= value >> 7;
   value ^= value << 17;
   value
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImplementationIdentity
{
   pub id: String,
   pub variant: String,
   pub source_commit: String,
   pub source_tree: String,
   pub build_command_hash: String,
   pub build_flags: Vec<String>,
   pub executable_or_bundle_sha256: String,
   pub shipping_payload_manifest_sha256: String,
   pub reference_audit_sha256: String,
   pub comparator_acceptance_status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommonIdentity
{
   pub harness_sha256: String,
   pub pass_instrumentation_sha256: String,
   pub scenario_manifest_sha256: String,
   pub trace_sha256: String,
   pub fixture_sha256: String,
   pub asset_manifest_sha256: String,
   pub font_pack_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ComparisonPlan
{
   pub schema_version: u32,
   pub suite_id: String,
   pub plan_id: String,
   pub plan_sha256: String,
   pub tier: Tier,
   pub platform: Platform,
   pub reference: ImplementationIdentity,
   pub contender: ImplementationIdentity,
   pub common: CommonIdentity,
   pub environment: serde_json::Value,
   pub seed: DecimalU64,
   pub scenario_ids: Vec<String>,
   pub scenario_packs: Vec<ScenarioPack>,
   pub controller_chunks: Vec<ControllerChunk>,
   pub measurement_pass_id: String,
   pub instrumentation_profile: String,
   pub pass_pair_count: DecimalU64,
   pub process_boundary_plan: String,
   pub comparison_cells: Vec<ComparisonCell>,
   pub metric_definitions: Vec<MetricDefinition>,
   pub decision_families: Vec<DecisionFamily>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ComparisonCell
{
   pub id: String,
   pub platform: Platform,
   pub reference_id: String,
   pub contender_id: String,
   pub scenario_id: String,
   pub cache_class: String,
   pub network_profile: String,
   pub refresh_track: String,
   pub pack_id: String,
   pub primary_metric_id: String,
   pub owning_pass_id: String,
   pub evidence_role: EvidenceRole,
   pub within_session_estimator: String,
   pub materiality_boundary: String,
   pub sufficiency_rule: String,
   pub required_guardrail_metric_ids: Vec<String>,
   pub guardrail_not_applicable_reasons: Vec<String>,
   pub oxide_superiority_family_id: Option<String>,
   pub reference_superiority_family_id: Option<String>,
   pub equivalence_lower_family_id: Option<String>,
   pub equivalence_upper_family_id: Option<String>,
   pub required_guardrail_family_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DecisionFamily
{
   pub id: String,
   pub claim_kind: DecisionClaimKind,
   pub alpha: f64,
   pub ordered_members: Vec<DecisionFamilyMember>,
   pub exact_test_resolution_floor: DecimalU64,
   pub maximum_pair_count: DecimalU64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionFamilyMember
{
   pub comparison_cell_id: String,
   pub metric_id: String,
   pub boundary_id: String,
   pub alternative: DecisionAlternative,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioPack
{
   pub id: String,
   pub ordered_scenario_ids: Vec<String>,
   pub isolation_class: String,
   pub reset_contract: String,
   pub common_ready_predicate: String,
   pub fixed_warmup_ns: DecimalU64,
   pub measured_duration_ns: DecimalU64,
   pub max_process_wall_ns: DecimalU64,
   pub sentinel_scenario_id: String,
   pub trace_capacity_limit: DecimalU64,
   pub calibration_evidence_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ControllerChunk
{
   pub id: String,
   pub ordered_pair_indices: Vec<DecimalU64>,
   pub pack_ids: Vec<String>,
   pub pass_id: String,
   pub max_occupied_ns: DecimalU64,
   pub expected_heartbeat_count: DecimalU64,
   pub bundled_plan_resource_sha256: String,
   pub checkpoint_generation: DecimalU64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ComparisonSession
{
   pub measurement_pass_id: String,
   pub pair_index: DecimalU64,
   pub order: ComparisonOrder,
   pub implementation_id: String,
   pub process_id: DecimalU64,
   pub monotonic_start_ns: DecimalU64,
   pub end_ns: DecimalU64,
   pub environment_before: serde_json::Value,
   pub environment_after: serde_json::Value,
   pub warmup_samples: Vec<RawObservationRow>,
   pub raw_sample_artifact: ArtifactIdentity,
   pub pass_artifact_hash: String,
   pub validation: String,
   pub invalid_reason: Option<String>,
   pub terminal_hard_outcome: Option<String>,
   pub durable_checkpoint_generation: DecimalU64,
   pub atomic_commit_sha256: String,
   pub artifact_hashes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetricDefinition
{
   pub id: String,
   pub unit: String,
   pub direction: MetricDirection,
   pub scope: String,
   pub comparability: String,
   pub source: String,
   pub owning_pass_id: String,
   pub allowed_primary_cell_types: Vec<String>,
   pub sample_unit: String,
   pub within_session_estimator: String,
   pub block_duration: String,
   pub pair_effect: String,
   pub across_session_estimator: String,
   pub zero_policy: String,
   pub availability_policy: String,
   pub materiality_boundary: String,
   pub guardrail_boundary: Option<String>,
   pub max_interval_width: Option<String>,
   pub decision_alpha: f64,
   pub decision_test: String,
   pub exact_test_resolution_floor: DecimalU64,
   pub max_clock_uncertainty_ns: DecimalU64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RawObservationTimestamp
{
   pub clock_id: String,
   pub timestamp_ns: DecimalU64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum RawObservationValue
{
   FiniteF64(f64),
   DecimalU64(DecimalU64),
   SignedI64(i64),
   Boolean(bool),
   Text(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RawObservationRow
{
   pub session_id: String,
   pub measurement_pass_id: String,
   pub scenario_id: String,
   pub phase_id: String,
   pub sample_index: DecimalU64,
   pub timestamps: Vec<RawObservationTimestamp>,
   pub metric_id: String,
   pub value: RawObservationValue,
   pub event_id: Option<String>,
   pub state_id: Option<String>,
   pub quality_flags: Vec<String>,
}
