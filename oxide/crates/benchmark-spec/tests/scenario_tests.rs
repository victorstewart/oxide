use oxide_benchmark_spec::{canonical_font_pack_json, canonical_scenario_json, validate_font_pack_manifest, validate_scenario, validate_scenario_artifacts, validate_trace, ArtifactIdentity, AssetFile, AssetManifest, FontPackFile, FontPackManifest, FontVariationAxis, InlineTextAsset, InlineTextAtlas, InlineTextAtlasVariant, ScenarioSpec, TraceEvent, TraceOperation, TraceValue};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn scenario_fixture() -> (Vec<u8>, ScenarioSpec)
{
   let path = Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("../..")
      .join("benchmarks/comparative/specs/v1/fixtures/scenario-v1.json");
   let bytes = fs::read(path).expect("read scenario fixture");
   let scenario = serde_json::from_slice(&bytes).expect("parse scenario fixture");
   (bytes, scenario)
}

#[test]
fn committed_font_pack_has_exact_pinned_bytes_and_licenses()
{
   let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join("benchmarks/comparative/specs/v1");
   let bytes = fs::read(root.join("font-packs/oxide-bench-fonts-v1.json")).expect("read committed font pack");
   let manifest = serde_json::from_slice::<FontPackManifest>(&bytes).expect("parse committed font pack");
   validate_font_pack_manifest(&root, &manifest, "oxide-bench-fonts-v1").expect("validate committed font pack");
   assert_eq!(bytes, canonical_font_pack_json(&manifest).expect("canonical font pack JSON"));
   assert_eq!(manifest.fonts.iter().map(|font| font.role.as_str()).collect::<Vec<_>>(), ["latin", "arabic", "cjk-simplified"]);
   assert_eq!(manifest.fonts[0].variation_axes, [FontVariationAxis {tag: String::from("wght"), value_millionths: 400_000_000}, FontVariationAxis {tag: String::from("wdth"), value_millionths: 100_000_000}]);
   assert_eq!(manifest.fonts[1].variation_axes, manifest.fonts[0].variation_axes);
   assert_eq!(manifest.fonts[2].variation_axes, [FontVariationAxis {tag: String::from("wght"), value_millionths: 400_000_000}]);
}

#[test]
fn font_pack_rejects_incomplete_reordered_duplicate_malformed_and_out_of_range_axes()
{
   let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join("benchmarks/comparative/specs/v1");
   let manifest_bytes = fs::read(root.join("font-packs/oxide-bench-fonts-v1.json")).expect("read committed font pack");
   let manifest = serde_json::from_slice::<FontPackManifest>(&manifest_bytes).expect("parse committed font pack");

   let mut incomplete = manifest.clone();
   incomplete.fonts[0].variation_axes.pop();
   assert!(validate_font_pack_manifest(&root, &incomplete, "oxide-bench-fonts-v1").expect_err("incomplete axes must fail").to_string().contains("axis count"));

   let mut reordered = manifest.clone();
   reordered.fonts[0].variation_axes.swap(0, 1);
   assert!(validate_font_pack_manifest(&root, &reordered, "oxide-bench-fonts-v1").expect_err("reordered axes must fail").to_string().contains("fvar order"));

   let mut duplicate = manifest.clone();
   duplicate.fonts[0].variation_axes[1].tag = String::from("wght");
   assert!(validate_font_pack_manifest(&root, &duplicate, "oxide-bench-fonts-v1").expect_err("duplicate axes must fail").to_string().contains("duplicated"));

   let mut malformed = manifest.clone();
   malformed.fonts[0].variation_axes[0].tag = String::from("wg\nh");
   assert!(validate_font_pack_manifest(&root, &malformed, "oxide-bench-fonts-v1").expect_err("malformed axis tag must fail").to_string().contains("printable ASCII"));

   let mut out_of_range = manifest;
   out_of_range.fonts[0].variation_axes[0].value_millionths = 901_000_000;
   assert!(validate_font_pack_manifest(&root, &out_of_range, "oxide-bench-fonts-v1").expect_err("out-of-range axis must fail").to_string().contains("outside fvar range"));
}

