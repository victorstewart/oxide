use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use oxide_feed_v1_reducer::{
   build_evidence_manifest, callback_deadline_counts, deterministic_bootstrap_interval,
   frozen_components, frozen_order_index, strict_validate_failure_json, strict_validate_run_json,
   travel_equivalence_passes, verify_attachment_export, visual_metrics, RgbaImage,
};
use serde_json::{json, Value};

fn valid_run_json() -> Result<Value, String>
{
   run_json(
      "primary-s00-p00-o0-uikit-idiomatic-forward-test",
      "primary",
      0,
      0,
      "uikit-idiomatic",
      "forward",
      0,
   )
}

#[test]
fn travel_equivalence_decision_honors_both_inclusive_frozen_boundaries()
{
   let cases = [
      ("center", 0.0, 0.0, 0.0, true),
      ("positive boundaries", 0.05, -0.10, 0.10, true),
      ("negative median boundary", -0.05, -0.10, 0.10, true),
      ("positive median violation", 0.050_000_001, -0.10, 0.10, false),
      ("negative median violation", -0.050_000_001, -0.10, 0.10, false),
      ("lower confidence violation", 0.0, -0.100_000_001, 0.05, false),
      ("upper confidence violation", 0.0, -0.05, 0.100_000_001, false),
   ];
   for (name, median, lower, upper, expected) in cases
   {
      assert_eq!(travel_equivalence_passes(median, lower, upper), expected, "{name}");
   }
}

#[test]
fn strict_success_schema_round_trips_and_rejects_unknown_fields() -> Result<(), String>
{
   let valid = valid_run_json()?;
   let bytes = serde_json::to_vec(&valid).map_err(|error| error.to_string())?;
   strict_validate_run_json(&bytes)?;

   let mut invalid = valid;
   let object = invalid.as_object_mut().ok_or_else(|| "run fixture is not an object".to_string())?;
   object.insert("unfrozen_extra".to_string(), json!(true));
   let bytes = serde_json::to_vec(&invalid).map_err(|error| error.to_string())?;
   assert!(strict_validate_run_json(&bytes).is_err());

   let mut invalid_start = valid_run_json()?;
   invalid_start["gesture"]["start_offset_points"] = json!(1.0);
   let bytes = serde_json::to_vec(&invalid_start).map_err(|error| error.to_string())?;
   assert!(strict_validate_run_json(&bytes).is_err());

   for (order_index, treatment) in ["uikit-idiomatic", "uikit-optimized", "oxide"].iter().enumerate()
   {
      let mut tiny_travel = valid_run_json()?;
      tiny_travel["run"]["nonce"] = json!(format!("primary-s00-p00-o{order_index}-{treatment}-forward-test"));
      tiny_travel["run"]["order_index"] = json!(order_index);
      tiny_travel["run"]["treatment"] = json!(treatment);
      tiny_travel["gesture"]["end_offset_points"] = json!(100.0);
      tiny_travel["gesture"]["signed_travel_points"] = json!(100.0);
      tiny_travel["gesture"]["travel_distance_points"] = json!(100.0);
      let bytes = serde_json::to_vec(&tiny_travel).map_err(|error| error.to_string())?;
      assert!(strict_validate_run_json(&bytes).is_err());
   }

   let mut transient_environment_change = valid_run_json()?;
   transient_environment_change["environment"]["thermal_state_change_count"] = json!(1);
   let bytes = serde_json::to_vec(&transient_environment_change).map_err(|error| error.to_string())?;
   assert!(strict_validate_run_json(&bytes).is_err());

   let mut drag_only = valid_run_json()?;
   drag_only["gesture"]["inertia_observed"] = json!(false);
   let bytes = serde_json::to_vec(&drag_only).map_err(|error| error.to_string())?;
   assert!(strict_validate_run_json(&bytes).is_err());
   Ok(())
}

