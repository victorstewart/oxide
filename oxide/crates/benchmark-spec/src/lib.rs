pub mod acquisition;
pub mod apple_campaign_plan;
pub mod apple_plan;
pub mod comparison;
pub mod comparator_acceptance;
pub mod correctness_geometry;
pub mod detection_coverage;
pub mod fixtures;
pub mod instrumentation_calibration;
pub mod instrumentation_statistics;
pub mod macos_attribution;
pub mod macos_scale;
pub mod migration;
pub mod pr_fixtures;
pub mod pr_scenarios;
pub mod release_candidate;
pub mod release_fixtures;
pub mod release_capture;
pub mod release_promotion;
pub mod scenario;
pub mod schema;
pub mod validate;
pub mod visual;

pub use comparison::{
   balanced_comparison_order, comparison_seed_from_content_sha256, CommonIdentity,
   ComparisonCell, ComparisonOrder, ComparisonPlan, ComparisonSession, ControllerChunk,
   DecisionAlternative, DecisionClaimKind, DecisionFamily, DecisionFamilyMember, DecimalU64,
   EvidenceRole, ImplementationIdentity, MetricDefinition, MetricDirection, RawObservationRow,
   RawObservationTimestamp, RawObservationValue, ScenarioPack, BALANCED_PAIR_ORDER_ALGORITHM,
};
pub use comparator_acceptance::{
   admit_comparator_acceptance, canonical_comparator_acceptance_json,
   comparator_acceptance_signed_payload_sha256, AuditDisposition,
   AuditDispositionStatus, ComparatorAcceptanceAudit, ComparatorAcceptanceStatus,
   ComparatorAdmissionExpectation, ComparatorGateChecklist, ComparatorIdentity,
   ComparatorReviewer, ComparatorReviewerSignoff, ComparatorSourceSnapshot,
   ComparatorVariantSelection, CpuStackAudit, ForbiddenWorkChecklist, NativeCeilingAudit,
   NativeCeilingGap, ProductionArchitectureChecklist, ScalingCheck, ScalingChecklist,
   ScenarioBoundedProfile, StallAudit, COMPARATOR_ACCEPTANCE_SCHEMA_VERSION,
   CPU_STACK_DISPOSITION_THRESHOLD_BASIS_POINTS,
   NATIVE_CEILING_GAP_THRESHOLD_BASIS_POINTS,
};
pub use correctness_geometry::{
   decode_macos_correctness_geometry, decode_macos_correctness_geometry_pair,
   validate_macos_correctness_geometry, validate_macos_correctness_geometry_pair,
   MacOsCorrectnessGeometryEvidence, MacOsCorrectnessGeometryNode,
   MacOsCorrectnessGeometryPairEvidence, MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE,
   MACOS_CANONICAL_CORRECTNESS_SCALE,
};
pub use detection_coverage::{canonical_detection_coverage_json, detection_coverage_sha256, explain_macos_detection_coverage, validate_macos_detection_coverage, DetectionCoverageExplanation, DetectionCoverageManifest, DetectionExpectedDirection, DetectionExpectation, DetectionFaultFamily, DetectionOmittedScenario, DetectionRiskCoverage, DetectionRiskDimension, DetectionSeverity, DetectionTierSelection, MACOS_DETECTION_CASE_COUNT, MACOS_DETECTION_COVERAGE_RELATIVE_PATH, MACOS_DETECTION_FAULT_FAMILY_COUNT, MACOS_DETECTION_SEEDS_PER_EXPECTATION, MACOS_DETECTION_SEVERITY_COUNT};
pub use fixtures::{canonical_apple_pr_acquisition_json, canonical_budget_json, canonical_font_pack_json, canonical_legacy_case_disposition_json, canonical_scenario_json, load_appkit_macos_native_production_audit, load_apple_pr_acquisition, load_apple_pr_plan, load_apple_pr_scenarios, load_default_budgets, load_default_font_pack, load_legacy_case_disposition, load_macos_detection_coverage, load_pr_vertical_scenarios, load_scenario, load_scenario_schema_fixture, APPKIT_MACOS_NATIVE_PRODUCTION_AUDIT, APPLE_PR_ACQUISITION, APPLE_PR_PLAN, BUDGET_RELATIVE_ROOT, DEFAULT_FONT_PACK, LEGACY_CASE_DISPOSITION, MACOS_DETECTION_COVERAGE, SCENARIO_RELATIVE_ROOT, SCENARIO_SCHEMA_FIXTURE};
pub use instrumentation_calibration::{canonical_instrumentation_calibration_input_json, reduce_instrumentation_calibration, InstrumentationCalibrationInput, InstrumentationCalibrationPair, InstrumentationCalibrationReport, InstrumentationEquivalenceDecision, TracePairOrder, INSTRUMENTATION_CALIBRATION_CPU_MARGIN_RATIO, INSTRUMENTATION_CALIBRATION_P50_MARGIN_RATIO, INSTRUMENTATION_CALIBRATION_P95_MARGIN_RATIO, INSTRUMENTATION_CALIBRATION_SCHEMA_VERSION};
pub use macos_attribution::{canonical_macos_full_attribution_plan_json, materialize_macos_full_attribution_plan, validate_macos_full_attribution_plan, MacOsAttributionAvailability, MacOsAttributionBudget, MacOsAttributionCollectorKind, MacOsAttributionReplaySpec, MacOsAttributionTraceSelector, MacOsFullAttributionPlan, MacOsGpuCounterConfiguration, MACOS_FULL_ATTRIBUTION_MAX_HARD_SECONDS, MACOS_FULL_ATTRIBUTION_MEASUREMENT_SECONDS, MACOS_FULL_ATTRIBUTION_PAIR_COUNT, MACOS_FULL_ATTRIBUTION_PLAN_ID, MACOS_FULL_ATTRIBUTION_RESET_SECONDS, MACOS_FULL_ATTRIBUTION_SETUP_SECONDS, MACOS_FULL_ATTRIBUTION_WARMUP_SECONDS};
pub use macos_scale::{canonical_macos_comparator_qualification_plan_json, canonical_macos_comparator_runtime_attestation_json, canonical_macos_comparator_scale_overlay_json, macos_comparator_scale_contract, validate_macos_comparator_scale_overlay, MacOsComparatorQualificationPlan, MacOsComparatorRuntimeAttestation, MacOsComparatorScale, MacOsComparatorScaleDimension, MacOsComparatorScaleOverlay, MacOsComparatorScaleTransform, MacOsComparatorScaleVariant, MacOsComparatorSide, MACOS_COMPARATOR_SCALE_SCHEMA_VERSION};
pub use migration::{validate_legacy_case_disposition, LegacyCaseDisposition, LegacyCoverageGap, LegacyDisposition, LegacyDispositionGroup, COMPARISON_SCENARIO_IDS};
pub use pr_fixtures::{ChatFixture, ChatMessage, ChatSelectionReplacement, DashboardCategories, DashboardFixture, EnduranceFixture, FeedFixture, FeedRow, ImageDecodeZoomFixture, ImageFileFixture, NavigationFixture, NavigationTransition, StartupCard, StartupFixture};
pub use pr_scenarios::{validate_apple_pr_scenario_set, validate_nightly_endurance_scenario, validate_pr_vertical_slice, PR_VERTICAL_SCENARIO_IDS};
pub use release_candidate::{load_release_candidate_for_capture, ReleaseCandidateCaptureSpec};
pub use release_fixtures::{EffectsAnimationFixture, EffectsFixture, GridColumnContract, GridFixture, MutationClassFixture, MutationFixture, MutationSelectionFormula, ResizeChangeFixture, ResizeFixture, TextCategoryFixture, TextFixture};
pub use release_capture::{canonical_release_candidate_capture_plan_json, load_release_candidate_capture_plan, validate_release_candidate_capture_plan, ReleaseCandidateCaptureBinding, ReleaseCandidateCapturePlan, RELEASE_CANDIDATE_CAPTURE_PLAN_ID};
pub use release_promotion::{promote_release_candidates, ReleasePromotionQualificationReport, ReleasePromotionReport, RELEASE_CANDIDATE_IDS};
pub use scenario::{ArtifactIdentity, AssetFile, AssetManifest, FairnessContract, FontPackFile, FontPackIdentity, FontPackManifest, FontVariationAxis, InlineTextAsset, InlineTextAtlas, InlineTextAtlasVariant, ParityCheckpoint, RoleCount, ScenarioPhase, ScenarioSpec, SceneContract, TraceEvent, TraceOperation, TraceValue};
pub use schema::{BudgetSpec, Platform, Tier, BENCHMARK_SPEC_SCHEMA_VERSION};
pub use validate::{validate_budget, validate_comparison_plan, validate_default_budget_set, validate_font_pack_manifest, validate_scenario, validate_scenario_artifacts, validate_trace};
pub use acquisition::{validate_apple_pr_acquisition, AcquisitionChunkBudget, AcquisitionPackBudget, AcquisitionResetSegment, ApplePrAcquisitionSpec, APPLE_PR_SCENARIO_IDS};
pub use apple_campaign_plan::{
   apple_campaign_plan_sha256, canonical_apple_campaign_plan_json,
   materialize_default_macos_campaign_plan, validate_apple_campaign_contract,
   validate_runnable_apple_campaign_plan, AppleCampaignBudgetComponent,
   AppleCampaignBudgetComponentSpec, AppleCampaignEvidenceRole, AppleCampaignPackSpec,
   AppleCampaignMeasurementTimingSpec, AppleCampaignPassRole, AppleCampaignPassSpec,
   AppleCampaignPassTimingSpec, AppleCampaignPlanSpec, AppleCampaignScenarioBinding,
   AppleCampaignScenarioTimingSpec, AppleCampaignTimingSpec,
   APPLE_NIGHTLY_SCENARIO_IDS, APPLE_RELEASE_SCENARIO_IDS,
};
pub use apple_plan::{apple_pr_plan_sha256, canonical_apple_pr_plan_json, validate_apple_pr_plan, ApplePrPlanScenario, ApplePrPlanSpec, ComparatorAuditBinding};
pub use visual::{
   compare_calibrated_static_pngs, compare_exact_static_pngs,
   reduce_normalized_png_visual_parity,
   CalibratedStaticVisualParityReport, CalibratedStaticVisualThresholds,
   ExactStaticVisualParityReport, LogicalRect, NonTextVisualMetrics, PhysicalRect,
   RasterVisualMetrics, TextGeometryEvidence, TextLineGeometry, TextValidationStatus,
   VisualParityReport, VisualThresholds, VoxelVisualMetrics, CALIBRATED_STATIC_VISUAL_ALGORITHM,
   EXACT_STATIC_VISUAL_ALGORITHM, NORMALIZED_PNG_VISUAL_ALGORITHM,
};
