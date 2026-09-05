use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use oxide_benchmark_spec::{
   apple_pr_plan_sha256, canonical_apple_pr_plan_json, load_apple_pr_acquisition,
   load_apple_pr_plan, load_default_budgets, validate_apple_pr_plan,
   ApplePrPlanScenario, ApplePrPlanSpec, ArtifactIdentity, ComparatorAuditBinding,
   ComparatorIdentity, Platform, Tier,
   APPLE_PR_SCENARIO_IDS,
};
use sha2::{Digest, Sha256};

#[test]
fn apple_pr_plan_transitively_binds_every_selected_scenario()
{
   let workspace = workspace_root();
   let source_root = workspace.join("benchmarks/comparative/specs/v1");
   let temporary_root = temporary_spec_root();
   copy_file(&source_root, &temporary_root, "acquisition/apple-pr.json");
   copy_file(&source_root, &temporary_root, "audits/macos-appkit-native-production.json");
   copy_file(&source_root, &temporary_root, "budgets/apple-pr.json");
   for scenario_id in APPLE_PR_SCENARIO_IDS
   {
      copy_file(&source_root, &temporary_root, &format!("scenarios/{}.json", scenario_id));
   }

   let (_, acquisition) = load_apple_pr_acquisition(&workspace).expect("load Apple PR acquisition");
   let budgets = load_default_budgets(&workspace).expect("load budgets");
   let budget = budgets.into_iter().find(|(_, budget)| budget.id == "apple-pr").map(|(_, budget)| budget).expect("Apple PR budget");
   let plan = plan_for_root(&temporary_root);
   validate_apple_pr_plan(&temporary_root, &plan, &acquisition, &budget).expect("validate transitive Apple PR plan");

   let old_plan_sha256 = apple_pr_plan_sha256(&plan).expect("hash plan");
   let scenario_path = temporary_root.join("scenarios/feed.variable-scroll.json");
   let mut drifted = fs::read(&scenario_path).expect("read copied feed scenario");
   drifted.push(b' ');
   fs::write(&scenario_path, drifted).expect("drift copied feed scenario");
   assert!(validate_apple_pr_plan(&temporary_root, &plan, &acquisition, &budget).is_err());

   let rematerialized = plan_for_root(&temporary_root);
   assert_ne!(apple_pr_plan_sha256(&rematerialized).expect("hash rematerialized plan"), old_plan_sha256);
   validate_apple_pr_plan(&temporary_root, &rematerialized, &acquisition, &budget).expect("validate rematerialized plan");
   fs::remove_dir_all(&temporary_root).expect("remove temporary plan root");
}

#[test]
fn committed_apple_pr_plan_is_canonical_and_valid()
{
   let workspace = workspace_root();
   let spec_root = workspace.join("benchmarks/comparative/specs/v1");
   let (path, plan) = load_apple_pr_plan(&workspace).expect("load committed Apple PR plan");
   let (_, acquisition) = load_apple_pr_acquisition(&workspace).expect("load Apple PR acquisition");
   let budgets = load_default_budgets(&workspace).expect("load budgets");
   let budget = budgets.iter().find(|(_, budget)| budget.id == "apple-pr").map(|(_, budget)| budget).expect("Apple PR budget");
   assert_eq!(canonical_apple_pr_plan_json(&plan).expect("canonical plan JSON"), fs::read(path).expect("read committed plan"));
   validate_apple_pr_plan(&spec_root, &plan, &acquisition, budget).expect("validate committed Apple PR plan");
}

fn plan_for_root(root: &Path) -> ApplePrPlanSpec
{
   ApplePrPlanSpec {
      schema_version: 1,
      id: String::from("apple-pr"),
      platform: Platform::Apple,
      tier: Tier::Pr,
      acquisition: identity(root, "acquisition/apple-pr.json"),
      budget: identity(root, "budgets/apple-pr.json"),
      comparator_audits: vec![ComparatorAuditBinding {
         identity: ComparatorIdentity {
            platform: String::from("macos"),
            framework: String::from("appkit"),
            implementation: String::from("appkit-production"),
            variant: String::from("native.production"),
         },
         audit: identity(root, "audits/macos-appkit-native-production.json"),
      }],
      scenarios: APPLE_PR_SCENARIO_IDS.iter().map(|id| ApplePrPlanScenario {
         id: String::from(*id),
         artifact: identity(root, &format!("scenarios/{}.json", id)),
      }).collect(),
   }
}

fn identity(root: &Path, relative: &str) -> ArtifactIdentity
{
   let bytes = fs::read(root.join(relative)).expect("read plan artifact");
   ArtifactIdentity {
      path: String::from(relative),
      sha256: format!("{:x}", Sha256::digest(bytes)),
   }
}

fn copy_file(source_root: &Path, destination_root: &Path, relative: &str)
{
   let destination = destination_root.join(relative);
   fs::create_dir_all(destination.parent().expect("plan artifact parent")).expect("create plan artifact directory");
   fs::copy(source_root.join(relative), destination).expect("copy plan artifact");
}

fn temporary_spec_root() -> PathBuf
{
   let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock before epoch").as_nanos();
   std::env::temp_dir().join(format!("oxide-benchmark-spec-plan-{}-{}", std::process::id(), nonce))
}

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("canonical workspace root")
}
