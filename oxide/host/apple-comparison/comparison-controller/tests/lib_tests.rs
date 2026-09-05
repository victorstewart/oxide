use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use oxide_apple_comparison_controller::{audit_comparison_source_boundary, build_generic_macos_campaign_plan, build_macos_campaign_plan, comparison_tree_manifest, macos_application_bundle, macos_campaign_sessions_for_scope, macos_controller_result_bundle_path, macos_process_id_from_ps, macos_session_arguments, macos_session_generation, reduce_apple_correctness_evidence, run_macos_campaign, validate_generic_macos_execution_support, validate_macos_appkit_comparator_admission, validate_macos_build_input_identity, validate_macos_build_manifest, validate_macos_gui_lock_ioreg, validate_macos_pair_shape, validate_macos_presentation_trace_bundle, validate_macos_recapture_admission, validate_macos_telemetry_coverage, validate_macos_trace_exports, ComparisonSide, MacOsCampaignConfig, MacOsCampaignScope};
use oxide_benchmark_spec::{balanced_comparison_order, canonical_instrumentation_calibration_input_json, comparison_seed_from_content_sha256, AppleCampaignEvidenceRole, AppleCampaignPassRole, AppleCampaignPlanSpec, ApplePrAcquisitionSpec, ApplePrPlanSpec, BudgetSpec, ComparisonOrder, InstrumentationCalibrationInput, InstrumentationCalibrationPair, ScenarioSpec, TracePairOrder};
use sha2::{Digest, Sha256};

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).expect("workspace root").to_path_buf()
}

fn acquisition() -> ApplePrAcquisitionSpec
{
   let path = workspace_root().join("benchmarks/comparative/specs/v1/acquisition/apple-pr.json");
   serde_json::from_slice(&fs::read(path).expect("acquisition bytes")).expect("acquisition")
}

fn test_tree_manifest(root: &Path) -> (String, u64, u64)
{
   fn collect(root: &Path, directory: &Path, entries: &mut Vec<(PathBuf, bool)>)
   {
      let mut directory_entries = fs::read_dir(directory).expect("tree directory").map(|entry| entry.expect("tree entry")).collect::<Vec<_>>();
      directory_entries.sort_by_key(|entry| entry.file_name());
      for entry in directory_entries
      {
         let path = entry.path();
         let metadata = fs::symlink_metadata(&path).expect("tree metadata");
         if metadata.file_type().is_symlink()
         {
            entries.push((path.strip_prefix(root).expect("tree-relative path").to_path_buf(), true));
         }
         else if metadata.is_dir()
         {
            collect(root, &path, entries);
         }
         else
         {
            entries.push((path.strip_prefix(root).expect("tree-relative path").to_path_buf(), false));
         }
      }
   }

   let mut entries = Vec::new();
   collect(root, root, &mut entries);
   entries.sort();
   let mut digest = Sha256::new();
   let mut bytes = 0_u64;
   for (relative, is_symlink) in &entries
   {
      let path = root.join(relative);
      let contents = if *is_symlink
      {
         fs::read_link(&path).expect("tree symlink").to_string_lossy().into_owned().into_bytes()
      }
      else
      {
         fs::read(&path).expect("tree file")
      };
      bytes += contents.len() as u64;
      let path = relative.to_string_lossy();
      digest.update((path.len() as u64).to_le_bytes());
      digest.update(path.as_bytes());
      digest.update([if *is_symlink {b'L'} else {b'F'}]);
      digest.update((contents.len() as u64).to_le_bytes());
      digest.update(Sha256::digest(&contents));
   }
   (format!("{:x}", digest.finalize()), entries.len() as u64, bytes)
}

fn test_build_product(root: &Path, implementation_id: &str, scheme: &str, bundle_name: &str, executable_name: &str) -> serde_json::Value
{
   let bundle = root.join(bundle_name);
   let executable = bundle.join("Contents/MacOS").join(executable_name);
   let resource = bundle.join("Contents/Resources/spec.json");
   fs::create_dir_all(executable.parent().expect("executable parent")).expect("executable directory");
   fs::create_dir_all(resource.parent().expect("resource parent")).expect("resource directory");
   fs::write(&executable, implementation_id.as_bytes()).expect("executable bytes");
   fs::write(&resource, b"canonical resource").expect("resource bytes");
   let executable_sha256 = format!("{:x}", Sha256::digest(fs::read(&executable).expect("executable bytes")));
   let (bundle_manifest_sha256, bundle_file_count, bundle_bytes) = test_tree_manifest(&bundle);
   serde_json::json!({
      "implementationId": implementation_id,
      "scheme": scheme,
      "bundlePath": bundle,
      "executablePath": executable,
      "executableSha256": executable_sha256,
      "bundleManifestSha256": bundle_manifest_sha256,
      "bundleFileCount": bundle_file_count,
      "bundleBytes": bundle_bytes,
   })
}

fn test_build_controller(root: &Path) -> serde_json::Value
{
   let xctestrun = root.join("MacOSComparisonController_macosx-test-arm64.xctestrun");
   let runner = root.join("Release/MacOSComparisonControllerUITests-Runner.app");
   let test_bundle = runner.join("Contents/PlugIns/MacOSComparisonControllerUITests.xctest/Contents/MacOS/MacOSComparisonControllerUITests");
   fs::create_dir_all(test_bundle.parent().expect("test bundle parent")).expect("test bundle directory");
   fs::write(&xctestrun, b"controller xctestrun").expect("xctestrun bytes");
   fs::write(&test_bundle, b"controller test binary").expect("test binary bytes");
   let xctestrun_sha256 = format!("{:x}", Sha256::digest(fs::read(&xctestrun).expect("xctestrun bytes")));
   let (runner_bundle_manifest_sha256, runner_bundle_file_count, runner_bundle_bytes) = test_tree_manifest(&runner);
   serde_json::json!({
      "scheme": "MacOSComparisonController",
      "xctestrunPath": xctestrun,
      "xctestrunSha256": xctestrun_sha256,
      "runnerBundlePath": runner,
      "runnerBundleManifestSha256": runner_bundle_manifest_sha256,
      "runnerBundleFileCount": runner_bundle_file_count,
      "runnerBundleBytes": runner_bundle_bytes,
   })
}

fn write_test_build_manifest(root: &Path) -> PathBuf
{
   let workspace = workspace_root();
   let benchmark_source_manifest_sha256 = comparison_tree_manifest(&workspace.join("host/apple-comparison"), true).expect("benchmark source manifest").0;
   let specification_manifest_sha256 = comparison_tree_manifest(&workspace.join("benchmarks/comparative/specs/v1"), false).expect("specification manifest").0;
   write_test_build_manifest_with_inputs(root, &benchmark_source_manifest_sha256, &specification_manifest_sha256)
}

fn write_test_build_manifest_with_inputs(root: &Path, benchmark_source_manifest_sha256: &str, specification_manifest_sha256: &str) -> PathBuf
{
   let native = test_build_product(root, "native.production", "AppKitComparison", "Native.app", "Native");
   let oxide = test_build_product(root, "oxide.production", "OxideMacComparison", "Oxide.app", "Oxide");
   let controller = test_build_controller(root);
   let manifest = serde_json::json!({
      "schemaVersion": 2,
      "platform": "macos",
      "configuration": "Release",
      "architecture": "arm64",
      "workspaceGitHead": "a".repeat(40),
      "workspaceStatusSha256": "b".repeat(64),
      "benchmarkSourceManifestSha256": benchmark_source_manifest_sha256,
      "specificationManifestSha256": specification_manifest_sha256,
      "xcodeVersion": "Xcode test",
      "swiftVersion": "Swift test",
      "rustVersion": "rustc test",
      "buildCommandSha256": "e".repeat(64),
      "controller": controller,
      "products": [native, oxide],
   });
   let path = root.join("build-manifest.json");
   fs::write(&path, serde_json::to_vec_pretty(&manifest).expect("manifest JSON")).expect("build manifest");
   path
}