#[test]
fn frozen_scenario_v1_round_trips_with_canonical_key_order()
{
   let (bytes, scenario) = scenario_fixture();
   validate_scenario(&scenario).expect("validate scenario fixture");
   assert_eq!(bytes, canonical_scenario_json(&scenario).expect("canonical scenario JSON"));
   assert_eq!(scenario.id, "feed.variable-scroll.v1");
   assert_eq!(scenario.phases.len(), 5);
   assert_eq!(scenario.parity_checkpoints.len(), 3);
   assert_eq!(scenario.parity_checkpoints[1].at_us, Some(120_000));
   assert_eq!(scenario.primary_metric, "frame.missed_display_opportunities_per_1000");
}

#[test]
fn scenario_validation_rejects_contract_weakening_and_duplicate_metrics()
{
   let (_, mut scenario) = scenario_fixture();
   scenario.fairness_contract.elapsed_time_driven = false;
   assert!(validate_scenario(&scenario).is_err());

   let (_, mut scenario) = scenario_fixture();
   scenario.optional_metrics.push(scenario.primary_metric.clone());
   assert!(validate_scenario(&scenario).is_err());

   let (_, mut scenario) = scenario_fixture();
   scenario.fixture.sha256 = "A".repeat(64);
   assert!(validate_scenario(&scenario).is_err());

   let (_, mut scenario) = scenario_fixture();
   scenario.parity_checkpoints[0].phase_id = String::from("missing-phase");
   assert!(validate_scenario(&scenario).is_err());

   let (_, mut scenario) = scenario_fixture();
   scenario.parity_checkpoints[0].expected_visible_role_counts[0].role = String::from("undeclared-role");
   assert!(validate_scenario(&scenario).is_err());

   let (_, mut scenario) = scenario_fixture();
   scenario.parity_checkpoints[2].at_us = Some(2_000_001);
   assert!(validate_scenario(&scenario).is_err());

   let (_, mut scenario) = scenario_fixture();
   scenario.scene.roles.push(scenario.scene.roles[0].clone());
   assert!(validate_scenario(&scenario).is_err());

   let (_, mut scenario) = scenario_fixture();
   scenario.parity_checkpoints[0].screenshot.path = String::from("checkpoints/not-a-png.json");
   assert!(validate_scenario(&scenario).is_err());

   let (_, mut scenario) = scenario_fixture();
   scenario.parity_checkpoints[0].recapture_status = Some(String::new());
   assert!(validate_scenario(&scenario).expect_err("empty recapture status must fail").to_string().contains("noncanonical recapture status"));

   let (_, mut scenario) = scenario_fixture();
   scenario.parity_checkpoints[0].recapture_status = Some(String::from(" Headed-Pending "));
   assert!(validate_scenario(&scenario).expect_err("noncanonical recapture status must fail").to_string().contains("noncanonical recapture status"));
}

#[test]
fn typed_trace_preserves_integer_time_coordinates_and_state_values()
{
   let events = vec![
      TraceEvent {
         at_us: 0,
         op: TraceOperation::PointerDown,
         pointer: Some(1),
         x_millionths: Some(500_000),
         y_millionths: Some(780_000),
         delta_x_millionths: None,
         delta_y_millionths: None,
         target: None,
         value: None,
         state_id: Some("drag-started".to_string()),
      },
      TraceEvent {
         at_us: 240_000,
         op: TraceOperation::PointerUp,
         pointer: Some(1),
         x_millionths: Some(500_000),
         y_millionths: Some(180_000),
         delta_x_millionths: None,
         delta_y_millionths: None,
         target: None,
         value: None,
         state_id: Some("drag-ended".to_string()),
      },
      TraceEvent {
         at_us: 3_000_000,
         op: TraceOperation::Mutate,
         pointer: None,
         x_millionths: None,
         y_millionths: None,
         delta_x_millionths: None,
         delta_y_millionths: None,
         target: Some("feed:item:731:favorite".to_string()),
         value: Some(TraceValue::Boolean(true)),
         state_id: Some("item-731-favorite".to_string()),
      },
   ];
   validate_trace(&events).expect("validate typed trace");
   let bytes = serde_json::to_vec(&events).expect("serialize typed trace");
   let decoded = serde_json::from_slice::<Vec<TraceEvent>>(&bytes).expect("deserialize typed trace");
   assert_eq!(decoded, events);
}

