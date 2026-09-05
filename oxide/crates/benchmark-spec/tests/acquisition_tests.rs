use std::fs;

use oxide_benchmark_spec::{
   canonical_apple_pr_acquisition_json, load_apple_pr_acquisition, load_default_budgets,
   validate_apple_pr_acquisition,
};

#[test]
fn apple_pr_expansion_freezes_four_acquisitions_and_budget_arithmetic()
{
   let root = workspace_root();
   let (path, spec) = load_apple_pr_acquisition(&root).expect("load Apple PR acquisition expansion");
   let budgets = load_default_budgets(&root).expect("load default budgets");
   let budget = budgets.iter().find(|(_, budget)| budget.id == "apple-pr").map(|(_, budget)| budget).expect("Apple PR budget");
   validate_apple_pr_acquisition(&spec, budget).expect("validate Apple PR acquisition expansion");
   assert_eq!(canonical_apple_pr_acquisition_json(&spec).expect("canonical acquisition JSON"), fs::read(path).expect("read committed acquisition JSON"));
}

#[test]
fn apple_pr_expansion_rejects_a_hidden_lean_replay()
{
   let root = workspace_root();
   let (_, mut spec) = load_apple_pr_acquisition(&root).expect("load Apple PR acquisition expansion");
   let budgets = load_default_budgets(&root).expect("load default budgets");
   let budget = budgets.iter().find(|(_, budget)| budget.id == "apple-pr").map(|(_, budget)| budget).expect("Apple PR budget");
   spec.controller_chunks[1].pass_id = String::from("lean");
   assert!(validate_apple_pr_acquisition(&spec, budget).is_err());
}

#[test]
fn apple_pr_expansion_rejects_missing_pair_coverage()
{
   let root = workspace_root();
   let (_, mut spec) = load_apple_pr_acquisition(&root).expect("load Apple PR acquisition expansion");
   let budgets = load_default_budgets(&root).expect("load default budgets");
   let budget = budgets.iter().find(|(_, budget)| budget.id == "apple-pr").map(|(_, budget)| budget).expect("Apple PR budget");
   spec.controller_chunks[2].ordered_pair_indices.pop();
   assert!(validate_apple_pr_acquisition(&spec, budget).is_err());
}

fn workspace_root() -> std::path::PathBuf
{
   std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("canonical workspace root")
}
