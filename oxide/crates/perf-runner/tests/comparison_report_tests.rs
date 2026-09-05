use std::collections::BTreeMap;
use std::fs;

use oxide_benchmark_spec::{
   ArtifactIdentity, CommonIdentity, ComparisonCell, ComparisonOrder, ComparisonPlan,
   ComparisonSession, ControllerChunk, DecisionAlternative, DecisionClaimKind,
   DecisionFamily, DecisionFamilyMember, DecimalU64, EvidenceRole,
   ImplementationIdentity, MetricDefinition, MetricDirection, Platform,
   RawObservationRow, RawObservationTimestamp, RawObservationValue, ScenarioPack, Tier,
};
use oxide_perf_runner::comparison_report::{
   analyze_comparison_bundle, render_comparison_report_markdown, GuardrailStatus,
};
use oxide_perf_runner::comparative::ComparisonClassification;
use oxide_perf_runner::paired::{balanced_pair_order, PairOrder};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

#[test]
fn bundle_analysis_is_deterministic_and_emits_a_publishable_oxide_win()
{
   let directory = tempdir().expect("temporary comparison bundle");
   write_bundle(directory.path(), "accepted", None);
   let first = analyze_comparison_bundle(directory.path()).expect("first comparison analysis");
   let second = analyze_comparison_bundle(directory.path()).expect("second comparison analysis");
   assert_eq!(first, second);
   assert_eq!(first.cells.len(), 1);
   assert_eq!(first.cells[0].classification, ComparisonClassification::OxideFaster);
   assert_eq!(first.cells[0].valid_pairs, DecimalU64(5));
   assert!(first.cells[0].primary_metric.effect.as_ref().expect("effect").reported_badness_ratio.is_some_and(|ratio| ratio < 0.81));
   assert_eq!(first.cells[0].guardrails[0].status, GuardrailStatus::Established);
   assert!(first.superiority_ledger[0].publishable_oxide_superiority);
   assert!(first.optimization_backlog.is_empty());
   assert_eq!(first.decision_families.len(), 5);
   assert!(first.decision_families.iter().all(|family| family.complete));
   let markdown = render_comparison_report_markdown(&first);
   assert!(markdown.find("## Superiority ledger").expect("ledger heading") < markdown.find("## Ranked optimization backlog").expect("backlog heading"));
   assert!(markdown.contains("oxide-faster"));
}

#[test]
fn unaccepted_comparator_is_a_hard_outcome_without_a_synthetic_effect()
{
   let directory = tempdir().expect("temporary comparison bundle");
   write_bundle(directory.path(), "independent-audit-pending", None);
   let report = analyze_comparison_bundle(directory.path()).expect("comparison analysis");
   let cell = &report.cells[0];
   assert_eq!(cell.classification, ComparisonClassification::HardFailureNoPerformanceClaim);
   assert_eq!(cell.terminal_hard_outcome.as_deref(), Some("comparator-acceptance-independent-audit-pending"));
   assert!(cell.primary_metric.effect.is_none());
   assert!(cell.primary_metric.paired_sessions.is_empty());
   assert!(!report.superiority_ledger[0].publishable_oxide_superiority);
}

#[test]
fn raw_artifact_hash_mismatch_fails_closed()
{
   let directory = tempdir().expect("temporary comparison bundle");
   write_bundle(directory.path(), "accepted", Some("bad-hash"));
   let error = analyze_comparison_bundle(directory.path()).expect_err("raw hash mismatch must fail");
   assert!(error.to_string().contains("raw artifact hash mismatch"));
}

