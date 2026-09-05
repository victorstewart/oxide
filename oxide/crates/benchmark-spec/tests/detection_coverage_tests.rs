use oxide_benchmark_spec::{
   explain_macos_detection_coverage, load_macos_detection_coverage,
   validate_macos_detection_coverage, DetectionSeverity, Tier,
   APPLE_NIGHTLY_SCENARIO_IDS, APPLE_RELEASE_SCENARIO_IDS,
   MACOS_DETECTION_CASE_COUNT, MACOS_DETECTION_FAULT_FAMILY_COUNT,
};
use std::path::{Path, PathBuf};

#[test]
fn committed_macos_fault_corpus_is_complete_and_every_default_tier_covers_it()
{
   let (_, manifest) = load_macos_detection_coverage(&workspace_root()).expect("committed macOS detection manifest");
   validate_macos_detection_coverage(&manifest).expect("valid macOS detection manifest");
   assert_eq!(manifest.risk_dimensions.len(), 12);
   assert_eq!(manifest.fault_families.len(), MACOS_DETECTION_FAULT_FAMILY_COUNT);
   assert_eq!(manifest.expectations.len(), 42);
   assert_eq!(manifest.expected_case_count as usize, MACOS_DETECTION_CASE_COUNT);
   assert_eq!(manifest.expectations.iter().filter(|expectation| expectation.must_detect).count(), 28);

   let pr = manifest.tier_selections.iter().find(|selection| selection.tier == Tier::Pr).expect("PR selection").selected_scenario_ids.clone();
   for (tier, scenarios) in [
      (Tier::Pr, pr),
      (Tier::Nightly, scenario_ids(APPLE_NIGHTLY_SCENARIO_IDS)),
      (Tier::ReleaseCore, scenario_ids(APPLE_RELEASE_SCENARIO_IDS)),
      (Tier::ClaimComplete, scenario_ids(APPLE_RELEASE_SCENARIO_IDS)),
   ]
   {
      let explanation = explain_macos_detection_coverage(&manifest, tier, &scenarios).expect("complete planned coverage");
      assert_eq!(explanation.planned_must_detect_rate_basis_points, 10_000);
      assert_eq!(explanation.planned_weighted_detection_rate_basis_points, 10_000);
      assert!(explanation.risk_coverage.iter().all(|risk| !risk.selected_scenario_ids.is_empty()));
   }
}

#[test]
fn explicit_macos_tiers_use_the_complete_release_risk_selection()
{
   let (_, manifest) = load_macos_detection_coverage(&workspace_root()).expect("committed macOS detection manifest");
   let scenarios = scenario_ids(APPLE_RELEASE_SCENARIO_IDS);
   assert!(explain_macos_detection_coverage(&manifest, Tier::Extended, &scenarios).is_ok());
   assert!(explain_macos_detection_coverage(&manifest, Tier::FullAttribution, &scenarios).is_ok());
}

#[test]
fn validator_rejects_weakened_or_stale_detection_contracts()
{
   let (_, manifest) = load_macos_detection_coverage(&workspace_root()).expect("committed macOS detection manifest");

   let mut missing_family = manifest.clone();
   missing_family.fault_families.pop();
   assert!(validate_macos_detection_coverage(&missing_family).is_err());

   let mut weakened_medium = manifest.clone();
   let medium = weakened_medium.expectations.iter_mut().find(|expectation| expectation.severity == DetectionSeverity::Medium).expect("medium expectation");
   medium.must_detect = false;
   assert!(validate_macos_detection_coverage(&weakened_medium).is_err());

   let mut weakened_terminal = manifest.clone();
   let terminal = weakened_terminal.expectations.iter_mut().find(|expectation| expectation.terminal_validator_id.is_some()).expect("terminal expectation");
   terminal.required_seed_count = 2;
   assert!(validate_macos_detection_coverage(&weakened_terminal).is_err());

   let mut false_positive_terminal = manifest.clone();
   let terminal = false_positive_terminal.expectations.iter_mut().find(|expectation| expectation.terminal_validator_id.is_some()).expect("terminal expectation");
   terminal.maximum_false_positive_rate_basis_points = 1;
   assert!(validate_macos_detection_coverage(&false_positive_terminal).is_err());

   let mut stale_omission = manifest;
   stale_omission.tier_selections[0].omitted_scenarios[0].marginal_expectation_count += 1;
   assert!(validate_macos_detection_coverage(&stale_omission).is_err());
}

fn scenario_ids(ids: &[&str]) -> Vec<String>
{
   ids.iter().map(|id| (*id).to_string()).collect()
}

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("canonical workspace root")
}
