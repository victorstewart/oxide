use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use oxide_benchmark_spec::{
   admit_comparator_acceptance, canonical_comparator_acceptance_json,
   comparator_acceptance_signed_payload_sha256, load_appkit_macos_native_production_audit,
   ArtifactIdentity, AuditDisposition,
   AuditDispositionStatus, ComparatorAcceptanceAudit, ComparatorAcceptanceStatus,
   ComparatorAdmissionExpectation, ComparatorGateChecklist, ComparatorIdentity,
   ComparatorReviewer, ComparatorReviewerSignoff, ComparatorSourceSnapshot,
   ComparatorVariantSelection, CpuStackAudit, ForbiddenWorkChecklist, NativeCeilingAudit,
   NativeCeilingGap, ProductionArchitectureChecklist, ScalingCheck, ScalingChecklist,
   ScenarioBoundedProfile, StallAudit,
};
use sha2::{Digest, Sha256};

#[test]
fn fully_signed_accepted_audit_is_admitted()
{
   let fixture = AcceptedAuditFixture::new();
   admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect("admit accepted comparator audit");
   fixture.remove();
}

#[test]
fn missing_or_changed_audit_fails_admission()
{
   let fixture = AcceptedAuditFixture::new();
   let missing = ArtifactIdentity {
      path: String::from("audits/missing.json"),
      sha256: fixture.audit_artifact.sha256.clone(),
   };
   assert!(admit_comparator_acceptance(&fixture.root, &missing, &fixture.expected).is_err());

   let path = fixture.root.join(&fixture.audit_artifact.path);
   let mut bytes = fs::read(&path).expect("read audit");
   bytes.push(b' ');
   fs::write(path, bytes).expect("change audit without updating identity");
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err("changed audit must fail");
   assert!(error.to_string().contains("SHA-256 mismatch"));
   fixture.remove();
}

#[test]
fn pending_audit_fails_admission()
{
   let mut fixture = AcceptedAuditFixture::new();
   fixture.audit.status = ComparatorAcceptanceStatus::Pending;
   fixture.rematerialize();
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err("pending audit must fail");
   assert!(error.to_string().contains("status is not accepted"));
   fixture.remove();
}

#[test]
fn accepted_audit_without_reviewer_signoff_fails_admission()
{
   let mut fixture = AcceptedAuditFixture::new();
   fixture.audit.reviewer_signoff = None;
   fixture.rematerialize();
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err("unsigned accepted audit must fail");
   assert!(error.to_string().contains("no reviewer signoff"));
   fixture.remove();
}

#[test]
fn missing_retained_scenario_profile_fails_admission()
{
   let mut fixture = AcceptedAuditFixture::new();
   fixture.audit.scenario_profiles.clear();
   fixture.rematerialize_and_resign();
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err("missing scenario profile must fail");
   assert!(error.to_string().contains("scenario coverage differs"));
   fixture.remove();
}

#[test]
fn five_percent_cpu_stack_without_disposition_fails_admission()
{
   let mut fixture = AcceptedAuditFixture::new();
   fixture.audit.scenario_profiles[0].top_cpu_stacks.push(CpuStackAudit {
      stack_identity: String::from("harness.force_redraw"),
      scenario_cpu_basis_points: 500,
      disposition: None,
   });
   fixture.rematerialize_and_resign();
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err(">=5% CPU stack without disposition must fail");
   assert!(error.to_string().contains(">=5% CPU stack"));
   fixture.remove();
}

#[test]
fn one_refresh_stall_without_disposition_fails_admission()
{
   let mut fixture = AcceptedAuditFixture::new();
   let refresh_interval_ns = fixture.audit.scenario_profiles[0].refresh_interval_ns;
   fixture.audit.scenario_profiles[0].stalls.push(StallAudit {
      stall_identity: String::from("main-thread-layout-stall"),
      duration_ns: refresh_interval_ns,
      disposition: None,
   });
   fixture.rematerialize_and_resign();
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err(">=1-refresh stall without disposition must fail");
   assert!(error.to_string().contains(">=1-refresh stall"));
   fixture.remove();
}

#[test]
fn retrospectively_selected_variant_fails_admission()
{
   let mut fixture = AcceptedAuditFixture::new();
   fixture.audit.selection.selected_before_oxide_results = false;
   fixture.rematerialize_and_resign();
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err("retrospective selection must fail");
   assert!(error.to_string().contains("selected retrospectively"));
   fixture.remove();
}

#[test]
fn unexplained_greater_than_ten_percent_ceiling_gap_fails_admission()
{
   let mut fixture = AcceptedAuditFixture::new();
   fixture.audit.native_ceiling.as_mut().expect("ceiling audit").primary_cell_gaps[0].native_production_slower_basis_points = 1_001;
   fixture.rematerialize_and_resign();
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err("unexplained >10% ceiling gap must fail");
   assert!(error.to_string().contains(">10% ceiling gap"));
   fixture.remove();
}

