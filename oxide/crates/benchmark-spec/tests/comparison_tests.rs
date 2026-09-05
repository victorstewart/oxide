use std::collections::{BTreeMap, BTreeSet};

use oxide_benchmark_spec::{
   balanced_comparison_order, comparison_seed_from_content_sha256, ArtifactIdentity,
   CommonIdentity, ComparisonCell, ComparisonOrder, ComparisonPlan, ComparisonSession,
   ControllerChunk, DecisionAlternative, DecisionClaimKind, DecisionFamily, DecisionFamilyMember,
   DecimalU64, EvidenceRole, ImplementationIdentity, MetricDefinition, MetricDirection, Platform,
   RawObservationRow, RawObservationTimestamp, RawObservationValue, ScenarioPack, Tier,
   BALANCED_PAIR_ORDER_ALGORITHM,
};
use oxide_benchmark_spec::validate_comparison_plan;
use serde_json::{json, Value};

#[test]
fn comparison_plan_round_trip_preserves_section_8_keys_and_order()
{
   let plan = sample_plan();
   validate_comparison_plan(&plan).expect("validate comparison plan");
   let serialized = serde_json::to_string(&plan).expect("serialize comparison plan");
   let round_trip = serde_json::from_str::<ComparisonPlan>(&serialized).expect("deserialize comparison plan");
   assert_eq!(round_trip, plan);
   assert_key_order(&serialized, &[
      "schema_version", "suite_id", "plan_id", "plan_sha256", "tier", "platform",
      "reference", "contender", "common", "environment", "seed", "scenario_ids",
      "scenario_packs", "controller_chunks", "measurement_pass_id",
      "instrumentation_profile", "pass_pair_count", "process_boundary_plan",
      "comparison_cells", "metric_definitions", "decision_families",
   ]);

   let value = serde_json::to_value(&plan).expect("comparison plan value");
   assert_object_keys(&value, &[
      "schema_version", "suite_id", "plan_id", "plan_sha256", "tier", "platform",
      "reference", "contender", "common", "environment", "seed", "scenario_ids",
      "scenario_packs", "controller_chunks", "measurement_pass_id",
      "instrumentation_profile", "pass_pair_count", "process_boundary_plan",
      "comparison_cells", "metric_definitions", "decision_families",
   ]);
   assert_object_keys(&value["reference"], &[
      "id", "variant", "source_commit", "source_tree", "build_command_hash",
      "build_flags", "executable_or_bundle_sha256", "shipping_payload_manifest_sha256",
      "reference_audit_sha256", "comparator_acceptance_status",
   ]);
   assert_object_keys(&value["common"], &[
      "harness_sha256", "pass_instrumentation_sha256", "scenario_manifest_sha256",
      "trace_sha256", "fixture_sha256", "asset_manifest_sha256", "font_pack_sha256",
   ]);
   assert_object_keys(&value["scenario_packs"][0], &[
      "id", "ordered_scenario_ids", "isolation_class", "reset_contract",
      "common_ready_predicate", "fixed_warmup_ns", "measured_duration_ns",
      "max_process_wall_ns", "sentinel_scenario_id", "trace_capacity_limit",
      "calibration_evidence_sha256",
   ]);
   assert_object_keys(&value["controller_chunks"][0], &[
      "id", "ordered_pair_indices", "pack_ids", "pass_id", "max_occupied_ns",
      "expected_heartbeat_count", "bundled_plan_resource_sha256",
      "checkpoint_generation",
   ]);
   assert_object_keys(&value["comparison_cells"][0], &[
      "id", "platform", "reference_id", "contender_id", "scenario_id", "cache_class",
      "network_profile", "refresh_track", "pack_id", "primary_metric_id",
      "owning_pass_id", "evidence_role", "within_session_estimator",
      "materiality_boundary", "sufficiency_rule", "required_guardrail_metric_ids",
      "guardrail_not_applicable_reasons", "oxide_superiority_family_id",
      "reference_superiority_family_id", "equivalence_lower_family_id",
      "equivalence_upper_family_id", "required_guardrail_family_id",
   ]);
   assert_object_keys(&value["metric_definitions"][0], &[
      "id", "unit", "direction", "scope", "comparability", "source", "owning_pass_id",
      "allowed_primary_cell_types", "sample_unit", "within_session_estimator",
      "block_duration", "pair_effect", "across_session_estimator", "zero_policy",
      "availability_policy", "materiality_boundary", "guardrail_boundary",
      "max_interval_width", "decision_alpha", "decision_test",
      "exact_test_resolution_floor", "max_clock_uncertainty_ns",
   ]);
   assert_object_keys(&value["decision_families"][0], &[
      "id", "claim_kind", "alpha", "ordered_members", "exact_test_resolution_floor",
      "maximum_pair_count",
   ]);
   assert_object_keys(&value["decision_families"][0]["ordered_members"][0], &[
      "comparison_cell_id", "metric_id", "boundary_id", "alternative",
   ]);

   assert_eq!(value["seed"], "42");
   assert_eq!(value["pass_pair_count"], "12");
   assert_eq!(value["scenario_packs"][0]["fixed_warmup_ns"], "1000000000");
   assert_eq!(value["controller_chunks"][0]["ordered_pair_indices"], json!(["0", "1"]));
   assert_eq!(value["comparison_cells"][0]["evidence_role"], "required_claim");
   assert_eq!(value["metric_definitions"][0]["direction"], "lower_is_better");
   assert_eq!(value["decision_families"][0]["claim_kind"], "oxide_superiority");
   assert_eq!(value["decision_families"][0]["ordered_members"][0]["alternative"], "lower");
}

