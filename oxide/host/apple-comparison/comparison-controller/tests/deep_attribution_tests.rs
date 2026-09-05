use oxide_apple_comparison_controller::{build_macos_deep_attribution_schedule, reduce_macos_deep_attribution_exports, ComparisonSide, MacOsDeepAttributionSchemaExport, MACOS_DEEP_ATTRIBUTION_CROSS_FRAMEWORK_SCOPE};
use oxide_benchmark_spec::{materialize_macos_full_attribution_plan, MacOsAttributionAvailability, MacOsAttributionCollectorKind, MacOsGpuCounterConfiguration};

#[test]
fn deep_attribution_reduction_uses_exact_pid_measured_windows_and_retains_raw_metric_identity()
{
   let plan = attribution_plan();
   let replay = plan.replays.iter().find(|replay| replay.collector == MacOsAttributionCollectorKind::Allocations).expect("Allocations replay");
   let export = MacOsDeepAttributionSchemaExport {
      schema: String::from("allocations"),
      xml: String::from(r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="allocations">
<col><mnemonic>time</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>size</mnemonic><unit>bytes</unit></col><col><mnemonic>address</mnemonic></col>
</schema>
<row><event-time>120</event-time><process fmt="OxideBenchMacOS (42)">42</process><size>64</size><address>1000</address></row>
<row><event-time>150</event-time><process fmt="Other (7)">7</process><size>4096</size><address>2000</address></row>
<row><event-time>320</event-time><process fmt="OxideBenchMacOS (42)">42</process><size>128</size><address>3000</address></row>
</node></trace-query-result>"#),
   };
   let summary = reduce_macos_deep_attribution_exports(replay, 42, signposts(), &[export]).expect("reduce deep attribution");

   assert!(summary.attached_to_exact_pid);
   assert_eq!(summary.cross_framework_scope, MACOS_DEEP_ATTRIBUTION_CROSS_FRAMEWORK_SCOPE);
   assert_eq!(summary.schemas.len(), 1);
   assert!(summary.schemas[0].exact_pid_filter_exposed);
   assert_eq!(summary.schemas[0].phases.len(), 1);
   assert_eq!(summary.schemas[0].phases[0].measured_row_count, 1);
   assert_eq!(summary.schemas[0].phases[0].exact_pid_row_count, 1);
   let size = summary.schemas[0].phases[0].metrics.iter().find(|metric| metric.mnemonic == "size").expect("size metric");
   assert_eq!(size.unit.as_deref(), Some("bytes"));
   assert_eq!((size.sample_count, size.minimum, size.maximum, size.sum), (1, 64, 64, 64));
}

#[test]
fn deep_attribution_reduction_rejects_missing_exact_pid_windows_and_overlapping_phases()
{
   let plan = attribution_plan();
   let replay = &plan.replays[0];
   let export = MacOsDeepAttributionSchemaExport {
      schema: String::from("allocations"),
      xml: String::from(r#"<?xml version="1.0"?><trace-query-result><node><schema name="allocations"><col><mnemonic>time</mnemonic></col><col><mnemonic>size</mnemonic></col></schema><row><event-time>120</event-time><size>1</size></row></node></trace-query-result>"#),
   };
   assert!(reduce_macos_deep_attribution_exports(replay, 7, signposts(), &[export.clone()]).is_err());
   let overlapping = signposts().replace(
      "<row><event-time>300</event-time>",
      "<row><event-time>150</event-time><process fmt=\"OxideBenchMacOS (42)\">42</process><event-type fmt=\"Event\">Event</event-type><signpost-name fmt=\"PhaseBegin\">PhaseBegin</signpost-name><subsystem fmt=\"com.oxide.comparison\">com.oxide.comparison</subsystem><category fmt=\"Presentation\">Presentation</category><message fmt=\"scenario=2 identifier=8 measured=1\">scenario=2 identifier=8 measured=1</message></row><row><event-time>300</event-time>",
   ).replace(
      "</node></trace-query-result>",
      "<row><event-time>350</event-time><process fmt=\"OxideBenchMacOS (42)\">42</process><event-type fmt=\"Event\">Event</event-type><signpost-name fmt=\"PhaseEnd\">PhaseEnd</signpost-name><subsystem fmt=\"com.oxide.comparison\">com.oxide.comparison</subsystem><category fmt=\"Presentation\">Presentation</category><message fmt=\"scenario=2 identifier=8 measured=1\">scenario=2 identifier=8 measured=1</message></row></node></trace-query-result>",
   );
   assert!(reduce_macos_deep_attribution_exports(replay, 42, &overlapping, &[export]).is_err());
}

#[test]
fn deep_attribution_schedule_expands_all_replays_into_balanced_exact_thirteen_scenario_pairs()
{
   let plan = attribution_plan();
   let schedule = build_macos_deep_attribution_schedule(&plan).expect("build deep-attribution schedule");
   assert_eq!(schedule.sessions.len(), 3 * 12 * 2);
   assert_eq!(schedule.unavailable_gpu_counter_configurations.len(), 1);
   assert!(schedule.unavailable_gpu_counter_configurations[0].contains("none-current-toolchain"));
   assert!(schedule.sessions.iter().all(|session| session.scenario_ids.len() == 13 && session.occupied_seconds == 343));
   for pair in schedule.sessions.chunks_exact(2)
   {
      assert_eq!(pair[0].replay_id, pair[1].replay_id);
      assert_eq!(pair[0].pair_index, pair[1].pair_index);
      assert_eq!(pair[0].order, pair[1].order);
      assert_ne!(pair[0].side, pair[1].side);
      assert!(matches!((pair[0].side, pair[1].side), (ComparisonSide::Native, ComparisonSide::Oxide) | (ComparisonSide::Oxide, ComparisonSide::Native)));
   }
}

fn signposts() -> &'static str
{
   r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="os-signpost">
<col><mnemonic>time</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>event-type</mnemonic></col><col><mnemonic>name</mnemonic></col><col><mnemonic>subsystem</mnemonic></col><col><mnemonic>category</mnemonic></col><col><mnemonic>message</mnemonic></col>
</schema>
<row><event-time>100</event-time><process fmt="OxideBenchMacOS (42)">42</process><event-type fmt="Event">Event</event-type><signpost-name fmt="PhaseBegin">PhaseBegin</signpost-name><subsystem fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category fmt="Presentation">Presentation</category><message fmt="scenario=2 identifier=7 measured=1">scenario=2 identifier=7 measured=1</message></row>
<row><event-time>300</event-time><process fmt="OxideBenchMacOS (42)">42</process><event-type fmt="Event">Event</event-type><signpost-name fmt="PhaseEnd">PhaseEnd</signpost-name><subsystem fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category fmt="Presentation">Presentation</category><message fmt="scenario=2 identifier=7 measured=1">scenario=2 identifier=7 measured=1</message></row>
</node></trace-query-result>"#
}

fn attribution_plan() -> oxide_benchmark_spec::MacOsFullAttributionPlan
{
   materialize_macos_full_attribution_plan(vec![MacOsGpuCounterConfiguration {
      id: String::from("none-current-toolchain"),
      availability: MacOsAttributionAvailability::UnavailableToolchain,
      selector: None,
      unavailable_reason: Some(String::from("no noninteractive GPU-counter configuration template was preregistered")),
   }]).expect("materialize attribution plan")
}
