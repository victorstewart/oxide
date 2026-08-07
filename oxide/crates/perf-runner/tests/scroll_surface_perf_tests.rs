use std::process::Command;

use oxide_perf_runner::{PerfCaseResult, PerfReport};

fn case<'a>(report: &'a PerfReport, id: &str) -> &'a PerfCaseResult
{
   report.cases.iter().find(|case| case.id == id).expect("perf case")
}

#[test]
fn filtered_scroll_surface_cases_preserve_raw_touch_and_programmatic_contracts()
{
   let mut json_out = std::env::temp_dir();
   json_out.push(format!("oxide-perf-runner-scroll-surface-{}.json", std::process::id()));
   let output = Command::new(env!("CARGO_BIN_EXE_oxide-perf-runner"))
      .env(
         "OXIDE_PERF_RUNNER_FILTER",
         "cpu.authoring.vertical_scroll_surface.input_advance,cpu.journey.feed_raw_touch_fling,cpu.journey.feed_scroll_matrix",
      )
      .arg("--run-suite")
      .arg("--smoke")
      .arg("--json-out")
      .arg(&json_out)
      .output()
      .expect("run filtered scroll-surface smoke suite");
   let stdout = String::from_utf8_lossy(&output.stdout);
   let stderr = String::from_utf8_lossy(&output.stderr);

   assert!(output.status.success(), "filtered suite failed: {stderr}");
   assert!(stdout.contains("cases=3"), "stdout: {stdout}");
   assert!(!stderr.contains("coverage is incomplete"), "stderr: {stderr}");

   let report = std::fs::read_to_string(&json_out).expect("read scroll-surface report");
   let report: PerfReport = serde_json::from_str(&report).expect("parse scroll-surface report");
   let authoring = case(&report, "cpu.authoring.vertical_scroll_surface.input_advance");
   assert_eq!(authoring.metrics["raw_touch_events_per_op"], 4.0);
   assert_eq!(authoring.metrics["inertial_steps_per_op"], 1.0);
   assert_eq!(authoring.metrics["inertia_active_after_step"], 1.0);
   assert!(authoring.metrics["surface_offset_after_step"] > 120.0);
   assert!(authoring.notes.iter().any(|note| note.contains("Warmed public")));

   let journey = case(&report, "cpu.journey.feed_raw_touch_fling");
   assert_eq!(journey.metrics["collection_count"], 2_000.0);
   assert_eq!(journey.metrics["raw_touch_events_per_journey"], 4.0);
   assert_eq!(journey.metrics["simulated_refresh_hz"], 120.0);
   assert_eq!(journey.metrics["settled_journey_ratio"], 1.0);
   assert!(journey.metrics["simulated_display_steps_to_settle_per_journey"] > 0.0);
   assert!(journey.metrics["simulated_display_steps_to_settle_per_journey"] <= 1_024.0);
   assert_eq!(
      journey.metrics["encoded_ui_frames_per_journey"],
      journey.metrics["simulated_display_steps_to_settle_per_journey"] + 4.0,
   );
   assert!(journey.metrics["draw_items_per_journey"] > 0.0);
   assert!(journey.metrics["final_offset_points"] > 120.0);

   let matrix = case(&report, "cpu.journey.feed_scroll_matrix");
   assert!(matrix.notes.iter().any(|note| note.contains("programmatic viewport transitions")));
   assert!(matrix.notes.iter().all(|note| !note.contains("hard fling")));
   let _ = std::fs::remove_file(json_out);
}
