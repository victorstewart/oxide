use oxide_apple_comparison_controller::reduce_macos_time_profiler_trace;

fn time_profile_xml() -> &'static str
{
   r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="time-profile">
<col><mnemonic>time</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>thread</mnemonic></col><col><mnemonic>weight</mnemonic></col><col><mnemonic>backtrace</mnemonic></col>
</schema>
<row><event-time>120</event-time><process id="1" fmt="OxideBenchMacOS (42)">oxide</process><thread id="2" fmt="Main Thread">main</thread><weight>10</weight><backtrace fmt="render &gt; layout">render</backtrace></row>
<row><event-time>140</event-time><process ref="1"/><thread ref="2"/><weight>20</weight><backtrace fmt="render &gt; layout">render</backtrace></row>
<row><event-time>220</event-time><process ref="1"/><thread fmt="worker-1">worker</thread><weight>7</weight><backtrace fmt="decode &gt; image">decode</backtrace></row>
<row><event-time>240</event-time><process ref="1"/><thread ref="2"/><weight>5</weight><backtrace fmt="commit &gt; layer">commit</backtrace></row>
<row><event-time>125</event-time><process fmt="Other (7)">other</process><thread ref="2"/><weight>99</weight><backtrace fmt="other">other</backtrace></row>
</node></trace-query-result>"#
}

fn signposts_xml() -> &'static str
{
   r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="os-signpost">
<col><mnemonic>time</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>event-type</mnemonic></col><col><mnemonic>name</mnemonic></col><col><mnemonic>subsystem</mnemonic></col><col><mnemonic>category</mnemonic></col><col><mnemonic>message</mnemonic></col>
</schema>
<row><event-time>100</event-time><process id="1" fmt="OxideBenchMacOS (42)">oxide</process><event-type id="2" fmt="Event">Event</event-type><signpost-name fmt="ScenarioBegin">ScenarioBegin</signpost-name><subsystem id="3" fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category id="4" fmt="Presentation">Presentation</category><message fmt="scenario=0 identifier=100">scenario=0 identifier=100</message></row>
<row><event-time>110</event-time><process ref="1"/><event-type ref="2"/><signpost-name fmt="PhaseBegin">PhaseBegin</signpost-name><subsystem ref="3"/><category ref="4"/><message fmt="scenario=0 identifier=11 measured=1">scenario=0 identifier=11 measured=1</message></row>
<row><event-time>160</event-time><process ref="1"/><event-type ref="2"/><signpost-name fmt="PhaseEnd">PhaseEnd</signpost-name><subsystem ref="3"/><category ref="4"/><message fmt="scenario=0 identifier=11 measured=1">scenario=0 identifier=11 measured=1</message></row>
<row><event-time>210</event-time><process ref="1"/><event-type ref="2"/><signpost-name fmt="PhaseBegin">PhaseBegin</signpost-name><subsystem ref="3"/><category ref="4"/><message fmt="scenario=0 identifier=12 measured=1">scenario=0 identifier=12 measured=1</message></row>
<row><event-time>260</event-time><process ref="1"/><event-type ref="2"/><signpost-name fmt="PhaseEnd">PhaseEnd</signpost-name><subsystem ref="3"/><category ref="4"/><message fmt="scenario=0 identifier=12 measured=1">scenario=0 identifier=12 measured=1</message></row>
<row><event-time>270</event-time><process ref="1"/><event-type ref="2"/><signpost-name fmt="ScenarioEnd">ScenarioEnd</signpost-name><subsystem ref="3"/><category ref="4"/><message fmt="scenario=0 identifier=100">scenario=0 identifier=100</message></row>
</node></trace-query-result>"#
}

