use oxide_apple_comparison_controller::{validate_macos_launch_evidence, validate_macos_launch_preparation_receipt, ComparisonSide, MacOsLaunchClass, MacOsLaunchEvidence, MacOsLaunchExpectation, MacOsLaunchPreparationReceipt, MACOS_LAUNCH_PRESENTATION_CALIBRATION_PENDING};

fn expectation(launch_class: MacOsLaunchClass) -> MacOsLaunchExpectation
{
   let fresh = launch_class == MacOsLaunchClass::FreshInstallFirstLaunch;
   MacOsLaunchExpectation {
      run_id: String::from("launch-contract"),
      plan_sha256: "a".repeat(64),
      chunk_id: String::from("launch-pairs-0-3"),
      pack_id: String::from("pr-launch"),
      pair_index: 0,
      generation: "b".repeat(64),
      executable_sha256: "c".repeat(64),
      side: ComparisonSide::Native,
      launch_class,
      installed_bundle_path: if fresh {String::from("/tmp/session/Fresh.app")} else {String::from("/Applications/Comparison.app")},
      data_container_path: fresh.then(|| String::from("/tmp/session/data")),
   }
}

fn evidence(launch_class: MacOsLaunchClass) -> MacOsLaunchEvidence
{
   let expected = expectation(launch_class);
   let warm_resume = launch_class == MacOsLaunchClass::WarmResume;
   MacOsLaunchEvidence {
      schema_version: 4,
      run_id: expected.run_id,
      plan_sha256: expected.plan_sha256,
      generation: expected.generation,
      executable_sha256: expected.executable_sha256,
      side: expected.side,
      launch_class: expected.launch_class,
      pid: 42,
      preparation_completed_ticks: if warm_resume {30} else {10},
      launch_request_ticks: if warm_resume {40} else {20},
      process_start_ticks: if warm_resume {10} else {30},
      process_observed_ticks: if warm_resume {15} else {35},
      application_did_finish_launching_ticks: if warm_resume {20} else {40},
      first_complete_ui_generation_ticks: 50,
      first_attributed_present_proxy_ticks: 60,
      first_interactive_input_request_ticks: 70,
      first_interactive_input_received_ticks: 75,
      first_interactive_response_generation_ticks: 80,
      first_interactive_response_present_proxy_ticks: 90,
      trace_started_before_launch_request: true,
      trace_all_processes: true,
      exact_pid_filtered: true,
      preparation_receipt_sha256: "d".repeat(64),
      presentation_calibration_status: String::from(MACOS_LAUNCH_PRESENTATION_CALIBRATION_PENDING),
      complete: true,
   }
}

fn preparation(launch_class: MacOsLaunchClass) -> MacOsLaunchPreparationReceipt
{
   let expected = expectation(launch_class);
   MacOsLaunchPreparationReceipt {
      schema_version: 1,
      run_id: expected.run_id,
      plan_sha256: expected.plan_sha256,
      generation: expected.generation,
      side: expected.side,
      launch_class,
      executable_sha256: expected.executable_sha256,
      installed_bundle_path: expected.installed_bundle_path,
      data_container_path: expected.data_container_path,
      cache_primed: launch_class == MacOsLaunchClass::TerminatedProcessWarmSystemCache,
      installed_bundle_was_absent: (launch_class == MacOsLaunchClass::FreshInstallFirstLaunch).then_some(true),
      data_container_was_absent: (launch_class == MacOsLaunchClass::FreshInstallFirstLaunch).then_some(true),
      completed_ticks: 10,
      complete: true,
   }
}

#[test]
fn terminated_warm_launch_evidence_accepts_complete_ordered_exact_pid_proof()
{
   let class = MacOsLaunchClass::TerminatedProcessWarmSystemCache;
   validate_macos_launch_evidence(&evidence(class), &expectation(class)).expect("valid launch evidence");
}

#[test]
fn fresh_install_and_warm_resume_accept_their_class_specific_ordering()
{
   for class in [MacOsLaunchClass::FreshInstallFirstLaunch, MacOsLaunchClass::WarmResume]
   {
      validate_macos_launch_evidence(&evidence(class), &expectation(class)).expect("valid class-specific launch evidence");
   }
}

#[test]
fn launch_preparation_receipts_require_the_exact_class_specific_proof()
{
   for class in [
      MacOsLaunchClass::TerminatedProcessWarmSystemCache,
      MacOsLaunchClass::FreshInstallFirstLaunch,
      MacOsLaunchClass::WarmResume,
   ]
   {
      validate_macos_launch_preparation_receipt(&preparation(class), &expectation(class)).expect("valid class-specific preparation");
   }
   let class = MacOsLaunchClass::FreshInstallFirstLaunch;
   let mut reused_container = preparation(class);
   reused_container.data_container_was_absent = Some(false);
   let error = validate_macos_launch_preparation_receipt(&reused_container, &expectation(class)).expect_err("reused data container must fail");
   assert!(error.to_string().contains("does not match its launch class"));
}

#[test]
fn launch_evidence_rejects_trace_started_after_launch_request()
{
   let class = MacOsLaunchClass::TerminatedProcessWarmSystemCache;
   let mut evidence = evidence(class);
   evidence.trace_started_before_launch_request = false;
   let error = validate_macos_launch_evidence(&evidence, &expectation(class)).expect_err("late trace must fail");
   assert!(error.to_string().contains("start before launch"));
}

#[test]
fn launch_evidence_rejects_prepared_ui_before_application_launch_callback()
{
   let class = MacOsLaunchClass::TerminatedProcessWarmSystemCache;
   let mut evidence = evidence(class);
   evidence.first_complete_ui_generation_ticks = 35;
   let error = validate_macos_launch_evidence(&evidence, &expectation(class)).expect_err("misordered UI generation must fail");
   assert!(error.to_string().contains("not strictly ordered"));
}

#[test]
fn launch_evidence_rejects_identity_or_primer_mutation()
{
   let class = MacOsLaunchClass::TerminatedProcessWarmSystemCache;
   let mut malformed_primer = evidence(class);
   malformed_primer.preparation_receipt_sha256 = String::from("not-a-hash");
   assert!(validate_macos_launch_evidence(&malformed_primer, &expectation(class)).is_err());

   let mut changed_generation = evidence(class);
   changed_generation.generation = "e".repeat(64);
   let error = validate_macos_launch_evidence(&changed_generation, &expectation(class)).expect_err("generation mutation must fail");
   assert!(error.to_string().contains("identity differs"));
}
