use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

use crate::scenario::ArtifactIdentity;

pub const COMPARATOR_ACCEPTANCE_SCHEMA_VERSION: u32 = 1;
pub const CPU_STACK_DISPOSITION_THRESHOLD_BASIS_POINTS: u32 = 500;
pub const NATIVE_CEILING_GAP_THRESHOLD_BASIS_POINTS: u32 = 1_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparatorAcceptanceStatus
{
   Pending,
   Accepted,
   Rejected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditDispositionStatus
{
   Pending,
   Pass,
   Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorIdentity
{
   pub platform: String,
   pub framework: String,
   pub implementation: String,
   pub variant: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorSourceSnapshot
{
   pub source_revision: String,
   pub source_tree: Vec<ArtifactIdentity>,
   pub dependencies: Vec<ArtifactIdentity>,
   pub build_recipe: ArtifactIdentity,
   pub build_flags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorReviewer
{
   pub identity: String,
   pub role: String,
   pub current_framework_experience: String,
   pub independent_of_oxide_implementation: bool,
   pub independence_declaration: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorVariantSelection
{
   pub selected_before_oxide_results: bool,
   pub disclosure: String,
   pub evidence: Option<ArtifactIdentity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuditDisposition
{
   pub status: AuditDispositionStatus,
   pub rationale: String,
   pub evidence: Vec<ArtifactIdentity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProductionArchitectureChecklist
{
   pub virtualization_and_reuse: AuditDisposition,
   pub layout: AuditDisposition,
   pub text: AuditDisposition,
   pub image_decode_and_cache: AuditDisposition,
   pub animation_and_compositing: AuditDisposition,
   pub input: AuditDisposition,
   pub accessibility: AuditDisposition,
   pub cleanup: AuditDisposition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ForbiddenWorkChecklist
{
   pub debug_work: AuditDisposition,
   pub synchronous_sleeps: AuditDisposition,
   pub benchmark_logging: AuditDisposition,
   pub forced_layout_or_render_loops: AuditDisposition,
   pub accidental_full_tree_rebuild: AuditDisposition,
   pub unbounded_native_or_dom_growth: AuditDisposition,
   pub harness_profiler_hotspot: AuditDisposition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScalingCheck
{
   pub fixture_scale: String,
   pub complexity_summary: String,
   pub disposition: AuditDisposition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScalingChecklist
{
   pub one_x: ScalingCheck,
   pub two_x: ScalingCheck,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CpuStackAudit
{
   pub stack_identity: String,
   pub scenario_cpu_basis_points: u32,
   pub disposition: Option<AuditDisposition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StallAudit
{
   pub stall_identity: String,
   pub duration_ns: u64,
   pub disposition: Option<AuditDisposition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioBoundedProfile
{
   pub scenario_id: String,
   pub bounded_window: String,
   pub profile: ArtifactIdentity,
   pub refresh_interval_ns: u64,
   pub top_cpu_stacks: Vec<CpuStackAudit>,
   pub stalls: Vec<StallAudit>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorGateChecklist
{
   pub parity: AuditDisposition,
   pub artifact: AuditDisposition,
   pub release_build: AuditDisposition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NativeCeilingGap
{
   pub primary_cell_id: String,
   pub native_production_slower_basis_points: u32,
   pub disposition: Option<AuditDisposition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NativeCeilingAudit
{
   pub track_id: String,
   pub preregistered_before_oxide_results: bool,
   pub headline_substitution_allowed: bool,
   pub identity: ComparatorIdentity,
   pub preregistration_evidence: ArtifactIdentity,
   pub primary_cell_gaps: Vec<NativeCeilingGap>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorReviewerSignoff
{
   pub signed_status: ComparatorAcceptanceStatus,
   pub signed_at_utc: String,
   pub signed_payload_sha256: String,
   pub signature_evidence: ArtifactIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ComparatorAcceptanceAudit
{
   pub schema_version: u32,
   pub audit_id: String,
   pub status: ComparatorAcceptanceStatus,
   pub identity: ComparatorIdentity,
   pub source: ComparatorSourceSnapshot,
   pub reviewer: Option<ComparatorReviewer>,
   pub selection: ComparatorVariantSelection,
   pub production_architecture: ProductionArchitectureChecklist,
   pub forbidden_work: ForbiddenWorkChecklist,
   pub scaling: ScalingChecklist,
   pub scenario_profiles: Vec<ScenarioBoundedProfile>,
   pub gates: ComparatorGateChecklist,
   pub native_ceiling: Option<NativeCeilingAudit>,
   pub reviewer_signoff: Option<ComparatorReviewerSignoff>,
   pub rejection_reasons: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparatorAdmissionExpectation
{
   pub identity: ComparatorIdentity,
   pub retained_scenario_ids: Vec<String>,
   pub primary_cell_ids: Vec<String>,
   pub required_source_paths: Vec<String>,
   pub required_dependency_paths: Vec<String>,
   pub required_build_recipe_path: Option<String>,
}

pub fn canonical_comparator_acceptance_json(audit: &ComparatorAcceptanceAudit) -> Result<Vec<u8>>
{
   let mut bytes = serde_json::to_vec_pretty(audit).context("serializing comparator acceptance audit")?;
   bytes.push(b'\n');
   Ok(bytes)
}

pub fn comparator_acceptance_signed_payload_sha256(audit: &ComparatorAcceptanceAudit) -> Result<String>
{
   let mut payload = audit.clone();
   payload.reviewer_signoff = None;
   Ok(format!("{:x}", Sha256::digest(canonical_comparator_acceptance_json(&payload)?)))
}

pub fn admit_comparator_acceptance(workspace_root: &Path, audit_artifact: &ArtifactIdentity, expected: &ComparatorAdmissionExpectation) -> Result<ComparatorAcceptanceAudit>
{
   let bytes = read_verified_artifact(workspace_root, audit_artifact, "comparator acceptance audit")?;
   let audit: ComparatorAcceptanceAudit = serde_json::from_slice(&bytes).context("parsing comparator acceptance audit")?;
   ensure!(canonical_comparator_acceptance_json(&audit)? == bytes, "comparator acceptance audit is not canonical JSON");
   validate_accepted_audit(workspace_root, &audit, expected)?;
   Ok(audit)
}

fn validate_accepted_audit(workspace_root: &Path, audit: &ComparatorAcceptanceAudit, expected: &ComparatorAdmissionExpectation) -> Result<()>
{
   ensure!(audit.schema_version == COMPARATOR_ACCEPTANCE_SCHEMA_VERSION, "comparator acceptance audit has unsupported schema version {}", audit.schema_version);
   ensure!(audit.status == ComparatorAcceptanceStatus::Accepted, "comparator acceptance audit status is not accepted");
   ensure!(!audit.audit_id.is_empty(), "comparator acceptance audit id is empty");
   ensure!(audit.identity == expected.identity, "comparator acceptance identity differs from the requested comparator");
   validate_identity(&audit.identity, "comparator")?;
   validate_source_snapshot(workspace_root, &audit.source)?;
   validate_source_snapshot_coverage(&audit.source, expected)?;

   let reviewer = audit.reviewer.as_ref().context("accepted comparator audit has no reviewer")?;
   ensure!(!reviewer.identity.is_empty() && !reviewer.role.is_empty(), "accepted comparator reviewer identity or role is empty");
   ensure!(!reviewer.current_framework_experience.is_empty(), "accepted comparator reviewer has no current framework experience disclosure");
   ensure!(reviewer.independent_of_oxide_implementation, "accepted comparator reviewer is not independent of the Oxide implementation");
   ensure!(!reviewer.independence_declaration.is_empty(), "accepted comparator reviewer has no independence declaration");

   ensure!(audit.selection.selected_before_oxide_results, "comparator variant was selected retrospectively after Oxide results were known");
   ensure!(!audit.selection.disclosure.is_empty(), "comparator selection disclosure is empty");
   validate_optional_required_artifact(workspace_root, audit.selection.evidence.as_ref(), "comparator selection evidence")?;

   validate_production_architecture(workspace_root, &audit.production_architecture)?;
   validate_forbidden_work(workspace_root, &audit.forbidden_work)?;
   ensure!(audit.scaling.one_x.fixture_scale == "1x" && audit.scaling.two_x.fixture_scale == "2x", "comparator scaling checklist must contain fixed 1x and 2x checks");
   validate_scaling_check(workspace_root, &audit.scaling.one_x, "1x scaling")?;
   validate_scaling_check(workspace_root, &audit.scaling.two_x, "2x scaling")?;
   validate_scenario_profiles(workspace_root, &audit.scenario_profiles, &expected.retained_scenario_ids)?;
   validate_required_disposition(workspace_root, &audit.gates.parity, "parity gate")?;
   validate_required_disposition(workspace_root, &audit.gates.artifact, "artifact gate")?;
   validate_required_disposition(workspace_root, &audit.gates.release_build, "release-build gate")?;
   if let Some(ceiling) = &audit.native_ceiling
   {
      validate_native_ceiling(workspace_root, ceiling, &audit.identity, &expected.primary_cell_ids)?;
   }

   let signoff = audit.reviewer_signoff.as_ref().context("accepted comparator audit has no reviewer signoff")?;
   ensure!(signoff.signed_status == ComparatorAcceptanceStatus::Accepted, "comparator reviewer did not sign an accepted disposition");
   ensure!(signoff.signed_at_utc.contains('T') && signoff.signed_at_utc.ends_with('Z'), "comparator reviewer signoff timestamp is not UTC");
   validate_sha256(&signoff.signed_payload_sha256, "signed comparator payload")?;
   ensure!(signoff.signed_payload_sha256 == comparator_acceptance_signed_payload_sha256(audit)?, "comparator reviewer signoff does not bind the current audit payload");
   ensure!(!read_verified_artifact(workspace_root, &signoff.signature_evidence, "comparator reviewer signature evidence")?.is_empty(), "comparator reviewer signature evidence is empty");
   ensure!(audit.rejection_reasons.is_empty(), "accepted comparator audit contains rejection reasons");
   Ok(())
}

fn validate_source_snapshot(workspace_root: &Path, source: &ComparatorSourceSnapshot) -> Result<()>
{
   ensure!(!source.source_revision.is_empty(), "comparator source revision is empty");
   ensure!(!source.source_tree.is_empty(), "comparator source tree is empty");
   ensure!(!source.dependencies.is_empty(), "comparator dependency snapshot is empty");
   ensure!(!source.build_flags.is_empty() && source.build_flags.iter().all(|flag| !flag.is_empty()), "comparator build flags are empty or incomplete");
   for artifact in &source.source_tree
   {
      read_verified_artifact(workspace_root, artifact, "comparator source tree entry")?;
   }
   for artifact in &source.dependencies
   {
      read_verified_artifact(workspace_root, artifact, "comparator dependency entry")?;
   }
   read_verified_artifact(workspace_root, &source.build_recipe, "comparator build recipe")?;
   Ok(())
}

fn validate_source_snapshot_coverage(source: &ComparatorSourceSnapshot, expected: &ComparatorAdmissionExpectation) -> Result<()>
{
   let source_paths = unique_nonempty_strings(&source.source_tree.iter().map(|artifact| artifact.path.clone()).collect::<Vec<_>>(), "comparator source path")?;
   let dependency_paths = unique_nonempty_strings(&source.dependencies.iter().map(|artifact| artifact.path.clone()).collect::<Vec<_>>(), "comparator dependency path")?;
   let required_source_paths = unique_nonempty_strings(&expected.required_source_paths, "required comparator source path")?;
   let required_dependency_paths = unique_nonempty_strings(&expected.required_dependency_paths, "required comparator dependency path")?;
   ensure!(required_source_paths.is_subset(&source_paths), "comparator source snapshot omits required source paths: {:?}", required_source_paths.difference(&source_paths).collect::<Vec<_>>());
   ensure!(required_dependency_paths.is_subset(&dependency_paths), "comparator source snapshot omits required dependency paths: {:?}", required_dependency_paths.difference(&dependency_paths).collect::<Vec<_>>());
   if let Some(required) = expected.required_build_recipe_path.as_ref()
   {
      ensure!(!required.is_empty() && source.build_recipe.path == *required, "comparator build recipe differs from the required path");
   }
   Ok(())
}

fn validate_production_architecture(workspace_root: &Path, checklist: &ProductionArchitectureChecklist) -> Result<()>
{
   for (disposition, label) in [
      (&checklist.virtualization_and_reuse, "virtualization/reuse architecture"),
      (&checklist.layout, "layout architecture"),
      (&checklist.text, "text architecture"),
      (&checklist.image_decode_and_cache, "image decode/cache architecture"),
      (&checklist.animation_and_compositing, "animation/compositing architecture"),
      (&checklist.input, "input architecture"),
      (&checklist.accessibility, "accessibility architecture"),
      (&checklist.cleanup, "cleanup architecture"),
   ]
   {
      validate_required_disposition(workspace_root, disposition, label)?;
   }
   Ok(())
}

fn validate_forbidden_work(workspace_root: &Path, checklist: &ForbiddenWorkChecklist) -> Result<()>
{
   for (disposition, label) in [
      (&checklist.debug_work, "debug-work absence"),
      (&checklist.synchronous_sleeps, "synchronous-sleep absence"),
      (&checklist.benchmark_logging, "benchmark-logging absence"),
      (&checklist.forced_layout_or_render_loops, "forced-loop absence"),
      (&checklist.accidental_full_tree_rebuild, "full-tree-rebuild absence"),
      (&checklist.unbounded_native_or_dom_growth, "unbounded-growth absence"),
      (&checklist.harness_profiler_hotspot, "harness-hotspot absence"),
   ]
   {
      validate_required_disposition(workspace_root, disposition, label)?;
   }
   Ok(())
}

fn validate_scaling_check(workspace_root: &Path, scaling: &ScalingCheck, label: &str) -> Result<()>
{
   ensure!(!scaling.complexity_summary.is_empty(), "{} has no complexity summary", label);
   validate_required_disposition(workspace_root, &scaling.disposition, label)
}

fn validate_scenario_profiles(workspace_root: &Path, profiles: &[ScenarioBoundedProfile], expected_ids: &[String]) -> Result<()>
{
   let expected = unique_nonempty_strings(expected_ids, "expected retained scenario")?;
   ensure!(!expected.is_empty(), "accepted comparator audit has no retained scenarios");
   let observed = unique_nonempty_strings(&profiles.iter().map(|profile| profile.scenario_id.clone()).collect::<Vec<_>>(), "profile scenario")?;
   ensure!(observed == expected, "comparator bounded-profile scenario coverage differs from the retained scenario set");
   for profile in profiles
   {
      ensure!(!profile.bounded_window.is_empty() && profile.refresh_interval_ns > 0, "scenario {} has an incomplete bounded-profile window", profile.scenario_id);
      read_verified_artifact(workspace_root, &profile.profile, &format!("scenario {} bounded profile", profile.scenario_id))?;
      for stack in &profile.top_cpu_stacks
      {
         ensure!(!stack.stack_identity.is_empty(), "scenario {} has an unnamed CPU stack", profile.scenario_id);
         if stack.scenario_cpu_basis_points >= CPU_STACK_DISPOSITION_THRESHOLD_BASIS_POINTS
         {
            validate_optional_required_disposition(workspace_root, stack.disposition.as_ref(), &format!("scenario {} >=5% CPU stack {}", profile.scenario_id, stack.stack_identity))?;
         }
      }
      for stall in &profile.stalls
      {
         ensure!(!stall.stall_identity.is_empty(), "scenario {} has an unnamed stall", profile.scenario_id);
         if stall.duration_ns >= profile.refresh_interval_ns
         {
            validate_optional_required_disposition(workspace_root, stall.disposition.as_ref(), &format!("scenario {} >=1-refresh stall {}", profile.scenario_id, stall.stall_identity))?;
         }
      }
   }
   Ok(())
}

fn validate_native_ceiling(workspace_root: &Path, ceiling: &NativeCeilingAudit, production: &ComparatorIdentity, expected_cell_ids: &[String]) -> Result<()>
{
   ensure!(ceiling.track_id == "native.ceiling", "native ceiling sanity track has a noncanonical track id");
   ensure!(ceiling.preregistered_before_oxide_results, "native ceiling sanity track was not preregistered before Oxide results");
   ensure!(!ceiling.headline_substitution_allowed, "native ceiling sanity track permits retrospective headline substitution");
   validate_identity(&ceiling.identity, "native ceiling")?;
   ensure!(production.variant == "native.production", "native ceiling sanity track requires a native.production headline comparator");
   ensure!(ceiling.identity.platform == production.platform && ceiling.identity.framework == production.framework, "native ceiling platform or framework differs from native.production");
   ensure!(ceiling.identity.variant == "native.ceiling", "native ceiling identity does not use the native.ceiling variant");
   read_verified_artifact(workspace_root, &ceiling.preregistration_evidence, "native ceiling preregistration evidence")?;
   let expected = unique_nonempty_strings(expected_cell_ids, "expected primary cell")?;
   let observed = unique_nonempty_strings(&ceiling.primary_cell_gaps.iter().map(|gap| gap.primary_cell_id.clone()).collect::<Vec<_>>(), "native ceiling primary cell")?;
   ensure!(observed == expected, "native ceiling gap coverage differs from the primary cell set");
   for gap in &ceiling.primary_cell_gaps
   {
      if gap.native_production_slower_basis_points > NATIVE_CEILING_GAP_THRESHOLD_BASIS_POINTS
      {
         validate_optional_required_disposition(workspace_root, gap.disposition.as_ref(), &format!("native.production >10% ceiling gap for {}", gap.primary_cell_id))?;
      }
      else if let Some(disposition) = &gap.disposition
      {
         validate_disposition_evidence(workspace_root, disposition, &format!("native ceiling gap for {}", gap.primary_cell_id))?;
      }
   }
   Ok(())
}

fn validate_identity(identity: &ComparatorIdentity, label: &str) -> Result<()>
{
   ensure!(!identity.platform.is_empty() && !identity.framework.is_empty() && !identity.implementation.is_empty() && !identity.variant.is_empty(), "{} identity is incomplete", label);
   Ok(())
}

fn validate_optional_required_artifact(workspace_root: &Path, artifact: Option<&ArtifactIdentity>, label: &str) -> Result<()>
{
   let artifact = artifact.with_context(|| format!("{} is missing", label))?;
   read_verified_artifact(workspace_root, artifact, label)?;
   Ok(())
}

fn validate_optional_required_disposition(workspace_root: &Path, disposition: Option<&AuditDisposition>, label: &str) -> Result<()>
{
   let disposition = disposition.with_context(|| format!("{} has no audit disposition", label))?;
   validate_required_disposition(workspace_root, disposition, label)
}

fn validate_required_disposition(workspace_root: &Path, disposition: &AuditDisposition, label: &str) -> Result<()>
{
   ensure!(disposition.status == AuditDispositionStatus::Pass, "{} disposition is not pass", label);
   validate_disposition_evidence(workspace_root, disposition, label)
}

fn validate_disposition_evidence(workspace_root: &Path, disposition: &AuditDisposition, label: &str) -> Result<()>
{
   ensure!(!disposition.rationale.is_empty(), "{} disposition has no rationale", label);
   ensure!(!disposition.evidence.is_empty(), "{} disposition has no evidence", label);
   for evidence in &disposition.evidence
   {
      read_verified_artifact(workspace_root, evidence, &format!("{} evidence", label))?;
   }
   Ok(())
}

fn read_verified_artifact(workspace_root: &Path, artifact: &ArtifactIdentity, label: &str) -> Result<Vec<u8>>
{
   validate_sha256(&artifact.sha256, label)?;
   ensure!(!artifact.path.is_empty(), "{} path is empty", label);
   ensure!(Path::new(&artifact.path).components().all(|component| matches!(component, Component::Normal(_))), "{} path must be workspace-relative without traversal: {}", label, artifact.path);
   let path = workspace_root.join(&artifact.path);
   let bytes = fs::read(&path).with_context(|| format!("reading {} {}", label, path.display()))?;
   let observed = format!("{:x}", Sha256::digest(&bytes));
   ensure!(observed == artifact.sha256, "{} SHA-256 mismatch: expected {}, observed {}", label, artifact.sha256, observed);
   Ok(bytes)
}

fn unique_nonempty_strings(values: &[String], label: &str) -> Result<BTreeSet<String>>
{
   let mut unique = BTreeSet::new();
   for value in values
   {
      ensure!(!value.is_empty() && unique.insert(value.clone()), "{} ids must be non-empty and unique: {}", label, value);
   }
   Ok(unique)
}

fn validate_sha256(sha256: &str, label: &str) -> Result<()>
{
   ensure!(sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)), "{} SHA-256 must be 64 lowercase hexadecimal characters", label);
   Ok(())
}
