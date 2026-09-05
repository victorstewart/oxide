use oxide_apple_comparison_controller::{canonical_macos_comparator_qualification_plan_json, reduce_macos_comparator_qualification, validate_macos_comparator_qualification_plan, MacOsComparatorFindingDisposition, MacOsComparatorFindingKind, MacOsComparatorProfileEvidence, MacOsComparatorQualificationPlan, MacOsComparatorRuntimeAttestation, MacOsComparatorScale, MacOsComparatorScaleDimension, MacOsComparatorScaleOverlay, MacOsComparatorScaleTransform, MacOsComparatorScaleVariant, MacOsComparatorSide, MacOsComparatorStall, MacOsTimeProfilerArtifact, MacOsTimeProfilerPhase, MacOsTimeProfilerStack};
use oxide_benchmark_spec::{ArtifactIdentity, AuditDisposition, AuditDispositionStatus};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const REFRESH_INTERVAL_NS: u64 = 16_666_667;
const PROFILE_WINDOW_SECONDS: u32 = 20;
const SCENARIOS: [(&str, MacOsComparatorScaleDimension, u64); 13] = [
   ("startup.first-screen", MacOsComparatorScaleDimension::DatasetCardinality, 24),
   ("dashboard.mixed-static", MacOsComparatorScaleDimension::OperationCardinality, 20),
   ("feed.variable-scroll", MacOsComparatorScaleDimension::DatasetCardinality, 2_000),
   ("grid.large-scroll", MacOsComparatorScaleDimension::DatasetCardinality, 10_000),
   ("chat.live-update", MacOsComparatorScaleDimension::DatasetCardinality, 5_000),
   ("navigation.modal", MacOsComparatorScaleDimension::OperationCardinality, 4),
   ("image.decode-zoom", MacOsComparatorScaleDimension::OperationCardinality, 1),
   ("effects.layers", MacOsComparatorScaleDimension::OperationCardinality, 3),
   ("mutation.damage", MacOsComparatorScaleDimension::DatasetCardinality, 10_000),
   ("text.multilingual", MacOsComparatorScaleDimension::DatasetCardinality, 1_000),
   ("resize.theme", MacOsComparatorScaleDimension::OperationCardinality, 10),
   ("idle.steady", MacOsComparatorScaleDimension::OperationCardinality, 1),
   ("endurance.churn", MacOsComparatorScaleDimension::OperationCardinality, 1_300),
];

#[test]
fn complete_frozen_matrix_reduces_deterministically_and_accepts_disposed_thresholds()
{
   let fixture = QualificationFixture::new();
   let first = reduce_macos_comparator_qualification(fixture.root.path(), &fixture.plan, &fixture.retained, &fixture.evidence, &fixture.dispositions).expect("reduce complete qualification matrix");
   let second = reduce_macos_comparator_qualification(fixture.root.path(), &fixture.plan, &fixture.retained, &fixture.evidence, &fixture.dispositions).expect("reduce complete qualification matrix again");
   assert_eq!(first, second);
   assert!(first.accepted);
   assert_eq!(first.profiles.len(), SCENARIOS.len() * 4);
   assert!(first.profiles.iter().all(|profile| profile.accepted && profile.top_cpu_stacks.len() == 1 && profile.top_cpu_stacks[0].scenario_cpu_basis_points == 500 && profile.stalls.len() == 1));
   assert_eq!(canonical_macos_comparator_qualification_plan_json(&fixture.plan).expect("canonical plan"), canonical_macos_comparator_qualification_plan_json(&fixture.plan).expect("canonical plan again"));
}

#[test]
fn missing_profile_or_variant_and_changed_artifact_fail_closed()
{
   let mut fixture = QualificationFixture::new();
   fixture.evidence.pop();
   assert!(reduce_macos_comparator_qualification(fixture.root.path(), &fixture.plan, &fixture.retained, &fixture.evidence, &fixture.dispositions).expect_err("missing profile must fail").to_string().contains("missing macOS comparator profile evidence"));

   let mut fixture = QualificationFixture::new();
   fixture.plan.variants.pop();
   assert!(validate_macos_comparator_qualification_plan(fixture.root.path(), &fixture.plan, &fixture.retained).expect_err("missing variant must fail").to_string().contains("missing 2x"));

   let fixture = QualificationFixture::new();
   fs::write(fixture.root.path().join(&fixture.evidence[0].profile.path), b"changed").expect("change profile artifact");
   assert!(reduce_macos_comparator_qualification(fixture.root.path(), &fixture.plan, &fixture.retained, &fixture.evidence, &fixture.dispositions).expect_err("changed profile must fail").to_string().contains("SHA-256 mismatch"));
}

