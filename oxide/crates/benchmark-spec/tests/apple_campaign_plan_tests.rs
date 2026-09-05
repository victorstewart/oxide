use oxide_benchmark_spec::{
   canonical_apple_campaign_plan_json, load_default_budgets,
   materialize_default_macos_campaign_plan, validate_apple_campaign_contract,
   validate_runnable_apple_campaign_plan, AppleCampaignMeasurementTimingSpec,
   AppleCampaignPassRole, AppleCampaignScenarioTimingSpec, Tier,
   APPLE_NIGHTLY_SCENARIO_IDS, APPLE_RELEASE_SCENARIO_IDS,
};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn nightly_plan_binds_eight_scenarios_and_the_declared_acquisition_structure()
{
   let workspace = workspace_root();
   let spec_root = workspace.join("benchmarks/comparative/specs/v1");
   let budget = budget("nightly-apple");
   let plan = materialize_default_macos_campaign_plan(&spec_root, &budget).expect("materialize nightly plan");
   validate_apple_campaign_contract(&plan, &budget).expect("validate nightly contract");

   assert!(plan.scenarios.iter().map(|scenario| scenario.id.as_str()).eq(APPLE_NIGHTLY_SCENARIO_IDS.iter().copied()));
   assert_eq!(plan.scenarios.len(), 8);
   assert_eq!(pass_ids(&plan, AppleCampaignPassRole::Primary), ["primary-presentation"]);
   assert_eq!(pass_ids(&plan, AppleCampaignPassRole::Launch), ["canonical-launch"]);
   assert_eq!(pass_ids(&plan, AppleCampaignPassRole::Attribution), ["attribution-time-profiler", "attribution-physical-footprint"]);
   assert_eq!(plan.passes.iter().find(|pass| pass.id == "primary-presentation").expect("nightly primary pass").pair_count, 6);
   assert_eq!(plan.passes.iter().find(|pass| pass.id == "canonical-launch").expect("nightly launch pass").launch_classes, ["terminated-warm-system-cache:8", "fresh-install-first-launch:2"]);
   assert_eq!(plan.passes.iter().find(|pass| pass.id == "idle").expect("nightly idle pass").pair_count, 2);
   assert_eq!(plan.passes.iter().find(|pass| pass.id == "endurance").expect("nightly endurance pass").pair_count, 2);
   assert_eq!(plan.timing.passes.len(), 5);
   for timing in &plan.timing.passes
   {
      assert_eq!(timing.reset_seconds_per_session, 5);
      assert_eq!(timing.readiness_timeout_seconds, 20);
      assert!(timing.scenarios.iter().all(|scenario| scenario.setup_seconds == 1));
   }
   assert_eq!(plan.timing.passes.iter().find(|timing| timing.pass_id == "idle").expect("idle timing").scenarios[0].measurement, AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: 60});
   assert_eq!(plan.timing.passes.iter().find(|timing| timing.pass_id == "endurance").expect("endurance timing").scenarios[0].measurement, AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: 300});
   let navigation = plan.timing.passes[0].scenarios.iter().find(|scenario| scenario.scenario_id == "navigation.modal").expect("nightly navigation timing");
   assert_eq!(navigation.measurement, AppleCampaignMeasurementTimingSpec::Iterations {phase_id: String::from("canonical-cycles"), source_iteration_count: 4, iteration_count: 10, occupied_seconds: 12});
   assert_eq!(serde_json::from_slice::<oxide_benchmark_spec::AppleCampaignPlanSpec>(&canonical_apple_campaign_plan_json(&plan).expect("canonical nightly JSON")).expect("decode canonical nightly JSON"), plan);

   validate_runnable_apple_campaign_plan(&spec_root, &plan, &budget).expect("current nightly artifacts must be runnable");
   let mut incomplete = plan.clone();
   incomplete.scenarios.iter_mut().find(|scenario| scenario.id == "endurance.churn").expect("endurance binding").artifact = None;
   let error = validate_runnable_apple_campaign_plan(&spec_root, &incomplete, &budget).expect_err("missing endurance must fail preflight");
   let message = format!("{:#}", error);
   assert!(message.contains("not runnable"));
   assert!(message.contains("scenarios/endurance.churn.json"));
}

