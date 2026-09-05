use std::fs;
use std::io::Write;
use std::path::Path;

use tempfile::tempdir;

use oxide_benchmark_spec::{canonical_apple_campaign_plan_json, AppleCampaignPlanSpec};
use oxide_apple_comparison_controller::MacOsCampaignPlan;
use oxide_perf_runner::density_acquisition::{canonical_macos_density_acquisition_plan_json, MacOsDensityAcquisitionPlan, MacOsDensityScenario};
use oxide_perf_runner::density_calibration::CalibrationTier;
use xtask::{build_and_bundle_shaders, run_cli};

fn rgb_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8>
{
   let mut bytes = Vec::new();
   {
      let mut encoder = png::Encoder::new(&mut bytes, width, height);
      encoder.set_color(png::ColorType::Rgb);
      encoder.set_depth(png::BitDepth::Eight);
      encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
      let mut writer = encoder.write_header().expect("PNG header");
      writer.write_image_data(pixels).expect("PNG pixels");
      writer.finish().expect("finish PNG");
   }
   bytes
}

fn solid_pixels(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8>
{
   let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
   for _ in 0..width as usize * height as usize
   {
      pixels.extend_from_slice(&rgb);
   }
   pixels
}

fn write_visual_inputs(root: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)
{
   let reference = root.join("reference.png");
   let candidate = root.join("candidate.png");
   let layout = root.join("layout.json");
   let text_geometry = root.join("text-geometry.json");
   let pixels = solid_pixels(16, 16, [72, 96, 120]);
   fs::write(&reference, rgb_png(16, 16, &pixels)).expect("reference PNG");
   fs::write(&candidate, rgb_png(16, 16, &pixels)).expect("candidate PNG");
   fs::write(&layout, r#"{"coordinate_space":"logical-points","root":{"x":0,"y":0,"width":8,"height":8},"text_comparison":{"pixel_mask":true,"baseline_tolerance":0.5,"ink_bounds_tolerance":0.5},"text_masks":[[0.5,0.5,1,1]]}"#).expect("layout JSON");
   fs::write(&text_geometry, r#"{"reference_lines":[{"baseline_y":2,"ink_bounds":{"x":0.5,"y":0.5,"width":1,"height":1}}],"candidate_lines":[{"baseline_y":2,"ink_bounds":{"x":0.5,"y":0.5,"width":1,"height":1}}]}"#).expect("text geometry JSON");
   (reference, candidate, layout, text_geometry)
}

fn reduce_visual_args(reference: &Path, candidate: &Path, layout: &Path, text_geometry: Option<&Path>, output: Option<&Path>, diagnostic: bool) -> Vec<String>
{
   let mut args = vec![
      String::from("compare-ui"),
      String::from("reduce-visual"),
      String::from("--reference"),
      reference.to_string_lossy().into_owned(),
      String::from("--candidate"),
      candidate.to_string_lossy().into_owned(),
      String::from("--layout"),
      layout.to_string_lossy().into_owned(),
      String::from("--canonical-scale"),
      String::from("2"),
   ];
   if let Some(text_geometry) = text_geometry
   {
      args.push(String::from("--text-geometry"));
      args.push(text_geometry.to_string_lossy().into_owned());
   }
   if let Some(output) = output
   {
      args.push(String::from("--output"));
      args.push(output.to_string_lossy().into_owned());
   }
   if diagnostic
   {
      args.push(String::from("--diagnostic"));
   }
   args
}

fn compare_static_exact_args(oxide: &Path, uikit: &Path, layout: &Path, output: &Path, diagnostic: bool) -> Vec<String>
{
   let mut args = vec![
      String::from("compare-ui"),
      String::from("compare-static-exact"),
      String::from("--oxide"),
      oxide.to_string_lossy().into_owned(),
      String::from("--uikit"),
      uikit.to_string_lossy().into_owned(),
      String::from("--layout"),
      layout.to_string_lossy().into_owned(),
      String::from("--canonical-scale"),
      String::from("2"),
      String::from("--output"),
      output.to_string_lossy().into_owned(),
   ];
   if diagnostic
   {
      args.push(String::from("--diagnostic"));
   }
   args
}

#[test]
fn run_cli_unknown_command_shows_usage() {
    assert!(run_cli(&[]).is_ok());
    assert!(run_cli(&["unknown".into()]).is_ok());
}

#[test]
fn experiments_check_cli_accepts_manifest_path() {
    let workspace = tempdir().expect("workspace");
    let manifest = workspace.path().join("perf-experiments.toml");
    fs::write(
        &manifest,
        r#"
[[experiments]]
id = "sample-accepted"
introduced_commit = "abc123"
introduced_date = "2026-06-01"
expires = "2026-06-23"
required_backends = ["oxide-perf-runner"]
required_devices = ["macOS host"]
correctness_gate = "focused tests"
performance_gate = "same-workload A/B"
decision_state = "accepted"
decision = "accepted after proof"
proof = ["current 1.0 ms vs legacy 2.0 ms"]
cleanup = ["deleted loser path"]
"#,
    )
    .expect("write manifest");

    let args = vec![
        String::from("experiments"),
        String::from("check"),
        String::from("--manifest"),
        manifest.to_string_lossy().into_owned(),
        String::from("--today"),
        String::from("2026-06-22"),
    ];
    assert!(run_cli(&args).is_ok());
}

#[test]
fn experiments_check_cli_help_does_not_require_manifest() {
    let args = vec![String::from("experiments"), String::from("check"), String::from("--help")];
    assert!(run_cli(&args).is_ok());
}

#[test]
fn compare_ui_validate_and_budget_plan_are_real_commands()
{
   assert!(run_cli(&[String::from("compare-ui"), String::from("validate")]).is_ok());
   assert!(run_cli(&[String::from("compare-ui"), String::from("self-test")]).is_ok());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--tier"),
      String::from("pr"),
      String::from("--explain-budget"),
      String::from("--explain-coverage"),
   ])
   .is_ok());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("ios"),
      String::from("--tier"),
      String::from("pr"),
      String::from("--explain-budget"),
   ])
   .is_ok());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("web"),
      String::from("--tier"),
      String::from("nightly"),
      String::from("--explain-budget"),
   ])
   .is_ok());
}