#[test]
fn mismatched_side_overlay_and_second_application_run_fail_closed()
{
   let mut fixture = QualificationFixture::new();
   rewrite_attestation(&mut fixture, |attestation| attestation.scale_overlay_sha256 = String::from("0").repeat(64));
   assert!(reduce_macos_comparator_qualification(fixture.root.path(), &fixture.plan, &fixture.retained, &fixture.evidence, &fixture.dispositions).expect_err("mismatched overlay must fail").to_string().contains("did not attest the requested frozen scale work"));

   let mut fixture = QualificationFixture::new();
   rewrite_attestation(&mut fixture, |attestation| attestation.application_run_count = 2);
   assert!(reduce_macos_comparator_qualification(fixture.root.path(), &fixture.plan, &fixture.retained, &fixture.evidence, &fixture.dispositions).expect_err("second application run must fail").to_string().contains("did not attest the requested frozen scale work"));
}

#[test]
fn undisposed_five_percent_stack_and_one_refresh_stall_are_surfaced_and_rejected()
{
   let mut fixture = QualificationFixture::new();
   fixture.dispositions.retain(|item| !(item.scenario_id == SCENARIOS[0].0 && item.side == MacOsComparatorSide::AppKit && item.scale == MacOsComparatorScale::OneX));
   let report = reduce_macos_comparator_qualification(fixture.root.path(), &fixture.plan, &fixture.retained, &fixture.evidence, &fixture.dispositions).expect("reduce pending findings");
   assert!(!report.accepted);
   let profile = report.profiles.iter().find(|profile| profile.scenario_id == SCENARIOS[0].0 && profile.side == MacOsComparatorSide::AppKit && profile.scale == MacOsComparatorScale::OneX).expect("profile with pending findings");
   assert_eq!(profile.top_cpu_stacks[0].scenario_cpu_basis_points, 500);
   assert!(profile.top_cpu_stacks[0].disposition.is_none());
   assert_eq!(profile.stalls[0].duration_ns, REFRESH_INTERVAL_NS);
   assert!(profile.stalls[0].disposition.is_none());
}

#[test]
fn visual_artifact_and_release_gates_remain_mandatory()
{
   for gate in 0..3
   {
      let mut fixture = QualificationFixture::new();
      match gate
      {
         0 => fixture.evidence[0].parity_gate_accepted = false,
         1 => fixture.evidence[0].artifact_gate_accepted = false,
         2 => fixture.evidence[0].release_build_gate_accepted = false,
         _ => unreachable!(),
      }
      let report = reduce_macos_comparator_qualification(fixture.root.path(), &fixture.plan, &fixture.retained, &fixture.evidence, &fixture.dispositions).expect("reduce rejected gate");
      assert!(!report.accepted);
      assert!(!report.profiles[0].gates_accepted);
   }
}

struct QualificationFixture
{
   root: TempDir,
   retained: Vec<String>,
   plan: MacOsComparatorQualificationPlan,
   evidence: Vec<MacOsComparatorProfileEvidence>,
   dispositions: Vec<MacOsComparatorFindingDisposition>,
}