#[test]
fn changed_referenced_evidence_fails_admission()
{
   let fixture = AcceptedAuditFixture::new();
   fs::write(fixture.root.join("evidence/source.txt"), b"changed").expect("change referenced evidence");
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err("changed evidence must fail");
   assert!(error.to_string().contains("SHA-256 mismatch"));
   fixture.remove();
}

#[test]
fn incomplete_source_snapshot_fails_admission()
{
   let mut fixture = AcceptedAuditFixture::new();
   fixture.expected.required_source_paths.push(String::from("evidence/compiled-but-unlisted.swift"));
   let error = admit_comparator_acceptance(&fixture.root, &fixture.audit_artifact, &fixture.expected).expect_err("omitted compiled source must fail");
   assert!(error.to_string().contains("omits required source paths"));
   fixture.remove();
}

#[test]
fn committed_appkit_audit_is_canonical_truthful_and_pending_independent_qualification()
{
   let workspace = workspace_root();
   let (path, audit) = load_appkit_macos_native_production_audit(&workspace).expect("load committed AppKit comparator audit");
   assert_eq!(canonical_comparator_acceptance_json(&audit).expect("canonical AppKit audit"), fs::read(path).expect("read committed AppKit audit"));
   assert_eq!(audit.status, ComparatorAcceptanceStatus::Pending);
   assert!(audit.reviewer.is_none() && audit.reviewer_signoff.is_none());
   assert_eq!(audit.production_architecture.virtualization_and_reuse.status, AuditDispositionStatus::Pending);
   assert_eq!(audit.production_architecture.layout.status, AuditDispositionStatus::Pending);
   assert_eq!(audit.production_architecture.input.status, AuditDispositionStatus::Pending);
   assert_eq!(audit.production_architecture.accessibility.status, AuditDispositionStatus::Pending);
   assert!(audit.rejection_reasons.iter().any(|reason| reason.contains("independent reviewer")));
   for artifact in audit.source.source_tree.iter().chain(&audit.source.dependencies).chain(std::iter::once(&audit.source.build_recipe))
   {
      assert_eq!(format!("{:x}", Sha256::digest(fs::read(workspace.join(&artifact.path)).expect("read frozen AppKit source artifact"))), artifact.sha256);
   }
}

struct AcceptedAuditFixture
{
   root: PathBuf,
   audit: ComparatorAcceptanceAudit,
   audit_artifact: ArtifactIdentity,
   expected: ComparatorAdmissionExpectation,
}