#[test]
fn trace_validation_rejects_reordering_and_invalid_pointer_coordinates()
{
   let mut events = vec![
      TraceEvent {
         at_us: 10,
         op: TraceOperation::PointerDown,
         pointer: Some(1),
         x_millionths: Some(500_000),
         y_millionths: Some(500_000),
         delta_x_millionths: None,
         delta_y_millionths: None,
         target: None,
         value: None,
         state_id: None,
      },
      TraceEvent {
         at_us: 9,
         op: TraceOperation::PointerMove,
         pointer: Some(1),
         x_millionths: Some(1_000_001),
         y_millionths: Some(500_000),
         delta_x_millionths: None,
         delta_y_millionths: None,
         target: None,
         value: None,
         state_id: None,
      },
   ];
   assert!(validate_trace(&events).is_err());
   events[1].at_us = 11;
   assert!(validate_trace(&events).is_err());
}

#[test]
fn runnable_scenario_requires_exact_in_root_artifact_bytes_and_valid_traces()
{
   let root = TestRoot::new();
   let (_, mut scenario) = scenario_fixture();
   materialize_artifacts(&root.0, &mut scenario);
   validate_scenario_artifacts(&root.0, &scenario).expect("validate materialized scenario artifacts");

   fs::write(root.0.join(&scenario.fixture.path), b"changed fixture").expect("corrupt fixture");
   assert!(validate_scenario_artifacts(&root.0, &scenario).expect_err("hash mismatch must fail").to_string().contains("SHA-256 mismatch"));

   materialize_artifacts(&root.0, &mut scenario);
   scenario.fixture.path = String::from("../outside.json");
   assert!(validate_scenario_artifacts(&root.0, &scenario).expect_err("path traversal must fail").to_string().contains("traversal"));
}

#[test]
fn inline_text_atlas_requires_pinned_bounded_unique_assets()
{
   let root = TestRoot::new();
   let (_, mut scenario) = scenario_fixture();
   materialize_artifacts(&root.0, &mut scenario);
   let manifest_bytes = fs::read(root.0.join(&scenario.assets.path)).expect("read test asset manifest");
   let mut manifest = serde_json::from_slice::<AssetManifest>(&manifest_bytes).expect("parse test asset manifest");
   let atlas = write_artifact(&root.0, "assets/inline-text.png", b"png");
   let license = write_artifact(&root.0, "assets/OFL-noto-emoji.txt", b"license");
   manifest.artifacts.push(AssetFile {
      role: String::from("inline-text-atlas"),
      artifact: atlas,
      media_type: String::from("image/png"),
      color_space: String::from("srgb"),
   });
   manifest.inline_text_atlas = Some(InlineTextAtlas {
      columns: 2,
      rows: 2,
      source_repository: String::from("https://github.com/googlefonts/noto-emoji"),
      source_commit: String::from("8998f5dd683424a73e2314a8c1f1e359c19e8742"),
      license,
      variants: vec![InlineTextAtlasVariant {
         artifact_role: String::from("inline-text-atlas"),
         pixel_width: 128,
         pixel_height: 128,
         em_pixels: 64,
      }],
      entries: vec![InlineTextAsset {
         grapheme: String::from("🌍"),
         column: 0,
         row: 0,
         advance_millionths: 1_000_000,
         top_from_baseline_millionths: -800_000,
         width_millionths: 1_000_000,
         height_millionths: 1_000_000,
      }],
   });
   scenario.assets = write_artifact(&root.0, "assets/manifest.json", &serde_json::to_vec_pretty(&manifest).expect("serialize inline-text manifest"));
   validate_scenario_artifacts(&root.0, &scenario).expect("validate pinned inline-text atlas");

   manifest.inline_text_atlas.as_mut().expect("inline-text atlas").entries[0].column = 2;
   scenario.assets = write_artifact(&root.0, "assets/manifest.json", &serde_json::to_vec_pretty(&manifest).expect("serialize invalid inline-text manifest"));
   assert!(validate_scenario_artifacts(&root.0, &scenario).expect_err("out-of-bounds inline asset must fail").to_string().contains("exceeds the atlas grid"));
}