#[test]
fn compare_ui_materializes_and_dry_expands_the_content_addressed_macos_nightly_plan()
{
   let temporary = tempdir().expect("temporary generic campaign");
   let materialized = temporary.path().join("nightly-apple.json");
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--tier"),
      String::from("nightly"),
      String::from("--out"),
      materialized.to_string_lossy().into_owned(),
   ]).is_ok());
   let bytes = fs::read(&materialized).expect("materialized nightly plan");
   let plan: AppleCampaignPlanSpec = serde_json::from_slice(&bytes).expect("nightly plan JSON");
   assert_eq!(bytes, canonical_apple_campaign_plan_json(&plan).expect("canonical nightly plan"));

   let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root");
   let dry_run = temporary.path().join("nightly-controller-plan.json");
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("run"),
      String::from("--plan"),
      workspace.join("benchmarks/comparative/specs/v1/plans/nightly-apple.json").to_string_lossy().into_owned(),
      String::from("--out"),
      dry_run.to_string_lossy().into_owned(),
      String::from("--dry-run"),
   ]).is_ok());
   let expanded: MacOsCampaignPlan = serde_json::from_slice(&fs::read(dry_run).expect("generic dry-run bytes")).expect("generic dry-run plan");
   assert_eq!(expanded.plan_resource_path.as_deref(), Some("plans/nightly-apple.json"));
   assert_eq!(expanded.sessions.len(), 78);
}

