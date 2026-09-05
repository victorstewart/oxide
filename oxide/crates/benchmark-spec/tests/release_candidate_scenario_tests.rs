use oxide_benchmark_spec::{validate_trace, TraceEvent};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const CANDIDATES: [(&str, &str, &str); 5] = [
   ("grid.large-scroll", "frame.missed_display_opportunities_per_1000", "rate-per-1000-eligible-display-opportunities"),
   ("effects.layers", "frame.missed_display_opportunities_per_1000", "rate-per-1000-eligible-display-opportunities"),
   ("mutation.damage", "mutation_10pct_to_attributed_presentation_ms", "median"),
   ("text.multilingual", "font_cold_to_first_visible_ms", "p50"),
   ("resize.theme", "change_to_settled_presentation_ms", "median"),
];

#[test]
fn release_candidates_are_frozen_hash_bound_and_fail_closed_on_screenshots()
{
   let root = spec_root();
   for (id, metric, estimator) in CANDIDATES
   {
      let path = root.join(format!("release-candidates/{id}.candidate.json"));
      let bytes = fs::read(&path).expect("read release candidate");
      let candidate = serde_json::from_slice::<Value>(&bytes).expect("parse release candidate");
      let mut canonical = serde_json::to_vec(&candidate).expect("serialize canonical release candidate");
      canonical.push(b'\n');
      assert_eq!(bytes, canonical, "{id} candidate JSON is not canonical compact JSON");
      assert_eq!(candidate["id"], id);
      assert_eq!(candidate["candidate_status"], "blocked-on-canonical-screenshots");
      assert_eq!(candidate["primary_metric"], metric);
      assert_eq!(candidate["within_session_estimator"], estimator);
      assert_eq!(candidate["owning_pass"], "minimal-presentation");
      assert_eq!(candidate["screenshot_materialization"]["required"], true);
      assert_eq!(candidate["screenshot_materialization"]["status"], "blocked");
      assert!(!root.join(format!("scenarios/{id}.json")).exists(), "{id} must remain non-loadable before canonical PNG materialization");

      assert_artifact(&root, &candidate["fixture"]);
      assert_artifact(&root, &candidate["assets"]);
      assert_manifest_artifact(&root, &candidate["font_pack"]);
      assert_artifact(&root, &candidate["scene"]["style_tokens"]);
      assert_artifact(&root, &candidate["scene"]["layout_assertions"]);

      for phase in candidate["phases"].as_array().expect("phase array")
      {
         if let Some(trace) = phase.get("trace")
         {
            let bytes = assert_artifact(&root, trace);
            let events = serde_json::from_slice::<Vec<TraceEvent>>(&bytes).expect("parse typed trace");
            validate_trace(&events).expect("validate typed trace");
            let duration_us = phase["duration_ms"].as_u64().expect("trace phase duration") * 1000;
            assert!(events.last().expect("nonempty trace").at_us <= duration_us, "{id} trace exceeds its phase");
         }
      }

      let expected = candidate["expected_visible_role_counts_by_checkpoint"].as_object().expect("expected role count map");
      let checkpoints = candidate["parity_checkpoints"].as_array().expect("checkpoint array");
      assert_eq!(expected.len(), checkpoints.len());
      for checkpoint in checkpoints
      {
         assert!(checkpoint.get("screenshot").is_none(), "candidate must not bind invented screenshot bytes");
         let checkpoint_id = checkpoint["id"].as_str().expect("checkpoint id");
         let state = serde_json::from_slice::<Value>(&assert_artifact(&root, &checkpoint["state"])).expect("parse state");
         let accessibility = serde_json::from_slice::<Value>(&assert_artifact(&root, &checkpoint["accessibility"])).expect("parse accessibility");
         assert_eq!(state["scenario_id"], id);
         assert_eq!(state["checkpoint_id"], checkpoint_id);
         assert_eq!(accessibility["scenario_id"], id);
         assert_eq!(accessibility["checkpoint_id"], checkpoint_id);
         assert_eq!(state["visible_role_counts"], expected[checkpoint_id]);
         let actual = accessibility["nodes"].as_array().expect("accessibility nodes").iter().map(|node| {
            serde_json::json!({"count": node["count"], "role": node["role"]})
         }).collect::<Vec<_>>();
         assert_eq!(Value::Array(actual), expected[checkpoint_id]);
      }
   }
}