#[test]
fn strict_failure_schema_round_trips_and_rejects_unknown_fields() -> Result<(), String>
{
   let mut failure = json!({
      "schema": "oxide.feed-v1.failure",
      "schema_revision": 1,
      "fixture": {
         "schema": "oxide.feed-v1.fixture",
         "revision": 1,
         "canonical_sha256": "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473",
         "canonical_byte_count": 717745
      },
      "nonce": "smoke-s00-p00-o0-oxide-forward-test",
      "treatment": "oxide",
      "stage": "launch",
      "message": "deliberate test failure"
   });
   strict_validate_failure_json(&serde_json::to_vec(&failure).map_err(|error| error.to_string())?)?;
   failure["nonce"] = Value::Null;
   strict_validate_failure_json(&serde_json::to_vec(&failure).map_err(|error| error.to_string())?)?;
   failure["nonce"] = json!("smoke-s00-p00-o0-oxide-forward-test");
   failure.as_object_mut().ok_or_else(|| "failure fixture is not an object".to_string())?
      .insert("unfrozen_extra".to_string(), json!(true));
   assert!(strict_validate_failure_json(&serde_json::to_vec(&failure).map_err(|error| error.to_string())?).is_err());
   Ok(())
}

#[test]
fn frozen_recipe_matches_contract_extent_and_component_states() -> Result<(), String>
{
   let top = frozen_components("top")?;
   let bottom = frozen_components("bottom")?;
   assert!(!top.is_empty());
   assert!(!bottom.is_empty());
   assert_eq!(top[0].id, "feed-v1-row-0000/row");
   assert!(bottom[0].row_index > 1_900);
   assert!(top.iter().all(|component| component.viewport_clip_px.y >= 0));
   assert!(bottom.iter().all(|component| component.viewport_clip_px.y >= 0));
   Ok(())
}

#[test]
fn visual_gate_rejects_one_corrupt_48_pixel_tile() -> Result<(), String>
{
   let reference = solid_rgba_image(96, 96, [247, 244, 238, 255]);
   let mut corrupt = reference.clone();
   for y in 48 .. 96
   {
      for x in 48 .. 96
      {
         let index = ((y * corrupt.width + x) * 4) as usize;
         corrupt.pixels[index .. index + 4].copy_from_slice(&[0, 0, 0, 255]);
      }
   }
   let exact = visual_metrics(&reference, &reference)?;
   let mutated = visual_metrics(&reference, &corrupt)?;
   assert!(exact.passes);
   assert_eq!(exact.ssim, 1.0);
   assert_eq!(exact.worst_tile_rgb_mae, 0.0);
   assert_eq!(exact.exact_rgb_mae, 0.0);
   assert!(!mutated.passes);
   assert!(mutated.worst_tile_rgb_mae > 18.0);
   Ok(())
}

#[test]
fn clustered_bootstrap_is_seeded_and_deterministic() -> Result<(), String>
{
   let deltas = [-0.02, -0.01, 0.0, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06];
   let first = deterministic_bootstrap_interval(&deltas)?;
   let second = deterministic_bootstrap_interval(&deltas)?;
   assert_eq!(first, second);
   assert!(first.0 <= first.1);
   Ok(())
}

#[test]
fn frozen_order_maps_each_treatment_and_both_directions_share_it() -> Result<(), String>
{
   assert_eq!(frozen_order_index("smoke", 0, 0, "uikit-idiomatic")?, 0);
   assert_eq!(frozen_order_index("smoke", 0, 0, "uikit-optimized")?, 1);
   assert_eq!(frozen_order_index("smoke", 0, 0, "oxide")?, 2);
   assert_eq!(frozen_order_index("primary", 0, 1, "uikit-optimized")?, 0);
   assert_eq!(frozen_order_index("primary", 0, 1, "oxide")?, 1);
   assert_eq!(frozen_order_index("primary", 0, 1, "uikit-idiomatic")?, 2);
   Ok(())
}