#[test]
fn compare_ui_unimplemented_or_incomplete_paths_fail_closed()
{
   assert!(run_cli(&[String::from("compare-ui")]).is_err());
   assert!(run_cli(&[String::from("compare-ui"), String::from("run")]).is_err());
   assert!(run_cli(&[String::from("compare-ui"), String::from("finalize-apple-transport")]).is_err());
   assert!(run_cli(&[String::from("compare-ui"), String::from("calibrate-density")]).is_err());
   assert!(run_cli(&[String::from("compare-ui"), String::from("calibrate-presentation")]).is_err());
   assert!(run_cli(&[String::from("compare-ui"), String::from("compare-apple-correctness")]).is_err());
   assert!(run_cli(&[String::from("compare-ui"), String::from("materialize-macos-analyzer")]).is_err());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("ios"),
      String::from("--tier"),
      String::from("pr"),
      String::from("--explain-coverage"),
   ])
   .is_err());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--tier"),
      String::from("nightly"),
   ])
   .is_ok());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--tier"),
      String::from("release-core"),
   ])
   .is_err());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--tier"),
      String::from("extended"),
   ])
   .is_err());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("plan"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--tier"),
      String::from("full-attribution"),
   ])
   .is_err());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("run"),
      String::from("--plan"),
      Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace")
         .join("benchmarks/comparative/specs/v1/plans/apple-pr.json").to_string_lossy().into_owned(),
      String::from("--out"),
      String::from("/tmp/oxide-invalid-correctness-dry-run.json"),
      String::from("--dry-run"),
      String::from("--correctness-only"),
   ])
   .is_err());
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("run"),
      String::from("--plan"),
      Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace")
         .join("benchmarks/comparative/specs/v1/plans/apple-pr.json").to_string_lossy().into_owned(),
      String::from("--out"),
      String::from("/tmp/oxide-invalid-resume-dry-run.json"),
      String::from("--dry-run"),
      String::from("--resume"),
   ])
   .is_err());
}

