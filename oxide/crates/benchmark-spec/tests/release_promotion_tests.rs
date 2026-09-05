use std::fs;
use std::path::{Path, PathBuf};

use oxide_benchmark_spec::{promote_release_candidates, APPLE_RELEASE_SCENARIO_IDS, RELEASE_CANDIDATE_IDS};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

#[test]
fn missing_evidence_fails_without_publishing_output()
{
   let fixture = Fixture::new();
   fs::remove_file(fixture.evidence.join("oxide.evidence").join(RELEASE_CANDIDATE_IDS[0]).join("initial/screenshot.actual.png")).expect("remove required evidence");
   let result = promote_release_candidates(&fixture.spec, &fixture.evidence, &fixture.output, 3);
   assert!(result.is_err());
   assert!(!fixture.output.exists());
}

#[test]
fn invalid_visual_evidence_fails_without_publishing_output()
{
   let fixture = Fixture::new();
   let changed = png([255, 0, 0]);
   replace_runtime_screenshot(&fixture.evidence, "oxide.evidence", RELEASE_CANDIDATE_IDS[0], "initial", &changed);
   let result = promote_release_candidates(&fixture.spec, &fixture.evidence, &fixture.output, 3);
   assert!(result.is_err());
   assert!(!fixture.output.exists());
}

#[test]
fn noncanonical_runtime_geometry_profile_fails_without_publishing_output()
{
   let fixture = Fixture::new();
   let geometry = serde_json::to_vec(&json!({
      "schema_version": 1,
      "coordinate_space": "logical-points",
      "capture_profile": "invented-profile",
      "canonical_scale": 3,
      "source": "appkit-view-tree",
      "position_tolerance_points": 0,
      "size_tolerance_points": 0,
      "root": {"x": 0, "y": 0, "width": 8, "height": 8},
      "nodes": [{"ordinal": 0, "kind": "text", "role": "heading", "identifier": "title", "bounds": {"x": 1, "y": 1, "width": 6, "height": 2}, "text_line_bounds": [{"x": 1, "y": 1, "width": 6, "height": 2}]}]
   })).expect("encode invalid geometry");
   for evidence_name in ["native.evidence", "oxide.evidence"]
   {
      replace_runtime_geometry(&fixture.evidence, evidence_name, RELEASE_CANDIDATE_IDS[0], "initial", &geometry);
   }
   let error = promote_release_candidates(&fixture.spec, &fixture.evidence, &fixture.output, 3).expect_err("noncanonical runtime profile must fail");
   assert!(error.to_string().contains("runtime geometry"));
   assert!(!fixture.output.exists());
}

#[test]
fn valid_evidence_atomically_publishes_all_five_runnable_manifests()
{
   let fixture = Fixture::new();
   let report = promote_release_candidates(&fixture.spec, &fixture.evidence, &fixture.output, 3).expect("promote complete evidence");
   assert_eq!(report.scenarios.len(), 5);
   for id in RELEASE_CANDIDATE_IDS
   {
      let manifest = fixture.output.join("scenarios").join(format!("{id}.json"));
      let value = serde_json::from_slice::<Value>(&fs::read(manifest).expect("read promoted manifest")).expect("parse promoted manifest");
      assert_eq!(value["id"], id);
      assert_eq!(value["parity_checkpoints"][0]["screenshot"]["sha256"], sha256(&png([32, 64, 96])));
      assert_eq!(value["parity_checkpoints"][0]["geometry"]["sha256"], report.scenarios.iter().find(|scenario| scenario.id == id).expect("scenario report").checkpoints[0].geometry_sha256);
      assert!(fixture.output.join(value["parity_checkpoints"][0]["geometry"]["path"].as_str().expect("geometry path")).is_file());
   }
   assert!(fixture.output.join("release-candidates/promotion-report.json").is_file());
   assert!(fixture.output.join(&report.qualification.execution_plan.path).is_file());
   assert!(fixture.output.join(&report.qualification.qualification_plan.path).is_file());
   assert_eq!(report.qualification.scale_overlays.len(), APPLE_RELEASE_SCENARIO_IDS.len() * 2);
}