fn write_bundle(root: &std::path::Path, comparator_status: &str, forced_raw_hash: Option<&str>)
{
   fs::create_dir_all(root.join("raw")).expect("raw directory");
   let plan = sample_plan(comparator_status);
   fs::write(root.join("plan.json"), serde_json::to_vec_pretty(&plan).expect("plan JSON")).expect("write plan");
   let orders = balanced_pair_order(plan.seed.0, plan.pass_pair_count.0 as usize);
   let mut sessions = String::new();
   for pair_index in 0..plan.pass_pair_count.0
   {
      for implementation_id in [&plan.reference.id, &plan.contender.id]
      {
         let session_id = format!("{}-{}", pair_index, implementation_id.replace('.', "-"));
         let raw_path = format!("raw/{}.jsonl", session_id);
         let value = if implementation_id == &plan.contender.id { 8.0 } else { 10.0 };
         let row = RawObservationRow {
            session_id: session_id.clone(),
            measurement_pass_id: String::from("minimal-presentation"),
            scenario_id: String::from("dashboard.mixed-static"),
            phase_id: String::from("steady"),
            sample_index: DecimalU64(0),
            timestamps: vec![RawObservationTimestamp {
               clock_id: String::from("continuous"),
               timestamp_ns: DecimalU64(1_000_000 + pair_index * 100),
            }],
            metric_id: String::from("event-to-present.p50"),
            value: RawObservationValue::FiniteF64(value),
            event_id: Some(format!("event-{}", pair_index)),
            state_id: Some(String::from("canonical")),
            quality_flags: Vec::new(),
         };
         let raw = format!("{}\n", serde_json::to_string(&row).expect("raw row JSON"));
         fs::write(root.join(&raw_path), raw.as_bytes()).expect("write raw row");
         let order = match orders[pair_index as usize] { PairOrder::Ab => ComparisonOrder::Ab, PairOrder::Ba => ComparisonOrder::Ba };
         let is_reference = implementation_id == &plan.reference.id;
         let starts_first = matches!((order, is_reference), (ComparisonOrder::Ab, true) | (ComparisonOrder::Ba, false));
         let start = 1_000_000_000 + pair_index * 1_000_000 + if starts_first { 0 } else { 100_000 };
         let session = ComparisonSession {
            measurement_pass_id: String::from("minimal-presentation"),
            pair_index: DecimalU64(pair_index),
            order,
            implementation_id: implementation_id.clone(),
            process_id: DecimalU64(100 + pair_index * 2 + u64::from(!is_reference)),
            monotonic_start_ns: DecimalU64(start),
            end_ns: DecimalU64(start + 50_000),
            environment_before: json!({"thermal_state": "nominal"}),
            environment_after: json!({"thermal_state": "nominal"}),
            warmup_samples: Vec::new(),
            raw_sample_artifact: ArtifactIdentity {
               path: raw_path,
               sha256: forced_raw_hash.map(String::from).unwrap_or_else(|| sha256(raw.as_bytes())),
            },
            pass_artifact_hash: hash("pass-artifact"),
            validation: String::from("valid"),
            invalid_reason: None,
            terminal_hard_outcome: None,
            durable_checkpoint_generation: DecimalU64(pair_index + 1),
            atomic_commit_sha256: hash(&format!("commit-{session_id}")),
            artifact_hashes: BTreeMap::new(),
         };
         sessions.push_str(&serde_json::to_string(&session).expect("session JSON"));
         sessions.push('\n');
      }
   }
   fs::write(root.join("sessions.jsonl"), sessions).expect("write sessions");
}