#[test]
fn compare_ui_density_acquisition_requires_explicit_macos_driver_protocol()
{
   let temporary = tempdir().expect("temporary density CLI");
   let plan_path = temporary.path().join("density-plan.json");
   let output = temporary.path().join("density-output");
   let plan = MacOsDensityAcquisitionPlan {
      schema_version: 1,
      calibration_id: "macos-density-cli".into(),
      platform_role: "macos-apple-silicon".into(),
      invalidation_key: "fixture-instrumentation-lifecycle-order-role".into(),
      tier: CalibrationTier::Pr,
      seed: 7,
      bootstrap_resamples: 1_000,
      minimum_pair_count: 4,
      maximum_pair_count: 4,
      sentinel_scenario_id: "scenario-0".into(),
      implementations: vec!["oxide".into(), "native-production".into()],
      scenarios: (0..10).map(|index| MacOsDensityScenario {
         id: format!("scenario-{index}"),
         primary_metric_id: "event-to-visible-p50".into(),
         carryover_margin_basis_points: 200,
         risk_weight_millionths: 1_000_000,
      }).collect(),
   };
   fs::write(&plan_path, canonical_macos_density_acquisition_plan_json(&plan).expect("canonical density CLI plan")).expect("write density CLI plan");
   let error = run_cli(&[
      String::from("compare-ui"),
      String::from("calibrate-density"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--plan"),
      plan_path.to_string_lossy().into_owned(),
      String::from("--driver"),
      String::from("/usr/bin/false"),
      String::from("--out"),
      output.to_string_lossy().into_owned(),
   ]).expect_err("failing density driver must fail the CLI");
   assert!(error.to_string().contains("density driver exited"));
   assert!(output.join("pairs/k-1/pair-00.request.json").is_file());
   assert!(!output.join("pairs/k-1/pair-00.result.json").exists());
}

#[test]
fn compare_ui_full_run_plumbs_required_instrumentation_calibration_and_rejects_it_for_dry_runs()
{
   let composition_root = include_str!("../src/lib.rs");
   let start = composition_root.find("fn compare_ui_run(").expect("compare-ui run parser");
   let end = composition_root[start..].find("fn compare_ui_plan(").expect("compare-ui run parser end") + start;
   let run = &composition_root[start..end];
   assert!(run.contains("\"--instrumentation-calibration\""));
   assert!(run.contains("--instrumentation-calibration requires a path"));
   assert!(run.contains("instrumentation_calibration.is_none()"));
   assert!(run.contains("instrumentation_calibration_path: instrumentation_calibration"));
}

#[test]
fn compare_ui_promoted_macos_qualification_dry_run_uses_generated_budget()
{
   let output = tempdir().expect("qualification dry-run output");
   let output_path = output.path().join("campaign.json");
   let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
   let plan_path = workspace.join("benchmarks/comparative/specs/v1/plans/macos-comparator-qualification.json");
   run_cli(&[
      String::from("compare-ui"),
      String::from("run"),
      String::from("--plan"),
      plan_path.to_string_lossy().into_owned(),
      String::from("--out"),
      output_path.to_string_lossy().into_owned(),
      String::from("--dry-run"),
   ]).expect("promoted qualification dry run");
   let bytes = fs::read(&output_path).expect("qualification campaign");
   let campaign: MacOsCampaignPlan = serde_json::from_slice(&bytes).expect("decode qualification campaign");
   assert_eq!(campaign.sessions.len(), 52);
}

#[test]
fn compare_apple_correctness_cli_delegates_to_the_controller_crate()
{
   let composition_root = include_str!("../src/lib.rs");
   let legacy_module = include_str!("../src/apple_comparison.rs");
   assert!(composition_root.contains("use oxide_apple_comparison_controller::{compare_apple_correctness_evidence"));
   assert!(composition_root.contains("let report = compare_apple_correctness_evidence("));
   assert!(!legacy_module.contains("struct AppleCorrectnessVisualReport"));
   assert!(!legacy_module.contains("fn compare_apple_correctness_evidence("));
}

#[test]
fn compare_ui_macos_build_products_stay_outside_the_workspace()
{
   let composition_root = include_str!("../src/lib.rs");
   let start = composition_root.find("fn compare_ui_build(root:").expect("compare-ui build function");
   let end = composition_root[start..].find("fn compare_ui_macos_xctestrun(").expect("compare-ui build end") + start;
   let build = &composition_root[start..end];
   assert!(build.contains("PathBuf::from(\"/private/tmp/oxide-comparison-build-macos\")"));
   assert!(build.contains("ensure!(requested_build_root.is_absolute()"));
   assert!(build.contains("ensure!(!build_root.starts_with(&workspace_root)"));
   assert!(build.contains("fs::canonicalize(&requested_build_root)"));
   assert!(build.contains("format!(\"SYMROOT={}\", build_root.display())"));
   assert!(build.contains("format!(\"OBJROOT={}\", build_root.display())"));
   assert!(build.contains("let bundle = build_root.join(\"Release\").join(bundle_name)"));
   assert!(build.contains("compare_ui_run_bounded_xcodebuild"));
   assert!(build.contains("COMPARE_UI_XCODE_BUILD_ROOT_LIMIT_BYTES"));
   assert!(build.contains("fs::remove_dir_all(build_root)"));
   assert!(!build.contains("project_root.join(\"build\")"));
   assert!(!build.contains("project_root.join(\"build/Release\")"));

   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("build"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--build-root"),
      String::from("relative-build-root"),
   ]).is_err());
   let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("build"),
      String::from("--platform"),
      String::from("macos"),
      String::from("--build-root"),
      workspace.join("comparison-build").to_string_lossy().into_owned(),
   ]).is_err());
}

#[test]
fn compare_ui_apple_pr_dry_run_writes_exact_controller_expansion()
{
   let output = tempdir().expect("output");
   let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
   let result = output.path().join("apple-pr-dry-run.json");
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("run"),
      String::from("--plan"),
      workspace.join("benchmarks/comparative/specs/v1/plans/apple-pr.json").to_string_lossy().into_owned(),
      String::from("--out"),
      result.to_string_lossy().into_owned(),
      String::from("--dry-run"),
   ]).is_ok());
   let value: serde_json::Value = serde_json::from_slice(&fs::read(result).expect("dry-run output")).expect("dry-run JSON");
   assert_eq!(value["build_for_testing_count"], 1);
   assert_eq!(value["acquisitions"].as_array().expect("acquisitions").len(), 4);
   assert_eq!(value["session_slots"].as_array().expect("session slots").len(), 48);
}