impl QualificationFixture
{
   fn new() -> Self
   {
      let root = tempfile::tempdir().expect("create qualification fixture root");
      fs::create_dir_all(root.path().join("fixtures")).expect("create fixture directory");
      fs::create_dir_all(root.path().join("overlays")).expect("create overlay directory");
      fs::create_dir_all(root.path().join("profiles")).expect("create profile directory");
      let retained = SCENARIOS.iter().map(|(id, _, _)| String::from(*id)).collect::<Vec<_>>();
      let mut variants = Vec::new();
      let mut overlay_hashes = Vec::new();
      for (scenario_id, dimension, base_cardinality) in SCENARIOS
      {
         let fixture_path = format!("fixtures/{}.json", scenario_id);
         let fixture = write_artifact(root.path(), &fixture_path, format!("{{\"id\":\"{}\"}}\n", scenario_id).as_bytes());
         for scale in [MacOsComparatorScale::OneX, MacOsComparatorScale::TwoX]
         {
            let effective_cardinality = if scale == MacOsComparatorScale::OneX {base_cardinality} else {base_cardinality * 2};
            let overlay = MacOsComparatorScaleOverlay {
               schema_version: 1,
               scenario_id: String::from(scenario_id),
               scale,
               dimension,
               transform: if dimension == MacOsComparatorScaleDimension::DatasetCardinality {MacOsComparatorScaleTransform::NamespacedDatasetShards} else {MacOsComparatorScaleTransform::IsolatedOperationShadow},
               base_cardinality,
               effective_cardinality,
               fixture: fixture.clone(),
            };
            let mut bytes = serde_json::to_vec_pretty(&overlay).expect("serialize overlay");
            bytes.push(b'\n');
            let overlay_path = format!("overlays/{}-{:?}.json", scenario_id, scale);
            let artifact = write_artifact(root.path(), &overlay_path, &bytes);
            overlay_hashes.push((String::from(scenario_id), scale, artifact.sha256.clone()));
            variants.push(MacOsComparatorScaleVariant {
               scenario_id: String::from(scenario_id),
               scale,
               overlay: artifact,
            });
         }
      }
      let plan = MacOsComparatorQualificationPlan {
         schema_version: 1,
         profile_window_seconds: PROFILE_WINDOW_SECONDS,
         refresh_interval_ns: REFRESH_INTERVAL_NS,
         variants,
      };
      let mut evidence = Vec::new();
      let mut dispositions = Vec::new();
      for (scenario_id, scale, overlay_sha256) in overlay_hashes
      {
         for side in [MacOsComparatorSide::AppKit, MacOsComparatorSide::Oxide]
         {
            let profile = profile_artifact();
            let profile_path = format!("profiles/{}-{:?}-{:?}.json", scenario_id, side, scale);
            let profile_artifact = write_artifact(root.path(), &profile_path, &serde_json::to_vec_pretty(&profile).expect("serialize profile"));
            let effective_cardinality = SCENARIOS.iter().find(|(id, _, _)| *id == scenario_id).map(|(_, _, cardinality)| if scale == MacOsComparatorScale::OneX {*cardinality} else {*cardinality * 2}).expect("scenario cardinality");
            let attestation = MacOsComparatorRuntimeAttestation {
               schema_version: 1,
               scenario_id: scenario_id.clone(),
               side,
               scale,
               scale_overlay_sha256: overlay_sha256.clone(),
               application_run_count: 1,
               effective_cardinality,
               completed: true,
            };
            let mut attestation_bytes = serde_json::to_vec_pretty(&attestation).expect("serialize runtime attestation");
            attestation_bytes.push(b'\n');
            let attestation_path = format!("profiles/{}-{:?}-{:?}.attestation.json", scenario_id, side, scale);
            evidence.push(MacOsComparatorProfileEvidence {
               scenario_id: scenario_id.clone(),
               side,
               scale,
               bounded_window_seconds: PROFILE_WINDOW_SECONDS,
               runtime_attestation: write_artifact(root.path(), &attestation_path, &attestation_bytes),
               profile: profile_artifact,
               stalls: vec![
                  MacOsComparatorStall {identity: String::from("short"), duration_ns: REFRESH_INTERVAL_NS - 1},
                  MacOsComparatorStall {identity: String::from("one-refresh"), duration_ns: REFRESH_INTERVAL_NS},
               ],
               parity_gate_accepted: true,
               artifact_gate_accepted: true,
               release_build_gate_accepted: true,
            });
            for (kind, identity) in [(MacOsComparatorFindingKind::CpuStack, "hot"), (MacOsComparatorFindingKind::Stall, "one-refresh")]
            {
               dispositions.push(MacOsComparatorFindingDisposition {
                  scenario_id: scenario_id.clone(),
                  side,
                  scale,
                  kind,
                  identity: String::from(identity),
                  disposition: AuditDisposition {
                     status: AuditDispositionStatus::Pass,
                     rationale: String::from("bounded profile finding reviewed and accepted"),
                     evidence: Vec::new(),
                  },
               });
            }
         }
      }
      Self {root, retained, plan, evidence, dispositions}
   }
}

fn profile_artifact() -> MacOsTimeProfilerArtifact
{
   MacOsTimeProfilerArtifact {
      schema_version: 1,
      pid: 42,
      process: String::from("Comparator (42)"),
      measured_sample_count: 20,
      measured_weight: 2_000,
      measured_main_thread_sample_count: 20,
      measured_main_thread_weight: 2_000,
      phases: vec![MacOsTimeProfilerPhase {
         scenario_index: 0,
         scenario_identifier: 1,
         phase_identifier: 2,
         start_ns: 1,
         end_ns: 2,
         sample_count: 20,
         weight: 2_000,
         main_thread_sample_count: 20,
         main_thread_weight: 2_000,
         top_stacks: std::iter::once(MacOsTimeProfilerStack {stack: String::from("hot"), sample_count: 1, weight: 100})
            .chain((0..19).map(|index| MacOsTimeProfilerStack {stack: format!("cold-{index:02}"), sample_count: 1, weight: 95}))
            .collect(),
      }],
   }
}

fn write_artifact(root: &Path, relative_path: &str, bytes: &[u8]) -> ArtifactIdentity
{
   let path = root.join(relative_path);
   fs::write(path, bytes).expect("write artifact");
   ArtifactIdentity {
      path: String::from(relative_path),
      sha256: format!("{:x}", Sha256::digest(bytes)),
   }
}

fn rewrite_attestation(fixture: &mut QualificationFixture, change: impl FnOnce(&mut MacOsComparatorRuntimeAttestation))
{
   let item = &fixture.evidence[0];
   let path = fixture.root.path().join(&item.runtime_attestation.path);
   let mut attestation: MacOsComparatorRuntimeAttestation = serde_json::from_slice(&fs::read(&path).expect("read attestation")).expect("decode attestation");
   change(&mut attestation);
   let mut bytes = serde_json::to_vec_pretty(&attestation).expect("serialize changed attestation");
   bytes.push(b'\n');
   fs::write(&path, &bytes).expect("write changed attestation");
   fixture.evidence[0].runtime_attestation.sha256 = format!("{:x}", Sha256::digest(&bytes));
}