fn write_correctness_evidence(root: &Path, evidence_name: &str, pack_id: &str) -> Vec<PathBuf>
{
   let specification = workspace_root().join("benchmarks/comparative/specs/v1");
   let plan: ApplePrPlanSpec = serde_json::from_slice(&fs::read(specification.join("plans/apple-pr.json")).expect("Apple PR plan bytes")).expect("Apple PR plan");
   let acquisition = acquisition();
   let selected = &acquisition.packs.iter().find(|pack| pack.id == pack_id).expect("selected pack").ordered_scenario_ids;
   let evidence_root = root.join(evidence_name);
   let mut evidence_paths = Vec::new();
   for entry in plan.scenarios.iter().filter(|entry| selected.contains(&entry.id))
   {
      let scenario: ScenarioSpec = serde_json::from_slice(&fs::read(specification.join(&entry.artifact.path)).expect("scenario bytes")).expect("scenario");
      for checkpoint in &scenario.parity_checkpoints
      {
         let directory = evidence_root.join(&scenario.id).join(&checkpoint.id);
         fs::create_dir_all(&directory).expect("correctness checkpoint directory");
         let screenshot = fs::read(specification.join(&checkpoint.screenshot.path)).expect("checkpoint screenshot");
         let screenshot_path = directory.join("screenshot.actual.png");
         fs::write(&screenshot_path, &screenshot).expect("actual screenshot");
         let state = serde_json::to_vec(&serde_json::json!({"checkpoint_id": checkpoint.id, "scenario_id": scenario.id})).expect("state JSON");
         let accessibility = serde_json::to_vec(&serde_json::json!({"checkpoint_id": checkpoint.id, "nodes": [], "scenario_id": scenario.id})).expect("accessibility JSON");
         let geometry_source = if evidence_name == "native.evidence" {"appkit-view-tree"} else {"oxide-semantic-tree"};
         let geometry = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "coordinate_space": "logical-points",
            "capture_profile": "macos-canonical-srgb8-3x-v1",
            "canonical_scale": 3,
            "source": geometry_source,
            "position_tolerance_points": 0,
            "size_tolerance_points": 0,
            "root": {"x": 0, "y": 0, "width": 390, "height": 844},
            "nodes": [{"ordinal": 0, "kind": "text", "role": "heading", "identifier": "title", "bounds": {"x": 16, "y": 20, "width": 200, "height": 48}, "text_line_bounds": [{"x": 16, "y": 20, "width": 200, "height": 48}]}],
         })).expect("geometry JSON");
         fs::write(directory.join("state.actual.json"), &state).expect("actual state");
         fs::write(directory.join("accessibility.actual.json"), &accessibility).expect("actual accessibility");
         fs::write(directory.join("geometry.actual.json"), &geometry).expect("actual geometry");
         let evidence_path = directory.join("evidence.json");
         let identity_root = Path::new("Runs/test/correctness/correctness/1").join(evidence_name).join(&scenario.id).join(&checkpoint.id);
         fs::write(&evidence_path, serde_json::to_vec(&serde_json::json!({
            "actualState": {
               "path": identity_root.join("state.actual.json"),
               "sha256": format!("{:x}", Sha256::digest(&state)),
            },
            "actualAccessibility": {
               "path": identity_root.join("accessibility.actual.json"),
               "sha256": format!("{:x}", Sha256::digest(&accessibility)),
            },
            "actualGeometry": {
               "path": identity_root.join("geometry.actual.json"),
               "sha256": format!("{:x}", Sha256::digest(&geometry)),
            },
            "actualScreenshot": {
               "path": identity_root.join("screenshot.actual.png"),
               "sha256": format!("{:x}", Sha256::digest(&screenshot)),
            },
            "validation": "exact-canonical-state-accessibility-and-role-counts",
         })).expect("evidence JSON")).expect("evidence bytes");
         evidence_paths.push(evidence_path);
      }
   }
   evidence_paths
}

#[test]
fn macos_plan_contains_correctness_then_four_balanced_pairs()
{
   let plan_sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
   let plan = build_macos_campaign_plan(
      &acquisition(),
      "mac-stage1",
      plan_sha256,
   ).expect("macOS plan");
   let expected_orders = balanced_comparison_order(plan.seed.0, 4);
   assert_eq!(plan.schema_version, 2);
   assert_eq!(plan.seed, comparison_seed_from_content_sha256(plan_sha256).expect("plan seed"));
   let serialized = serde_json::to_value(&plan).expect("serialized macOS plan");
   assert_eq!(serialized["seed"], plan.seed.0.to_string());
   assert!(serialized["sessions"].as_array().expect("serialized sessions").iter().all(|session| session["order"] == "ab" || session["order"] == "ba"));
   assert_eq!(plan.sessions.len(), 20);
   assert_eq!(plan.sessions.iter().filter(|session| session.pass_id == "correctness").count(), 4);
   assert_eq!(plan.sessions.iter().filter(|session| session.pass_id == "minimal-presentation").count(), 8);
   assert_eq!(plan.sessions.iter().filter(|session| session.pass_id == "canonical-launch").count(), 8);
   assert_eq!(
      plan.sessions.iter().filter(|session| session.pass_id == "correctness").map(|session| session.pair_index).collect::<Vec<_>>(),
      vec![0, 0, 1, 1],
   );
   for pair in 0..4
   {
      let sides = plan.sessions.iter()
         .filter(|session| session.pass_id == "minimal-presentation" && session.pair_index == pair)
         .map(|session| (session.order, session.side))
         .collect::<Vec<_>>();
      let order = expected_orders[pair as usize];
      let expected = if order == ComparisonOrder::Ab
      {
         vec![(order, ComparisonSide::Native), (order, ComparisonSide::Oxide)]
      }
      else
      {
         vec![(order, ComparisonSide::Oxide), (order, ComparisonSide::Native)]
      };
      assert_eq!(sides, expected);
   }
}

#[test]
fn generic_nightly_plan_expands_every_declared_pass_pack_and_pair_with_its_content_path()
{
   let spec_root = workspace_root().join("benchmarks/comparative/specs/v1");
   let plan_bytes = fs::read(spec_root.join("plans/nightly-apple.json")).expect("nightly plan bytes");
   let plan: AppleCampaignPlanSpec = serde_json::from_slice(&plan_bytes).expect("nightly plan");
   let budget: BudgetSpec = serde_json::from_slice(&fs::read(spec_root.join("budgets/nightly-apple.json")).expect("nightly budget bytes")).expect("nightly budget");
   let expanded = build_generic_macos_campaign_plan(
      &plan,
      &budget,
      &spec_root,
      "plans/nightly-apple.json",
      "nightly-controller",
      &format!("{:x}", Sha256::digest(&plan_bytes)),
   ).expect("expanded nightly campaign");
   assert_eq!(expanded.schema_version, 2);
   assert_eq!(expanded.seed, comparison_seed_from_content_sha256(&expanded.plan_sha256).expect("nightly plan seed"));
   assert_eq!(expanded.plan_resource_path.as_deref(), Some("plans/nightly-apple.json"));
   assert_eq!(expanded.sessions.len(), 78);
   assert_eq!(expanded.sessions.iter().filter(|session| session.pass_id == "correctness").count(), 2);
   assert_eq!(expanded.sessions.iter().filter(|session| session.pass_id == "primary-presentation").count(), 36);
   assert_eq!(expanded.sessions.iter().filter(|session| session.pass_id == "canonical-launch").count(), 20);
   assert_eq!(expanded.sessions.iter().filter(|session| session.pass_id == "idle").count(), 4);
   assert_eq!(expanded.sessions.iter().filter(|session| session.pass_id == "endurance").count(), 4);
   assert_eq!(expanded.sessions.iter().filter(|session| session.pass_id == "attribution-time-profiler").count(), 6);
   assert_eq!(expanded.sessions.iter().filter(|session| session.pass_id == "attribution-physical-footprint").count(), 6);
   let launch = expanded.sessions.iter().filter(|session| session.pass_role == AppleCampaignPassRole::Launch).collect::<Vec<_>>();
   assert_eq!(launch.iter().filter(|session| session.launch_class.as_deref() == Some("terminated-warm-system-cache")).count(), 16);
   assert_eq!(launch.iter().filter(|session| session.launch_class.as_deref() == Some("fresh-install-first-launch")).count(), 4);
   let time_profiler = expanded.sessions.iter().filter(|session| session.collector.as_deref() == Some("time-profiler")).collect::<Vec<_>>();
   assert_eq!(time_profiler.len(), 6);
   assert!(time_profiler.iter().all(|session| session.pass_role == AppleCampaignPassRole::Attribution && session.evidence_role == AppleCampaignEvidenceRole::DescriptiveDiagnostic));
   let primary = expanded.sessions.iter().filter(|session| session.pass_role == AppleCampaignPassRole::Primary).collect::<Vec<_>>();
   assert!(primary.iter().all(|session| session.timing.as_ref().is_some_and(|timing| timing.reset_seconds_per_session == 5 && timing.readiness_timeout_seconds == 20)));
   assert!(time_profiler.iter().all(|session| session.timing.as_ref().is_some_and(|timing| timing.scenarios.len() == 4)));
   for pass in &plan.passes
   {
      let pair_count = if pass.role == AppleCampaignPassRole::Correctness {1} else {pass.pair_count};
      let expected_orders = balanced_comparison_order(expanded.seed.0, pair_count as usize);
      for pair_index in 0..pair_count
      {
         let sessions = expanded.sessions.iter()
            .filter(|session| session.pass_id == pass.id && session.pair_index == pair_index)
            .collect::<Vec<_>>();
         for pair in sessions.chunks_exact(2)
         {
            assert_eq!(pair[0].order, expected_orders[pair_index as usize]);
            validate_macos_pair_shape(pair[0], pair[1]).expect("seeded generic pair shape");
         }
      }
   }
   validate_generic_macos_execution_support(&expanded, MacOsCampaignScope::CorrectnessOnly).expect("generic correctness remains executable");
   validate_generic_macos_execution_support(&expanded, MacOsCampaignScope::Full).expect("generic nightly launch and collector classes are executable");
   let mut system_trace_plan = expanded.clone();
   for session in system_trace_plan.sessions.iter_mut().filter(|session| session.collector.as_deref() == Some("time-profiler"))
   {
      session.collector = Some(String::from("system-trace"));
   }
   validate_generic_macos_execution_support(&system_trace_plan, MacOsCampaignScope::Full).expect("system-trace substitution remains executable");
   let arguments = macos_session_arguments(&expanded, &expanded.sessions[0], Path::new("/tmp/nightly"));
   assert_eq!(arguments.windows(2).find(|pair| pair[0] == "-oxide-compare-plan-path").map(|pair| pair[1].as_str()), Some("plans/nightly-apple.json"));
   assert_eq!(arguments.last().map(String::as_str), Some("-oxide-compare-controlled-start"));
   let xctest = fs::read_to_string(workspace_root().join("host/apple-comparison/MacOSComparisonControllerUITests/MacOSComparisonControllerUITests.swift")).expect("macOS controller XCTest source");
   assert!(xctest.contains("OXIDE_COMPARISON_PLAN_PATH"));
   assert!(xctest.contains("[\"-oxide-compare-plan-path\", planPath]"));
   assert!(xctest.contains("dataContainerWasAbsent"));
   assert!(xctest.contains("Darwin.kill(initialProcessIdentifier, SIGSTOP)"));
   assert!(xctest.contains("waitForProcessStatus(initialProcessIdentifier"));
   assert!(xctest.contains("resumedProcessIdentifier == initialProcessIdentifier"));
   let controller = include_str!("../src/lib.rs");
   assert!(controller.contains("command.env(\"OXIDE_COMPARISON_PLAN_PATH\", path)"));
   assert!(controller.contains("Command::new(\"/usr/bin/ditto\")"));
   assert!(controller.contains(".env(\"OXIDE_COMPARISON_LAUNCH_CLASS\", expected.launch_class.as_str())"));
   assert!(controller.contains("command.env(\"OXIDE_COMPARISON_DATA_ROOT\", data_root)"));
}

