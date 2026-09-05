use oxide_apple_comparison_controller::{correlate_macos_launch_presentation_trace, correlate_macos_presentation_trace, MacOsLaunchClockAnchorInterval};

fn signposts_xml(extra_display: &str) -> String
{
   format!(r#"<?xml version="1.0"?>
<trace-query-result>
<node><schema name="os-signpost">
<col><mnemonic>time</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>event-type</mnemonic></col><col><mnemonic>name</mnemonic></col><col><mnemonic>subsystem</mnemonic></col><col><mnemonic>category</mnemonic></col><col><mnemonic>message</mnemonic></col>
</schema>
<row><event-time id="1">100</event-time><process id="2" fmt="OxideBenchMacOS (42)"><pid>42</pid></process><event-type id="3" fmt="Event">Event</event-type><signpost-name id="4" fmt="DisplayOpportunity">DisplayOpportunity</signpost-name><subsystem id="5" fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category id="6" fmt="Presentation">Presentation</category><os-log-metadata id="7" fmt="scenario= 2  generation= 4">scenario=2 generation=4</os-log-metadata></row>
{}
<row><event-time id="8">120</event-time><process ref="2"/><event-type ref="3"/><signpost-name id="9" fmt="InputReceived">InputReceived</signpost-name><subsystem ref="5"/><category ref="6"/><os-log-metadata ref="7"/></row>
<row><event-time id="10">150</event-time><process ref="2"/><event-type ref="3"/><signpost-name id="11" fmt="VisualGeneration">VisualGeneration</signpost-name><subsystem ref="5"/><category ref="6"/><os-log-metadata id="12" fmt="scenario= 2  generation= 5">scenario=2 generation=5</os-log-metadata></row>
</node>
</trace-query-result>"#, extra_display)
}

fn updates_xml() -> &'static str
{
   r#"<?xml version="1.0"?>
<trace-query-result>
<node><schema name="hitches-updates">
<col><mnemonic>start</mnemonic></col><col><mnemonic>duration</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>display</mnemonic></col><col><mnemonic>swap-id</mnemonic></col>
</schema>
<row><start-time>151</start-time><duration>10</duration><process fmt="Other (7)"><pid>7</pid></process><display-name fmt="Display 1">Display 1</display-name><uint32>41</uint32></row>
<row><start-time>200</start-time><duration>20</duration><process id="1" fmt="OxideBenchMacOS (42)"><pid>42</pid></process><display-name id="2" fmt="Display 1">Display 1</display-name><uint32 id="3">42</uint32></row>
<row><start-time>200</start-time><duration>20</duration><process ref="1"/><display-name ref="2"/><uint32 ref="3"/></row>
</node>
</trace-query-result>"#
}

fn frame_lifetimes_xml() -> &'static str
{
   r#"<?xml version="1.0"?>
<trace-query-result>
<node><schema name="hitches-frame-lifetimes">
<col><mnemonic>start</mnemonic></col><col><mnemonic>duration</mnemonic></col><col><mnemonic>display</mnemonic></col><col><mnemonic>swap-id</mnemonic></col>
</schema>
<row><start-time id="1">190</start-time><duration id="2">50</duration><display-name id="3" fmt="Display 1">Display 1</display-name><uint32 id="4">42</uint32></row>
</node>
</trace-query-result>"#
}

#[test]
fn correlates_generation_markers_through_exact_process_update_and_swap_id()
{
   let extra_display = r#"<row><event-time>110</event-time><process ref="2"/><event-type ref="3"/><signpost-name ref="4"/><subsystem ref="5"/><category ref="6"/><os-log-metadata ref="7"/></row>"#;
   let artifact = correlate_macos_presentation_trace(&signposts_xml(extra_display), updates_xml(), frame_lifetimes_xml()).expect("correlated presentation trace");
   assert_eq!(artifact.schema_version, 1);
   assert_eq!(artifact.availability, "available-correlated-frame-lifetime-calibration-pending");
   assert!(artifact.calibration_status.contains("signpost-to-swap"));
   assert_eq!(artifact.correlations.len(), 1);
   assert!(artifact.uncorrelated_visual_generations.is_empty());
   let correlation = &artifact.correlations[0];
   assert_eq!(correlation.process, "OxideBenchMacOS (42)");
   assert_eq!(correlation.scenario_index, 2);
   assert_eq!(correlation.input_generation, 4);
   assert_eq!(correlation.visual_generation, 5);
   assert_eq!(correlation.display_opportunity_ns, Some(110));
   assert_eq!(correlation.input_received_ns, 120);
   assert_eq!(correlation.visual_generation_ns, 150);
   assert_eq!(correlation.update_start_ns, 200);
   assert_eq!(correlation.update_end_ns, 220);
   assert_eq!(correlation.update_selection, "first-exact-process-update-after-visual-generation");
   assert_eq!(correlation.swap_id, 42);
   assert_eq!(correlation.frame_lifetime_start_ns, 190);
   assert_eq!(correlation.frame_lifetime_end_ns, 240);
   assert_eq!(correlation.candidate_input_to_frame_lifetime_end_ns, 120);
   assert_eq!(correlation.candidate_visual_to_frame_lifetime_end_ns, 90);
}

#[test]
fn selects_a_unique_update_that_contains_visual_generation()
{
   let updates = updates_xml().replace("<start-time>200</start-time><duration>20</duration>", "<start-time>140</start-time><duration>80</duration>");
   let artifact = correlate_macos_presentation_trace(&signposts_xml(""), &updates, frame_lifetimes_xml()).expect("containing update correlation");
   assert_eq!(artifact.correlations[0].update_selection, "contains-visual-generation");
}

#[test]
fn preserves_missing_per_generation_display_opportunity_and_rejects_invalid_generation_or_swap()
{
   let missing_display = signposts_xml("").replace("DisplayOpportunity", "UnrelatedMarker");
   let artifact = correlate_macos_presentation_trace(&missing_display, updates_xml(), frame_lifetimes_xml()).expect("correlation without a per-generation display marker");
   assert_eq!(artifact.correlations[0].display_opportunity_ns, None);

   let generation_zero = signposts_xml("").replace("generation= 5", "generation= 0").replace("generation=5", "generation=0");
   assert!(correlate_macos_presentation_trace(&generation_zero, updates_xml(), frame_lifetimes_xml()).is_err());

   let duplicate_frame = frame_lifetimes_xml().replace("</node>", r#"<row><start-time>191</start-time><duration>50</duration><display-name fmt="Display 1">Display 1</display-name><uint32>42</uint32></row></node>"#);
   assert!(correlate_macos_presentation_trace(&signposts_xml(""), updates_xml(), &duplicate_frame).is_err());
}

#[test]
fn records_an_intervening_visual_generation_as_uncorrelated_instead_of_dropping_it()
{
   let intervening = r#"<row><event-time>160</event-time><process ref="2"/><event-type ref="3"/><signpost-name fmt="DisplayOpportunity">DisplayOpportunity</signpost-name><subsystem ref="5"/><category ref="6"/><os-log-metadata fmt="scenario= 3 generation= 7">scenario=3 generation=7</os-log-metadata></row><row><event-time>165</event-time><process ref="2"/><event-type ref="3"/><signpost-name fmt="InputReceived">InputReceived</signpost-name><subsystem ref="5"/><category ref="6"/><os-log-metadata fmt="scenario= 3 generation= 7">scenario=3 generation=7</os-log-metadata></row><row><event-time>175</event-time><process ref="2"/><event-type ref="3"/><signpost-name fmt="VisualGeneration">VisualGeneration</signpost-name><subsystem ref="5"/><category ref="6"/><os-log-metadata fmt="scenario= 3 generation= 8">scenario=3 generation=8</os-log-metadata></row>"#;
   let artifact = correlate_macos_presentation_trace(&signposts_xml(intervening), updates_xml(), frame_lifetimes_xml()).expect("superseded generation evidence");
   assert_eq!(artifact.correlations.len(), 1);
   assert_eq!(artifact.correlations[0].scenario_index, 3);
   assert_eq!(artifact.uncorrelated_visual_generations.len(), 1);
   let uncorrelated = &artifact.uncorrelated_visual_generations[0];
   assert_eq!(uncorrelated.scenario_index, 2);
   assert_eq!(uncorrelated.visual_generation, 5);
   assert_eq!(uncorrelated.superseded_by_scenario_index, 3);
   assert_eq!(uncorrelated.superseded_by_visual_generation, 8);
   assert_eq!(uncorrelated.next_exact_process_update_start_ns, 200);
}

#[test]
fn rejects_malformed_xml_references_and_marker_messages()
{
   let unresolved = signposts_xml("").replace("<process ref=\"2\"/>", "<process ref=\"missing\"/>");
   assert!(correlate_macos_presentation_trace(&unresolved, updates_xml(), frame_lifetimes_xml()).is_err());

   let malformed = signposts_xml("").replace("scenario= 2  generation= 5", "scenario= x  generation= 5");
   assert!(correlate_macos_presentation_trace(&malformed, updates_xml(), frame_lifetimes_xml()).is_err());
}

#[test]
fn correlates_launch_and_trusted_response_through_exact_pid_and_clock_anchors()
{
   let signposts = r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="os-signpost">
<col><mnemonic>time</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>event-type</mnemonic></col><col><mnemonic>name</mnemonic></col><col><mnemonic>subsystem</mnemonic></col><col><mnemonic>category</mnemonic></col><col><mnemonic>message</mnemonic></col>
</schema>
<row><event-time>100</event-time><process fmt="MacOSComparisonControllerUITests-Runner (9)">runner</process><event-type fmt="Event">Event</event-type><signpost-name fmt="ClockAnchor">ClockAnchor</signpost-name><subsystem fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category fmt="ClockMap">ClockMap</category><os-log-metadata fmt="anchor=1">anchor=1</os-log-metadata></row>
<row><event-time>150</event-time><process fmt="OxideBenchMacOS (42)">oxide</process><event-type fmt="Event">Event</event-type><signpost-name fmt="VisualGeneration">VisualGeneration</signpost-name><subsystem fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category fmt="Presentation">Presentation</category><os-log-metadata fmt="scenario=0 generation=1">scenario=0 generation=1</os-log-metadata></row>
<row><event-time>300</event-time><process fmt="OxideBenchMacOS (42)">oxide</process><event-type fmt="Event">Event</event-type><signpost-name fmt="InputReceived">InputReceived</signpost-name><subsystem fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category fmt="Presentation">Presentation</category><os-log-metadata fmt="scenario=0 generation=1">scenario=0 generation=1</os-log-metadata></row>
<row><event-time>310</event-time><process fmt="OxideBenchMacOS (42)">oxide</process><event-type fmt="Event">Event</event-type><signpost-name fmt="VisualGeneration">VisualGeneration</signpost-name><subsystem fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category fmt="Presentation">Presentation</category><os-log-metadata fmt="scenario=0 generation=2">scenario=0 generation=2</os-log-metadata></row>
<row><event-time>500</event-time><process fmt="MacOSComparisonControllerUITests-Runner (9)">runner</process><event-type fmt="Event">Event</event-type><signpost-name fmt="ClockAnchor">ClockAnchor</signpost-name><subsystem fmt="com.oxide.comparison">com.oxide.comparison</subsystem><category fmt="ClockMap">ClockMap</category><os-log-metadata fmt="anchor=2">anchor=2</os-log-metadata></row>
</node></trace-query-result>"#;
   let updates = r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="hitches-updates">
<col><mnemonic>start</mnemonic></col><col><mnemonic>duration</mnemonic></col><col><mnemonic>process</mnemonic></col><col><mnemonic>display</mnemonic></col><col><mnemonic>swap-id</mnemonic></col>
</schema>
<row><start-time>160</start-time><duration>20</duration><process fmt="OxideBenchMacOS (42)">oxide</process><display-name fmt="Display 1">Display 1</display-name><uint32>1</uint32></row>
<row><start-time>320</start-time><duration>20</duration><process fmt="OxideBenchMacOS (42)">oxide</process><display-name fmt="Display 1">Display 1</display-name><uint32>2</uint32></row>
</node></trace-query-result>"#;
   let frame_lifetimes = r#"<?xml version="1.0"?>
<trace-query-result><node><schema name="hitches-frame-lifetimes">
<col><mnemonic>start</mnemonic></col><col><mnemonic>duration</mnemonic></col><col><mnemonic>display</mnemonic></col><col><mnemonic>swap-id</mnemonic></col>
</schema>
<row><start-time>155</start-time><duration>45</duration><display-name fmt="Display 1">Display 1</display-name><uint32>1</uint32></row>
<row><start-time>315</start-time><duration>35</duration><display-name fmt="Display 1">Display 1</display-name><uint32>2</uint32></row>
</node></trace-query-result>"#;
   let anchors = [
      MacOsLaunchClockAnchorInterval {id: 1, before_ticks: 995, after_ticks: 1_005},
      MacOsLaunchClockAnchorInterval {id: 2, before_ticks: 4_995, after_ticks: 5_005},
   ];
   let correlation = correlate_macos_launch_presentation_trace(signposts, updates, frame_lifetimes, 42, &anchors).expect("launch presentation correlation");
   assert_eq!(correlation.process, "OxideBenchMacOS (42)");
   assert_eq!(correlation.first_attributed_present_proxy_ticks, 2_000);
   assert_eq!(correlation.input_received_ticks, 3_000);
   assert_eq!(correlation.response_attributed_present_proxy_ticks, 3_500);
   assert!(correlation.exact_pid_filtered);
   assert_eq!(correlation.clock_mapping_max_uncertainty_ticks, 12);
}
