use oxide_apple_comparison_controller::{reduce_macos_common_gpu, MacOsCommonGpuSample, MacOsCommonGpuSamples};

fn samples() -> MacOsCommonGpuSamples
{
   MacOsCommonGpuSamples {
      schema_version: 1,
      pid: 42,
      cadence_ns: 50_000_000,
      source: String::from("task-info-power-v2-task-gpu-utilisation-ns"),
      samples: vec![
         MacOsCommonGpuSample {mach_continuous_time: 50, task_gpu_time_ns: 0},
         MacOsCommonGpuSample {mach_continuous_time: 250, task_gpu_time_ns: 10},
         MacOsCommonGpuSample {mach_continuous_time: 450, task_gpu_time_ns: 40},
         MacOsCommonGpuSample {mach_continuous_time: 650, task_gpu_time_ns: 40},
      ],
   }
}

fn signposts() -> &'static str
{
   r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="os-signpost">
<col><mnemonic>time</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>event-type</mnemonic></col><col><mnemonic>name</mnemonic></col><col><mnemonic>subsystem</mnemonic></col><col><mnemonic>category</mnemonic></col><col><mnemonic>message</mnemonic></col>
</schema>
<row><event-time>100</event-time><process id="1" fmt="AppKitComparison (42)">appkit</process><event-type id="2" fmt="Event">Event</event-type><signpost-name fmt="ScenarioBegin">ScenarioBegin</signpost-name><subsystem id="3" fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category id="4" fmt="Presentation">Presentation</category><message fmt="scenario=0 identifier=100">scenario=0 identifier=100</message></row>
<row><event-time>200</event-time><process ref="1"/><event-type ref="2"/><signpost-name fmt="PhaseBegin">PhaseBegin</signpost-name><subsystem ref="3"/><category ref="4"/><message fmt="scenario=0 identifier=11 measured=1">scenario=0 identifier=11 measured=1</message></row>
<row><event-time>500</event-time><process ref="1"/><event-type ref="2"/><signpost-name fmt="PhaseEnd">PhaseEnd</signpost-name><subsystem ref="3"/><category ref="4"/><message fmt="scenario=0 identifier=11 measured=1">scenario=0 identifier=11 measured=1</message></row>
<row><event-time>600</event-time><process ref="1"/><event-type ref="2"/><signpost-name fmt="ScenarioEnd">ScenarioEnd</signpost-name><subsystem ref="3"/><category ref="4"/><message fmt="scenario=0 identifier=100">scenario=0 identifier=100</message></row>
</node></trace-query-result>"#
}

#[test]
fn reduces_exact_pid_gpu_time_as_noncomparable_phase_diagnostic()
{
   let summary = reduce_macos_common_gpu(&samples(), signposts(), 42).expect("common-GPU reduction");
   assert_eq!(summary.process, "AppKitComparison (42)");
   assert_eq!(summary.measured_gpu_time_ns, 30);
   assert!(!summary.comparison_eligible);
   assert_eq!(summary.availability, "process-scoped-diagnostic-only-compositor-ownership-asymmetric");
   assert_eq!(summary.phases[0].sample_count, 2);
}

#[test]
fn rejects_pid_boundary_counter_and_unavailable_zero_evidence()
{
   assert!(reduce_macos_common_gpu(&samples(), signposts(), 99).is_err());
   assert!(reduce_macos_common_gpu(&samples(), &signposts().replace("PhaseEnd", "Other"), 42).is_err());
   let mut nonmonotonic = samples();
   nonmonotonic.samples[2].task_gpu_time_ns = 5;
   assert!(reduce_macos_common_gpu(&nonmonotonic, signposts(), 42).is_err());
   let mut zero = samples();
   for sample in &mut zero.samples {sample.task_gpu_time_ns = 0;}
   assert!(reduce_macos_common_gpu(&zero, signposts(), 42).is_err());
}