impl AcceptedAuditFixture
{
   fn new() -> Self
   {
      let root = temporary_root();
      fs::create_dir_all(root.join("evidence")).expect("create evidence directory");
      fs::create_dir_all(root.join("audits")).expect("create audit directory");
      let evidence = write_artifact(&root, "evidence/source.txt", b"frozen evidence");
      let signature = write_artifact(&root, "evidence/reviewer-signature.txt", b"reviewer signature");
      let identity = ComparatorIdentity {
         platform: String::from("macos"),
         framework: String::from("appkit"),
         implementation: String::from("appkit-production"),
         variant: String::from("native.production"),
      };
      let pass = AuditDisposition {
         status: AuditDispositionStatus::Pass,
         rationale: String::from("reviewed evidence satisfies the frozen requirement"),
         evidence: vec![evidence.clone()],
      };
      let mut audit = ComparatorAcceptanceAudit {
         schema_version: 1,
         audit_id: String::from("macos-appkit-production-v1"),
         status: ComparatorAcceptanceStatus::Accepted,
         identity: identity.clone(),
         source: ComparatorSourceSnapshot {
            source_revision: String::from("frozen-revision"),
            source_tree: vec![evidence.clone()],
            dependencies: vec![evidence.clone()],
            build_recipe: evidence.clone(),
            build_flags: vec![String::from("Release"), String::from("-O")],
         },
         reviewer: Some(ComparatorReviewer {
            identity: String::from("Independent AppKit Reviewer"),
            role: String::from("Staff macOS Engineer"),
            current_framework_experience: String::from("Ships production AppKit applications in the current macOS SDK"),
            independent_of_oxide_implementation: true,
            independence_declaration: String::from("I did not implement or optimize the Oxide contender."),
         }),
         selection: ComparatorVariantSelection {
            selected_before_oxide_results: true,
            disclosure: String::from("native.production was frozen before contender acquisition"),
            evidence: Some(evidence.clone()),
         },
         production_architecture: ProductionArchitectureChecklist {
            virtualization_and_reuse: pass.clone(),
            layout: pass.clone(),
            text: pass.clone(),
            image_decode_and_cache: pass.clone(),
            animation_and_compositing: pass.clone(),
            input: pass.clone(),
            accessibility: pass.clone(),
            cleanup: pass.clone(),
         },
         forbidden_work: ForbiddenWorkChecklist {
            debug_work: pass.clone(),
            synchronous_sleeps: pass.clone(),
            benchmark_logging: pass.clone(),
            forced_layout_or_render_loops: pass.clone(),
            accidental_full_tree_rebuild: pass.clone(),
            unbounded_native_or_dom_growth: pass.clone(),
            harness_profiler_hotspot: pass.clone(),
         },
         scaling: ScalingChecklist {
            one_x: ScalingCheck {
               fixture_scale: String::from("1x"),
               complexity_summary: String::from("bounded production behavior at the canonical fixture size"),
               disposition: pass.clone(),
            },
            two_x: ScalingCheck {
               fixture_scale: String::from("2x"),
               complexity_summary: String::from("no unexplained complexity jump at twice the fixture size"),
               disposition: pass.clone(),
            },
         },
         scenario_profiles: vec![ScenarioBoundedProfile {
            scenario_id: String::from("dashboard.mixed-static"),
            bounded_window: String::from("20 seconds after common readiness"),
            profile: evidence.clone(),
            refresh_interval_ns: 16_666_667,
            top_cpu_stacks: vec![CpuStackAudit {
               stack_identity: String::from("appkit.layout"),
               scenario_cpu_basis_points: 499,
               disposition: None,
            }],
            stalls: vec![StallAudit {
               stall_identity: String::from("short-main-thread-stall"),
               duration_ns: 16_666_666,
               disposition: None,
            }],
         }],
         gates: ComparatorGateChecklist {
            parity: pass.clone(),
            artifact: pass.clone(),
            release_build: pass,
         },
         native_ceiling: Some(NativeCeilingAudit {
            track_id: String::from("native.ceiling"),
            preregistered_before_oxide_results: true,
            headline_substitution_allowed: false,
            identity: ComparatorIdentity {
               platform: String::from("macos"),
               framework: String::from("appkit"),
               implementation: String::from("appkit-ceiling"),
               variant: String::from("native.ceiling"),
            },
            preregistration_evidence: evidence,
            primary_cell_gaps: vec![NativeCeilingGap {
               primary_cell_id: String::from("macos.dashboard.present"),
               native_production_slower_basis_points: 1_000,
               disposition: None,
            }],
         }),
         reviewer_signoff: None,
         rejection_reasons: Vec::new(),
      };
      audit.reviewer_signoff = Some(ComparatorReviewerSignoff {
         signed_status: ComparatorAcceptanceStatus::Accepted,
         signed_at_utc: String::from("2026-07-19T00:00:00Z"),
         signed_payload_sha256: comparator_acceptance_signed_payload_sha256(&audit).expect("hash signed payload"),
         signature_evidence: signature,
      });
      let expected = ComparatorAdmissionExpectation {
         identity,
         retained_scenario_ids: vec![String::from("dashboard.mixed-static")],
         primary_cell_ids: vec![String::from("macos.dashboard.present")],
         required_source_paths: vec![String::from("evidence/source.txt")],
         required_dependency_paths: vec![String::from("evidence/source.txt")],
         required_build_recipe_path: Some(String::from("evidence/source.txt")),
      };
      let audit_artifact = write_audit(&root, &audit);
      Self {root, audit, audit_artifact, expected}
   }

   fn rematerialize_and_resign(&mut self)
   {
      self.audit.reviewer_signoff = None;
      let signature_evidence = identity(&self.root, "evidence/reviewer-signature.txt");
      self.audit.reviewer_signoff = Some(ComparatorReviewerSignoff {
         signed_status: ComparatorAcceptanceStatus::Accepted,
         signed_at_utc: String::from("2026-07-19T00:00:00Z"),
         signed_payload_sha256: comparator_acceptance_signed_payload_sha256(&self.audit).expect("hash changed signed payload"),
         signature_evidence,
      });
      self.rematerialize();
   }

   fn rematerialize(&mut self)
   {
      self.audit_artifact = write_audit(&self.root, &self.audit);
   }

   fn remove(self)
   {
      fs::remove_dir_all(self.root).expect("remove temporary comparator audit root");
   }
}

fn write_audit(root: &Path, audit: &ComparatorAcceptanceAudit) -> ArtifactIdentity
{
   let bytes = canonical_comparator_acceptance_json(audit).expect("canonical audit JSON");
   fs::write(root.join("audits/comparator.json"), bytes).expect("write comparator audit");
   identity(root, "audits/comparator.json")
}

fn write_artifact(root: &Path, relative: &str, bytes: &[u8]) -> ArtifactIdentity
{
   fs::write(root.join(relative), bytes).expect("write comparator evidence");
   identity(root, relative)
}

fn identity(root: &Path, relative: &str) -> ArtifactIdentity
{
   let bytes = fs::read(root.join(relative)).expect("read comparator artifact");
   ArtifactIdentity {
      path: String::from(relative),
      sha256: format!("{:x}", Sha256::digest(bytes)),
   }
}

fn temporary_root() -> PathBuf
{
   static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);
   let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock before epoch").as_nanos();
   let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
   std::env::temp_dir().join(format!("oxide-comparator-audit-{}-{}-{}", std::process::id(), nonce, sequence))
}

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("canonical workspace root")
}
