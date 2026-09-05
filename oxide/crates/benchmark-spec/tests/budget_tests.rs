use oxide_benchmark_spec::{
   canonical_budget_json, load_default_budgets, validate_budget, validate_default_budget_set,
};
use std::fs;
use std::path::Path;

#[test]
fn committed_default_budgets_match_v1_arithmetic_and_canonical_json()
{
   let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
   let budgets = load_default_budgets(&root).expect("load default budgets");
   validate_default_budget_set(&budgets).expect("validate default budget set");
   assert_eq!(budgets.len(), 9);
   for (path, budget) in budgets
   {
      assert_eq!(fs::read(&path).expect("read budget fixture"), canonical_budget_json(&budget).expect("canonical budget JSON"));
   }
}

#[test]
fn apple_pr_budget_preserves_four_acquisition_ceiling_arithmetic()
{
   let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
   let budgets = load_default_budgets(&root).expect("load default budgets");
   let budget = budgets.iter().find(|(_, budget)| budget.id == "apple-pr").map(|(_, budget)| budget).expect("Apple PR budget");
   assert_eq!(budget.correctness_install_pulls_seconds, 155);
   assert_eq!(budget.primary_dynamic_presentation_seconds, 480);
   assert_eq!(budget.launch_or_startup_delivery_seconds, 120);
   assert_eq!(budget.pre_reserve_seconds, 755);
   assert_eq!(budget.reserve_seconds, 151);
   assert_eq!(budget.hard_total_seconds, 906);
   assert!(budget.hard_total_seconds <= 20 * 60);
}

#[test]
fn nightly_web_shards_preserve_aggregate_and_critical_wall_contracts()
{
   let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
   let budgets = load_default_budgets(&root).expect("load default budgets");
   let engine = budgets.iter().find(|(_, budget)| budget.id == "nightly-web-engine").map(|(_, budget)| budget).expect("nightly engine budget");
   let mobile = budgets.iter().find(|(_, budget)| budget.id == "nightly-web-mobile").map(|(_, budget)| budget).expect("nightly mobile budget");
   assert_eq!(engine.hard_total_seconds + mobile.hard_total_seconds, 6_408);
   assert_eq!(engine.campaign_aggregate_seconds, 6_408);
   assert_eq!(mobile.campaign_aggregate_seconds, 6_408);
   assert_eq!(engine.campaign_critical_wall_seconds, 4_968);
   assert_eq!(mobile.campaign_critical_wall_seconds, 4_968);
}

#[test]
fn budget_validation_fails_when_a_component_or_reserve_is_changed()
{
   let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
   let mut budgets = load_default_budgets(&root).expect("load default budgets");
   let budget = &mut budgets[0].1;
   budget.launch_or_startup_delivery_seconds = 0;
   assert!(validate_budget(budget).is_err());

   let mut budgets = load_default_budgets(&root).expect("reload default budgets");
   let budget = &mut budgets[0].1;
   budget.reserve_seconds = 0;
   assert!(validate_budget(budget).is_err());

   let mut budgets = load_default_budgets(&root).expect("reload default budgets");
   let budget = &mut budgets[0].1;
   budget.correctness_install_pulls_seconds = u64::MAX;
   assert!(validate_budget(budget).is_err());
}