#[test]
fn valid_evidence_refreshes_an_already_promoted_spec_tree()
{
   let fixture = Fixture::new();
   promote_release_candidates(&fixture.spec, &fixture.evidence, &fixture.output, 3).expect("initial promotion");
   let refreshed = fixture._temporary.path().join("refreshed");
   let report = promote_release_candidates(&fixture.output, &fixture.evidence, &refreshed, 3).expect("refresh promoted spec");

   assert_eq!(report.scenarios.len(), RELEASE_CANDIDATE_IDS.len());
   assert!(refreshed.join("release-candidates/promotion-report.json").is_file());
   assert!(refreshed.join(&report.qualification.execution_plan.path).is_file());
}

struct Fixture
{
   _temporary: TempDir,
   spec: PathBuf,
   evidence: PathBuf,
   output: PathBuf,
}

impl Fixture
{
   fn new() -> Self
   {
      let temporary = tempfile::tempdir().expect("create fixture root");
      let spec = temporary.path().join("spec");
      let evidence = temporary.path().join("evidence");
      let output = temporary.path().join("promoted");
      fs::create_dir(&spec).expect("create spec root");
      fs::create_dir(&evidence).expect("create evidence root");
      for directory in ["release-candidates", "fixtures", "assets", "font-packs", "styles", "layout", "checkpoints", "scenarios"]
      {
         fs::create_dir(spec.join(directory)).expect("create spec directory");
      }
      write(&spec, "fixtures/shared.json", b"{}\n");
      write(&spec, "assets/shared.json", b"{}\n");
      write(&spec, "font-packs/shared.json", b"{}\n");
      write(&spec, "styles/shared.json", b"{}\n");
      write(&spec, "layout/shared.json", b"{}\n");
      let state = serde_json::to_vec(&json!({"checkpoint_id": "initial"})).expect("encode state");
      let accessibility = serde_json::to_vec(&json!({"nodes": [{"count": 1, "role": "surface"}]})).expect("encode accessibility");
      let native_geometry = serde_json::to_vec(&json!({
         "schema_version": 1,
         "coordinate_space": "logical-points",
         "capture_profile": "macos-canonical-srgb8-3x-v1",
         "canonical_scale": 3,
         "source": "appkit-view-tree",
         "position_tolerance_points": 0,
         "size_tolerance_points": 0,
         "root": {"x": 0, "y": 0, "width": 8, "height": 8},
         "nodes": [{"ordinal": 0, "kind": "text", "role": "heading", "identifier": "title", "bounds": {"x": 1, "y": 1, "width": 6, "height": 2}, "text_line_bounds": [{"x": 1, "y": 1, "width": 6, "height": 2}]}]
      })).expect("encode native geometry");
      let oxide_geometry = serde_json::to_vec(&json!({
         "schema_version": 1,
         "coordinate_space": "logical-points",
         "capture_profile": "macos-canonical-srgb8-3x-v1",
         "canonical_scale": 3,
         "source": "oxide-semantic-tree",
         "position_tolerance_points": 0,
         "size_tolerance_points": 0,
         "root": {"x": 0, "y": 0, "width": 8, "height": 8},
         "nodes": [{"ordinal": 0, "kind": "text", "role": "heading", "identifier": "title", "bounds": {"x": 1, "y": 1, "width": 6, "height": 2}, "text_line_bounds": [{"x": 1, "y": 1, "width": 6, "height": 2}]}]
      })).expect("encode Oxide geometry");
      let image = png([32, 64, 96]);
      write(&spec, "checkpoints/shared.png", &image);
      for id in RELEASE_CANDIDATE_IDS
      {
         let checkpoint_root = format!("checkpoints/{id}/initial");
         write(&spec, &format!("{checkpoint_root}/state.json"), &state);
         write(&spec, &format!("{checkpoint_root}/accessibility.json"), &accessibility);
         let candidate = json!({
            "assets": artifact("assets/shared.json", &spec),
            "candidate_status": "blocked-on-canonical-screenshots",
            "expected_visible_role_counts_by_checkpoint": {"initial": [{"count": 1, "role": "surface"}]},
            "fixture": artifact("fixtures/shared.json", &spec),
            "font_pack": {"id": "test-fonts", "manifest": "font-packs/shared.json", "sha256": hash_file(&spec.join("font-packs/shared.json"))},
            "id": id,
            "owning_pass": "minimal-presentation",
            "parity_checkpoints": [{
               "accessibility": artifact(&format!("{checkpoint_root}/accessibility.json"), &spec),
               "id": "initial",
               "phase_id": "measure",
               "state": artifact(&format!("{checkpoint_root}/state.json"), &spec)
            }],
            "phases": [{"duration_ms": 1, "id": "measure", "measured": true}],
            "primary_metric": "candidate.primary",
            "scene": {"layout_assertions": artifact("layout/shared.json", &spec), "roles": ["surface"], "style_tokens": artifact("styles/shared.json", &spec)},
            "schema_version": 1,
            "screenshot_materialization": {"required": true, "scenario_manifest_path": format!("scenarios/{id}.json"), "status": "blocked", "unblocker": "canonical evidence"},
            "viewport_classes": ["test"],
            "within_session_estimator": "p50"
         });
         let mut bytes = serde_json::to_vec(&candidate).expect("encode candidate");
         bytes.push(b'\n');
         write(&spec, &format!("release-candidates/{id}.candidate.json"), &bytes);
         write_runtime_checkpoint(&evidence, "native.evidence", id, "initial", &state, &accessibility, &native_geometry, &image);
         write_runtime_checkpoint(&evidence, "oxide.evidence", id, "initial", &state, &accessibility, &oxide_geometry, &image);
      }
      for id in APPLE_RELEASE_SCENARIO_IDS
      {
         if RELEASE_CANDIDATE_IDS.contains(id) {continue}
         let scenario = json!({
            "schema_version": 1,
            "id": id,
            "fixture": artifact("fixtures/shared.json", &spec),
            "assets": artifact("assets/shared.json", &spec),
            "font_pack": {"id": "test-fonts", "manifest": "font-packs/shared.json", "sha256": hash_file(&spec.join("font-packs/shared.json"))},
            "viewport_class": "test",
            "scene": {"roles": ["surface"], "style_tokens": artifact("styles/shared.json", &spec), "layout_assertions": artifact("layout/shared.json", &spec)},
            "phases": [{"id": "measure", "measured": true, "duration_ms": 1}],
            "primary_metric": "frame.present_ms",
            "required_metrics": ["first.interactive_ms"],
            "optional_metrics": [],
            "parity_checkpoints": [{
               "id": "initial",
               "phase_id": "measure",
               "state": artifact("fixtures/shared.json", &spec),
               "accessibility": artifact("assets/shared.json", &spec),
               "screenshot": artifact("checkpoints/shared.png", &spec),
               "expected_visible_role_counts": [{"count": 1, "role": "surface"}]
            }],
            "fairness_contract": {
               "locale": "en_US_POSIX",
               "timezone": "UTC",
               "direction": "ltr",
               "logical_viewport_width": 8,
               "logical_viewport_height": 8,
               "expected_visible_role_counts": [{"count": 1, "role": "surface"}],
               "schedule_tolerance_us": 1_000,
               "coordinate_tolerance_microunits": 1_000,
               "elapsed_time_driven": true
            }
         });
         let mut bytes = serde_json::to_vec_pretty(&scenario).expect("encode runnable scenario");
         bytes.push(b'\n');
         write(&spec, &format!("scenarios/{id}.json"), &bytes);
      }
      Self {_temporary: temporary, spec, evidence, output}
   }
}