#[test]
fn release_plans_bind_all_thirteen_candidates_and_fail_on_every_missing_artifact()
{
   let workspace = workspace_root();
   let spec_root = workspace.join("benchmarks/comparative/specs/v1");
   for budget_id in ["apple-release-core", "apple-release-claim-complete"]
   {
      let budget = budget(budget_id);
      let plan = materialize_default_macos_campaign_plan(&spec_root, &budget).expect("materialize release plan");
      validate_apple_campaign_contract(&plan, &budget).expect("validate release contract");
      let committed = fs::read(spec_root.join("plans").join(format!("{}.json", budget_id))).expect("read committed release plan");
      assert_eq!(committed, canonical_apple_campaign_plan_json(&plan).expect("canonical release plan"));
      assert!(plan.scenarios.iter().map(|scenario| scenario.id.as_str()).eq(APPLE_RELEASE_SCENARIO_IDS.iter().copied()));
      assert_eq!(plan.scenarios.len(), 13);
      assert_eq!(plan.packs.iter().map(|pack| pack.id.as_str()).collect::<Vec<_>>(), ["launch", "core-interaction", "scroll-damage", "media-text-warm", "soak-idle", "soak-endurance"]);
      assert_eq!(pass_ids(&plan, AppleCampaignPassRole::Attribution), ["attribution-time-profiler", "attribution-system-trace", "common-gpu", "attribution-physical-footprint"]);
      let common_gpu = plan.passes.iter().find(|pass| pass.id == "common-gpu").expect("common GPU pass");
      assert_eq!(common_gpu.evidence_role, oxide_benchmark_spec::AppleCampaignEvidenceRole::DescriptiveDiagnostic);
      assert_eq!(plan.timing.passes.len(), 8);
      assert!(plan.timing.passes.iter().all(|timing| timing.reset_seconds_per_session == 5 && timing.readiness_timeout_seconds == 30));
      let primary = plan.timing.passes.iter().find(|timing| timing.pass_id == "primary-presentation").expect("release primary timing");
      let navigation = primary.scenarios.iter().find(|scenario| scenario.scenario_id == "navigation.modal").expect("release navigation timing");
      let resize = primary.scenarios.iter().find(|scenario| scenario.scenario_id == "resize.theme").expect("release resize timing");
      assert_eq!(navigation.measurement, AppleCampaignMeasurementTimingSpec::Iterations {phase_id: String::from("canonical-cycles"), source_iteration_count: 4, iteration_count: 20, occupied_seconds: 20});
      assert_eq!(resize.measurement, AppleCampaignMeasurementTimingSpec::Iterations {phase_id: String::from("ten-changes"), source_iteration_count: 10, iteration_count: 10, occupied_seconds: 20});
      let energy = plan.timing.passes.iter().find(|timing| timing.pass_id == "energy").expect("release energy timing");
      assert_eq!(energy.scenarios, [AppleCampaignScenarioTimingSpec {
         scenario_id: String::from("dashboard.mixed-static"),
         setup_seconds: 1,
         warmup_seconds: 120,
         measurement: AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: 120},
      }]);

      let missing = plan.scenarios.iter().filter(|scenario| scenario.artifact.is_none()).map(|scenario| scenario.expected_path.clone()).collect::<Vec<_>>();
      if missing.is_empty()
      {
         validate_runnable_apple_campaign_plan(&spec_root, &plan, &budget).expect("complete release artifacts must be runnable");
      }
      else
      {
         let error = validate_runnable_apple_campaign_plan(&spec_root, &plan, &budget).expect_err("incomplete release must fail preflight");
         let message = format!("{:#}", error);
         for path in missing
         {
            assert!(message.contains(&path), "missing artifact error omitted {}: {}", path, message);
         }
      }
      let mut incomplete = plan.clone();
      incomplete.scenarios.iter_mut().find(|scenario| scenario.id == "grid.large-scroll").expect("grid binding").artifact = None;
      assert!(validate_runnable_apple_campaign_plan(&spec_root, &incomplete, &budget).is_err());
   }
}

