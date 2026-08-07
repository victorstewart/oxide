use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use oxide_feed_v1_reducer::{
   callback_deadline_counts, exact_median_confidence_interval, frozen_components,
   frozen_order_index, reduce, strict_validate_failure_json, strict_validate_run_json,
   travel_equivalence_passes, verify_attachment_export, verify_smoke, visual_metrics,
   ReducePaths, RgbaImage,
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
fn exact_nine_cluster_interval_freezes_ranks_and_coverage() -> Result<(), String>
{
   let deltas = [-0.02, -0.01, 0.0, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06];
   let interval = exact_median_confidence_interval(&deltas)?;
   assert_eq!(interval.method, "exact-binomial-median");
   assert_eq!(interval.target_coverage, 0.95);
   assert_eq!(interval.achieved_coverage, 0.960_937_5);
   assert_eq!(interval.sample_count, 9);
   assert_eq!([interval.lower_rank, interval.upper_rank], [2, 8]);
   assert_eq!(interval.bounds, [-0.01, 0.05]);
   let error = exact_median_confidence_interval(&deltas[..8]).unwrap_err();
   assert_eq!(error, "exact median confidence interval has 8 gesture pairs, expected 9");
   let mut nonfinite = deltas;
   nonfinite[4] = f64::NAN;
   assert!(exact_median_confidence_interval(&nonfinite).is_err());
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
fn reducer_hard_blocks_failure_and_malformed_controlled_records() -> Result<(), String>
{
   let root = temporary_root("failure-block")?;
   fs::create_dir_all(&root).map_err(|error| error.to_string())?;
   let failure = json!({
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
   fs::write(
      root.join("oxide-feed-v1-failure.json"),
      serde_json::to_vec(&failure).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   fs::write(root.join("oxide-feed-v1-malformed.json"), b"{").map_err(|error| error.to_string())?;
   fs::write(root.join("oxide-feed-v1-missing-schema.json"), b"{}").map_err(|error| error.to_string())?;
   fs::write(root.join("feed-v1-foreign-schema.json"), br#"{"schema":"other"}"#)
      .map_err(|error| error.to_string())?;
   let paths = ReducePaths {
      run_root: root.clone(),
      output_json: root.join("latest.json"),
      output_markdown: root.join("latest.md"),
   };
   reduce(&paths)?;
   let report: Value = serde_json::from_slice(&fs::read(&paths.output_json).map_err(|error| error.to_string())?)
      .map_err(|error| error.to_string())?;
   assert_eq!(report["status"], "blocked");
   let blockers = report["blockers"].as_array().ok_or_else(|| "report blockers are not an array".to_string())?;
   assert!(blockers.iter().any(|blocker| blocker.as_str().is_some_and(|text| text.contains("app failure"))));
   assert!(blockers.iter().any(|blocker| blocker.as_str().is_some_and(|text| text.contains("malformed controlled JSON"))));
   assert!(blockers.iter().any(|blocker| blocker.as_str().is_some_and(|text| {
      text.contains("unrecognized controlled schema missing")
   })));
   assert!(blockers.iter().any(|blocker| blocker.as_str().is_some_and(|text| {
      text.contains("unrecognized controlled schema other")
   })));
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[test]
fn reducer_emits_only_admitted_nonsecret_device_identity() -> Result<(), String>
{
   let root = temporary_root("device-identity")?;
   let raw = root.join("raw");
   fs::create_dir_all(&raw).map_err(|error| error.to_string())?;
   let details = json!({
      "result": {
         "identifier": "private-core-device-id",
         "hardwareProperties": {
            "marketingName": "iPhone 17 Pro Max",
            "productType": "iPhone18,2",
            "reality": "physical",
            "platform": "iOS",
            "deviceType": "iPhone",
            "cpuType": { "name": "arm64e" },
            "supportedCPUTypes": [{ "name": "arm64e" }, { "name": "arm64" }]
         },
         "deviceProperties": {
            "osVersionNumber": "26.5.2",
            "osBuildUpdate": "23F84",
            "bootState": "booted"
         }
      }
   });
   let lock = json!({
      "result": {
         "deviceIdentifier": "private-core-device-id",
         "passcodeRequired": false
      }
   });
   let detail_bytes = serde_json::to_vec(&details).map_err(|error| error.to_string())?;
   let lock_bytes = serde_json::to_vec(&lock).map_err(|error| error.to_string())?;
   fs::write(raw.join("device-before.json"), &detail_bytes).map_err(|error| error.to_string())?;
   fs::write(raw.join("device-after.json"), &detail_bytes).map_err(|error| error.to_string())?;
   fs::write(raw.join("lock-before-build.json"), &lock_bytes).map_err(|error| error.to_string())?;
   fs::write(raw.join("lock-before-test.json"), &lock_bytes).map_err(|error| error.to_string())?;
   let paths = ReducePaths {
      run_root: root.clone(),
      output_json: root.join("latest.json"),
      output_markdown: root.join("latest.md"),
   };
   reduce(&paths)?;
   let report: Value = serde_json::from_slice(&fs::read(&paths.output_json).map_err(|error| error.to_string())?)
      .map_err(|error| error.to_string())?;
   assert_eq!(report["device"]["marketing_name"], "iPhone 17 Pro Max");
   assert_eq!(report["device"]["product_type"], "iPhone18,2");
   assert_eq!(report["device"]["cpu"], "arm64e");
   assert!(report["retained_input_bytes"].as_u64().is_some_and(|bytes| bytes > 0));
   assert!(!serde_json::to_string(&report["device"]).map_err(|error| error.to_string())?
      .contains("private-core-device-id"));
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
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

#[test]
fn six_valid_smoke_tuples_pass_smoke_but_not_full_publication() -> Result<(), String>
{
   let root = temporary_root("valid-smoke")?;
   write_valid_smoke_root(&root)?;
   verify_attachment_export(&root.join("raw/attachments"))?;
   verify_smoke(&root)?;
   rewrite_travel(
      &root.join("raw/uikit-documents/oxide-feed-v1-smoke-s00-p00-o1-uikit-optimized-forward-test.json"),
      2_140.0,
   )?;
   verify_smoke(&root)?;
   assert!(!root.join("latest.json").exists());
   assert!(!root.join("latest.md").exists());

   let paths = ReducePaths {
      run_root: root.clone(),
      output_json: root.join("full.json"),
      output_markdown: root.join("full.md"),
   };
   reduce(&paths)?;
   let report: Value = serde_json::from_slice(&fs::read(&paths.output_json).map_err(|error| error.to_string())?)
      .map_err(|error| error.to_string())?;
   assert_eq!(report["status"], "blocked");
   assert!(report["comparisons"].as_array().is_some_and(Vec::is_empty));
   assert!(report["runs"].as_array().is_some_and(Vec::is_empty));
   assert!(report["blockers"].as_array().is_some_and(|blockers| blockers.iter().any(|blocker| {
      blocker.as_str().is_some_and(|text| text.contains("run population is 6, expected 60"))
   })));
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[test]
fn smoke_admits_named_output_placeholders_as_evidence() -> Result<(), String>
{
   let root = temporary_root("named-output-evidence")?;
   write_valid_smoke_root(&root)?;
   let failure = json!({
      "schema": "oxide.feed-v1.failure",
      "schema_revision": 1,
      "fixture": {
         "schema": "oxide.feed-v1.fixture",
         "revision": 1,
         "canonical_sha256": "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473",
         "canonical_byte_count": 717745
      },
      "nonce": null,
      "treatment": null,
      "stage": "capture",
      "message": "must not be hidden by the smoke verifier's placeholder paths"
   });
   fs::write(
      root.join("unused-smoke-report.json"),
      serde_json::to_vec(&failure).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("must not be hidden"));
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[test]
fn smoke_scans_build_and_target_named_directories() -> Result<(), String>
{
   let root = temporary_root("strict-evidence-walk")?;
   write_valid_smoke_root(&root)?;
   let raw = root.join("raw");
   let build = raw.join("build");
   fs::create_dir_all(&build).map_err(|error| error.to_string())?;
   fs::write(build.join("oxide-feed-v1-hidden.json"), br#"{"schema":"oxide.feed-v1.unknown"}"#)
      .map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("unknown controlled schema"));
   fs::remove_dir_all(&build).map_err(|error| error.to_string())?;

   let target = raw.join("target");
   fs::create_dir_all(&target).map_err(|error| error.to_string())?;
   fs::write(target.join("oxide-feed-v1-hidden.json"), br#"{"schema":"oxide.feed-v1.unknown"}"#)
      .map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("unknown controlled schema"));
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[cfg(unix)]
#[test]
fn smoke_rejects_symlink_roots_entries_and_nonregular_files() -> Result<(), String>
{
   use std::os::unix::fs::symlink;
   use std::os::unix::net::UnixListener;

   let root = temporary_root("ln")?;
   write_valid_smoke_root(&root)?;
   let raw = root.join("raw");
   let symlink_path = raw.join("alias.json");
   symlink(raw.join("cleanup.json"), &symlink_path).map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("evidence path is a symlink"));
   fs::remove_file(&symlink_path).map_err(|error| error.to_string())?;

   let socket_path = raw.join("s");
   let socket = UnixListener::bind(&socket_path).map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("not a regular file or directory"));
   drop(socket);
   fs::remove_file(&socket_path).map_err(|error| error.to_string())?;

   let alias = temporary_root("la")?;
   symlink(&root, &alias).map_err(|error| error.to_string())?;
   assert!(verify_smoke(&alias).unwrap_err().contains("evidence root is a symlink"));
   fs::remove_file(alias).map_err(|error| error.to_string())?;
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[test]
fn smoke_rejects_cleanup_and_artifact_provenance_mutations() -> Result<(), String>
{
   let root = temporary_root("smoke-provenance-mutations")?;
   write_valid_smoke_root(&root)?;
   let raw = root.join("raw");

   let cleanup_path = raw.join("cleanup.json");
   let cleanup_bytes = fs::read(&cleanup_path).map_err(|error| error.to_string())?;
   let mut cleanup: Value = serde_json::from_slice(&cleanup_bytes).map_err(|error| error.to_string())?;
   cleanup["test_succeeded"] = json!(false);
   cleanup["verified_attachment_count"] = json!(5);
   cleanup["source_snapshot_preserved"] = json!(false);
   cleanup["reducer_binary_absent_from_result_root"] = json!(false);
   fs::write(&cleanup_path, serde_json::to_vec(&cleanup).map_err(|error| error.to_string())?)
      .map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("cleanup/runtime/cap proof failed"));
   fs::write(&cleanup_path, &cleanup_bytes).map_err(|error| error.to_string())?;

   let runtime_path = raw.join("controller-documents/oxide-feed-v1-controller-runtime.json");
   let runtime_bytes = fs::read(&runtime_path).map_err(|error| error.to_string())?;
   let mut runtime: Value = serde_json::from_slice(&runtime_bytes).map_err(|error| error.to_string())?;
   runtime["total_runtime_seconds"] = json!(602.0);
   runtime["uikit_idiomatic_runtime_seconds"] = json!(601.0);
   fs::write(&runtime_path, serde_json::to_vec(&runtime).map_err(|error| error.to_string())?)
      .map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("20/10-minute limits"));
   fs::write(&runtime_path, &runtime_bytes).map_err(|error| error.to_string())?;

   let nonce = "smoke-s00-p00-o0-uikit-idiomatic-forward-test";
   let record_name = format!("oxide-feed-v1-{nonce}.json");
   let record_path = raw.join("uikit-documents").join(&record_name);
   let wrong_record_path = raw.join("oxide-documents").join(&record_name);
   fs::rename(&record_path, &wrong_record_path).map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("wrong app container"));
   fs::rename(&wrong_record_path, &record_path).map_err(|error| error.to_string())?;

   let attachment_manifest_path = raw.join("attachments/manifest.json");
   let attachment_manifest_bytes = fs::read(&attachment_manifest_path).map_err(|error| error.to_string())?;
   let mut attachment_manifest: Value = serde_json::from_slice(&attachment_manifest_bytes)
      .map_err(|error| error.to_string())?;
   let attachments = attachment_manifest[0]["attachments"].as_array_mut()
      .ok_or_else(|| "attachment manifest has no attachment array".to_string())?;
   attachments[0]["suggestedHumanReadableName"] = json!("feed-v1-unexpected.png");
   fs::write(
      &attachment_manifest_path,
      serde_json::to_vec(&attachment_manifest).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("attachment manifest names are not exactly the six frozen smoke captures"));
   fs::write(&attachment_manifest_path, &attachment_manifest_bytes).map_err(|error| error.to_string())?;

   let capture_path = raw.join("attachments").join(format!("export-{nonce}.png"));
   let capture_bytes = fs::read(&capture_path).map_err(|error| error.to_string())?;
   fs::write(&capture_path, cropped_smoke_png("top")?).map_err(|error| error.to_string())?;
   assert!(verify_smoke(&root).unwrap_err().contains("expected full XCUIScreen canvas 1320x2868"));
   fs::write(&capture_path, capture_bytes).map_err(|error| error.to_string())?;

   fs::copy(&cleanup_path, raw.join("oxide-feed-v1-duplicate-cleanup.json"))
      .map_err(|error| error.to_string())?;
   fs::copy(
      raw.join("evidence-manifest.json"),
      raw.join("oxide-feed-v1-duplicate-evidence-manifest.json"),
   ).map_err(|error| error.to_string())?;
   let error = verify_smoke(&root).unwrap_err();
   assert!(error.contains("multiple cleanup proofs"));
   assert!(error.contains("multiple evidence manifests"));

   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[test]
fn full_reduction_is_byte_identical_when_repeated() -> Result<(), String>
{
   let root = temporary_root("valid-full-idempotence")?;
   write_valid_full_root(&root)?;
   verify_attachment_export(&root.join("raw/attachments"))?;
   let paths = ReducePaths {
      run_root: root.clone(),
      output_json: root.join("latest.json"),
      output_markdown: root.join("latest.md"),
   };
   reduce(&paths)?;
   let first_json = fs::read(&paths.output_json).map_err(|error| error.to_string())?;
   let first_markdown = fs::read(&paths.output_markdown).map_err(|error| error.to_string())?;
   let report: Value = serde_json::from_slice(&first_json).map_err(|error| error.to_string())?;
   assert_eq!(report["status"], "complete");
   assert_eq!(report["schema_revision"], 4);
   assert_eq!(report["run_count_total"], 60);
   assert_eq!(report["run_count_primary"], 54);
   let travel = report["travel_equivalence"].as_array()
      .ok_or_else(|| "report travel equivalence is not an array".to_string())?;
   assert_eq!(travel.len(), 4);
   assert!(travel.iter().all(|result| {
      matches!(result["treatment"].as_str(), Some("uikit-optimized" | "oxide"))
         && matches!(result["direction"].as_str(), Some("forward" | "reverse"))
         && result["pair_count"] == 9
         && result["median_relative_delta"].as_f64().is_some()
         && result["confidence_interval"]["method"] == "exact-binomial-median"
         && result["confidence_interval"]["achieved_coverage"] == 0.960_937_5
         && result["confidence_interval"]["lower_rank"] == 2
         && result["confidence_interval"]["upper_rank"] == 8
         && result["confidence_interval"]["bounds"].as_array().is_some_and(|bounds| bounds.len() == 2)
         && result.get("confidence_95_lower").is_none()
         && result.get("confidence_95_upper").is_none()
         && result["median_margin"] == 0.05
         && result["confidence_margin"] == 0.10
         && result["passes"] == true
   }));
   let comparisons = report["comparisons"].as_array()
      .ok_or_else(|| "report comparisons is not an array".to_string())?;
   assert_eq!(comparisons.len(), 2);
   assert!(comparisons.iter().all(|comparison| {
      comparison["confidence_interval"]["method"] == "exact-binomial-median"
         && comparison["confidence_interval"]["target_coverage"] == 0.95
         && comparison["confidence_interval"]["achieved_coverage"] == 0.960_937_5
         && comparison["confidence_interval"]["sample_count"] == 9
         && comparison["confidence_interval"]["lower_rank"] == 2
         && comparison["confidence_interval"]["upper_rank"] == 8
         && comparison.get("bootstrap_resamples").is_none()
         && comparison.get("bootstrap_seed_hex").is_none()
         && comparison.get("confidence_95_lower").is_none()
         && comparison.get("confidence_95_upper").is_none()
   }));
   let runs = report["runs"].as_array().ok_or_else(|| "report runs are not an array".to_string())?;
   assert_eq!(runs.len(), 54);
   assert!(runs.iter().all(|run| {
      run["inertia_observed"] == true
         && run["thermal_state_change_count"] == 0
         && run["low_power_mode_change_count"] == 0
         && run["callback_count"].as_u64().is_some_and(|count| count >= 2)
         && run["interval_p50_ms"].as_f64().is_some()
         && run["interval_p95_ms"].as_f64().is_some()
         && run["interval_p99_ms"].as_f64().is_some()
         && run["interval_peak_ms"].as_f64().is_some()
   }));
   let markdown = String::from_utf8_lossy(&first_markdown);
   assert!(markdown.contains("## Travel equivalence"));
   assert!(markdown.contains("| oxide | forward | 9 |"));
   assert!(markdown.contains("`exact-binomial-median`, ranks 2-8 of 9"));
   assert!(markdown.contains("## Per-run callback evidence"));

   reduce(&paths)?;
   assert_eq!(first_json, fs::read(&paths.output_json).map_err(|error| error.to_string())?);
   assert_eq!(first_markdown, fs::read(&paths.output_markdown).map_err(|error| error.to_string())?);
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

#[test]
fn full_reduction_rejects_systematic_primary_travel_mismatch() -> Result<(), String>
{
   let root = temporary_root("full-travel-mismatch")?;
   write_valid_full_root(&root)?;
   for session in 0 .. 3
   {
      for pair in 0 .. 3
      {
         let rotation = (session + pair) % 3;
         let order = (2 + 3 - rotation) % 3;
         for direction in ["forward", "reverse"]
         {
            let nonce = format!("primary-s{session:02}-p{pair:02}-o{order}-oxide-{direction}-test");
            rewrite_travel(
               &root.join("raw/oxide-documents").join(format!("oxide-feed-v1-{nonce}.json")),
               2_120.0,
            )?;
         }
      }
   }

   let paths = ReducePaths {
      run_root: root.clone(),
      output_json: root.join("latest.json"),
      output_markdown: root.join("latest.md"),
   };
   reduce(&paths)?;
   let report: Value = serde_json::from_slice(&fs::read(&paths.output_json).map_err(|error| error.to_string())?)
      .map_err(|error| error.to_string())?;
   assert_eq!(report["status"], "blocked");
   assert!(report["blockers"].as_array().is_some_and(|blockers| blockers.iter().any(|blocker| {
      blocker.as_str().is_some_and(|text| text.contains("primary travel equivalence oxide"))
   })));
   let travel = report["travel_equivalence"].as_array()
      .ok_or_else(|| "blocked report travel equivalence is not an array".to_string())?;
   let oxide_results: Vec<&Value> = travel.iter().filter(|result| result["treatment"] == "oxide").collect();
   assert_eq!(oxide_results.len(), 2);
   assert!(oxide_results.iter().all(|result| {
      result["median_relative_delta"].as_f64().is_some_and(|delta| (delta - 0.06).abs() < 1e-12)
         && result["confidence_interval"]["bounds"][0].as_f64().is_some_and(|lower| lower > 0.05)
         && result["confidence_interval"]["bounds"][1].as_f64().is_some_and(|upper| upper < 0.10)
         && result["passes"] == false
   }));
   fs::remove_dir_all(root).map_err(|error| error.to_string())?;
   Ok(())
}

fn write_valid_smoke_root(root: &Path) -> Result<(), String>
{
   write_valid_root(root, false)
}

fn write_valid_full_root(root: &Path) -> Result<(), String>
{
   write_valid_root(root, true)
}

fn write_valid_root(root: &Path, full: bool) -> Result<(), String>
{
   let raw = root.join("raw");
   let attachments = raw.join("attachments");
   fs::create_dir_all(&attachments).map_err(|error| error.to_string())?;
   fs::create_dir_all(raw.join("controller-documents")).map_err(|error| error.to_string())?;
   write_smoke_device_evidence(&raw)?;
   fs::write(raw.join("evidence-manifest.json"), serde_json::to_vec(&json!({
      "schema": "oxide.feed-v1.evidence-manifest",
      "schema_revision": 2,
      "fixture_sha256": "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473",
      "repository_ref": "refs/heads/main",
      "repository_head_commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "repository_tree": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "source_files": { "reducer/src/lib.rs": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" },
      "uikit_app_sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "oxide_app_sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
      "controller_runner_sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
      "controller_runner_binary_sha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
      "controller_xctest_sha256": "1111111111111111111111111111111111111111111111111111111111111111",
      "controller_xctest_binary_sha256": "2222222222222222222222222222222222222222222222222222222222222222",
      "reducer_binary_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "regular_font_sha256": "7d494f276293fb0a8e2aab1fc0e386baa3e8a1d90927f518abb152b5c73e29f9",
      "bold_font_sha256": "7f4feacd835eed23e104413f800a74b9f0270ce8c754c990bfc09b796a3ca628"
   })).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
   fs::write(raw.join("cleanup.json"), serde_json::to_vec(&json!({
      "schema": "oxide.feed-v1.cleanup",
      "schema_revision": 2,
      "test_succeeded": true,
      "verified_attachment_count": 6,
      "apps_uninstalled": true,
      "controller_uninstalled": true,
      "controller_process_absent": true,
      "source_snapshot_preserved": true,
      "external_build_removed": true,
      "result_bundle_removed": true,
      "reducer_binary_absent_from_result_root": true,
      "raw_evidence_bytes": 0,
      "runtime_seconds": 1.0
   })).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
   fs::write(
      raw.join("controller-documents/oxide-feed-v1-controller-runtime.json"),
      serde_json::to_vec(&json!({
         "schema": "oxide.feed-v1.controller-runtime",
         "schema_revision": 1,
         "mode": if full { "full" } else { "smoke" },
         "total_runtime_seconds": 0.2,
         "uikit_idiomatic_runtime_seconds": 0.05,
         "uikit_optimized_runtime_seconds": 0.05,
         "oxide_runtime_seconds": 0.05
      })).map_err(|error| error.to_string())?,
   ).map_err(|error| error.to_string())?;

   let top_png = smoke_png("top")?;
   let bottom_png = smoke_png("bottom")?;
   let mut exported_attachments = Vec::new();
   let treatments = ["uikit-idiomatic", "uikit-optimized", "oxide"];
   let mut cases: Vec<(&str, u32, u32, u32, &str, &str)> = Vec::new();
   for (order_index, treatment) in ["uikit-idiomatic", "uikit-optimized", "oxide"].iter().enumerate()
   {
      for direction in ["forward", "reverse"]
      {
         cases.push(("smoke", 0, 0, order_index as u32, treatment, direction));
      }
   }
   if full
   {
      for session_index in 0 .. 3
      {
         for pair_index in 0 .. 3
         {
            let rotation = (session_index + pair_index) as usize % treatments.len();
            for order_index in 0 .. 3
            {
               let treatment = treatments[(rotation + order_index) % treatments.len()];
               for direction in ["forward", "reverse"]
               {
                  cases.push((
                     "primary",
                     session_index,
                     pair_index,
                     order_index as u32,
                     treatment,
                     direction,
                  ));
               }
            }
         }
      }
   }
   for (phase, session_index, pair_index, order_index, treatment, direction) in cases
   {
      let nonce = format!("{phase}-s{session_index:02}-p{pair_index:02}-o{order_index}-{treatment}-{direction}-test");
      let run = run_json(&nonce, phase, session_index, pair_index, treatment, direction, order_index)?;
      let documents = if treatment == "oxide" { "oxide-documents" } else { "uikit-documents" };
      let document_root = raw.join(documents);
      fs::create_dir_all(&document_root).map_err(|error| error.to_string())?;
      fs::write(
         document_root.join(format!("oxide-feed-v1-{nonce}.json")),
         serde_json::to_vec(&run).map_err(|error| error.to_string())?,
      ).map_err(|error| error.to_string())?;

      if phase != "smoke"
      {
         continue;
      }

      let png = if direction == "forward" { &top_png } else { &bottom_png };
      let name = format!("feed-v1-{nonce}_0_01234567-89AB-CDEF-0123-456789ABCDEF.png");
      let export = format!("export-{nonce}.png");
      fs::write(attachments.join(&export), png).map_err(|error| error.to_string())?;
      exported_attachments.push(json!({
         "suggestedHumanReadableName": name,
         "exportedFileName": export
      }));
   }
   fs::write(attachments.join("manifest.json"), serde_json::to_vec(&json!([{
      "testIdentifier": "FeedV1ControllerTests/testFeedV1PhysicalDevicePilot()",
      "attachments": exported_attachments
   }])).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
   Ok(())
}

fn write_smoke_device_evidence(raw: &Path) -> Result<(), String>
{
   let details = json!({
      "result": {
         "identifier": "private-core-device-id",
         "hardwareProperties": {
            "marketingName": "iPhone 17 Pro Max",
            "productType": "iPhone18,2",
            "reality": "physical",
            "platform": "iOS",
            "deviceType": "iPhone",
            "cpuType": { "name": "arm64e" },
            "supportedCPUTypes": [{ "name": "arm64e" }, { "name": "arm64" }]
         },
         "deviceProperties": {
            "osVersionNumber": "26.5.2",
            "osBuildUpdate": "23F84",
            "bootState": "booted"
         }
      }
   });
   let lock = json!({
      "result": {
         "deviceIdentifier": "private-core-device-id",
         "passcodeRequired": false
      }
   });
   let details = serde_json::to_vec(&details).map_err(|error| error.to_string())?;
   let lock = serde_json::to_vec(&lock).map_err(|error| error.to_string())?;
   fs::write(raw.join("device-before.json"), &details).map_err(|error| error.to_string())?;
   fs::write(raw.join("device-after.json"), &details).map_err(|error| error.to_string())?;
   fs::write(raw.join("lock-before-build.json"), &lock).map_err(|error| error.to_string())?;
   fs::write(raw.join("lock-before-test.json"), &lock).map_err(|error| error.to_string())?;
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

fn rewrite_travel(path: &Path, distance: f64) -> Result<(), String>
{
   let bytes = fs::read(path).map_err(|error| error.to_string())?;
   let mut run: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
   let start = run["gesture"]["start_offset_points"].as_f64()
      .ok_or_else(|| "test run has no start offset".to_string())?;
   let sign = if run["run"]["direction"] == "forward" { 1.0 } else { -1.0 };
   let signed = distance * sign;
   run["gesture"]["end_offset_points"] = json!(start + signed);
   run["gesture"]["signed_travel_points"] = json!(signed);
   run["gesture"]["travel_distance_points"] = json!(distance);
   fs::write(path, serde_json::to_vec(&run).map_err(|error| error.to_string())?)
      .map_err(|error| error.to_string())
}

fn smoke_png(state: &str) -> Result<Vec<u8>, String>
{
   encode_smoke_png(state, true)
}

fn cropped_smoke_png(state: &str) -> Result<Vec<u8>, String>
{
   encode_smoke_png(state, false)
}

fn encode_smoke_png(state: &str, full_canvas: bool) -> Result<Vec<u8>, String>
{
   let (width, height, origin_x, origin_y, background) = if full_canvas
   {
      (1_320, 2_868, 75, 168, [0, 0, 0, 255])
   }
   else
   {
      (1_170, 2_532, 0, 0, [247, 244, 238, 255])
   };
   let mut image = solid_rgba_image(width, height, background);
   for component in frozen_components(state)?
   {
      let color = match component.kind.as_str()
      {
         "row" if component.row_index % 2 == 0 => [225, 221, 213, 255],
         "row" => [236, 230, 220, 255],
         "image" => [24, 95, 205, 255],
         "title" => [18, 18, 20, 255],
         "caption" => [92, 34, 138, 255],
         "metadata" => [17, 118, 54, 255],
         "separator" => [118, 111, 103, 255],
         kind => return Err(format!("unknown smoke component kind {kind}")),
      };
      let rect = oxide_feed_v1_reducer::Rect {
         x: component.viewport_clip_px.x + origin_x,
         y: component.viewport_clip_px.y + origin_y,
         width: component.viewport_clip_px.width,
         height: component.viewport_clip_px.height,
      };
      paint_test_rect(&mut image, rect, color)?;
   }
   let mut bytes = Vec::new();
   {
      let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
      encoder.set_color(png::ColorType::Rgba);
      encoder.set_depth(png::BitDepth::Eight);
      let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
      writer.write_image_data(&image.pixels).map_err(|error| error.to_string())?;
   }
   Ok(bytes)
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

fn paint_test_rect(image: &mut RgbaImage, rect: oxide_feed_v1_reducer::Rect, color: [u8; 4]) -> Result<(), String>
{
   if rect.x < 0
      || rect.y < 0
      || rect.width <= 0
      || rect.height <= 0
      || rect.x + rect.width > image.width as i32
      || rect.y + rect.height > image.height as i32
   {
      return Err("smoke test rectangle leaves the image".to_string());
   }
   for y in rect.y as u32 .. (rect.y + rect.height) as u32
   {
      for x in rect.x as u32 .. (rect.x + rect.width) as u32
      {
         let index = ((y * image.width + x) * 4) as usize;
         image.pixels[index .. index + 4].copy_from_slice(&color);
      }
   }
   Ok(())
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
