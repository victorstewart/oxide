use oxide_benchmark_spec::{
   canonical_scenario_json, compare_exact_static_pngs, load_apple_pr_scenarios,
   load_pr_vertical_scenarios, reduce_normalized_png_visual_parity, validate_apple_pr_scenario_set,
   validate_pr_vertical_slice, LogicalRect, TextGeometryEvidence, TextLineGeometry,
   VisualThresholds,
};
use serde_json::Value;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn committed_pr_vertical_slice_is_canonical_complete_and_runnable()
{
   let root = workspace_root();
   let spec_root = root.join("benchmarks/comparative/specs/v1");
   let scenarios = load_pr_vertical_scenarios(&root).expect("load committed PR vertical scenarios");
   validate_pr_vertical_slice(&spec_root, &scenarios).expect("validate runnable PR vertical slice");

   for (path, scenario) in &scenarios
   {
      assert_eq!(fs::read(path).expect("read scenario manifest"), canonical_scenario_json(scenario).expect("canonical scenario JSON"));
      for checkpoint in &scenario.parity_checkpoints
      {
         let bytes = fs::read(spec_root.join(&checkpoint.screenshot.path)).expect("read checkpoint screenshot");
         assert_canonical_png(&bytes);
      }
   }

   let dashboard = artifact_json(&spec_root, &scenarios[0].1.fixture.path);
   assert_eq!(dashboard["visible_node_count"], 300);
   let feed = artifact_json(&spec_root, &scenarios[1].1.fixture.path);
   assert_eq!(feed["row_count"], 2_000);
   assert_eq!(feed["rows"].as_array().expect("feed rows").len(), 2_000);
   let navigation = artifact_json(&spec_root, &scenarios[2].1.fixture.path);
   assert_eq!(navigation["cycle_count"], 4);
   let assets = artifact_json(&spec_root, &scenarios[0].1.assets.path);
   assert_eq!(assets["tile_count"], 128);
}

#[test]
fn committed_apple_pr_set_is_canonical_complete_and_runnable()
{
   let root = workspace_root();
   let spec_root = root.join("benchmarks/comparative/specs/v1");
   let scenarios = load_apple_pr_scenarios(&root).expect("load committed Apple PR scenarios");
   validate_apple_pr_scenario_set(&spec_root, &scenarios).expect("validate runnable Apple PR scenarios");

   for (path, scenario) in &scenarios
   {
      assert_eq!(fs::read(path).expect("read scenario manifest"), canonical_scenario_json(scenario).expect("canonical scenario JSON"));
      for checkpoint in &scenario.parity_checkpoints
      {
         let bytes = fs::read(spec_root.join(&checkpoint.screenshot.path)).expect("read checkpoint screenshot");
         assert_canonical_png(&bytes);
      }
      let layout = fs::read(spec_root.join(&scenario.scene.layout_assertions.path)).expect("read checkpoint layout");
      let layout_json: Value = serde_json::from_slice(&layout).expect("parse checkpoint layout");
      assert!(!layout_json["text_masks"].as_array().expect("layout text masks").is_empty(), "{} has no text masks", scenario.id);
      let screenshot = fs::read(spec_root.join(&scenario.parity_checkpoints[0].screenshot.path)).expect("read reducer screenshot");
      let exact = compare_exact_static_pngs(&screenshot, &screenshot, &layout, 3).expect("exact self-compare canonical screenshot");
      assert!(exact.accepted, "{} exact canonical self-comparison did not pass", scenario.id);
      let report = reduce_normalized_png_visual_parity(&screenshot, &screenshot, &layout, 3, Some(&matching_text()), VisualThresholds::default()).expect("self-compare canonical screenshot");
      assert!(report.accepted, "{} canonical self-comparison did not pass", scenario.id);
   }
   let pending = scenarios.iter().flat_map(|(_, scenario)| scenario.parity_checkpoints.iter().filter_map(|checkpoint| checkpoint.recapture_status.as_deref().map(|status| (scenario.id.as_str(), checkpoint.id.as_str(), status)))).collect::<Vec<_>>();
   assert!(pending.is_empty());

   let style = artifact_json(&spec_root, &scenarios[0].1.scene.style_tokens.path);
   assert_eq!(style["text_pixel_policy"], "normalized-srgb8-exact-static-full-frame");

   let startup = artifact_json(&spec_root, &scenarios[0].1.fixture.path);
   assert_eq!(startup["card_count"], 24);
   assert_eq!(startup["data_size_bytes"], 24 * 1_024);
   let chat = artifact_json(&spec_root, &scenarios[3].1.fixture.path);
   assert_eq!(chat["message_count"], 5_000);
   assert_eq!(chat["avatar_count"], 64);
   let image = artifact_json(&spec_root, &scenarios[5].1.fixture.path);
   assert_eq!(image["source"]["width"], 4_096);
   assert_eq!(image["source"]["height"], 3_072);

   let dashboard = artifact_json(&spec_root, &scenarios[1].1.scene.layout_assertions.path);
   assert_eq!(dashboard["grid"]["card_height"], 38);
   assert!(dashboard["backdrop_regions"].as_array().expect("dashboard backdrop regions").iter().all(|region| region[3] == 96));

   let navigation = &scenarios[4].1;
   let checkpoint_times = navigation.parity_checkpoints.iter()
      .filter(|checkpoint| checkpoint.id.starts_with("modal-"))
      .map(|checkpoint| checkpoint.at_us.expect("navigation modal checkpoint timestamp"))
      .collect::<Vec<_>>();
   assert_eq!(checkpoint_times, vec![325_000, 400_000, 475_000, 550_000, 625_000]);
}

fn assert_canonical_png(bytes: &[u8])
{
   let decoder = png::Decoder::new(Cursor::new(bytes));
   let mut reader = decoder.read_info().expect("decode checkpoint PNG header");
   assert_eq!((reader.info().width, reader.info().height), (1_170, 2_532));
   assert!(reader.info().srgb.is_some(), "checkpoint PNG must declare sRGB");
   let mut pixels = vec![0; reader.output_buffer_size()];
   let output = reader.next_frame(&mut pixels).expect("decode checkpoint PNG pixels");
   assert_eq!(output.color_type, png::ColorType::Rgba);
   assert!(pixels[..output.buffer_size()].chunks_exact(4).all(|pixel| pixel[3] == 255), "checkpoint PNG contains nonopaque pixels");
}

fn matching_text() -> TextGeometryEvidence
{
   let line = TextLineGeometry {
      baseline_y: 20.0,
      ink_bounds: LogicalRect {x: 16.0, y: 12.0, width: 120.0, height: 12.0},
   };
   TextGeometryEvidence {reference_lines: vec![line], candidate_lines: vec![line]}
}

fn artifact_json(root: &Path, relative: &str) -> Value
{
   serde_json::from_slice(&fs::read(root.join(relative)).expect("read JSON artifact")).expect("parse JSON artifact")
}