#[test]
fn comparison_plan_rejects_cell_outside_selected_pack()
{
   let mut plan = sample_plan();
   plan.scenario_ids.push(String::from("feed.variable-scroll"));
   plan.comparison_cells[0].scenario_id = String::from("feed.variable-scroll");
   let error = validate_comparison_plan(&plan).expect_err("cell scenario outside pack must fail");
   assert!(error.to_string().contains("not present in its selected pack"));
}

#[test]
fn comparison_plan_rejects_wrong_family_claim_kind()
{
   let mut plan = sample_plan();
   plan.decision_families[0].claim_kind = DecisionClaimKind::ReferenceSuperiority;
   let error = validate_comparison_plan(&plan).expect_err("wrong family claim kind must fail");
   assert!(error.to_string().contains("wrong claim kind"));
}

#[test]
fn comparison_session_and_raw_row_round_trip_with_typed_decimal_values()
{
   let session = sample_session();
   let serialized = serde_json::to_string(&session).expect("serialize comparison session");
   let round_trip = serde_json::from_str::<ComparisonSession>(&serialized).expect("deserialize comparison session");
   assert_eq!(round_trip, session);
   assert_key_order(&serialized, &[
      "measurement_pass_id", "pair_index", "order", "implementation_id", "process_id",
      "monotonic_start_ns", "end_ns", "environment_before", "environment_after",
      "warmup_samples", "raw_sample_artifact", "pass_artifact_hash", "validation",
      "invalid_reason", "terminal_hard_outcome", "durable_checkpoint_generation",
      "atomic_commit_sha256", "artifact_hashes",
   ]);

   let value = serde_json::to_value(&session).expect("comparison session value");
   assert_object_keys(&value, &[
      "measurement_pass_id", "pair_index", "order", "implementation_id", "process_id",
      "monotonic_start_ns", "end_ns", "environment_before", "environment_after",
      "warmup_samples", "raw_sample_artifact", "pass_artifact_hash", "validation",
      "invalid_reason", "terminal_hard_outcome", "durable_checkpoint_generation",
      "atomic_commit_sha256", "artifact_hashes",
   ]);
   let row = &value["warmup_samples"][0];
   assert_object_keys(row, &[
      "session_id", "measurement_pass_id", "scenario_id", "phase_id", "sample_index",
      "timestamps", "metric_id", "value", "event_id", "state_id", "quality_flags",
   ]);
   assert_object_keys(&row["timestamps"][0], &["clock_id", "timestamp_ns"]);
   assert_object_keys(&row["value"], &["kind", "value"]);
   assert_eq!(value["order"], "ab");
   assert_eq!(value["process_id"], "731");
   assert_eq!(row["sample_index"], "0");
   assert_eq!(row["timestamps"][0]["timestamp_ns"], "1000000001");
   assert_eq!(row["value"], json!({"kind": "decimal_u64", "value": "4096"}));
}

#[test]
fn decimal_u64_requires_a_string_that_fits_u64()
{
   assert_eq!(serde_json::to_string(&DecimalU64(u64::MAX)).expect("serialize u64 maximum"), format!("\"{}\"", u64::MAX));
   assert_eq!(serde_json::from_str::<DecimalU64>("\"18446744073709551615\"").expect("deserialize u64 maximum"), DecimalU64(u64::MAX));
   assert!(serde_json::from_str::<DecimalU64>("1").is_err());
   assert!(serde_json::from_str::<DecimalU64>("\"18446744073709551616\"").is_err());
   assert!(serde_json::from_str::<DecimalU64>("\"not-a-counter\"").is_err());
}

