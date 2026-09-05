use oxide_benchmark_spec::{canonical_release_candidate_capture_plan_json, load_release_candidate_capture_plan, validate_release_candidate_capture_plan, RELEASE_CANDIDATE_IDS};
use std::path::{Path, PathBuf};

fn spec_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join("benchmarks/comparative/specs/v1")
}

#[test]
fn release_candidate_capture_plan_binds_exactly_five_candidates_and_eighteen_checkpoints()
{
   let root = spec_root();
   let plan = load_release_candidate_capture_plan(&root).expect("release-candidate capture plan");
   validate_release_candidate_capture_plan(&root, &plan).expect("valid release-candidate capture plan");
   assert_eq!(plan.candidates.iter().map(|candidate| candidate.id.as_str()).collect::<Vec<_>>(), RELEASE_CANDIDATE_IDS);
   assert_eq!(plan.candidates.iter().map(|candidate| candidate.checkpoint_ids.len()).sum::<usize>(), 18);
   assert_eq!(canonical_release_candidate_capture_plan_json(&plan).expect("canonical capture plan"), std::fs::read(root.join("plans/macos-release-candidate-capture.json")).expect("capture plan bytes"));
}

#[test]
fn release_candidate_capture_plan_rejects_order_and_timing_claim_drift()
{
   let root = spec_root();
   let mut plan = load_release_candidate_capture_plan(&root).expect("release-candidate capture plan");
   plan.candidates.swap(0, 1);
   assert!(validate_release_candidate_capture_plan(&root, &plan).is_err());
   plan.candidates.swap(0, 1);
   plan.timing_claim = String::from("claim-bearing");
   assert!(validate_release_candidate_capture_plan(&root, &plan).is_err());
}