fn artifact(path: &str, root: &Path) -> Value
{
   json!({"path": path, "sha256": hash_file(&root.join(path))})
}

fn hash_file(path: &Path) -> String
{
   sha256(&fs::read(path).expect("read hashed fixture"))
}

fn sha256(bytes: &[u8]) -> String
{
   format!("{:x}", Sha256::digest(bytes))
}

fn write(root: &Path, relative: &str, bytes: &[u8])
{
   let path = root.join(relative);
   fs::create_dir_all(path.parent().expect("fixture path parent")).expect("create fixture parent");
   fs::write(path, bytes).expect("write fixture")
}

fn write_runtime_checkpoint(root: &Path, evidence_name: &str, scenario_id: &str, checkpoint_id: &str, state: &[u8], accessibility: &[u8], geometry: &[u8], screenshot: &[u8])
{
   let directory = root.join(evidence_name).join(scenario_id).join(checkpoint_id);
   fs::create_dir_all(&directory).expect("create runtime evidence checkpoint");
   fs::write(directory.join("state.actual.json"), state).expect("write runtime state");
   fs::write(directory.join("accessibility.actual.json"), accessibility).expect("write runtime accessibility");
   fs::write(directory.join("geometry.actual.json"), geometry).expect("write runtime geometry");
   fs::write(directory.join("screenshot.actual.png"), screenshot).expect("write runtime screenshot");
   let identity_root = Path::new("Runs/test/correctness/correctness/0").join(evidence_name).join(scenario_id).join(checkpoint_id);
   fs::write(directory.join("evidence.json"), serde_json::to_vec(&json!({
      "actualState": {"path": identity_root.join("state.actual.json"), "sha256": sha256(state)},
      "actualAccessibility": {"path": identity_root.join("accessibility.actual.json"), "sha256": sha256(accessibility)},
      "actualGeometry": {"path": identity_root.join("geometry.actual.json"), "sha256": sha256(geometry)},
      "actualScreenshot": {"path": identity_root.join("screenshot.actual.png"), "sha256": sha256(screenshot)},
      "validation": "exact-canonical-state-accessibility-and-role-counts"
   })).expect("runtime evidence JSON")).expect("write runtime evidence JSON");
}