#[test]
fn incomplete_release_plan_is_persisted_but_cannot_expand_or_launch()
{
   let spec_root = workspace_root().join("benchmarks/comparative/specs/v1");
   let plan_bytes = fs::read(spec_root.join("plans/apple-release-core.json")).expect("release plan bytes");
   let plan: AppleCampaignPlanSpec = serde_json::from_slice(&plan_bytes).expect("release plan");
   let budget: BudgetSpec = serde_json::from_slice(&fs::read(spec_root.join("budgets/apple-release-core.json")).expect("release budget bytes")).expect("release budget");
   let error = build_generic_macos_campaign_plan(
      &plan,
      &budget,
      &spec_root,
      "plans/apple-release-core.json",
      "release-controller",
      &format!("{:x}", Sha256::digest(&plan_bytes)),
   ).expect_err("missing release artifacts must fail closed");
   assert!(error.to_string().contains("not runnable"));
   assert!(error.to_string().contains("scenarios/grid.large-scroll.json"));
}

#[test]
fn correctness_scope_selects_only_the_four_untimed_sessions()
{
   let plan = build_macos_campaign_plan(
      &acquisition(),
      "mac-stage1",
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
   ).expect("macOS plan");
   let correctness = macos_campaign_sessions_for_scope(&plan, MacOsCampaignScope::CorrectnessOnly);
   assert_eq!(correctness.len(), 4);
   assert!(correctness.iter().all(|session| session.pass_id == "correctness"));
   assert_eq!(macos_campaign_sessions_for_scope(&plan, MacOsCampaignScope::Full).len(), 20);
   assert_eq!(macos_campaign_sessions_for_scope(&plan, MacOsCampaignScope::QualificationOnly).len(), 20);
}

#[test]
fn pending_checkpoint_recaptures_allow_correctness_but_block_qualification_and_full()
{
   let specification = workspace_root().join("benchmarks/comparative/specs/v1");
   let plan_path = specification.join("plans/apple-pr.json");
   let acquisition_path = specification.join("acquisition/apple-pr.json");
   let plan = build_macos_campaign_plan(
      &acquisition(),
      "recapture-admission",
      &format!("{:x}", Sha256::digest(fs::read(&plan_path).expect("plan bytes"))),
   ).expect("macOS plan");
   let config = |scope| MacOsCampaignConfig {
      build_manifest_path: PathBuf::from("unused-build-manifest.json"),
      plan_path: plan_path.clone(),
      acquisition_path: acquisition_path.clone(),
      output_root: PathBuf::from("unused-output"),
      run_id: String::from("recapture-admission"),
      capture_presentation_traces: false,
      xctrace_template: String::from("Logging"),
      scope,
      resume: false,
      require_live_gui_session: false,
      energy_meter_config_path: None,
      instrumentation_calibration_path: None,
   };
   validate_macos_recapture_admission(&config(MacOsCampaignScope::CorrectnessOnly), &plan).expect("correctness recapture remains admissible");
   for scope in [MacOsCampaignScope::QualificationOnly, MacOsCampaignScope::Full]
   {
      let error = validate_macos_recapture_admission(&config(scope), &plan).expect_err("pending recapture must block measured admission");
      assert!(error.to_string().contains("pending recapture status"));
      assert!(error.to_string().contains("chat.live-update/selection-replaced"));
   }
}

#[test]
fn correctness_reducer_accepts_the_complete_calibrated_pack_and_binds_evidence_paths()
{
   let temporary = tempfile::tempdir().expect("temporary correctness evidence");
   write_correctness_evidence(temporary.path(), "oxide.evidence", "pr-launch");
   let native_evidence = write_correctness_evidence(temporary.path(), "native.evidence", "pr-launch");
   let specification = workspace_root().join("benchmarks/comparative/specs/v1");
   let report = reduce_apple_correctness_evidence(
      &temporary.path().join("oxide.evidence"),
      &temporary.path().join("native.evidence"),
      &specification,
      Some("pr-launch"),
   ).expect("accepted calibrated-static launch pack");
   assert_eq!(report.schema_version, 5);
   assert_eq!(report.algorithm, "apple-correctness-semantic-region-static-matrix-v5");
   assert_eq!(report.accepted_checkpoint_count, 3);
   assert_eq!(report.rejected_checkpoint_count, 0);
   assert!(report.accepted);
   assert!(report.checkpoints.iter().all(|checkpoint| checkpoint.geometry_accepted && checkpoint.canonical_scale == 3 && checkpoint.geometry_capture_profile == "macos-canonical-srgb8-3x-v1"));

   let evidence_path = &native_evidence[0];
   let mut evidence: serde_json::Value = serde_json::from_slice(&fs::read(evidence_path).expect("evidence bytes")).expect("evidence JSON");
   evidence["actualScreenshot"]["path"] = serde_json::Value::String(String::from("Runs/test/oxide.evidence/wrong/checkpoint/screenshot.actual.png"));
   fs::write(evidence_path, serde_json::to_vec(&evidence).expect("mutated evidence JSON")).expect("mutated evidence bytes");
   let error = reduce_apple_correctness_evidence(
      &temporary.path().join("oxide.evidence"),
      &temporary.path().join("native.evidence"),
      &specification,
      Some("pr-launch"),
   ).expect_err("unbound screenshot path must fail closed");
   assert!(error.to_string().contains("does not bind the evidence root and checkpoint"));
}

#[test]
fn every_campaign_scope_ends_on_symmetric_pair_boundaries()
{
   let plan = build_macos_campaign_plan(
      &acquisition(),
      "mac-stage1",
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
   ).expect("macOS plan");
   for scope in [MacOsCampaignScope::CorrectnessOnly, MacOsCampaignScope::QualificationOnly, MacOsCampaignScope::Full]
   {
      let sessions = macos_campaign_sessions_for_scope(&plan, scope);
      let pairs = sessions.chunks_exact(2);
      assert!(pairs.remainder().is_empty());
      for pair in pairs
      {
         validate_macos_pair_shape(pair[0], pair[1]).expect("symmetric macOS pair");
      }
   }
   let mut invalid = plan.sessions[1].clone();
   invalid.side = plan.sessions[0].side;
   assert!(validate_macos_pair_shape(&plan.sessions[0], &invalid).is_err());
   let mut invalid = plan.sessions[1].clone();
   invalid.order = if invalid.order == ComparisonOrder::Ab {ComparisonOrder::Ba} else {ComparisonOrder::Ab};
   assert!(validate_macos_pair_shape(&plan.sessions[0], &invalid).is_err());
}

