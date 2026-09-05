use std::fs;
use std::process::{Command, Stdio};
use tempfile::tempdir;
use xtask::xctrace_record::{trace_working_set_bytes, XctraceRecordProcess};

#[test]
fn record_process_isolates_all_apple_temporary_directory_variables_and_cleans_scratch()
{
   let root = tempdir().expect("temporary record root");
   let trace_path = root.path().join("metal.trace");
   let stdout_path = root.path().join("record.stdout.log");
   let stderr_path = root.path().join("record.stderr.log");
   let environment_path = root.path().join("environment.txt");
   let args = vec![
      String::from("-c"),
      String::from("printf '%s\\n%s\\n%s\\n' \"$TMPDIR\" \"$TMP\" \"$TEMP\" > \"$1\""),
      String::from("xctrace-stand-in"),
      environment_path.to_string_lossy().into_owned(),
   ];
   let mut record = XctraceRecordProcess::spawn(
      root.path(),
      "/bin/sh",
      &args,
      &trace_path,
      &stdout_path,
      &stderr_path,
      1024,
   )
   .expect("spawn record stand-in");
   let scratch_path = record.scratch_path().to_path_buf();
   let status = record.wait().expect("wait for record stand-in");
   assert!(status.success());
   record.commit().expect("commit record stand-in");
   let paths = fs::read_to_string(environment_path).expect("read isolated environment");
   assert_eq!(paths.lines().collect::<Vec<_>>(), vec![
      scratch_path.to_string_lossy().as_ref(),
      scratch_path.to_string_lossy().as_ref(),
      scratch_path.to_string_lossy().as_ref(),
   ]);
   assert!(!scratch_path.exists());
}

#[test]
fn record_fuse_counts_scratch_and_final_trace_together_and_removes_partial_output()
{
   let root = tempdir().expect("temporary record root");
   let trace_path = root.path().join("metal.trace");
   let stdout_path = root.path().join("record.stdout.log");
   let stderr_path = root.path().join("record.stderr.log");
   let args = vec![String::from("30")];
   let mut record = XctraceRecordProcess::spawn(
      root.path(),
      "/bin/sleep",
      &args,
      &trace_path,
      &stdout_path,
      &stderr_path,
      23,
   )
   .expect("spawn record stand-in");
   let scratch_path = record.scratch_path().to_path_buf();
   fs::create_dir(&trace_path).expect("trace directory");
   fs::write(trace_path.join("bundle"), [0_u8; 11]).expect("trace bytes");
   fs::write(scratch_path.join("instruments.ktrace"), [0_u8; 13]).expect("scratch bytes");
   assert_eq!(trace_working_set_bytes(&trace_path, &scratch_path).expect("working set"), 24);
   let error = record.enforce_working_set_limit().expect_err("fuse must reject combined growth");
   assert!(error.to_string().contains("working set") || error.to_string().contains("working-set"));
   drop(record);
   assert!(!scratch_path.exists());
   assert!(!trace_path.exists());
}

#[test]
fn dropping_record_owner_terminates_child_and_removes_scratch_and_partial_trace()
{
   let root = tempdir().expect("temporary record root");
   let trace_path = root.path().join("metal.trace");
   let stdout_path = root.path().join("record.stdout.log");
   let stderr_path = root.path().join("record.stderr.log");
   let args = vec![String::from("30")];
   let record = XctraceRecordProcess::spawn(
      root.path(),
      "/bin/sleep",
      &args,
      &trace_path,
      &stdout_path,
      &stderr_path,
      1024,
   )
   .expect("spawn record stand-in");
   let scratch_path = record.scratch_path().to_path_buf();
   let pid = record.id();
   fs::write(&trace_path, [0_u8; 7]).expect("partial trace");
   drop(record);
   assert!(!scratch_path.exists());
   assert!(!trace_path.exists());
   let status = Command::new("/bin/kill")
      .args(["-0", &pid.to_string()])
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .status()
      .expect("query record stand-in");
   assert!(!status.success());
}

#[test]
fn every_xtask_xctrace_record_launch_uses_the_owned_record_process()
{
   let source = include_str!("../src/lib.rs");
   assert_eq!(source.matches("String::from(\"record\")").count(), 2);
   let react_start = source.find("fn run_react_device_perf_case(").expect("React trace function");
   let react_end = source[react_start..]
      .find("fn install_uikit_device_app(")
      .map(|offset| react_start + offset)
      .expect("React trace function end");
   assert!(source[react_start..react_end].contains("XctraceRecordProcess::spawn("));
   let launched_start = source
      .find("fn run_uikit_device_launched_trace(")
      .expect("launched UIKit trace function");
   let launched_end = source[launched_start..]
      .find("fn launched_trace_has_bounded_workload_windows(")
      .map(|offset| launched_start + offset)
      .expect("launched UIKit trace function end");
   assert!(source[launched_start..launched_end].contains("XctraceRecordProcess::spawn("));
}