#[test]
fn reduces_exact_pid_samples_into_measured_phase_and_main_thread_attribution()
{
   let artifact = reduce_macos_time_profiler_trace(time_profile_xml(), signposts_xml(), 42).expect("Time Profiler reduction");
   assert_eq!(artifact.schema_version, 1);
   assert_eq!(artifact.process, "OxideBenchMacOS (42)");
   assert_eq!(artifact.measured_sample_count, 4);
   assert_eq!(artifact.measured_weight, 42);
   assert_eq!(artifact.measured_main_thread_sample_count, 3);
   assert_eq!(artifact.measured_main_thread_weight, 35);
   assert_eq!(artifact.phases.len(), 2);
   assert_eq!(artifact.phases[0].scenario_identifier, 100);
   assert_eq!(artifact.phases[0].phase_identifier, 11);
   assert_eq!(artifact.phases[0].top_stacks[0].stack, "render > layout");
   assert_eq!(artifact.phases[0].top_stacks[0].weight, 30);
   assert_eq!(artifact.phases[1].phase_identifier, 12);
   assert_eq!(artifact.phases[1].main_thread_weight, 5);
}

#[test]
fn rejects_pid_boundary_and_sample_integrity_failures()
{
   assert!(reduce_macos_time_profiler_trace(time_profile_xml(), signposts_xml(), 99).is_err());
   assert!(reduce_macos_time_profiler_trace(time_profile_xml(), &signposts_xml().replace("PhaseEnd", "Other"), 42).is_err());
   assert!(reduce_macos_time_profiler_trace(&time_profile_xml().replace("<weight>10</weight>", "<weight>0</weight>"), signposts_xml(), 42).is_err());
   let outside = time_profile_xml()
      .replace("<event-time>120</event-time>", "<event-time>170</event-time>")
      .replace("<event-time>140</event-time>", "<event-time>180</event-time>")
      .replace("<event-time>220</event-time>", "<event-time>190</event-time>")
      .replace("<event-time>240</event-time>", "<event-time>200</event-time>");
   assert!(reduce_macos_time_profiler_trace(&outside, signposts_xml(), 42).is_err());
}

#[test]
fn preserves_sentinel_sample_weight_as_explicitly_unavailable_attribution()
{
   let xml = time_profile_xml().replace(
      "</node>",
      "<row><event-time>150</event-time><process ref=\"1\"/><thread ref=\"2\"/><weight>3</weight><sentinel/></row></node>"
   );
   let artifact = reduce_macos_time_profiler_trace(&xml, signposts_xml(), 42).expect("sentinel Time Profiler reduction");
   assert_eq!(artifact.measured_sample_count, 5);
   assert_eq!(artifact.measured_weight, 45);
   assert!(artifact.phases[0].top_stacks.iter().any(|stack| stack.stack == "<unavailable-sentinel>" && stack.weight == 3));
}

#[test]
fn resolves_xctrace_references_across_sibling_table_nodes()
{
   let xml = time_profile_xml().replacen(
      "</backtrace></row>\n<row><event-time>140",
      "</backtrace></row></node><node>\n<row><event-time>140",
      1
   );
   let artifact = reduce_macos_time_profiler_trace(&xml, signposts_xml(), 42).expect("split-node Time Profiler reduction");
   assert_eq!(artifact.measured_sample_count, 4);
   assert_eq!(artifact.measured_weight, 42);
}

#[test]
fn retains_short_zero_sample_phase_when_the_measured_scenario_has_samples()
{
   let signposts = signposts_xml().replace(
      "<row><event-time>210",
      "<row><event-time>170</event-time><process ref=\"1\"/><event-type ref=\"2\"/><signpost-name fmt=\"PhaseBegin\">PhaseBegin</signpost-name><subsystem ref=\"3\"/><category ref=\"4\"/><message fmt=\"scenario=0 identifier=13 measured=1\">scenario=0 identifier=13 measured=1</message></row>\n<row><event-time>180</event-time><process ref=\"1\"/><event-type ref=\"2\"/><signpost-name fmt=\"PhaseEnd\">PhaseEnd</signpost-name><subsystem ref=\"3\"/><category ref=\"4\"/><message fmt=\"scenario=0 identifier=13 measured=1\">scenario=0 identifier=13 measured=1</message></row>\n<row><event-time>210"
   );
   let artifact = reduce_macos_time_profiler_trace(time_profile_xml(), &signposts, 42).expect("zero-sample phase reduction");
   let phase = artifact.phases.iter().find(|phase| phase.phase_identifier == 13).expect("zero-sample phase");
   assert_eq!(phase.sample_count, 0);
   assert_eq!(phase.weight, 0);
   assert!(phase.top_stacks.is_empty());
}
