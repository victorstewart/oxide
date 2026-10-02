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
   assert_eq!(source.matches("String::from(\"record\")").count(), 1);
   let attached_start = source
      .find("fn run_uikit_device_attached_trace(")
      .expect("attached Oxide trace function");
   let attached_end = source[attached_start..]
      .find("fn observe_trace_completion_before_exit(")
      .map(|offset| attached_start + offset)
      .expect("attached Oxide trace function end");
   assert!(source[attached_start..attached_end].contains("String::from(\"--attach\")"));
   assert!(source[attached_start..attached_end].contains("XctraceRecordProcess::spawn("));
}

#[test]
fn failed_comparison_can_retain_partial_trace_without_leaking_recorder_or_scratch()
{
   let root = tempdir().expect("temporary record root");
   let trace = root.path().join("failed.trace");
   let mut record = XctraceRecordProcess::spawn(root.path(), "/bin/sleep", &[String::from("30")],
      &trace, &root.path().join("stdout"), &root.path().join("stderr"), 1024).expect("spawn recorder");
   record.preserve_partial_trace();
   let scratch = record.scratch_path().to_path_buf();
   let pid = record.id();
   fs::write(&trace, b"partial evidence").expect("partial trace");
   drop(record);
   assert_eq!(fs::read(&trace).expect("retained evidence"), b"partial evidence");
   assert!(!scratch.exists());
   assert!(!Command::new("/bin/kill").args(["-0", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status().expect("query child").success());
}