#[test]
fn compare_ui_reduce_visual_atomically_writes_an_accepted_report()
{
   let workspace = tempdir().expect("workspace");
   let (reference, candidate, layout, text_geometry) = write_visual_inputs(workspace.path());
   let output = workspace.path().join("nested/visual-report.json");
   fs::create_dir_all(output.parent().expect("report parent")).expect("report parent directory");
   fs::write(&output, b"stale report").expect("stale report");

   let args = reduce_visual_args(&reference, &candidate, &layout, Some(&text_geometry), Some(&output), false);
   assert!(run_cli(&args).is_ok());
   let first = fs::read(&output).expect("visual report");
   assert!(first.ends_with(b"\n"));
   let report: serde_json::Value = serde_json::from_slice(&first).expect("visual report JSON");
   assert_eq!(report["accepted"], true);
   assert_eq!(report["canonical_scale"], 2);
   assert_eq!(report["text_validation"]["status"], "validated");
   assert!(run_cli(&args).is_ok());
   assert_eq!(fs::read(&output).expect("repeated visual report"), first);
   assert_eq!(fs::read_dir(output.parent().expect("report parent")).expect("report directory").count(), 1);
   assert!(run_cli(&reduce_visual_args(&reference, &candidate, &layout, Some(&text_geometry), None, false)).is_ok());
}

#[test]
fn compare_ui_reduce_visual_persists_pending_and_rejected_reports_before_failure()
{
   let workspace = tempdir().expect("workspace");
   let (reference, candidate, layout, text_geometry) = write_visual_inputs(workspace.path());
   let pending_output = workspace.path().join("pending.json");
   let pending_args = reduce_visual_args(&reference, &candidate, &layout, None, Some(&pending_output), false);
   assert!(run_cli(&pending_args).is_err());
   let pending: serde_json::Value = serde_json::from_slice(&fs::read(&pending_output).expect("pending report")).expect("pending report JSON");
   assert_eq!(pending["accepted"], false);
   assert_eq!(pending["text_validation"]["status"], "pending");

   let mut degraded = solid_pixels(16, 16, [72, 96, 120]);
   for pixel in degraded.chunks_exact_mut(3).skip(128).take(32)
   {
      pixel.copy_from_slice(&[0, 0, 0]);
   }
   fs::write(&candidate, rgb_png(16, 16, &degraded)).expect("degraded candidate PNG");
   let rejected_output = workspace.path().join("rejected.json");
   let rejected_args = reduce_visual_args(&reference, &candidate, &layout, Some(&text_geometry), Some(&rejected_output), false);
   assert!(run_cli(&rejected_args).is_err());
   let rejected: serde_json::Value = serde_json::from_slice(&fs::read(&rejected_output).expect("rejected report")).expect("rejected report JSON");
   assert_eq!(rejected["accepted"], false);
   assert_eq!(rejected["non_text_accepted"], false);

   let diagnostic_output = workspace.path().join("diagnostic.json");
   let diagnostic_args = reduce_visual_args(&reference, &candidate, &layout, Some(&text_geometry), Some(&diagnostic_output), true);
   assert!(run_cli(&diagnostic_args).is_ok());
   let diagnostic: serde_json::Value = serde_json::from_slice(&fs::read(&diagnostic_output).expect("diagnostic report")).expect("diagnostic report JSON");
   assert_eq!(diagnostic["accepted"], false);
}

#[test]
fn compare_ui_reduce_visual_requires_canonical_inputs()
{
   assert!(run_cli(&[
      String::from("compare-ui"),
      String::from("reduce-visual"),
      String::from("--reference"),
      String::from("reference.png"),
   ])
   .is_err());
}

