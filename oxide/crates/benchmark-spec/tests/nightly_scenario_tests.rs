use oxide_benchmark_spec::{canonical_scenario_json, load_scenario, validate_nightly_endurance_scenario, validate_scenario, validate_scenario_artifacts, TraceEvent};
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf
{
   Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn idle_scenario_is_canonical_full_dashboard_work_with_a_fixed_sixty_second_window()
{
   let workspace = workspace_root();
   let spec_root = workspace.join("benchmarks/comparative/specs/v1");
   let (path, scenario) = load_scenario(&workspace, "idle.steady.json").expect("load idle scenario");
   validate_scenario(&scenario).expect("validate idle scenario");
   validate_scenario_artifacts(&spec_root, &scenario).expect("validate idle scenario artifacts");
   assert_eq!(fs::read(path).expect("idle scenario bytes"), canonical_scenario_json(&scenario).expect("canonical idle scenario"));
   assert_eq!(scenario.id, "idle.steady");
   assert_eq!(scenario.primary_metric, "cpu.process_ms_per_wall_s");
   let measured = scenario.phases.iter().filter(|phase| phase.measured).collect::<Vec<_>>();
   assert_eq!(measured.len(), 1);
   assert_eq!(measured[0].id, "settled-idle");
   assert_eq!(measured[0].duration_ms, Some(60_000));
   assert!(measured[0].trace.is_none());
   assert_eq!(scenario.parity_checkpoints.len(), 1);
   assert_eq!(scenario.parity_checkpoints[0].at_us, Some(60_000_000));
}

#[test]
fn endurance_scenario_freezes_five_minutes_of_legacy_derived_churn()
{
   let workspace = workspace_root();
   let spec_root = workspace.join("benchmarks/comparative/specs/v1");
   let (path, scenario) = load_scenario(&workspace, "endurance.churn.json").expect("load endurance scenario");
   validate_scenario(&scenario).expect("validate endurance scenario");
   validate_nightly_endurance_scenario(&spec_root, &scenario).expect("validate nightly endurance contract");
   assert_eq!(fs::read(path).expect("endurance scenario bytes"), canonical_scenario_json(&scenario).expect("canonical endurance scenario"));
   assert_eq!(scenario.phases.iter().filter(|phase| phase.measured).map(|phase| phase.duration_ms.expect("measured duration")).sum::<u64>(), 300_000);
   assert_eq!(trace_event_count(&spec_root, &scenario, "open-close-heavy-screen"), 200);
   assert_eq!(trace_event_count(&spec_root, &scenario, "tab-switch-heavy"), 500);
   assert_eq!(trace_event_count(&spec_root, &scenario, "idle-animation"), 600);
   assert_eq!(scenario.parity_checkpoints.len(), 4);
   assert!(scenario.parity_checkpoints.iter().all(|checkpoint| checkpoint.screenshot.path == "checkpoints/dashboard.mixed-static/idle/screenshot.png"));
}

fn trace_event_count(spec_root: &Path, scenario: &oxide_benchmark_spec::ScenarioSpec, phase_id: &str) -> usize
{
   let trace = scenario.phases.iter().find(|phase| phase.id == phase_id).and_then(|phase| phase.trace.as_ref()).expect("phase trace");
   serde_json::from_slice::<Vec<TraceEvent>>(&fs::read(spec_root.join(&trace.path)).expect("trace bytes")).expect("trace JSON").len()
}