#[test]
fn content_seed_and_four_pair_blocks_are_frozen_for_analyzer_compatibility()
{
   let content_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
   let seed = comparison_seed_from_content_sha256(content_sha256).expect("content-derived seed");
   assert_eq!(seed, DecimalU64(0x0123_4567_89ab_cdef));
   assert_eq!(BALANCED_PAIR_ORDER_ALGORITHM, "sha256-prefix-be64-xorshift64-abba-baab-v1");
   assert_eq!(
      balanced_comparison_order(seed.0, 8),
      vec![
         ComparisonOrder::Ab,
         ComparisonOrder::Ba,
         ComparisonOrder::Ba,
         ComparisonOrder::Ab,
         ComparisonOrder::Ba,
         ComparisonOrder::Ab,
         ComparisonOrder::Ab,
         ComparisonOrder::Ba,
      ],
   );
   assert!(comparison_seed_from_content_sha256(&"A".repeat(64)).is_err());
   assert!(comparison_seed_from_content_sha256("abcd").is_err());
}

fn sample_plan() -> ComparisonPlan
{
   ComparisonPlan {
      schema_version: 1,
      suite_id: String::from("ui-comparison"),
      plan_id: String::from("apple-release"),
      plan_sha256: hash("plan"),
      tier: Tier::ReleaseCore,
      platform: Platform::Apple,
      reference: implementation("native.production"),
      contender: implementation("oxide.production"),
      common: CommonIdentity {
         harness_sha256: hash("harness"),
         pass_instrumentation_sha256: hash("instrumentation"),
         scenario_manifest_sha256: hash("scenario-manifest"),
         trace_sha256: hash("trace"),
         fixture_sha256: hash("fixture"),
         asset_manifest_sha256: hash("assets"),
         font_pack_sha256: hash("fonts"),
      },
      environment: json!({"manifest_id": "iphone-native"}),
      seed: DecimalU64(42),
      scenario_ids: vec![String::from("dashboard.mixed-static")],
      scenario_packs: vec![ScenarioPack {
         id: String::from("pack-main"),
         ordered_scenario_ids: vec![String::from("dashboard.mixed-static")],
         isolation_class: String::from("warm-process"),
         reset_contract: String::from("canonical-state-checksum"),
         common_ready_predicate: String::from("first-interactive"),
         fixed_warmup_ns: DecimalU64(1_000_000_000),
         measured_duration_ns: DecimalU64(5_000_000_000),
         max_process_wall_ns: DecimalU64(30_000_000_000),
         sentinel_scenario_id: String::from("dashboard.mixed-static"),
         trace_capacity_limit: DecimalU64(65_536),
         calibration_evidence_sha256: hash("calibration"),
      }],
      controller_chunks: vec![ControllerChunk {
         id: String::from("chunk-0"),
         ordered_pair_indices: vec![DecimalU64(0), DecimalU64(1)],
         pack_ids: vec![String::from("pack-main")],
         pass_id: String::from("minimal-presentation"),
         max_occupied_ns: DecimalU64(120_000_000_000),
         expected_heartbeat_count: DecimalU64(12),
         bundled_plan_resource_sha256: hash("bundled-plan"),
         checkpoint_generation: DecimalU64(7),
      }],
      measurement_pass_id: String::from("minimal-presentation"),
      instrumentation_profile: String::from("displayed-frame"),
      pass_pair_count: DecimalU64(12),
      process_boundary_plan: String::from("one-process-per-side-session"),
      comparison_cells: vec![sample_cell()],
      metric_definitions: vec![sample_metric()],
      decision_families: vec![DecisionFamily {
         id: String::from("oxide-superiority-main"),
         claim_kind: DecisionClaimKind::OxideSuperiority,
         alpha: 0.05,
         ordered_members: vec![DecisionFamilyMember {
            comparison_cell_id: String::from("apple-dashboard-warm"),
            metric_id: String::from("event-to-present.p95"),
            boundary_id: String::from("material-superiority"),
            alternative: DecisionAlternative::Lower,
         }],
         exact_test_resolution_floor: DecimalU64(5),
         maximum_pair_count: DecimalU64(12),
      }],
   }
}

fn sample_cell() -> ComparisonCell
{
   ComparisonCell {
      id: String::from("apple-dashboard-warm"),
      platform: Platform::Apple,
      reference_id: String::from("native.production"),
      contender_id: String::from("oxide.production"),
      scenario_id: String::from("dashboard.mixed-static"),
      cache_class: String::from("warm"),
      network_profile: String::from("offline"),
      refresh_track: String::from("native"),
      pack_id: String::from("pack-main"),
      primary_metric_id: String::from("event-to-present.p95"),
      owning_pass_id: String::from("minimal-presentation"),
      evidence_role: EvidenceRole::RequiredClaim,
      within_session_estimator: String::from("p95"),
      materiality_boundary: String::from("log_ratio=-0.05"),
      sufficiency_rule: String::from("frames>=300,pairs>=12"),
      required_guardrail_metric_ids: Vec::new(),
      guardrail_not_applicable_reasons: Vec::new(),
      oxide_superiority_family_id: Some(String::from("oxide-superiority-main")),
      reference_superiority_family_id: None,
      equivalence_lower_family_id: None,
      equivalence_upper_family_id: None,
      required_guardrail_family_id: None,
   }
}