fn sample_plan(comparator_status: &str) -> ComparisonPlan
{
   let metric = MetricDefinition {
      id: String::from("event-to-present.p50"),
      unit: String::from("ms"),
      direction: MetricDirection::LowerIsBetter,
      scope: String::from("phase"),
      comparability: String::from("direct"),
      source: String::from("displayed-frame-trace"),
      owning_pass_id: String::from("minimal-presentation"),
      allowed_primary_cell_types: vec![String::from("dynamic")],
      sample_unit: String::from("interaction"),
      within_session_estimator: String::from("median"),
      block_duration: String::from("1"),
      pair_effect: String::from("badness-ratio"),
      across_session_estimator: String::from("median-direction-normalized"),
      zero_policy: String::from("strictly-positive"),
      availability_policy: String::from("required"),
      materiality_boundary: String::from("log_ratio=0.05"),
      guardrail_boundary: Some(String::from("log_ratio=0.05")),
      max_interval_width: Some(String::from("log_ratio=0.10")),
      decision_alpha: 0.05,
      decision_test: String::from("exact-paired-sign"),
      exact_test_resolution_floor: DecimalU64(5),
      max_clock_uncertainty_ns: DecimalU64(100_000),
   };
   let family_specs = [
      ("oxide", DecisionClaimKind::OxideSuperiority, DecisionAlternative::Lower),
      ("reference", DecisionClaimKind::ReferenceSuperiority, DecisionAlternative::Upper),
      ("equivalence-lower", DecisionClaimKind::EquivalenceLower, DecisionAlternative::Upper),
      ("equivalence-upper", DecisionClaimKind::EquivalenceUpper, DecisionAlternative::Lower),
      ("guardrail", DecisionClaimKind::RequiredGuardrailNoninferiority, DecisionAlternative::Lower),
   ];
   let families = family_specs.into_iter().map(|(id, claim_kind, alternative)| DecisionFamily {
      id: String::from(id),
      claim_kind,
      alpha: 0.05,
      ordered_members: vec![DecisionFamilyMember {
         comparison_cell_id: String::from("mac-dashboard-warm"),
         metric_id: String::from("event-to-present.p50"),
         boundary_id: format!("{id}-boundary"),
         alternative,
      }],
      exact_test_resolution_floor: DecimalU64(5),
      maximum_pair_count: DecimalU64(5),
   }).collect();
   ComparisonPlan {
      schema_version: 1,
      suite_id: String::from("oxide-production-comparison"),
      plan_id: String::from("mac-golden"),
      plan_sha256: hash("plan"),
      tier: Tier::ClaimComplete,
      platform: Platform::Apple,
      reference: implementation("native.production", comparator_status),
      contender: implementation("oxide.production", "accepted"),
      common: CommonIdentity {
         harness_sha256: hash("harness"),
         pass_instrumentation_sha256: hash("instrumentation"),
         scenario_manifest_sha256: hash("scenarios"),
         trace_sha256: hash("trace"),
         fixture_sha256: hash("fixtures"),
         asset_manifest_sha256: hash("assets"),
         font_pack_sha256: hash("fonts"),
      },
      environment: json!({"host": "golden"}),
      seed: DecimalU64(42),
      scenario_ids: vec![String::from("dashboard.mixed-static")],
      scenario_packs: vec![ScenarioPack {
         id: String::from("main"),
         ordered_scenario_ids: vec![String::from("dashboard.mixed-static")],
         isolation_class: String::from("warm-process"),
         reset_contract: String::from("canonical-state"),
         common_ready_predicate: String::from("first-interactive"),
         fixed_warmup_ns: DecimalU64(0),
         measured_duration_ns: DecimalU64(1_000_000_000),
         max_process_wall_ns: DecimalU64(2_000_000_000),
         sentinel_scenario_id: String::from("dashboard.mixed-static"),
         trace_capacity_limit: DecimalU64(1_024),
         calibration_evidence_sha256: hash("calibration"),
      }],
      controller_chunks: vec![ControllerChunk {
         id: String::from("chunk"),
         ordered_pair_indices: (0..5).map(DecimalU64).collect(),
         pack_ids: vec![String::from("main")],
         pass_id: String::from("minimal-presentation"),
         max_occupied_ns: DecimalU64(10_000_000_000),
         expected_heartbeat_count: DecimalU64(5),
         bundled_plan_resource_sha256: hash("bundle"),
         checkpoint_generation: DecimalU64(1),
      }],
      measurement_pass_id: String::from("minimal-presentation"),
      instrumentation_profile: String::from("displayed-frame"),
      pass_pair_count: DecimalU64(5),
      process_boundary_plan: String::from("one-process-per-side-session"),
      comparison_cells: vec![ComparisonCell {
         id: String::from("mac-dashboard-warm"),
         platform: Platform::Apple,
         reference_id: String::from("native.production"),
         contender_id: String::from("oxide.production"),
         scenario_id: String::from("dashboard.mixed-static"),
         cache_class: String::from("warm"),
         network_profile: String::from("offline"),
         refresh_track: String::from("native"),
         pack_id: String::from("main"),
         primary_metric_id: String::from("event-to-present.p50"),
         owning_pass_id: String::from("minimal-presentation"),
         evidence_role: EvidenceRole::RequiredClaim,
         within_session_estimator: String::from("median"),
         materiality_boundary: String::from("log_ratio=0.05"),
         sufficiency_rule: String::from("samples>=1,pairs>=5"),
         required_guardrail_metric_ids: vec![String::from("event-to-present.p50")],
         guardrail_not_applicable_reasons: Vec::new(),
         oxide_superiority_family_id: Some(String::from("oxide")),
         reference_superiority_family_id: Some(String::from("reference")),
         equivalence_lower_family_id: Some(String::from("equivalence-lower")),
         equivalence_upper_family_id: Some(String::from("equivalence-upper")),
         required_guardrail_family_id: Some(String::from("guardrail")),
      }],
      metric_definitions: vec![metric],
      decision_families: families,
   }
}

fn implementation(id: &str, comparator_status: &str) -> ImplementationIdentity
{
   ImplementationIdentity {
      id: String::from(id),
      variant: String::from("production"),
      source_commit: hash("commit"),
      source_tree: hash("tree"),
      build_command_hash: hash("build"),
      build_flags: vec![String::from("--release")],
      executable_or_bundle_sha256: hash("binary"),
      shipping_payload_manifest_sha256: hash("payload"),
      reference_audit_sha256: hash("audit"),
      comparator_acceptance_status: String::from(comparator_status),
   }
}

fn hash(label: &str) -> String
{
   format!("{:064x}", label.bytes().fold(0_u64, |value, byte| value.wrapping_mul(31).wrapping_add(u64::from(byte))))
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}