fn materialize_artifacts(root: &Path, scenario: &mut ScenarioSpec)
{
   scenario.fixture = write_artifact(root, "fixtures/feed.json", b"fixture");
   let asset_file = write_artifact(root, "assets/thumbnail.png", b"asset");
   let asset_manifest = AssetManifest {
      schema_version: 1,
      id: String::from("test-assets"),
      artifacts: vec![AssetFile {
         role: String::from("thumbnail-atlas"),
         artifact: asset_file,
         media_type: String::from("image/png"),
         color_space: String::from("srgb"),
      }],
      inline_text_atlas: None,
   };
   scenario.assets = write_artifact(root, "assets/manifest.json", &serde_json::to_vec_pretty(&asset_manifest).expect("serialize asset manifest"));
   let committed_font = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join("benchmarks/comparative/specs/v1/font-packs/oxide-bench-fonts-v1/NotoSans-VF.ttf");
   let font_bytes = fs::read(committed_font).expect("read committed variable font fixture");
   let font_file = write_artifact(root, "font-packs/files/regular.ttf", &font_bytes);
   let font_license = write_artifact(root, "font-packs/licenses/OFL.txt", b"license");
   let font_manifest = FontPackManifest {
      schema_version: 1,
      id: scenario.font_pack.id.clone(),
      fonts: vec![FontPackFile {
         role: String::from("latin"),
         artifact: font_file,
         variation_axes: vec![
            FontVariationAxis {tag: String::from("wght"), value_millionths: 400_000_000},
            FontVariationAxis {tag: String::from("wdth"), value_millionths: 100_000_000},
         ],
         source_repository: String::from("https://github.com/example/fonts"),
         source_commit: String::from("0123456789abcdef0123456789abcdef01234567"),
         source_path: String::from("fonts/regular.ttf"),
         license: font_license,
      }],
   };
   let font_manifest_bytes = serde_json::to_vec_pretty(&font_manifest).expect("serialize font manifest");
   let font = write_artifact(root, "font-packs/oxide-bench-fonts-v1.json", &font_manifest_bytes);
   scenario.font_pack.manifest = font.path;
   scenario.font_pack.sha256 = font.sha256;
   scenario.scene.style_tokens = write_artifact(root, "styles/neutral-v1.json", b"styles");
   scenario.scene.layout_assertions = write_artifact(root, "layout/feed.json", b"layout");
   for (index, checkpoint) in scenario.parity_checkpoints.iter_mut().enumerate()
   {
      checkpoint.state = write_artifact(root, &format!("checkpoints/{}/state.json", index), b"state");
      checkpoint.accessibility = write_artifact(root, &format!("checkpoints/{}/accessibility.json", index), b"accessibility");
      checkpoint.screenshot = write_artifact(root, &format!("checkpoints/{}/screenshot.png", index), b"png");
   }
   let trace_bytes = serde_json::to_vec(&[TraceEvent {
      at_us: 0,
      op: TraceOperation::Mutate,
      pointer: None,
      x_millionths: None,
      y_millionths: None,
      delta_x_millionths: None,
      delta_y_millionths: None,
      target: Some(String::from("fixture:ready")),
      value: Some(TraceValue::Boolean(true)),
      state_id: Some(String::from("ready")),
   }]).expect("serialize trace");
   for (index, phase) in scenario.phases.iter_mut().filter(|phase| phase.trace.is_some()).enumerate()
   {
      phase.trace = Some(write_artifact(root, &format!("traces/{}.json", index), &trace_bytes));
   }
}

fn write_artifact(root: &Path, relative: &str, bytes: &[u8]) -> ArtifactIdentity
{
   let path = root.join(relative);
   fs::create_dir_all(path.parent().expect("artifact parent")).expect("create artifact parent");
   fs::write(&path, bytes).expect("write artifact");
   ArtifactIdentity {
      path: String::from(relative),
      sha256: format!("{:x}", Sha256::digest(bytes)),
   }
}

struct TestRoot(std::path::PathBuf);

impl TestRoot
{
   fn new() -> Self
   {
      let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock").as_nanos();
      let path = std::env::temp_dir().join(format!("oxide-benchmark-spec-{}-{}", std::process::id(), nonce));
      fs::create_dir(&path).expect("create test root");
      Self(path)
   }
}

impl Drop for TestRoot
{
   fn drop(&mut self)
   {
      let _ = fs::remove_dir_all(&self.0);
   }
}
