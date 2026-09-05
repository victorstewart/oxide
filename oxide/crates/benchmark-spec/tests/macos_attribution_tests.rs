use oxide_benchmark_spec::{canonical_macos_full_attribution_plan_json, materialize_macos_full_attribution_plan, validate_macos_full_attribution_plan, MacOsAttributionAvailability, MacOsAttributionCollectorKind, MacOsAttributionTraceSelector, MacOsGpuCounterConfiguration, APPLE_RELEASE_SCENARIO_IDS, MACOS_FULL_ATTRIBUTION_MAX_HARD_SECONDS, MACOS_FULL_ATTRIBUTION_PAIR_COUNT};

#[test]
fn full_attribution_schedules_every_available_configuration_as_an_isolated_thirteen_scenario_replay()
{
   let plan = materialize_macos_full_attribution_plan(vec![
      MacOsGpuCounterConfiguration {
         id: String::from("apple-gpu-core-v1"),
         availability: MacOsAttributionAvailability::Available,
         selector: Some(MacOsAttributionTraceSelector {template: Some(String::from("/tmp/apple-gpu-core-v1.tracetemplate")), instrument: None}),
         unavailable_reason: None,
      },
      MacOsGpuCounterConfiguration {
         id: String::from("tile-statistics-v1"),
         availability: MacOsAttributionAvailability::UnavailableDevice,
         selector: None,
         unavailable_reason: Some(String::from("selected host GPU does not expose the preregistered tile-statistics counter set")),
      },
   ]).expect("materialize full attribution");
   validate_macos_full_attribution_plan(&plan).expect("validate full attribution");

   assert_eq!(plan.replays.len(), 4);
   assert!(plan.replays.iter().all(|replay| replay.pair_count == MACOS_FULL_ATTRIBUTION_PAIR_COUNT));
   assert!(plan.replays.iter().all(|replay| replay.scenario_ids.iter().map(String::as_str).eq(APPLE_RELEASE_SCENARIO_IDS.iter().copied())));
   assert!(plan.replays.iter().all(|replay| replay.combination_calibration == "none-isolated-replay"));
   let gpu = plan.replays.iter().find(|replay| replay.collector == MacOsAttributionCollectorKind::GpuCounters).expect("available GPU replay");
   assert_eq!(gpu.configuration_id.as_deref(), Some("apple-gpu-core-v1"));
   assert!(!plan.replays.iter().any(|replay| replay.configuration_id.as_deref() == Some("tile-statistics-v1")));
   assert_eq!(plan.budget.available_replay_count, 4);
   assert!(plan.budget.hard_total_seconds <= MACOS_FULL_ATTRIBUTION_MAX_HARD_SECONDS);
   let json = canonical_macos_full_attribution_plan_json(&plan).expect("canonical full-attribution JSON");
   assert_eq!(serde_json::from_slice::<oxide_benchmark_spec::MacOsFullAttributionPlan>(&json).expect("decode full attribution"), plan);
}

#[test]
fn full_attribution_rejects_missing_or_combined_replays_and_implicit_counter_availability()
{
   let error = materialize_macos_full_attribution_plan(vec![MacOsGpuCounterConfiguration {
      id: String::from("implicit"),
      availability: MacOsAttributionAvailability::Available,
      selector: None,
      unavailable_reason: None,
   }]).expect_err("available GPU counter without selector must fail");
   assert!(format!("{:#}", error).contains("no trace selector"));

   let unavailable = MacOsGpuCounterConfiguration {
      id: String::from("none-current-toolchain"),
      availability: MacOsAttributionAvailability::UnavailableToolchain,
      selector: None,
      unavailable_reason: Some(String::from("no noninteractive GPU-counter configuration template was preregistered")),
   };
   let mut plan = materialize_macos_full_attribution_plan(vec![unavailable.clone()]).expect("materialize mandatory replay set");
   plan.replays.retain(|replay| replay.id != "vm-tracker");
   assert!(validate_macos_full_attribution_plan(&plan).is_err());

   let mut plan = materialize_macos_full_attribution_plan(vec![unavailable]).expect("rematerialize mandatory replay set");
   plan.replays[0].selector.instrument = Some(String::from("VM Tracker"));
   assert!(validate_macos_full_attribution_plan(&plan).is_err());

   let mut plan = materialize_macos_full_attribution_plan(vec![MacOsGpuCounterConfiguration {
      id: String::from("unsupported"),
      availability: MacOsAttributionAvailability::UnavailableToolchain,
      selector: None,
      unavailable_reason: Some(String::from("xctrace exposes the instrument but no noninteractive configuration selector")),
   }]).expect("materialize explicit unavailable counter");
   plan.gpu_counter_configurations[0].unavailable_reason = None;
   assert!(validate_macos_full_attribution_plan(&plan).is_err());
}
