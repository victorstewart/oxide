use xtask::oxide_device_launch_environment_json;

#[test]
fn oxide_device_environment_marks_smoke_runs()
{
   let environment = oxide_device_launch_environment_json(true).expect("encode environment");

   assert!(environment.contains("OXIDE_PERF_RUNNER"));
   assert!(environment.contains("OXIDE_PERF_RUNNER_SMOKE"));
}