#[test]
fn session_arguments_bind_every_identity_and_output_root()
{
   let plan = build_macos_campaign_plan(
      &acquisition(),
      "mac-stage1",
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
   ).expect("macOS plan");
   let arguments = macos_session_arguments(&plan, &plan.sessions[0], Path::new("/tmp/comparison"));
   let generation = macos_session_generation(&plan, &plan.sessions[0]);
   assert_eq!(arguments.windows(2).find(|pair| pair[0] == "-oxide-compare-plan-sha").map(|pair| pair[1].as_str()), Some(plan.plan_sha256.as_str()));
   assert_eq!(arguments.windows(2).find(|pair| pair[0] == "-oxide-compare-generation").map(|pair| pair[1].as_str()), Some(generation.as_str()));
   assert_eq!(arguments.windows(2).find(|pair| pair[0] == "-oxide-compare-output-root").map(|pair| pair[1].as_str()), Some("/tmp/comparison"));
   assert_eq!(arguments.last().map(String::as_str), Some("-oxide-compare-controlled-start"));
   assert_eq!(generation.len(), 64);
}

#[test]
fn launch_controller_result_bundles_are_session_owned_bounded_and_cleanup_guarded()
{
   let plan = build_macos_campaign_plan(
      &acquisition(),
      "mac-stage1",
      "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
   ).expect("macOS plan");
   let measure = macos_controller_result_bundle_path(&plan, &plan.sessions[0], Path::new("/tmp/comparison"), "measure").expect("measure result bundle");
   let primer = macos_controller_result_bundle_path(&plan, &plan.sessions[0], Path::new("/tmp/comparison"), "cache-primer").expect("primer result bundle");
   assert!(measure.starts_with("/tmp/comparison/Runs/mac-stage1"));
   assert_eq!(measure.extension().and_then(|extension| extension.to_str()), Some("xcresult"));
   assert_ne!(measure, primer);
   assert!(macos_controller_result_bundle_path(&plan, &plan.sessions[0], Path::new("/tmp/comparison"), "unknown").is_err());

   let controller = include_str!("../src/lib.rs");
   assert!(controller.contains(".arg(\"-resultBundlePath\")"));
   assert!(controller.contains("refusing to overwrite session-owned XCTest result bundle"));
   assert!(controller.contains("impl Drop for MacOsControllerResultBundle"));
   assert!(controller.contains("fs::remove_dir_all(&self.path)"));
}

#[test]
fn exact_bundle_and_process_resolution_reject_lookalikes()
{
   let temporary = tempfile::tempdir().expect("temporary application bundle");
   let macos = temporary.path().join("Exact.app/Contents/MacOS");
   fs::create_dir_all(&macos).expect("application executable directory");
   let executable = macos.join("Exact");
   fs::write(&executable, b"binary").expect("application executable");
   assert_eq!(macos_application_bundle(&executable).expect("exact application bundle"), temporary.path().join("Exact.app"));
   assert!(macos_application_bundle(&temporary.path().join("Exact")).is_err());

   let ps = format!(
      "  101 {} -oxide-compare-run-id exact\n  102 /bin/sh -c {}\n  103 {}-lookalike\n",
      executable.display(),
      executable.display(),
      executable.display(),
   );
   assert_eq!(macos_process_id_from_ps(&executable, &ps).expect("anchored process match"), Some(101));
   assert_eq!(macos_process_id_from_ps(&executable, "  102 /bin/sh -c /tmp/Exact\n").expect("no shell substring match"), None);
}

#[test]
fn build_manifest_admission_rehashes_complete_application_bundles()
{
   let temporary = tempfile::tempdir().expect("temporary build identity root");
   let native = test_build_product(temporary.path(), "native.production", "AppKitComparison", "Native.app", "Native");
   let oxide = test_build_product(temporary.path(), "oxide.production", "OxideMacComparison", "Oxide.app", "Oxide");
   let controller = test_build_controller(temporary.path());
   let manifest = serde_json::json!({
      "schemaVersion": 2,
      "platform": "macos",
      "configuration": "Release",
      "architecture": "arm64",
      "workspaceGitHead": "a".repeat(40),
      "workspaceStatusSha256": "b".repeat(64),
      "benchmarkSourceManifestSha256": "c".repeat(64),
      "specificationManifestSha256": "d".repeat(64),
      "xcodeVersion": "Xcode test",
      "swiftVersion": "Swift test",
      "rustVersion": "rustc test",
      "buildCommandSha256": "e".repeat(64),
      "controller": controller,
      "products": [native, oxide],
   });
   let manifest_path = temporary.path().join("build-manifest.json");
   fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).expect("manifest JSON")).expect("build manifest");
   validate_macos_build_manifest(&manifest_path).expect("complete build identity");

   let controller_binary = temporary.path().join("Release/MacOSComparisonControllerUITests-Runner.app/Contents/PlugIns/MacOSComparisonControllerUITests.xctest/Contents/MacOS/MacOSComparisonControllerUITests");
   fs::write(&controller_binary, b"mutated controller").expect("mutated controller");
   let error = validate_macos_build_manifest(&manifest_path).expect_err("controller mutation must fail");
   assert!(error.to_string().contains("launch-controller runner bundle differs"));
   fs::write(&controller_binary, b"controller test binary").expect("restored controller");
   validate_macos_build_manifest(&manifest_path).expect("restored build identity");

   fs::write(temporary.path().join("Oxide.app/Contents/Resources/spec.json"), b"mutated resource").expect("mutated resource");
   let error = validate_macos_build_manifest(&manifest_path).expect_err("bundle mutation must fail");
   assert!(error.to_string().contains("oxide.production application bundle differs"));
}

#[test]
fn campaign_build_input_identity_rehashes_source_and_specification_trees()
{
   let temporary = tempfile::tempdir().expect("temporary build-input identity root");
   let workspace = temporary.path().join("workspace");
   let source = workspace.join("host/apple-comparison");
   let specification = workspace.join("benchmarks/comparative/specs/v1");
   let plan = specification.join("plans/apple-pr.json");
   fs::create_dir_all(source.join("build")).expect("comparison source build directory");
   fs::create_dir_all(plan.parent().expect("plan parent")).expect("comparison plans directory");
   fs::write(workspace.join("Cargo.toml"), "[workspace]\n").expect("workspace manifest");
   fs::write(source.join("controller.rs"), "source-v1\n").expect("comparison source");
   fs::write(source.join("build/ignored"), "ignored-v1\n").expect("excluded build output");
   fs::write(&plan, "plan-v1\n").expect("comparison plan");
   let source_sha256 = comparison_tree_manifest(&source, true).expect("source identity").0;
   let specification_sha256 = comparison_tree_manifest(&specification, false).expect("specification identity").0;
   let products = temporary.path().join("products");
   let manifest = write_test_build_manifest_with_inputs(&products, &source_sha256, &specification_sha256);

   validate_macos_build_input_identity(&manifest, &plan).expect("matching build inputs");
   fs::write(source.join("build/ignored"), "ignored-v2\n").expect("mutated excluded build output");
   validate_macos_build_input_identity(&manifest, &plan).expect("excluded build output must not invalidate source identity");

   fs::write(source.join("controller.rs"), "source-v2\n").expect("mutated comparison source");
   let error = validate_macos_build_input_identity(&manifest, &plan).expect_err("source mutation must fail");
   assert!(error.to_string().contains("benchmark source tree differs"));

   fs::write(source.join("controller.rs"), "source-v1\n").expect("restored comparison source");
   fs::write(&plan, "plan-v2\n").expect("mutated comparison plan");
   let error = validate_macos_build_input_identity(&manifest, &plan).expect_err("specification mutation must fail");
   assert!(error.to_string().contains("specification tree differs"));
}

#[test]
fn run_plan_rejects_path_traversal_and_noncanonical_hashes()
{
   assert!(build_macos_campaign_plan(&acquisition(), "../escape", &"a".repeat(64)).is_err());
   assert!(build_macos_campaign_plan(&acquisition(), "safe", &"A".repeat(64)).is_err());
}

#[test]
fn comparison_harness_markers_remain_outside_production_sources()
{
   audit_comparison_source_boundary(&workspace_root()).expect("isolated comparison harnesses");
}