#[test]
fn release_fixture_cardinalities_and_phase_contracts_match_the_frozen_matrix()
{
   let root = spec_root();
   let grid = load(&root, "fixtures/grid.large-scroll.json");
   assert_eq!((grid["tile_count"].as_u64(), grid["thumbnail_count"].as_u64()), (Some(10_000), Some(256)));
   assert_eq!(grid["column_contracts"][0]["columns"], 3);
   assert_eq!(grid["column_contracts"][1]["columns"], 6);

   let effects = load(&root, "fixtures/effects.layers.json");
   assert_eq!((effects["card_count"].as_u64(), effects["clip_count"].as_u64(), effects["shadow_count"].as_u64(), effects["backdrop_blur_count"].as_u64()), (Some(100), Some(100), Some(32), Some(8)));

   let mutation = load(&root, "fixtures/mutation.damage.json");
   assert_eq!(mutation["node_count"], 10_000);
   assert_eq!(mutation["mutation_repetitions"], 5);
   assert_eq!(mutation["mutation_classes"].as_array().expect("mutation classes").iter().map(|class| class["changed_node_count"].as_u64().expect("changed count")).collect::<Vec<_>>(), [100, 1_000, 10_000]);

   let text = load(&root, "fixtures/text.multilingual.json");
   assert_eq!(text["label_count"], 1_000);
   assert_eq!(text["categories"].as_array().expect("text categories").iter().map(|category| category["count"].as_u64().expect("category count")).sum::<u64>(), 1_000);
   assert_eq!(text["wrap_width_rotation"], serde_json::json!([96, 144, 192, 240]));

   let resize = load(&root, "fixtures/resize.theme.json");
   assert_artifact(&root, &resize["dashboard_fixture"]);
   assert_eq!(resize["change_count"], 10);
   let changes = resize["changes"].as_array().expect("resize changes");
   assert_eq!(changes.len(), 10);
   let expected_themes = ["dark", "light", "light", "dark", "dark", "light", "light", "dark", "dark", "light"];
   for (index, change) in changes.iter().enumerate()
   {
      let previous_orientation = if index == 0 {resize["initial_orientation"].as_str()} else {changes[index - 1]["orientation"].as_str()};
      assert_ne!(change["orientation"].as_str(), previous_orientation);
      assert_eq!(change["theme"], expected_themes[index]);
   }
}

fn spec_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join("benchmarks/comparative/specs/v1")
}

fn load(root: &Path, path: &str) -> Value
{
   serde_json::from_slice(&fs::read(root.join(path)).expect("read JSON artifact")).expect("parse JSON artifact")
}

fn assert_manifest_artifact(root: &Path, artifact: &Value) -> Vec<u8>
{
   assert_path_hash(root, artifact["manifest"].as_str().expect("manifest path"), artifact["sha256"].as_str().expect("manifest SHA-256"))
}

fn assert_artifact(root: &Path, artifact: &Value) -> Vec<u8>
{
   assert_path_hash(root, artifact["path"].as_str().expect("artifact path"), artifact["sha256"].as_str().expect("artifact SHA-256"))
}

fn assert_path_hash(root: &Path, relative: &str, expected: &str) -> Vec<u8>
{
   assert!(!relative.starts_with('/') && !relative.split('/').any(|component| component == ".."));
   let bytes = fs::read(root.join(relative)).expect("read hash-bound artifact");
   assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected, "SHA-256 mismatch for {relative}");
   bytes
}