fn sample_metric() -> MetricDefinition
{
   MetricDefinition {
      id: String::from("event-to-present.p95"),
      unit: String::from("ns"),
      direction: MetricDirection::LowerIsBetter,
      scope: String::from("phase"),
      comparability: String::from("direct"),
      source: String::from("displayed-frame-trace"),
      owning_pass_id: String::from("minimal-presentation"),
      allowed_primary_cell_types: vec![String::from("dynamic")],
      sample_unit: String::from("interaction"),
      within_session_estimator: String::from("p95"),
      block_duration: String::from("one-second"),
      pair_effect: String::from("badness-ratio"),
      across_session_estimator: String::from("median-direction-normalized"),
      zero_policy: String::from("strictly-positive"),
      availability_policy: String::from("required"),
      materiality_boundary: String::from("log_ratio=-0.05"),
      guardrail_boundary: Some(String::from("log_ratio=0.05")),
      max_interval_width: Some(String::from("log_ratio=0.10")),
      decision_alpha: 0.05,
      decision_test: String::from("exact-paired-sign"),
      exact_test_resolution_floor: DecimalU64(5),
      max_clock_uncertainty_ns: DecimalU64(100_000),
   }
}

fn sample_session() -> ComparisonSession
{
   let mut artifact_hashes = BTreeMap::new();
   artifact_hashes.insert(String::from("trace"), hash("trace-artifact"));
   ComparisonSession {
      measurement_pass_id: String::from("minimal-presentation"),
      pair_index: DecimalU64(0),
      order: ComparisonOrder::Ab,
      implementation_id: String::from("native.production"),
      process_id: DecimalU64(731),
      monotonic_start_ns: DecimalU64(1_000_000_000),
      end_ns: DecimalU64(6_000_000_000),
      environment_before: json!({"thermal_state": "nominal"}),
      environment_after: json!({"thermal_state": "fair"}),
      warmup_samples: vec![RawObservationRow {
         session_id: String::from("session-0-a"),
         measurement_pass_id: String::from("minimal-presentation"),
         scenario_id: String::from("dashboard.mixed-static"),
         phase_id: String::from("warmup"),
         sample_index: DecimalU64(0),
         timestamps: vec![RawObservationTimestamp {
            clock_id: String::from("monotonic"),
            timestamp_ns: DecimalU64(1_000_000_001),
         }],
         metric_id: String::from("encoded-bytes"),
         value: RawObservationValue::DecimalU64(DecimalU64(4_096)),
         event_id: None,
         state_id: Some(String::from("canonical")),
         quality_flags: vec![String::from("warmup_excluded")],
      }],
      raw_sample_artifact: ArtifactIdentity {
         path: String::from("raw/session-0-a.jsonl.zst"),
         sha256: hash("raw-samples"),
      },
      pass_artifact_hash: hash("pass-artifact"),
      validation: String::from("valid"),
      invalid_reason: None,
      terminal_hard_outcome: None,
      durable_checkpoint_generation: DecimalU64(7),
      atomic_commit_sha256: hash("atomic-commit"),
      artifact_hashes,
   }
}

fn implementation(id: &str) -> ImplementationIdentity
{
   ImplementationIdentity {
      id: String::from(id),
      variant: String::from("production"),
      source_commit: hash("commit"),
      source_tree: hash("tree"),
      build_command_hash: hash("build-command"),
      build_flags: vec![String::from("--release")],
      executable_or_bundle_sha256: hash("bundle"),
      shipping_payload_manifest_sha256: hash("shipping-payload"),
      reference_audit_sha256: hash("reference-audit"),
      comparator_acceptance_status: String::from("accepted"),
   }
}

fn hash(label: &str) -> String
{
   format!("{:064x}", label.bytes().fold(0_u64, |value, byte| value.wrapping_mul(31).wrapping_add(u64::from(byte))))
}

fn assert_object_keys(value: &Value, expected: &[&str])
{
   let actual = value.as_object().expect("JSON object").keys().map(String::as_str).collect::<BTreeSet<_>>();
   let expected = expected.iter().copied().collect::<BTreeSet<_>>();
   assert_eq!(actual, expected);
}

fn assert_key_order(serialized: &str, expected: &[&str])
{
   let mut offset = 0;
   for key in expected
   {
      let needle = format!("\"{key}\":");
      let relative = serialized[offset..].find(&needle).unwrap_or_else(|| panic!("missing ordered key {key}"));
      offset += relative + needle.len();
   }
}