#[test]
fn trace_exports_require_concrete_hitches_schemas_and_all_generation_markers()
{
   let toc = r#"<trace-toc><table schema="os-signpost"/><table schema="hitches-updates"/><table schema="hitches-frame-lifetimes"/></trace-toc>"#;
   let signposts = "InputReceived VisualGeneration DisplayOpportunity";
   validate_macos_trace_exports(toc, signposts).expect("complete trace exports");
   assert!(validate_macos_trace_exports("<table schema=\"os-signpost\"/>", signposts).is_err());
   assert!(validate_macos_trace_exports(r#"<table schema="os-signpost"/><table schema="hitches-updates"/>"#, signposts).is_err());
   assert!(validate_macos_trace_exports(toc, "InputReceived VisualGeneration").is_err());
}

#[test]
fn presentation_trace_bundle_rejects_an_unfinalized_document_shell()
{
   let temporary = tempfile::tempdir().expect("temporary trace root");
   let trace = temporary.path().join("comparison.trace");
   fs::create_dir_all(trace.join("Trace1.run")).expect("trace shell directory");
   fs::write(trace.join("Trace1.run/RunIssues.storedata"), b"incomplete").expect("trace shell issue store");
   assert!(validate_macos_presentation_trace_bundle(&trace).is_err());

   fs::write(trace.join("form.template"), b"template").expect("trace form template");
   for directory in ["corespace", "instrument_data", "shared_data"]
   {
      fs::create_dir(trace.join(directory)).expect("finalized trace directory");
   }
   validate_macos_presentation_trace_bundle(&trace).expect("finalized trace bundle");
}

#[test]
fn presentation_trace_waits_for_xctrace_start_notification()
{
   let controller = include_str!("../src/lib.rs");
   let start = controller.find("fn start_presentation_trace(").expect("presentation trace start");
   let end = controller[start..].find("fn finish_presentation_trace(").expect("presentation trace start end") + start;
   let acquisition = &controller[start..end];
   assert!(acquisition.contains("--notify-tracing-started"));
   assert!(acquisition.contains("\"--instrument\", \"os_signpost\""));
   assert!(!controller.contains("\"--instrument\", \"Hitches\""));
   assert!(acquisition.contains("wait_for_presentation_trace_start"));
   assert!(acquisition.contains("MacOsTraceScratch::create"));
   assert!(acquisition.contains("command.env(\"TMPDIR\""));
   assert!(acquisition.contains("let trace_seconds = session.max_occupied_seconds.max(1);"));
   assert!(!acquisition.contains("max_occupied_seconds.saturating_sub(30)"));
   assert!(controller.contains("MACOS_TRACE_WORKING_SET_LIMIT_BYTES"));
   assert!(controller.contains("trace_working_set_bytes(&paths.trace, &paths.trace_scratch)"));
   assert!(controller.contains("fn finish_presentation_trace(trace: &mut MacOsPresentationTrace, trace_path: &Path) -> Result<()>\n{\n   interrupt_presentation_trace(trace)?;"));
   assert!(controller.contains("post_notification(stop_notification)?;\n      wait_for_exact_process_exit(executable, pid, Duration::from_secs(10))\n   })();\n   if let Err(error) = stopped"));
   assert!(!acquisition.contains("thread::sleep(Duration::from_secs(1))"));
}

#[test]
fn every_xctrace_child_has_an_isolated_temporary_environment()
{
   let controller = include_str!("../src/lib.rs");
   let system_trace = include_str!("../src/system_trace.rs");
   let child_count = controller.matches("native_xcrun_command()").count() - 1
      + system_trace.matches("native_xcrun_command()").count();
   for variable in ["TMPDIR", "TMP", "TEMP"]
   {
      let binding = format!(".env(\"{}\"", variable);
      assert_eq!(controller.matches(&binding).count() + system_trace.matches(&binding).count(), child_count);
   }
   assert!(controller.contains("let mut scratch = MacOsTraceScratch::create(&scratch_path)?;"));
   assert!(controller.contains("scratch.cleanup()?;"));
}

#[test]
fn comparison_xcode_builds_never_write_the_workspace_cargo_target()
{
   let project = include_str!("../../AppleComparison.xcodeproj/project.pbxproj");
   let project_source = include_str!("../../project.yml");
   let guard = include_str!("../../scripts/guard-cargo-target.sh");
   assert!(!project.contains("CARGO_TARGET_DIR=\\\"${REPO_ROOT}/target\\\""));
   assert_eq!(project.matches("CARGO_TARGET_DIR=\\\"${OBJROOT}/oxide-comparison-cargo/${PLATFORM_NAME}\\\"").count(), 2);
   assert_eq!(project.matches("guard-cargo-target.sh").count(), 2);
   assert!(!project_source.contains("CARGO_TARGET_DIR=\"${REPO_ROOT}/target\""));
   assert_eq!(project_source.matches("CARGO_TARGET_DIR=\"${OBJROOT}/oxide-comparison-cargo/${PLATFORM_NAME}\"").count(), 2);
   assert_eq!(project_source.matches("guard-cargo-target.sh").count(), 2);
   assert!(guard.contains("OXIDE_COMPARISON_TARGET_LIMIT_KIB:-4194304"));
   assert!(guard.contains("cargo clean --manifest-path"));
   assert!(guard.contains("exit 70"));
}

#[test]
fn comparison_cargo_target_guard_cleans_an_observed_breach_and_fails_closed()
{
   let root = tempfile::tempdir().expect("guard test root");
   let source = root.path().join("src");
   let target = root.path().join("target");
   fs::create_dir(&source).expect("guard package source");
   fs::create_dir(&target).expect("guard target");
   fs::write(
      root.path().join("Cargo.toml"),
      "[package]\nname = \"comparison-guard-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
   ).expect("guard manifest");
   fs::write(source.join("lib.rs"), "pub fn fixture() {}\n").expect("guard source");
   fs::write(target.join("oversized"), [0_u8; 4_096]).expect("oversized target fixture");
   let output = Command::new("/bin/sh")
      .arg(workspace_root().join("host/apple-comparison/scripts/guard-cargo-target.sh"))
      .arg(&target)
      .arg(root.path().join("Cargo.toml"))
      .env("OXIDE_COMPARISON_TARGET_LIMIT_KIB", "1")
      .output()
      .expect("run comparison target guard");
   assert_eq!(output.status.code(), Some(70));
   assert!(!target.exists());
   assert!(String::from_utf8(output.stderr).expect("guard stderr").contains("cleaning it and failing closed"));
}

#[test]
fn campaign_resume_is_owned_by_atomic_pair_checkpoints()
{
   let controller = include_str!("../src/lib.rs");
   let campaign_start = controller.find("pub fn run_macos_campaign(").expect("macOS campaign runner");
   let campaign_end = controller[campaign_start..].find("pub fn macos_session_arguments(").expect("macOS campaign runner end") + campaign_start;
   let campaign = &controller[campaign_start..campaign_end];
   assert!(campaign.contains("sessions.chunks_exact(2)"));
   assert!(campaign.contains("predecessor_pair_sha256"));
   assert!(campaign.contains("run_or_resume_pair"));

   let pair_start = controller.find("fn run_or_resume_pair(").expect("pair runner");
   let pair_end = controller[pair_start..].find("fn run_or_resume_session(").expect("pair runner end") + pair_start;
   let pair = &controller[pair_start..pair_end];
   assert!(pair.contains("pair.complete.json"));
   assert!(pair.contains("cross-invocation side completion is forbidden"));
   assert!(pair.contains("recovered-before-pair-checkpoint"));
   assert!(pair.contains("refusing to overwrite atomic pair checkpoint"));
}

#[test]
fn session_artifacts_are_namespaced_by_pack_before_pair_index()
{
   let controller = include_str!("../src/lib.rs");
   let paths_start = controller.find("fn session_paths(").expect("session paths");
   let paths_end = controller[paths_start..].find("fn export_and_validate_trace(").expect("session paths end") + paths_start;
   let paths = &controller[paths_start..paths_end];
   assert!(paths.contains(".join(&session.pack_id)"));
   assert!(paths.find(".join(&session.pack_id)").expect("pack path")
      < paths.find(".join(session.pair_index.to_string())").expect("pair path"));

   for source in [
      include_str!("../../Shared/ComparatorApp/BenchmarkCampaignExecutor.swift"),
      include_str!("../../Shared/ComparatorApp/MacOSCampaignControl.swift"),
      include_str!("../../Shared/ComparatorApp/MacOSTrustedInputControl.swift"),
      include_str!("../../Shared/ComparatorApp/MacOSCanonicalLaunchExecutor.swift"),
      include_str!("../../MacOSComparisonControllerUITests/MacOSComparisonControllerUITests.swift"),
   ]
   {
      assert!(source.contains("packID"));
   }
}

#[test]
fn measured_pairs_are_owned_by_the_live_exact_static_gate()
{
   let controller = include_str!("../src/lib.rs");
   let campaign_start = controller.find("pub fn run_macos_campaign(").expect("macOS campaign runner");
   let campaign_end = controller[campaign_start..].find("pub fn macos_session_arguments(").expect("macOS campaign runner end") + campaign_start;
   let campaign = &controller[campaign_start..campaign_end];
   assert!(campaign.contains("first.pass_id != \"correctness\" && !correctness_gate_accepted"));
   assert!(campaign.contains("macOS measured acquisition is blocked because the complete calibrated-static correctness matrix was not accepted"));
   assert!(campaign.contains("full macOS acquisition requires the exact Animation Hitches minimal-presentation template"));
   assert!(campaign.contains("campaign.complete.json"));

   let pair_start = controller.find("fn persist_pair_checkpoint(").expect("pair checkpoint persistence");
   let pair_end = controller[pair_start..].find("fn session_evidence_matches(").expect("pair checkpoint persistence end") + pair_start;
   let checkpoint = &controller[pair_start..pair_end];
   assert!(checkpoint.contains("reduce_or_validate_pair_correctness"));
   assert!(checkpoint.contains("pair.correctness.json"));
   assert!(checkpoint.contains("schema_version: 2"));
   assert!(controller.contains("pair.complete.v2.json"));
}

#[test]
fn schema_one_pair_checkpoint_fails_closed_without_overwrite_or_launch()
{
   let temporary = tempfile::tempdir().expect("temporary legacy pair recovery");
   let specification = workspace_root().join("benchmarks/comparative/specs/v1");
   let plan_path = specification.join("plans/apple-pr.json");
   let acquisition_path = specification.join("acquisition/apple-pr.json");
   let plan_sha256 = format!("{:x}", Sha256::digest(fs::read(&plan_path).expect("plan bytes")));
   let build_manifest_path = write_test_build_manifest(temporary.path());
   let output = temporary.path().join("output");
   let directory = output.join("Runs/legacy-pair/correctness/correctness/pr-non-launch/0");
   fs::create_dir_all(&directory).expect("legacy pair directory");
   let legacy_path = directory.join("pair.complete.json");
   let legacy = serde_json::to_vec_pretty(&serde_json::json!({
      "schema_version": 1,
      "run_id": "legacy-pair",
      "plan_sha256": plan_sha256,
      "build_manifest_sha256": format!("{:x}", Sha256::digest(fs::read(&build_manifest_path).expect("build manifest bytes"))),
      "chunk_id": "correctness",
      "pass_id": "correctness",
      "pack_id": "pr-non-launch",
      "pair_index": 0,
      "predecessor_pair_sha256": null,
      "sessions": [],
      "complete": true,
   })).expect("legacy checkpoint JSON");
   fs::write(&legacy_path, &legacy).expect("legacy checkpoint");
   let config = MacOsCampaignConfig {
      build_manifest_path,
      plan_path,
      acquisition_path,
      output_root: output,
      run_id: String::from("legacy-pair"),
      capture_presentation_traces: false,
      xctrace_template: String::from("Logging"),
      scope: MacOsCampaignScope::CorrectnessOnly,
      resume: true,
      require_live_gui_session: false,
      energy_meter_config_path: None,
      instrumentation_calibration_path: None,
   };
   let error = run_macos_campaign(&config).expect_err("legacy checkpoint without complete side evidence must fail before launch");
   assert!(error.to_string().contains("preserved but has no complete side evidence"));
   assert_eq!(fs::read(&legacy_path).expect("preserved legacy checkpoint"), legacy);
   assert!(!directory.join("pair.complete.v2.json").exists());
   assert!(!directory.join("oxide.stdout.log").exists());
   assert!(!directory.join("native.stdout.log").exists());
}

#[test]
fn current_pending_appkit_comparator_cannot_enter_measured_acquisition()
{
   let plan = workspace_root().join("benchmarks/comparative/specs/v1/plans/apple-pr.json");
   let error = validate_macos_appkit_comparator_admission(&plan).expect_err("pending AppKit comparator must not be admitted");
   assert!(error.to_string().contains("status is not accepted"));
}

#[test]
fn full_campaign_rejects_missing_or_failed_instrumentation_calibration_before_gui_and_build_work()
{
   let temporary = tempfile::tempdir().expect("instrumentation preflight fixture");
   let output = temporary.path().join("output");
   let mut config = MacOsCampaignConfig {
      build_manifest_path: temporary.path().join("missing-build.json"),
      plan_path: temporary.path().join("missing-plan.json"),
      acquisition_path: temporary.path().join("missing-acquisition.json"),
      output_root: output.clone(),
      run_id: String::from("calibration-preflight"),
      capture_presentation_traces: true,
      xctrace_template: String::from("Animation Hitches"),
      scope: MacOsCampaignScope::Full,
      resume: false,
      require_live_gui_session: true,
      energy_meter_config_path: None,
      instrumentation_calibration_path: None,
   };
   let missing = run_macos_campaign(&config).expect_err("missing calibration must fail");
   assert!(missing.to_string().contains("requires --instrumentation-calibration"));
   assert!(!output.exists());

   let calibration = InstrumentationCalibrationInput {
      schema_version: 1,
      calibration_id: String::from("rejected"),
      platform_role: String::from("macos-apple-silicon"),
      template_id: String::from("Animation Hitches"),
      sensor_sample_hz: 1_000,
      alpha: 0.05,
      pairs: (0..8).map(|pair_index| InstrumentationCalibrationPair {
         pair_index,
         order: if pair_index % 2 == 0 {TracePairOrder::TraceOnFirst} else {TracePairOrder::TraceOffFirst},
         trace_on_sensor_p50_ms: 10.2,
         trace_off_sensor_p50_ms: 10.0,
         trace_on_sensor_p95_ms: 20.0,
         trace_off_sensor_p95_ms: 20.0,
         trace_on_process_cpu_ms: 100.0,
         trace_off_process_cpu_ms: 100.0,
         valid: true,
      }).collect(),
   };
   let path = temporary.path().join("rejected-calibration.json");
   fs::write(&path, canonical_instrumentation_calibration_input_json(&calibration).expect("canonical calibration")).expect("calibration input");
   config.instrumentation_calibration_path = Some(path);
   let rejected = run_macos_campaign(&config).expect_err("rejected calibration must fail");
   assert!(rejected.to_string().contains("rejected or differs"));
   assert!(!output.exists());
}

#[test]
fn resource_collector_contract_stays_controller_owned_and_starts_at_pid_discovery()
{
   let controller = include_str!("../src/lib.rs");
   let resources = include_str!("../src/resource.rs");
   let collector_start = controller.find("MacOsResourceCollector::start(pid, &session.pass_id)").expect("resource collector start");
   let activation = controller.find("activate_exact_process(pid)?").expect("exact process activation");
   assert!(collector_start < activation);
   assert!(resources.contains("MACOS_RESOURCE_CADENCE_NS: u64 = 50_000_000"));
   assert!(resources.contains("MACOS_LOW_FREQUENCY_RESOURCE_CADENCE_NS: u64 = 1_000_000_000"));
   assert!(resources.contains("\"idle\" | \"endurance\" | \"energy\" | \"memory\" | \"attribution-physical-footprint\""));
   assert!(controller.contains("if measured_session && !energy_requested"));
   assert!(resources.contains("proc_pid_rusage(pid, 4, &mut usage)"));
   assert!(resources.contains("CollectorCommand::Finish"));
   assert!(controller.contains("resource_sha256: &'a str"));
   assert!(controller.contains("resource_sha256: String"));
}

#[test]
fn time_profiler_collector_is_isolated_from_the_primary_presentation_trace()
{
   let controller = include_str!("../src/lib.rs");
   let executor = include_str!("../../Shared/ComparatorApp/BenchmarkCampaignExecutor.swift");
   assert!(controller.contains("session.collector.as_deref() == Some(\"time-profiler\")"));
   assert!(controller.contains("start_attached_trace(\"Time Profiler\", \"time-profiler\""));
   assert!(controller.contains("&& !time_profiler_requested"));
   assert!(controller.contains("Time Profiler attribution must have one isolated complete Time Profiler trace and no Animation Hitches trace"));
   assert!(controller.contains("session_trace_working_set_bytes(paths)? > MACOS_TRACE_WORKING_SET_LIMIT_BYTES"));
   assert!(controller.contains("/data/table[@schema=\\\"time-profile\\\"]"));
   assert!(controller.contains("/data/table[@schema=\\\"os-signpost\\\"]"));
   for marker in ["ScenarioBegin", "ScenarioEnd", "PhaseBegin", "PhaseEnd"]
   {
      assert!(executor.contains(marker));
   }
   assert!(executor.contains("measured=%{public}d"));
}

#[test]
fn common_gpu_is_exact_pid_phase_bounded_and_never_claim_bearing()
{
   let controller = include_str!("../src/lib.rs");
   let collector = include_str!("../src/common_gpu.rs");
   let release = include_str!("../../../../benchmarks/comparative/specs/v1/plans/apple-release-core.json");
   let claim_complete = include_str!("../../../../benchmarks/comparative/specs/v1/plans/apple-release-claim-complete.json");
   assert!(collector.contains("task_name_for_pid"));
   assert!(collector.contains("task_info(port.0, 26"));
   assert!(collector.contains("task-info-power-v2-task-gpu-utilisation-ns"));
   assert!(collector.contains("process-scoped-diagnostic-only-compositor-ownership-asymmetric"));
   assert!(collector.contains("comparison_eligible: false"));
   assert!(controller.contains("start_attached_trace(\"Points of Interest\", \"common-gpu\""));
   assert!(controller.contains("claim-bearing common-gpu is unavailable"));
   assert!(claim_complete.contains("\"id\": \"common-gpu\",\n      \"role\": \"attribution\",\n      \"evidence_role\": \"descriptive-diagnostic\""));
   assert!(controller.contains("!low_frequency_window"));
   assert!(release.contains("\"pass_id\": \"idle\""));
   assert!(release.contains("\"duration_seconds\": 60"));
   assert!(release.contains("\"pass_id\": \"endurance\""));
   assert!(release.contains("\"duration_seconds\": 300"));
}

#[test]
fn system_trace_collector_is_exact_pid_isolated_fused_and_independently_reduced()
{
   let controller = include_str!("../src/lib.rs");
   let system_trace = include_str!("../src/system_trace.rs");
   assert!(controller.contains("session.collector.as_deref() == Some(\"system-trace\")"));
   assert!(controller.contains("&& !time_profiler_requested && !system_trace_requested"));
   assert!(system_trace.contains("\"--template\", \"System Trace\""));
   assert!(!system_trace.contains("--instrument"));
   assert!(system_trace.contains("\"--attach\", &pid.to_string()"));
   assert!(system_trace.contains(".process_group(0)"));
   assert!(system_trace.contains("SYSTEM_TRACE_WORKING_SET_LIMIT_BYTES"));
   assert!(system_trace.contains("terminate_process_group(process_group)"));
   for schema in ["os-signpost", "thread-info", "thread-state", "context-switch"]
   {
      assert!(system_trace.contains(schema));
   }
   assert!(controller.contains("System Trace attribution must have one isolated complete System Trace and no Animation Hitches or Time Profiler trace"));
   assert!(controller.contains("reduce_macos_system_trace("));
}

#[test]
fn foreground_activation_retries_the_exact_pid_before_failing_closed()
{
   let controller = include_str!("../src/lib.rs");
   let start = controller.find("fn activate_exact_process(pid: u32)").expect("foreground activation");
   let end = controller[start..].find("fn post_notification").expect("foreground activation end") + start;
   let activation = &controller[start..end];
   assert!(activation.contains("first process whose unix id is {}"));
   assert!(activation.contains("Duration::from_secs(10)"));
   assert!(activation.contains("Duration::from_millis(50)"));
   assert!(activation.contains("String::from_utf8_lossy(&output.stdout).trim() == \"true\""));
}

#[test]
fn campaign_rejects_a_locked_gui_session_before_expensive_validation()
{
   let controller = include_str!("../src/lib.rs");
   let start = controller.find("pub fn run_macos_campaign(").expect("campaign entry point");
   let end = controller[start..].find("pub fn validate_macos_appkit_comparator_admission").expect("campaign entry point end") + start;
   let campaign = &controller[start..end];
   let gui = campaign.find("validate_macos_gui_session()?").expect("GUI preflight");
   let wake = campaign.find("MacOsWakeAssertion::acquire()?").expect("bounded wake assertion");
   let plan = campaign.find("fs::read(&config.plan_path)").expect("plan read");
   assert!(gui < plan);
   assert!(gui < wake && wake < plan);
   assert!(campaign.contains("frontmost is true"));
   assert!(campaign.contains("frontmost == \"loginwindow\""));
   assert!(campaign.contains("measured macOS acquisition cannot disable live GUI-session validation"));
   assert!(controller.contains(".args([\"-d\", \"-i\", \"-w\", &pid])"));
   assert!(controller.contains("impl Drop for MacOsWakeAssertion"));
}

#[test]
fn foreground_activation_waits_for_the_durable_ready_barrier()
{
   let controller = include_str!("../src/lib.rs");
   let session_start = controller.find("fn run_foreground_session(").expect("foreground session");
   let session_end = controller[session_start..].find("fn read_and_validate_ready").expect("foreground session end") + session_start;
   let session = &controller[session_start..session_end];
   assert!(session.contains("wait_for_ready_notification("));
   assert!(session.contains("Duration::from_secs(session.max_occupied_seconds)"));
   let ready = session.find("read_and_validate_ready(plan, session, paths, generation)?").expect("durable ready validation");
   assert!(!session[..ready].contains("Duration::from_secs(20)"));
   let measured = session[ready..].find("if measured_session").expect("measured-session activation gate") + ready;
   let activation = session.find("activate_exact_process(pid)?").expect("exact process activation");
   let start = session.find("post_notification(start_notification)?").expect("controlled start notification");
   assert!(ready < measured);
   assert!(measured < activation);
   assert!(activation < start);
}

#[test]
fn readiness_waiter_fails_immediately_when_the_exact_process_exits()
{
   let controller = include_str!("../src/lib.rs");
   let start = controller.find("fn wait_for_ready_notification(").expect("readiness waiter");
   let end = controller[start..].find("fn exact_process_id").expect("readiness waiter end") + start;
   let waiter = &controller[start..end];
   let durable = waiter.find("if ready_path.is_file()").expect("durable readiness check");
   let notification = waiter.find("child.try_wait()").expect("notification readiness check");
   let process = waiter.find("match exact_process_id(executable)?").expect("process identity check");
   assert!(durable < notification);
   assert!(notification < process);
   assert!(waiter.contains("terminate_child(child)"));
   assert!(waiter.contains("match exact_process_id(executable)?"));
   assert!(waiter.contains("Some(observed) if observed == pid"));
   assert!(waiter.contains("exited before readiness"));
   assert!(waiter.contains("terminate_child(child)"));
   assert!(waiter.contains("Duration::from_millis(50)"));
   assert!(controller.contains("macOS comparison app reported failure before readiness"));
   let control = fs::read_to_string(workspace_root().join("host/apple-comparison/Shared/ComparatorApp/MacOSCampaignControl.swift")).expect("macOS campaign control source");
   assert!(control.contains("func prepareMacOSCampaign("));
   assert!(control.contains(".failure.txt"));
   assert!(control.contains("try? store.durableWrite"));
}

#[test]
fn macos_comparison_apps_order_a_window_before_campaign_readiness()
{
   for relative in ["host/apple-comparison/Oxide-macOS/main.swift", "host/apple-comparison/AppKit-macOS/main.swift"]
   {
      let source = fs::read_to_string(workspace_root().join(relative)).expect("macOS comparison main source");
      let retained = source.find("self.window = window").expect("retained comparison window");
      let ordered = source.find("window.makeKeyAndOrderFront(nil)").expect("ordered comparison window");
      let activated = source.find("NSApp.activate(ignoringOtherApps: true)").expect("activated comparison application");
      let campaign = source.find("try runCampaign(window: window").expect("campaign preparation");
      assert!(retained < ordered, "{} must retain its window before ordering it", relative);
      assert!(ordered < activated, "{} must order its window before activation", relative);
      assert!(activated < campaign, "{} must activate before the durable ready barrier", relative);
   }
}

#[test]
fn measured_macos_sessions_launch_through_the_xcui_input_controller()
{
   let controller = include_str!("../src/lib.rs");
   let start = controller.find("fn run_or_resume_session(").expect("session runner");
   let end = controller[start..].find("fn preflight_macos_session_trusted_input").expect("session runner end") + start;
   let session = &controller[start..end];
   let correctness = session.find("session.pass_role == AppleCampaignPassRole::Correctness").expect("correctness launch branch");
   let direct = session.find("Command::new(\"/usr/bin/open\")").expect("untimed correctness direct launch");
   let measured = session.find("foreground_controller_command(").expect("measured XCUI launch");
   assert!(correctness < direct && direct < measured);
   assert_eq!(session.matches("Command::new(\"/usr/bin/open\")").count(), 1);
   assert!(session.contains("wait_for_exact_process_with_controller"));
   assert!(session.contains("waiting for measured macOS XCUI input controller dismissal"));
}

#[test]
fn macos_campaign_stop_uses_a_registered_application_handshake()
{
   let controller = include_str!("../src/lib.rs");
   let start = controller.find("fn run_or_resume_session(").expect("session runner");
   let end = controller[start..].find("fn preflight_macos_session_trusted_input").expect("session runner end") + start;
   let session = &controller[start..end];
   let register = session.find("registering macOS campaign stop-ready notification").expect("stop-ready registration");
   let run = session.find("run_foreground_session(").expect("foreground session");
   assert!(register < run);

   let control = fs::read_to_string(workspace_root().join("host/apple-comparison/Shared/ComparatorApp/MacOSCampaignControl.swift")).expect("macOS campaign control");
   let wait = control.find("runWaitingForComparisonDarwinNotification(stop").expect("stop observer");
   let ready = control[wait..].find("postComparisonDarwinNotification(ready)").expect("stop-ready signal") + wait;
   let received = control[ready..].find("macOSTrustedInputStopReceiptControlFile").expect("durable stop-received receipt") + ready;
   assert!(wait < ready && ready < received);

   let xcui = fs::read_to_string(workspace_root().join("host/apple-comparison/MacOSComparisonControllerUITests/MacOSTrustedInputXCUIController.swift")).expect("macOS trusted-input XCUI controller");
   let service_start = xcui.find("func serviceUntilApplicationExits").expect("controller service");
   let service_end = xcui[service_start..].find("private func service(").expect("controller service end") + service_start;
   let service = &xcui[service_start..service_end];
   assert!(service.contains("waitForStopReceipt"));
   assert!(service.contains("macOSTrustedInputStopReceiptControlFile"));
   assert!(service.contains("serviceUntilFlush"));
   assert!(!service.contains("application.state"));

   let foreground_start = controller.find("fn run_foreground_session(").expect("foreground session implementation");
   let foreground_end = controller[foreground_start..].find("fn wait_for_exact_process_exit").expect("foreground session end") + foreground_start;
   let foreground = &controller[foreground_start..foreground_end];
   let teardown_start = foreground.find("let stopped =").expect("successful teardown");
   let teardown = &foreground[teardown_start..];
   let await_ready = teardown.find("wait_for_stop_ready_notification").expect("stop-ready wait");
   let post_stop = teardown.find("post_notification(stop_notification)").expect("stop notification");
   assert!(await_ready < post_stop);
}

#[test]
fn macos_campaign_preparation_failure_never_presents_a_modal_alert()
{
   for relative in ["host/apple-comparison/Oxide-macOS/main.swift", "host/apple-comparison/AppKit-macOS/main.swift"]
   {
      let source = fs::read_to_string(workspace_root().join(relative)).expect("macOS comparison main source");
      let campaign = source.find("let campaign = CommandLine.arguments.contains(\"-oxide-compare-chunk\")").expect("campaign launch classification");
      let failure = source.find("\n      catch\n").expect("campaign preparation failure handling");
      let automated = source[failure..].find("if campaign").expect("automated campaign failure branch") + failure;
      let stderr = source[automated..].find("fputs(").expect("campaign failure stderr") + automated;
      let manual = source[stderr..].find("else").expect("manual launch failure branch") + stderr;
      let alert = source[manual..].find("NSAlert(error: error).runModal()").expect("manual launch alert") + manual;
      let terminate = source[alert..].find("NSApp.terminate(nil)").expect("failure termination") + alert;
      assert!(source.contains("try prepareMacOSCampaign(executor, invocation: invocation, store: store)"));
      assert!(campaign < failure, "{} must classify the launch before campaign preparation", relative);
      assert!(failure < automated && automated < stderr && stderr < manual && manual < alert && alert < terminate, "{} must terminate automated failures without a modal alert", relative);
   }
}

#[test]
fn macos_telemetry_coverage_reconciles_every_kind_with_the_binary_ring()
{
   let telemetry = telemetry_with_kinds(&[23, 1, 3, 5, 6, 24, 26, 4, 2]);
   let coverage = telemetry_coverage_json(ComparisonSide::Native, "primary-presentation", &[23, 1, 3, 5, 6, 24, 26, 4, 2]);
   validate_macos_telemetry_coverage(&coverage, &telemetry, ComparisonSide::Native, "primary-presentation").expect("valid telemetry coverage");

   let mut stale: serde_json::Value = serde_json::from_slice(&coverage).expect("coverage JSON");
   stale["entries"][22]["observedCount"] = serde_json::json!(0);
   assert!(validate_macos_telemetry_coverage(&serde_json::to_vec(&stale).expect("stale coverage"), &telemetry, ComparisonSide::Native, "primary-presentation").is_err());
}

#[test]
fn macos_telemetry_coverage_rejects_external_presentation_as_a_ring_sample()
{
   let kinds = [23, 1, 3, 5, 6, 24, 26, 21, 4, 2];
   let telemetry = telemetry_with_kinds(&kinds);
   let coverage = telemetry_coverage_json(ComparisonSide::Native, "primary-presentation", &kinds);
   assert!(validate_macos_telemetry_coverage(&coverage, &telemetry, ComparisonSide::Native, "primary-presentation").is_err());
}

#[test]
fn timed_session_controller_reopens_and_hashes_envelope_bound_coverage()
{
   let controller = include_str!("../src/lib.rs");
   assert!(controller.contains("let coverage_identity = envelope.telemetry_coverage.as_ref().context"));
   assert!(controller.contains("coverage_identity.path != expected_coverage_path"));
   assert!(controller.contains("let coverage_bytes = fs::read(&paths.telemetry_coverage)"));
   assert!(controller.contains("coverage_identity.sha256 != sha256(&coverage_bytes)"));
   assert!(controller.contains("validate_macos_telemetry_coverage(&coverage_bytes, &telemetry"));
}

#[test]
fn density_gui_gate_rejects_locked_or_unknown_console_state()
{
   assert!(validate_macos_gui_lock_ioreg(b"<key>IOConsoleLocked</key><true/>").is_err());
   assert!(validate_macos_gui_lock_ioreg(b"<key>IOConsoleLocked</key><false/>").is_ok());
   assert!(validate_macos_gui_lock_ioreg(b"<plist></plist>").is_err());
}

fn telemetry_with_kinds(kinds: &[u16]) -> Vec<u8>
{
   const HEADER_BYTES: usize = 136;
   const RECORD_BYTES: usize = 44;
   let mut bytes = Vec::with_capacity(HEADER_BYTES + kinds.len() * RECORD_BYTES + 32);
   bytes.extend_from_slice(b"OXBTEL02");
   bytes.extend_from_slice(&2_u32.to_le_bytes());
   bytes.extend_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
   bytes.extend_from_slice(&(RECORD_BYTES as u32).to_le_bytes());
   bytes.extend_from_slice(&1_u32.to_le_bytes());
   bytes.extend_from_slice(&(kinds.len() as u64).to_le_bytes());
   bytes.extend_from_slice(&(kinds.len() as u64).to_le_bytes());
   bytes.resize(HEADER_BYTES, 0);
   for (index, kind) in kinds.iter().enumerate()
   {
      bytes.extend_from_slice(&(index as u64).to_le_bytes());
      bytes.extend_from_slice(&(index as u64 + 1).to_le_bytes());
      bytes.extend_from_slice(&kind.to_le_bytes());
      bytes.extend_from_slice(&0_u16.to_le_bytes());
      bytes.extend_from_slice(&u64::from(*kind).to_le_bytes());
      bytes.extend_from_slice(&0_i64.to_le_bytes());
      bytes.extend_from_slice(&0_i64.to_le_bytes());
   }
   let digest = Sha256::digest(&bytes);
   bytes.extend_from_slice(&digest);
   bytes
}

fn telemetry_coverage_json(side: ComparisonSide, pass_id: &str, kinds: &[u16]) -> Vec<u8>
{
   let names = [
      "scenarioBegin", "scenarioEnd", "phaseBegin", "phaseEnd", "checkpointBegin", "checkpointEnd",
      "inputReceived", "mutationBegin", "mutationEnd", "layoutBegin", "layoutEnd", "sceneUpdateBegin",
      "sceneUpdateEnd", "renderPrepareBegin", "renderPrepareEnd", "encodeBegin", "encodeEnd", "commandSubmit",
      "gpuStart", "gpuEnd", "presentation", "firstMeaningfulFrame", "readyToInput", "displayOpportunity",
      "resetComplete", "callbackCadence", "drawableWait", "inflightDepth", "updateBacklog", "logicalUpdateCompleted",
      "logicalUpdateSkipped", "quiescence", "gpuDuration",
   ];
   let diagnostic = pass_id == "common-gpu" || pass_id == "full-attribution";
   let entries = names.iter().enumerate().map(|(index, name)| {
      let raw = (index + 1) as u16;
      let availability = match raw
      {
         1..=6 | 23 | 24 | 26 => "ring-required",
         7..=9 | 12 | 13 | 25 | 30 | 32 => "ring-conditional",
         21 | 22 => "external-evidence",
         14..=18 | 27 | 33 if side == ComparisonSide::Oxide && diagnostic => "oxide-diagnostic",
         14..=18 | 27 | 33 if side == ComparisonSide::Oxide => "diagnostic-not-enabled-for-pass",
         _ => "unavailable",
      };
      serde_json::json!({
         "kind": name,
         "rawValue": raw,
         "availability": availability,
         "observedCount": kinds.iter().filter(|kind| **kind == raw).count(),
         "source": "test-observed-boundary",
      })
   }).collect::<Vec<_>>();
   serde_json::to_vec(&serde_json::json!({
      "schemaVersion": 1,
      "side": side,
      "passID": pass_id,
      "entries": entries,
      "validation": "complete-kind-availability-and-observed-counts",
   })).expect("coverage JSON")
}