#[test]
fn custom_tiers_require_explicit_contracts_and_full_attribution_covers_the_matrix()
{
   let workspace = workspace_root();
   let spec_root = workspace.join("benchmarks/comparative/specs/v1");
   let mut audit_budget = budget("apple-release-core");
   audit_budget.id = String::from("apple-full-attribution-audit");
   audit_budget.tier = Tier::FullAttribution;
   assert!(materialize_default_macos_campaign_plan(&spec_root, &audit_budget).is_err());

   let release_budget = budget("apple-release-core");
   let mut plan = materialize_default_macos_campaign_plan(&spec_root, &release_budget).expect("materialize release plan");
   plan.id = audit_budget.id.clone();
   plan.budget_id = audit_budget.id.clone();
   plan.tier = Tier::FullAttribution;
   for pass in plan.passes.iter_mut().filter(|pass| pass.role == AppleCampaignPassRole::Attribution)
   {
      pass.scenario_ids = APPLE_RELEASE_SCENARIO_IDS.iter().map(|id| String::from(*id)).collect();
   }
   for timing in plan.timing.passes.iter_mut().filter(|timing| !["primary-presentation", "idle", "endurance", "energy"].contains(&timing.pass_id.as_str()))
   {
      timing.scenarios = APPLE_RELEASE_SCENARIO_IDS.iter().map(|scenario_id| AppleCampaignScenarioTimingSpec {
         scenario_id: String::from(*scenario_id),
         setup_seconds: 1,
         warmup_seconds: 5,
         measurement: AppleCampaignMeasurementTimingSpec::Duration {duration_seconds: 20},
      }).collect();
   }
   let full_attribution = plan.passes.iter_mut().find(|pass| pass.id == "common-gpu").expect("common GPU pass");
   full_attribution.id = String::from("full-attribution");
   full_attribution.collector = Some(String::from("full-attribution"));
   let full_attribution_timing = plan.timing.passes.iter_mut().find(|timing| timing.pass_id == "common-gpu").expect("common GPU timing");
   full_attribution_timing.pass_id = String::from("full-attribution");
   let attribution_component = plan.budget_components.iter_mut().find(|component| component.pass_ids.iter().any(|id| id == "common-gpu")).expect("attribution component");
   *attribution_component.pass_ids.iter_mut().find(|id| id.as_str() == "common-gpu").expect("common GPU budget pass") = String::from("full-attribution");
   validate_apple_campaign_contract(&plan, &audit_budget).expect("validate explicit full-attribution contract");

   plan.passes.iter_mut().find(|pass| pass.id == "full-attribution").expect("full-attribution pass").scenario_ids.pop();
   assert!(validate_apple_campaign_contract(&plan, &audit_budget).is_err());
}

#[test]
fn campaign_contract_rejects_scenario_or_budget_ownership_drift()
{
   let workspace = workspace_root();
   let spec_root = workspace.join("benchmarks/comparative/specs/v1");
   let budget = budget("nightly-apple");
   let mut plan = materialize_default_macos_campaign_plan(&spec_root, &budget).expect("materialize nightly plan");
   plan.scenarios.swap(0, 1);
   assert!(validate_apple_campaign_contract(&plan, &budget).is_err());

   let mut plan = materialize_default_macos_campaign_plan(&spec_root, &budget).expect("rematerialize nightly plan");
   plan.budget_components[0].pass_ids.push(String::from("primary-presentation"));
   assert!(validate_apple_campaign_contract(&plan, &budget).is_err());
}

fn pass_ids(plan: &oxide_benchmark_spec::AppleCampaignPlanSpec, role: AppleCampaignPassRole) -> Vec<&str>
{
   plan.passes.iter().filter(|pass| pass.role == role).map(|pass| pass.id.as_str()).collect()
}

fn budget(id: &str) -> oxide_benchmark_spec::BudgetSpec
{
   load_default_budgets(&workspace_root()).expect("load budgets").into_iter().find(|(_, budget)| budget.id == id).map(|(_, budget)| budget).expect("named budget")
}

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("canonical workspace root")
}