#[test]
fn compare_ui_static_exact_rejects_one_changed_channel_and_persists_evidence()
{
   let workspace = tempdir().expect("workspace");
   let (oxide, uikit, layout, _) = write_visual_inputs(workspace.path());
   let output = workspace.path().join("exact.json");
   let accepted_args = compare_static_exact_args(&oxide, &uikit, &layout, &output, false);
   assert!(run_cli(&accepted_args).is_ok());
   let accepted: serde_json::Value = serde_json::from_slice(&fs::read(&output).expect("accepted exact report")).expect("accepted exact report JSON");
   assert_eq!(accepted["accepted"], true);
   assert_eq!(accepted["oxide_png_sha256"].as_str().expect("Oxide input SHA").len(), 64);
   assert_eq!(accepted["uikit_png_sha256"].as_str().expect("UIKit input SHA").len(), 64);
   assert_eq!(accepted["layout_json_sha256"].as_str().expect("layout input SHA").len(), 64);
   assert_eq!(accepted["differing_pixel_count"], 0);

   let mut changed = solid_pixels(16, 16, [72, 96, 120]);
   changed[1] += 1;
   fs::write(&uikit, rgb_png(16, 16, &changed)).expect("changed UIKit PNG");
   assert!(run_cli(&compare_static_exact_args(&oxide, &uikit, &layout, &output, false)).is_err());
   let rejected: serde_json::Value = serde_json::from_slice(&fs::read(&output).expect("rejected exact report")).expect("rejected exact report JSON");
   assert_eq!(rejected["accepted"], false);
   assert_eq!(rejected["differing_pixel_count"], 1);
   assert_eq!(rejected["differing_channel_count"], 1);
   assert!(run_cli(&compare_static_exact_args(&oxide, &uikit, &layout, &output, true)).is_ok());
}

fn with_stub_xcrun<F>(f: F)
where
    F: FnOnce(&Path),
{
    let temp = tempdir().expect("tempdir");
    let bin = temp.path().join("bin");
    fs::create_dir_all(&bin).expect("bin dir");
    let stub = bin.join("xcrun");
    let script = "#!/bin/bash\ncmd=\"${3:-}\"\nif [[ \"$cmd\" == \"metal\" ]]; then\n  out=\"\"\n  for ((i=1;i<=$#;i++)); do\n    arg=${!i}\n    if [[ \"$arg\" == \"-o\" ]]; then\n      j=$((i+1))\n      out=${!j}\n    fi\n  done\n  touch \"$out\"\n  exit 0\nelif [[ \"$cmd\" == \"metallib\" ]]; then\n  out=\"\"\n  for ((i=1;i<=$#;i++)); do\n    arg=${!i}\n    if [[ \"$arg\" == \"-o\" ]]; then\n      j=$((i+1))\n      out=${!j}\n    fi\n  done\n  touch \"$out\"\n  exit 0\nfi\nexit 0\n";
    fs::write(&stub, script).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&stub).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub, perms).expect("chmod");
    }

    let prev_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin.display(), prev_path);
    std::env::set_var("PATH", &new_path);
    f(temp.path());
    std::env::set_var("PATH", prev_path);
}

#[test]
fn shader_bundler_skips_when_no_shaders() {
    let workspace = tempdir().expect("workspace");
    let root = workspace.path();
    let app_dir = root.join("app");
    fs::create_dir_all(&app_dir).expect("app dir");
    assert!(build_and_bundle_shaders(root, &app_dir).is_ok());
    assert!(!app_dir.join("Resources").exists());
}

#[test]
fn shader_bundler_runs_with_stub_compiler() {
    with_stub_xcrun(|root| {
        let shaders = root.join("crates/renderer-metal/shaders");
        fs::create_dir_all(&shaders).expect("shaders dir");
        let shader_path = shaders.join("demo.metal");
        fs::File::create(&shader_path)
            .and_then(|mut f| f.write_all(b"// metal shader"))
            .expect("write shader");

        let app_dir = root.join("app");
        fs::create_dir_all(&app_dir).expect("app dir");
        let prev_target = std::env::var("TARGET").ok();
        std::env::set_var("TARGET", "aarch64-apple-ios-sim");
        let result = build_and_bundle_shaders(root, &app_dir);
        if let Some(val) = prev_target {
            std::env::set_var("TARGET", val);
        } else {
            std::env::remove_var("TARGET");
        }
        assert!(result.is_ok());
        let metallib = app_dir.join("Resources/default.metallib");
        assert!(metallib.exists());
        let air_files: Vec<_> = fs::read_dir(app_dir.join("Resources"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("air"))
            .collect();
        assert!(air_files.is_empty());
    });
}
