use anyhow::{bail, ensure, Context, Result};
use oxide_benchmark_spec::{ArtifactIdentity, AuditDisposition, AuditDispositionStatus, CPU_STACK_DISPOSITION_THRESHOLD_BASIS_POINTS};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

use super::MacOsTimeProfilerArtifact;

pub const MACOS_COMPARATOR_QUALIFICATION_SCHEMA_VERSION: u32 = 1;
pub const MACOS_COMPARATOR_PROFILE_MAX_SECONDS: u32 = 30;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsComparatorScale
{
   OneX,
   TwoX,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsComparatorSide
{
   AppKit,
   Oxide,
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
pub struct MacOsComparatorScaleVariant
{
   pub scenario_id: String,
   pub scale: MacOsComparatorScale,
   pub overlay: ArtifactIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsComparatorQualificationPlan
{
   pub schema_version: u32,
   pub profile_window_seconds: u32,
   pub refresh_interval_ns: u64,
   pub variants: Vec<MacOsComparatorScaleVariant>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsComparatorStall
{
   pub identity: String,
   pub duration_ns: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsComparatorProfileEvidence
{
   pub scenario_id: String,
   pub side: MacOsComparatorSide,
   pub scale: MacOsComparatorScale,
   pub bounded_window_seconds: u32,
   pub runtime_attestation: ArtifactIdentity,
   pub profile: ArtifactIdentity,
   pub stalls: Vec<MacOsComparatorStall>,
   pub parity_gate_accepted: bool,
   pub artifact_gate_accepted: bool,
   pub release_build_gate_accepted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MacOsComparatorFindingKind
{
   CpuStack,
   Stall,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsComparatorFindingDisposition
{
   pub scenario_id: String,
   pub side: MacOsComparatorSide,
   pub scale: MacOsComparatorScale,
   pub kind: MacOsComparatorFindingKind,
   pub identity: String,
   pub disposition: AuditDisposition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsComparatorCpuFinding
{
   pub stack_identity: String,
   pub scenario_cpu_basis_points: u32,
   pub disposition: Option<AuditDisposition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsComparatorStallFinding
{
   pub stall_identity: String,
   pub duration_ns: u64,
   pub disposition: Option<AuditDisposition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsComparatorProfileQualification
{
   pub scenario_id: String,
   pub side: MacOsComparatorSide,
   pub scale: MacOsComparatorScale,
   pub profile: ArtifactIdentity,
   pub top_cpu_stacks: Vec<MacOsComparatorCpuFinding>,
   pub stalls: Vec<MacOsComparatorStallFinding>,
   pub gates_accepted: bool,
   pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MacOsComparatorQualificationReport
{
   pub schema_version: u32,
   pub plan_sha256: String,
   pub profiles: Vec<MacOsComparatorProfileQualification>,
   pub accepted: bool,
}

pub fn canonical_macos_comparator_qualification_plan_json(plan: &MacOsComparatorQualificationPlan) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(plan).context("serializing macOS comparator qualification plan")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn validate_macos_comparator_qualification_plan(workspace_root: &Path, plan: &MacOsComparatorQualificationPlan, retained_scenario_ids: &[String]) -> Result<BTreeMap<(String, MacOsComparatorScale), MacOsComparatorScaleOverlay>>
{
   ensure!(plan.schema_version == MACOS_COMPARATOR_QUALIFICATION_SCHEMA_VERSION, "macOS comparator qualification plan has unsupported schema version {}", plan.schema_version);
   ensure!(plan.profile_window_seconds > 0 && plan.profile_window_seconds <= MACOS_COMPARATOR_PROFILE_MAX_SECONDS, "macOS comparator qualification profile window must be within 1..={} seconds", MACOS_COMPARATOR_PROFILE_MAX_SECONDS);
   ensure!(plan.refresh_interval_ns > 0, "macOS comparator qualification refresh interval is zero");
   let retained = retained_scenario_ids.iter().cloned().collect::<BTreeSet<_>>();
   ensure!(!retained.is_empty() && retained.len() == retained_scenario_ids.len() && retained.iter().all(|id| !id.is_empty()), "macOS comparator retained scenario identities are empty or duplicated");
   let mut overlays = BTreeMap::new();
   for variant in &plan.variants
   {
      ensure!(retained.contains(&variant.scenario_id), "macOS comparator qualification variant names unretained scenario {}", variant.scenario_id);
      let bytes = read_verified_artifact(workspace_root, &variant.overlay, "macOS comparator scale overlay")?;
      let overlay: MacOsComparatorScaleOverlay = serde_json::from_slice(&bytes).context("decoding macOS comparator scale overlay")?;
      ensure!(canonical_overlay_json(&overlay)? == bytes, "macOS comparator scale overlay is not canonical JSON");
      ensure!(overlay.schema_version == MACOS_COMPARATOR_QUALIFICATION_SCHEMA_VERSION, "macOS comparator scale overlay has unsupported schema version {}", overlay.schema_version);
      ensure!(overlay.scenario_id == variant.scenario_id && overlay.scale == variant.scale, "macOS comparator scale overlay identity differs from its plan binding");
      let (dimension, transform) = expected_scale_contract(&overlay.scenario_id)?;
      ensure!(overlay.dimension == dimension && overlay.transform == transform, "macOS comparator scale transform is invalid for {}", overlay.scenario_id);
      ensure!(overlay.base_cardinality > 0, "macOS comparator scale overlay has zero base cardinality");
      let expected_cardinality = match overlay.scale
      {
         MacOsComparatorScale::OneX => overlay.base_cardinality,
         MacOsComparatorScale::TwoX => overlay.base_cardinality.checked_mul(2).context("macOS comparator 2x cardinality overflow")?,
      };
      ensure!(overlay.effective_cardinality == expected_cardinality, "macOS comparator scale overlay effective cardinality is not exact {:?}", overlay.scale);
      read_verified_artifact(workspace_root, &overlay.fixture, "macOS comparator fixture")?;
      if overlays.insert((variant.scenario_id.clone(), variant.scale), overlay).is_some()
      {
         bail!("duplicate macOS comparator qualification variant for {} {:?}", variant.scenario_id, variant.scale);
      }
   }
   for scenario_id in &retained
   {
      let one_x = overlays.get(&(scenario_id.clone(), MacOsComparatorScale::OneX)).with_context(|| format!("missing 1x macOS comparator qualification variant for {}", scenario_id))?;
      let two_x = overlays.get(&(scenario_id.clone(), MacOsComparatorScale::TwoX)).with_context(|| format!("missing 2x macOS comparator qualification variant for {}", scenario_id))?;
      ensure!(one_x.fixture == two_x.fixture && one_x.dimension == two_x.dimension && one_x.transform == two_x.transform && one_x.base_cardinality == two_x.base_cardinality, "macOS comparator 1x/2x variants do not share one frozen fixture transformation for {}", scenario_id);
   }
   Ok(overlays)
}

pub fn reduce_macos_comparator_qualification(workspace_root: &Path, plan: &MacOsComparatorQualificationPlan, retained_scenario_ids: &[String], evidence: &[MacOsComparatorProfileEvidence], dispositions: &[MacOsComparatorFindingDisposition]) -> Result<MacOsComparatorQualificationReport>
{
   let overlays = validate_macos_comparator_qualification_plan(workspace_root, plan, retained_scenario_ids)?;
   let disposition_map = disposition_map(workspace_root, dispositions)?;
   let mut evidence_map = BTreeMap::new();
   for item in evidence
   {
      let key = (item.scenario_id.clone(), item.side, item.scale);
      if evidence_map.insert(key.clone(), item).is_some()
      {
         bail!("duplicate macOS comparator profile evidence for {} {:?} {:?}", key.0, key.1, key.2);
      }
   }
   let mut profiles = Vec::with_capacity(retained_scenario_ids.len() * 4);
   for scenario_id in retained_scenario_ids
   {
      for side in [MacOsComparatorSide::AppKit, MacOsComparatorSide::Oxide]
      {
         for scale in [MacOsComparatorScale::OneX, MacOsComparatorScale::TwoX]
         {
            let key = (scenario_id.clone(), side, scale);
            let item = evidence_map.remove(&key).with_context(|| format!("missing macOS comparator profile evidence for {} {:?} {:?}", scenario_id, side, scale))?;
            let overlay = overlays.get(&(scenario_id.clone(), scale)).context("validated macOS comparator overlay disappeared")?;
            let variant = plan.variants.iter().find(|variant| variant.scenario_id == *scenario_id && variant.scale == scale).context("validated macOS comparator variant disappeared")?;
            ensure!(item.bounded_window_seconds == plan.profile_window_seconds, "macOS comparator profile window differs from the frozen plan");
            let attestation_bytes = read_verified_artifact(workspace_root, &item.runtime_attestation, "macOS comparator runtime scale attestation")?;
            let attestation: MacOsComparatorRuntimeAttestation = serde_json::from_slice(&attestation_bytes).context("decoding macOS comparator runtime scale attestation")?;
            ensure!(canonical_runtime_attestation_json(&attestation)? == attestation_bytes, "macOS comparator runtime scale attestation is not canonical JSON");
            ensure!(attestation.schema_version == MACOS_COMPARATOR_QUALIFICATION_SCHEMA_VERSION
               && attestation.scenario_id == *scenario_id
               && attestation.side == side
               && attestation.scale == scale
               && attestation.scale_overlay_sha256 == variant.overlay.sha256
               && attestation.application_run_count == 1
               && attestation.effective_cardinality == overlay.effective_cardinality
               && attestation.completed,
               "macOS comparator runtime did not attest the requested frozen scale work for {} {:?} {:?}", scenario_id, side, scale);
            let bytes = read_verified_artifact(workspace_root, &item.profile, "macOS comparator Time Profiler artifact")?;
            let artifact: MacOsTimeProfilerArtifact = serde_json::from_slice(&bytes).context("decoding macOS comparator Time Profiler artifact")?;
            validate_profile_artifact(&artifact)?;
            let top_cpu_stacks = cpu_findings(scenario_id, side, scale, &artifact, &disposition_map)?;
            let stalls = stall_findings(scenario_id, side, scale, plan.refresh_interval_ns, &item.stalls, &disposition_map)?;
            let gates_accepted = item.parity_gate_accepted && item.artifact_gate_accepted && item.release_build_gate_accepted;
            let accepted = gates_accepted
               && top_cpu_stacks.iter().all(|finding| finding.disposition.as_ref().is_some_and(disposition_passes))
               && stalls.iter().all(|finding| finding.disposition.as_ref().is_some_and(disposition_passes));
            profiles.push(MacOsComparatorProfileQualification {
               scenario_id: scenario_id.clone(),
               side,
               scale,
               profile: item.profile.clone(),
               top_cpu_stacks,
               stalls,
               gates_accepted,
               accepted,
            });
         }
      }
   }
   ensure!(evidence_map.is_empty(), "macOS comparator qualification contains unexpected profile evidence");
   let plan_sha256 = format!("{:x}", Sha256::digest(canonical_macos_comparator_qualification_plan_json(plan)?));
   let accepted = profiles.iter().all(|profile| profile.accepted);
   Ok(MacOsComparatorQualificationReport {
      schema_version: MACOS_COMPARATOR_QUALIFICATION_SCHEMA_VERSION,
      plan_sha256,
      profiles,
      accepted,
   })
}

fn expected_scale_contract(scenario_id: &str) -> Result<(MacOsComparatorScaleDimension, MacOsComparatorScaleTransform)>
{
   match scenario_id
   {
      "startup.first-screen" | "feed.variable-scroll" | "grid.large-scroll" | "chat.live-update" | "mutation.damage" | "text.multilingual" => Ok((MacOsComparatorScaleDimension::DatasetCardinality, MacOsComparatorScaleTransform::NamespacedDatasetShards)),
      "dashboard.mixed-static" | "navigation.modal" | "image.decode-zoom" | "effects.layers" | "resize.theme" | "idle.steady" | "endurance.churn" => Ok((MacOsComparatorScaleDimension::OperationCardinality, MacOsComparatorScaleTransform::IsolatedOperationShadow)),
      _ => bail!("macOS comparator qualification has no frozen 2x transformation for {}", scenario_id),
   }
}

fn canonical_overlay_json(overlay: &MacOsComparatorScaleOverlay) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(overlay).context("serializing macOS comparator scale overlay")?;
   bytes.push(b'\n');
   Ok(bytes)
}

fn canonical_runtime_attestation_json(attestation: &MacOsComparatorRuntimeAttestation) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(attestation).context("serializing macOS comparator runtime scale attestation")?;
   bytes.push(b'\n');
   Ok(bytes)
}

fn validate_profile_artifact(artifact: &MacOsTimeProfilerArtifact) -> Result<()>
{
   ensure!(artifact.schema_version == 1 && artifact.pid > 0 && !artifact.process.is_empty(), "macOS comparator Time Profiler artifact identity is invalid");
   ensure!(artifact.measured_sample_count > 0 && artifact.measured_weight > 0 && !artifact.phases.is_empty(), "macOS comparator Time Profiler artifact has no bounded measured samples");
   let phase_weight = artifact.phases.iter().try_fold(0_u64, |total, phase| total.checked_add(phase.weight).context("macOS comparator Time Profiler phase-weight overflow"))?;
   let phase_samples = artifact.phases.iter().try_fold(0_u64, |total, phase| total.checked_add(phase.sample_count).context("macOS comparator Time Profiler phase-sample overflow"))?;
   ensure!(phase_weight == artifact.measured_weight && phase_samples == artifact.measured_sample_count, "macOS comparator Time Profiler artifact totals differ from its phases");
   Ok(())
}

type DispositionKey = (String, MacOsComparatorSide, MacOsComparatorScale, MacOsComparatorFindingKind, String);

fn disposition_map(workspace_root: &Path, dispositions: &[MacOsComparatorFindingDisposition]) -> Result<BTreeMap<DispositionKey, AuditDisposition>>
{
   let mut result = BTreeMap::new();
   for item in dispositions
   {
      ensure!(!item.identity.is_empty(), "macOS comparator finding disposition has an empty identity");
      if item.disposition.status != AuditDispositionStatus::Pending
      {
         ensure!(!item.disposition.rationale.is_empty(), "macOS comparator finding disposition has no rationale");
      }
      for evidence in &item.disposition.evidence
      {
         read_verified_artifact(workspace_root, evidence, "macOS comparator finding disposition evidence")?;
      }
      let key = (item.scenario_id.clone(), item.side, item.scale, item.kind, item.identity.clone());
      if result.insert(key, item.disposition.clone()).is_some()
      {
         bail!("duplicate macOS comparator finding disposition");
      }
   }
   Ok(result)
}

fn cpu_findings(scenario_id: &str, side: MacOsComparatorSide, scale: MacOsComparatorScale, artifact: &MacOsTimeProfilerArtifact, dispositions: &BTreeMap<DispositionKey, AuditDisposition>) -> Result<Vec<MacOsComparatorCpuFinding>>
{
   let mut weights = BTreeMap::<String, u64>::new();
   for phase in &artifact.phases
   {
      for stack in &phase.top_stacks
      {
         let weight = weights.entry(stack.stack.clone()).or_default();
         *weight = weight.checked_add(stack.weight).context("macOS comparator CPU stack weight overflow")?;
      }
   }
   let mut findings = Vec::new();
   for (identity, weight) in weights
   {
      let basis_points_u64 = weight.checked_mul(10_000).context("macOS comparator CPU basis-point overflow")? / artifact.measured_weight;
      let basis_points = u32::try_from(basis_points_u64).context("macOS comparator CPU basis points exceed u32")?;
      if basis_points >= CPU_STACK_DISPOSITION_THRESHOLD_BASIS_POINTS
      {
         let key = (String::from(scenario_id), side, scale, MacOsComparatorFindingKind::CpuStack, identity.clone());
         findings.push(MacOsComparatorCpuFinding {
            stack_identity: identity,
            scenario_cpu_basis_points: basis_points,
            disposition: dispositions.get(&key).cloned(),
         });
      }
   }
   findings.sort_by(|left, right| right.scenario_cpu_basis_points.cmp(&left.scenario_cpu_basis_points).then_with(|| left.stack_identity.cmp(&right.stack_identity)));
   Ok(findings)
}

fn stall_findings(scenario_id: &str, side: MacOsComparatorSide, scale: MacOsComparatorScale, refresh_interval_ns: u64, stalls: &[MacOsComparatorStall], dispositions: &BTreeMap<DispositionKey, AuditDisposition>) -> Result<Vec<MacOsComparatorStallFinding>>
{
   let mut identities = BTreeSet::new();
   let mut findings = Vec::new();
   for stall in stalls
   {
      ensure!(!stall.identity.is_empty() && identities.insert(stall.identity.clone()), "macOS comparator stalls have empty or duplicate identities");
      if stall.duration_ns >= refresh_interval_ns
      {
         let key = (String::from(scenario_id), side, scale, MacOsComparatorFindingKind::Stall, stall.identity.clone());
         findings.push(MacOsComparatorStallFinding {
            stall_identity: stall.identity.clone(),
            duration_ns: stall.duration_ns,
            disposition: dispositions.get(&key).cloned(),
         });
      }
   }
   findings.sort_by(|left, right| right.duration_ns.cmp(&left.duration_ns).then_with(|| left.stall_identity.cmp(&right.stall_identity)));
   Ok(findings)
}

fn disposition_passes(disposition: &AuditDisposition) -> bool
{
   disposition.status == AuditDispositionStatus::Pass
}

fn read_verified_artifact(workspace_root: &Path, artifact: &ArtifactIdentity, label: &str) -> Result<Vec<u8>>
{
   let relative = Path::new(&artifact.path);
   ensure!(!relative.as_os_str().is_empty() && !relative.is_absolute() && relative.components().all(|component| matches!(component, Component::Normal(_))), "{} path is not a normalized workspace-relative path", label);
   ensure!(artifact.sha256.len() == 64 && artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()), "{} SHA-256 identity is invalid", label);
   let path = workspace_root.join(relative);
   let bytes = fs::read(&path).with_context(|| format!("reading {} {}", label, path.display()))?;
   ensure!(format!("{:x}", Sha256::digest(&bytes)) == artifact.sha256, "{} SHA-256 mismatch", label);
   Ok(bytes)
}