fn replace_runtime_screenshot(root: &Path, evidence_name: &str, scenario_id: &str, checkpoint_id: &str, screenshot: &[u8])
{
   let directory = root.join(evidence_name).join(scenario_id).join(checkpoint_id);
   fs::write(directory.join("screenshot.actual.png"), screenshot).expect("replace runtime screenshot");
   let evidence_path = directory.join("evidence.json");
   let mut evidence = serde_json::from_slice::<Value>(&fs::read(&evidence_path).expect("read runtime evidence JSON")).expect("parse runtime evidence JSON");
   evidence["actualScreenshot"]["sha256"] = Value::String(sha256(screenshot));
   fs::write(evidence_path, serde_json::to_vec(&evidence).expect("encode runtime evidence JSON")).expect("update runtime evidence JSON");
}

fn replace_runtime_geometry(root: &Path, evidence_name: &str, scenario_id: &str, checkpoint_id: &str, geometry: &[u8])
{
   let directory = root.join(evidence_name).join(scenario_id).join(checkpoint_id);
   fs::write(directory.join("geometry.actual.json"), geometry).expect("replace runtime geometry");
   let evidence_path = directory.join("evidence.json");
   let mut evidence = serde_json::from_slice::<Value>(&fs::read(&evidence_path).expect("read runtime evidence JSON")).expect("parse runtime evidence JSON");
   evidence["actualGeometry"]["sha256"] = Value::String(sha256(geometry));
   fs::write(evidence_path, serde_json::to_vec(&evidence).expect("encode runtime evidence JSON")).expect("update runtime evidence JSON");
}

fn png(rgb: [u8; 3]) -> Vec<u8>
{
   let mut bytes = Vec::new();
   {
      let mut encoder = png::Encoder::new(&mut bytes, 24, 24);
      encoder.set_color(png::ColorType::Rgb);
      encoder.set_depth(png::BitDepth::Eight);
      encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
      let mut writer = encoder.write_header().expect("write PNG header");
      writer.write_image_data(&rgb.repeat(576)).expect("write PNG pixels");
   }
   bytes
}