#[test]
fn deadline_formula_uses_previous_callback_target_period() -> Result<(), String>
{
   let counts = callback_deadline_counts(&[(10.0, 10.008), (10.016, 10.032)])?;
   assert_eq!(counts, (1, 2));
   Ok(())
}

#[test]
fn callback_admission_rejects_nonfinite_and_terminal_invalid_targets()
{
   let mut samples: Vec<(f64, f64)> = (0 .. 21).map(|index| {
      let timestamp = 10.0 + f64::from(index) / 120.0;
      (timestamp, timestamp + 1.0 / 120.0)
   }).collect();
   samples[0].1 = f64::NAN;
   assert!(callback_deadline_counts(&samples).is_err());

   samples[0].1 = samples[0].0 + 1.0 / 120.0;
   let last = samples.len() - 1;
   samples[last].1 = samples[last].0;
   assert!(callback_deadline_counts(&samples).is_err());

   let sixty_hz: Vec<(f64, f64)> = (0 .. 21).map(|index| {
      let timestamp = 10.0 + f64::from(index) / 60.0;
      (timestamp, timestamp + 1.0 / 60.0)
   }).collect();
   let error = callback_deadline_counts(&sixty_hz).unwrap_err();
   assert!(error.contains("inside 7.5-9.2 ms"));
}
#[test]
fn attachment_export_verifier_requires_one_complete_manifest() -> Result<(), String>
{
   let root = temporary_root("attachment-export")?;
   fs::create_dir_all(&root).map_err(|error| error.to_string())?;
   let mut attachments = Vec::new();
   for index in 0 .. 6
   {
      let export = format!("exported-{index}.png");
      fs::write(root.join(&export), format!("png-{index}")).map_err(|error| error.to_string())?;
      attachments.push(json!({
         "suggestedHumanReadableName": format!("feed-v1-test-{index}_0_01234567-89AB-CDEF-0123-456789ABCDEF.png"),
         "exportedFileName": export
      }));
   }
   let manifest = json!([{
      "testIdentifier": "FeedV1ControllerTests/testFeedV1PhysicalDevicePilot()",
      "attachments": attachments
   }]);
   fs::write(
      root.join("manifest.json"),
      serde_json::to_vec(&manifest).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   verify_attachment_export(&root)?;

   let mut malformed = manifest.clone();
   malformed[0]["attachments"][0]["suggestedHumanReadableName"] = json!("feed-v1-test-0_0_not-a-uuid.png");
   fs::write(
      root.join("manifest.json"),
      serde_json::to_vec(&malformed).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_attachment_export(&root).unwrap_err().contains("malformed Xcode suffix"));

   let mut duplicate_canonical = manifest.clone();
   duplicate_canonical[0]["attachments"][1]["suggestedHumanReadableName"] =
      json!("feed-v1-test-0.png");
   fs::write(
      root.join("manifest.json"),
      serde_json::to_vec(&duplicate_canonical).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_attachment_export(&root).unwrap_err().contains("duplicate canonical"));

   let mut aliased = manifest.clone();
   aliased[0]["attachments"][1]["exportedFileName"] = json!("exported-0.png");
   fs::write(
      root.join("manifest.json"),
      serde_json::to_vec(&aliased).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_attachment_export(&root).unwrap_err().contains("alias"));

   let mut foreign = manifest.clone();
   foreign[0]["testIdentifier"] = json!("ForeignTests/testUnexpected()");
   fs::write(
      root.join("manifest.json"),
      serde_json::to_vec(&foreign).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_attachment_export(&root).unwrap_err().contains("test identifier"));

   let mut multiple_details = manifest.as_array().cloned()
      .ok_or_else(|| "test attachment manifest is not an array".to_string())?;
   multiple_details.push(manifest[0].clone());
   fs::write(
      root.join("manifest.json"),
      serde_json::to_vec(&multiple_details).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_attachment_export(&root).unwrap_err().contains("2 test details, expected 1"));

   let escape = root.with_file_name(format!(
      "{}-escape.png",
      root.file_name().and_then(|name| name.to_str()).ok_or_else(|| "test root has no name".to_string())?
   ));
   fs::write(&escape, b"escape").map_err(|error| error.to_string())?;
   let escape_name = format!("../{}", escape.file_name().and_then(|name| name.to_str())
      .ok_or_else(|| "escape file has no name".to_string())?);
   let mut escaped = manifest.clone();
   escaped[0]["attachments"][0]["exportedFileName"] = json!(escape_name);
   fs::write(
      root.join("manifest.json"),
      serde_json::to_vec(&escaped).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_attachment_export(&root).unwrap_err().contains("leaves the export root"));
   fs::remove_file(escape).map_err(|error| error.to_string())?;
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[cfg(unix)]
#[test]
fn attachment_export_rejects_hard_link_file_identity_aliases() -> Result<(), String>
{
   let root = temporary_root("attachment-hard-link")?;
   fs::create_dir_all(&root).map_err(|error| error.to_string())?;
   fs::write(root.join("exported-0.png"), b"png-0").map_err(|error| error.to_string())?;
   fs::hard_link(root.join("exported-0.png"), root.join("exported-1.png"))
      .map_err(|error| error.to_string())?;
   let mut attachments = Vec::new();
   for index in 0 .. 6
   {
      if index >= 2
      {
         fs::write(root.join(format!("exported-{index}.png")), format!("png-{index}"))
            .map_err(|error| error.to_string())?;
      }
      attachments.push(json!({
         "suggestedHumanReadableName": format!("feed-v1-hard-link-{index}.png"),
         "exportedFileName": format!("exported-{index}.png")
      }));
   }
   fs::write(
      root.join("manifest.json"),
      serde_json::to_vec(&json!([{
         "testIdentifier": "FeedV1ControllerTests/testFeedV1PhysicalDevicePilot()",
         "attachments": attachments
      }])).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_attachment_export(&root).unwrap_err().contains("file identity"));
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[test]
fn evidence_manifest_excludes_stale_reports_results_and_build_outputs() -> Result<(), String>
{
   let root = temporary_root("manifest-allowlist")?;
   let output_root = temporary_root("manifest-output")?;
   let source = root.join("oxide/benchmarks/pilots/feed-v1");
   let font_root = root.join("oxide/crates/ui-core/assets");
   let uikit_app = root.join("apps/UIKit.app");
   let oxide_app = root.join("apps/Oxide.app");
   let controller_runner = root.join("apps/FeedV1Controller-Runner.app");
   let controller_xctest = controller_runner.join("PlugIns/FeedV1Controller.xctest");
   fs::create_dir_all(source.join("ios/target")).map_err(|error| error.to_string())?;
   fs::create_dir_all(source.join("ios/raw")).map_err(|error| error.to_string())?;
   fs::create_dir_all(source.join("ios/feed-v1-smoke.xcresult")).map_err(|error| error.to_string())?;
   fs::create_dir_all(source.join("reducer/src")).map_err(|error| error.to_string())?;
   fs::create_dir_all(source.join("reducer/target")).map_err(|error| error.to_string())?;
   fs::create_dir_all(&font_root).map_err(|error| error.to_string())?;
   fs::create_dir_all(&uikit_app).map_err(|error| error.to_string())?;
   fs::create_dir_all(&oxide_app).map_err(|error| error.to_string())?;
   fs::create_dir_all(&controller_xctest).map_err(|error| error.to_string())?;
   fs::create_dir_all(&output_root).map_err(|error| error.to_string())?;

   fs::write(source.join("protocol.md"), b"protocol").map_err(|error| error.to_string())?;
   fs::write(source.join("ios/source.swift"), b"source").map_err(|error| error.to_string())?;
   fs::write(source.join("reducer/Cargo.toml"), b"[package]").map_err(|error| error.to_string())?;
   fs::write(source.join("reducer/src/lib.rs"), b"source").map_err(|error| error.to_string())?;
   fs::write(source.join("latest.json"), b"stale").map_err(|error| error.to_string())?;
   fs::write(source.join("latest.md"), b"stale").map_err(|error| error.to_string())?;
   fs::write(source.join("ios/latest.md"), b"stale").map_err(|error| error.to_string())?;
   fs::write(source.join("ios/evidence-manifest.json"), b"stale").map_err(|error| error.to_string())?;
   fs::write(source.join("ios/target/poison.rs"), b"stale").map_err(|error| error.to_string())?;
   fs::write(source.join("reducer/target/poison.rs"), b"stale").map_err(|error| error.to_string())?;
   fs::write(source.join("ios/raw/README.md"), b"stale").map_err(|error| error.to_string())?;
   fs::write(source.join("ios/feed-v1-smoke.xcresult/poison.rs"), b"stale").map_err(|error| error.to_string())?;
   fs::write(&uikit_app.join("UIKit"), b"uikit").map_err(|error| error.to_string())?;
   fs::write(&oxide_app.join("Oxide"), b"oxide").map_err(|error| error.to_string())?;
   fs::write(controller_runner.join("FeedV1Controller-Runner"), b"runner")
      .map_err(|error| error.to_string())?;
   fs::write(controller_xctest.join("FeedV1Controller"), b"xctest")
      .map_err(|error| error.to_string())?;

   let repository_fonts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../crates/ui-core/assets");
   fs::copy(repository_fonts.join("Asap-Regular.ttf"), font_root.join("Asap-Regular.ttf"))
      .map_err(|error| error.to_string())?;
   fs::copy(repository_fonts.join("Asap-Bold.ttf"), font_root.join("Asap-Bold.ttf"))
      .map_err(|error| error.to_string())?;

   git(&root, &["init", "-b", "main"])?;
   git(&root, &["config", "user.name", "Feed V1 Test"])?;
   git(&root, &["config", "user.email", "feed-v1-test@example.invalid"])?;
   git(&root, &["add", "-f", "."])?;
   git(&root, &["commit", "-m", "frozen feed-v1 test source"])?;

   let output = output_root.join("evidence-manifest.json");
   oxide_feed_v1_reducer::build_evidence_manifest(
      &source,
      &root,
      &uikit_app,
      &oxide_app,
      &controller_runner,
      &controller_xctest,
      &output,
   )?;
   let manifest: Value = serde_json::from_slice(&fs::read(&output).map_err(|error| error.to_string())?)
      .map_err(|error| error.to_string())?;
   assert_eq!(manifest["schema_revision"], 2);
   assert_eq!(manifest["repository_ref"], "refs/heads/main");
   assert!(manifest["repository_head_commit"].as_str().is_some_and(|value| value.len() == 40));
   assert!(manifest["repository_tree"].as_str().is_some_and(|value| value.len() == 40));
   assert!(manifest["controller_runner_sha256"].as_str().is_some_and(|value| value.len() == 64));
   assert!(manifest["controller_runner_binary_sha256"].as_str().is_some_and(|value| value.len() == 64));
   assert!(manifest["controller_xctest_sha256"].as_str().is_some_and(|value| value.len() == 64));
   assert!(manifest["controller_xctest_binary_sha256"].as_str().is_some_and(|value| value.len() == 64));
   let source_files = manifest["source_files"].as_object()
      .ok_or_else(|| "manifest has no source_files object".to_string())?;
   let paths: Vec<&str> = source_files.keys().map(String::as_str).collect();
   assert_eq!(paths, vec!["ios/source.swift", "protocol.md", "reducer/Cargo.toml", "reducer/src/lib.rs"]);
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   fs::remove_dir_all(output_root).map_err(|error| error.to_string())?;
   Ok(())
}
fn run_json(nonce: &str, phase: &str, session_index: u32, pair_index: u32, treatment: &str, direction: &str, order_index: u32) -> Result<Value, String>
{
   let start_state = if direction == "forward" { "top" } else { "bottom" };
   let captured_offset = if direction == "forward" { 0.0 } else { 236_616.0 };
   let signed_travel = if direction == "forward" { 2_000.0 } else { -2_000.0 };
   let end_offset = captured_offset + signed_travel;
   Ok(json!({
      "schema": "oxide.feed-v1.run",
      "schema_revision": 1,
      "fixture": {
         "schema": "oxide.feed-v1.fixture",
         "revision": 1,
         "canonical_sha256": "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473",
         "canonical_byte_count": 717745
      },
      "run": {
         "nonce": nonce,
         "phase": phase,
         "session_index": session_index,
         "pair_index": pair_index,
         "order_index": order_index,
         "treatment": treatment,
         "start_state": start_state,
         "direction": direction
      },
      "canvas": {
         "host_width_points": 440,
         "host_height_points": 956,
         "surface_origin_x_points": 25,
         "surface_origin_y_points": 56,
         "surface_width_points": 390,
         "surface_height_points": 844,
         "scale": 3
      },
      "geometry": {
         "row_count": 2000,
         "manifest_component_count": 12000,
         "content_extent_points": 237460.0,
         "maximum_content_offset_points": 236616.0,
         "captured_content_offset_points": captured_offset,
         "viewport_clip_px": { "x": 0, "y": 0, "width": 1170, "height": 2532 },
         "visible_components": frozen_components(start_state)?
      },
      "environment": {
         "before": {
            "thermal_state": "nominal",
            "low_power_mode": false,
            "maximum_frames_per_second": 120,
            "configured_frame_rate": { "minimum": 120.0, "maximum": 120.0, "preferred": 120.0 }
         },
         "after": {
            "thermal_state": "nominal",
            "low_power_mode": false,
            "maximum_frames_per_second": 120,
            "configured_frame_rate": { "minimum": 120.0, "maximum": 120.0, "preferred": 120.0 }
         },
         "thermal_state_change_count": 0,
         "low_power_mode_change_count": 0
      },
      "gesture": {
         "start_offset_points": captured_offset,
         "end_offset_points": end_offset,
         "signed_travel_points": signed_travel,
         "travel_distance_points": 2000.0,
         "duration_seconds": 1.0,
         "inertia_observed": true,
         "settled": true
      },
      "display_link": {
         "clock": "CADisplayLink.timestamp/targetTimestamp",
         "callback_only": true,
         "samples": [
            { "timestamp_seconds": 100.0, "target_timestamp_seconds": 100.008333333333 },
            { "timestamp_seconds": 100.008333333333, "target_timestamp_seconds": 100.016666666666 }
         ]
      },
      "status": "complete",
      "failure": null
   }))
}
fn solid_rgba_image(width: u32, height: u32, rgba: [u8; 4]) -> RgbaImage
{
   let mut pixels = vec![0; width as usize * height as usize * 4];
   for pixel in pixels.chunks_exact_mut(4)
   {
      pixel.copy_from_slice(&rgba);
   }
   RgbaImage { width, height, pixels }
}

fn git(root: &Path, arguments: &[&str]) -> Result<(), String>
{
   let output = Command::new("git").arg("-C").arg(root).args(arguments).output()
      .map_err(|error| format!("run git {}: {error}", arguments.join(" ")))?;
   if !output.status.success()
   {
      return Err(format!(
         "git {} failed: {}",
         arguments.join(" "),
         String::from_utf8_lossy(&output.stderr).trim()
      ));
   }
   Ok(())
}

fn temporary_root(label: &str) -> Result<PathBuf, String>
{
   let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
      .map_err(|error| error.to_string())?.as_nanos();
   Ok(std::env::temp_dir().join(format!("oxide-feed-v1-{label}-{}-{nanos}", std::process::id())))
}
