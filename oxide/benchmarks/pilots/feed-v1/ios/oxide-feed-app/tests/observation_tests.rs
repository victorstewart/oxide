use oxide_feed_v1_app::contract::{self, FeedFixture, StartState};
use oxide_feed_v1_app::observation::{
   nonce_is_valid,
   write_failure_json,
   write_record_json,
   CallbackSample,
   DeviceState,
   FailureRecord,
   ObservedComponent,
   ObservedRect,
   RunRecord,
};
use serde_json::Value;

#[test]
fn complete_record_matches_the_authoritative_nested_schema()
{
   let fixture = FeedFixture::new();
   let callbacks = [
      CallbackSample { timestamp_ns: 100_000_000_000, target_timestamp_ns: 100_008_333_333 },
      CallbackSample { timestamp_ns: 100_008_333_333, target_timestamp_ns: 100_016_666_666 },
   ];
   let components = sample_components(&fixture, StartState::Top);
   let record = sample_record(&fixture, StartState::Top, &callbacks, &components);
   let value = serialize(&record);
   assert_eq!(value["schema"], contract::RUN_RECORD_SCHEMA);
   assert_eq!(value["schema_revision"], contract::RUN_RECORD_SCHEMA_REVISION);
   assert_eq!(value["fixture"]["schema"], contract::SCHEMA);
   assert_eq!(value["fixture"]["canonical_sha256"], contract::EXPECTED_CANONICAL_SHA256);
   assert_eq!(
      value["fixture"]["canonical_byte_count"],
      contract::EXPECTED_CANONICAL_BYTE_COUNT,
   );
   assert_eq!(value["run"]["treatment"], contract::OXIDE_VARIANT_VALUE);
   assert_eq!(value["run"]["start_state"], "top");
   assert_eq!(value["run"]["direction"], "forward");
   assert_eq!(value["geometry"]["manifest_component_count"], 12_000);
   assert_eq!(value["status"], "complete");
   assert!(value["failure"].is_null());
   assert_eq!(value["gesture"]["inertia_observed"], true);
   assert_eq!(value["environment"]["thermal_state_change_count"], 3);
   assert_eq!(value["environment"]["low_power_mode_change_count"], 2);
   assert_exact_keys(
      &value["gesture"],
      &[
         "start_offset_points",
         "end_offset_points",
         "signed_travel_points",
         "travel_distance_points",
         "duration_seconds",
         "settled",
         "inertia_observed",
      ],
   );
   assert_exact_keys(
      &value["environment"],
      &[
         "before",
         "after",
         "thermal_state_change_count",
         "low_power_mode_change_count",
      ],
   );
   assert_eq!(value["display_link"]["samples"].as_array().map(Vec::len), Some(2));
   assert_schema_keys(&value);
   assert_visible_components(&value, StartState::Top);
}

#[test]
fn bottom_components_keep_content_coordinates_and_translate_their_clips()
{
   let fixture = FeedFixture::new();
   let callbacks = [
      CallbackSample { timestamp_ns: 1_000_000_000, target_timestamp_ns: 1_008_333_333 },
      CallbackSample { timestamp_ns: 1_008_333_333, target_timestamp_ns: 1_016_666_666 },
   ];
   let components = sample_components(&fixture, StartState::Bottom);
   let record = sample_record(&fixture, StartState::Bottom, &callbacks, &components);
   let value = serialize(&record);
   assert_eq!(value["run"]["start_state"], "bottom");
   assert_eq!(value["run"]["direction"], "reverse");
   assert_eq!(
      value["geometry"]["captured_content_offset_points"],
      contract::MAXIMUM_CONTENT_OFFSET_POINTS as f64,
   );
   assert_visible_components(&value, StartState::Bottom);
   let components = value["geometry"]["visible_components"].as_array();
   assert!(components.is_some_and(|items|
   {
      items.iter().any(|item|
      {
         item["content_rect_px"]["y"].as_u64().unwrap_or(0)
            > item["viewport_clip_px"]["y"].as_u64().unwrap_or(u64::MAX)
      })
   }));
}

#[test]
fn failure_record_uses_the_strict_non_measurable_schema()
{
   let record = FailureRecord {
      nonce: Some("primary-s00-p00-o0-oxide-forward-test"),
      treatment: Some(contract::OXIDE_VARIANT_VALUE),
      stage: "initial-admission",
      message: "renderer upload unavailable",
   };
   let mut bytes = Vec::new();
   assert!(write_failure_json(&record, &mut bytes).is_ok());
   let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
   assert_exact_keys(
      &value,
      &["schema", "schema_revision", "fixture", "nonce", "treatment", "stage", "message"],
   );
   assert_eq!(value["schema"], contract::FAILURE_RECORD_SCHEMA);
   assert_eq!(value["schema_revision"], contract::RUN_RECORD_SCHEMA_REVISION);
   assert_fixture_identity(&value["fixture"]);
   assert_eq!(value["nonce"], "primary-s00-p00-o0-oxide-forward-test");
   assert_eq!(value["treatment"], contract::OXIDE_VARIANT_VALUE);
   assert_eq!(value["stage"], "initial-admission");
   assert_eq!(value["message"], "renderer upload unavailable");
   assert!(value.get("run").is_none());
   assert!(value.get("display_link").is_none());
}

#[test]
fn launch_failure_can_truthfully_omit_unparsed_inputs()
{
   let record = FailureRecord {
      nonce: None,
      treatment: None,
      stage: "launch",
      message: "launch input parsing failed",
   };
   let mut bytes = Vec::new();
   assert!(write_failure_json(&record, &mut bytes).is_ok());
   let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
   assert!(value["nonce"].is_null());
   assert!(value["treatment"].is_null());
   assert_eq!(value["stage"], "launch");
}

#[test]
fn nonce_validation_prevents_path_or_notification_injection()
{
   for valid in ["smoke-0", "primary-s01-p02", "A9"]
   {
      assert!(nonce_is_valid(valid));
   }
   for invalid in ["", "../escape", "under_score", "has.dot", "slash/name", "space name", "line\nbreak"]
   {
      assert!(!nonce_is_valid(invalid));
   }
}

fn sample_record<'a>(fixture: &'a FeedFixture, state: StartState, callbacks: &'a [CallbackSample], visible_components: &'a [ObservedComponent]) -> RunRecord<'a>
{
   RunRecord {
      nonce: "primary-s00-p00-o0-oxide-forward-test",
      phase: "primary",
      session_index: 0,
      pair_index: 0,
      order_index: 0,
      state,
      fixture,
      captured_content_offset_points: state.offset_points() as f32,
      visible_components,
      start_offset_points: state.offset_points() as f32,
      end_offset_points: if state == StartState::Top { 2_000.0 } else { 234_616.0 },
      gesture_start_timestamp_ns: 1_000_000_000,
      settled_timestamp_ns: 2_000_000_000,
      inertia_observed: true,
      device_before: device_state(),
      device_after: device_state(),
      thermal_state_change_count: 3,
      low_power_mode_change_count: 2,
      callbacks,
   }
}

fn sample_components(fixture: &FeedFixture, state: StartState) -> Vec<ObservedComponent>
{
   let mut components = Vec::new();
   let scale = contract::SURFACE_SCALE as f32;
   for row_index in fixture.visible_row_range(state.offset_points())
   {
      for (kind_index, kind) in contract::COMPONENT_KINDS.iter().copied().enumerate()
      {
         let Some(rect) = fixture.component_rect_physical_pixels(row_index, kind) else
         {
            continue;
         };
         components.push(ObservedComponent {
            row_index,
            kind_index,
            content_rect_points: ObservedRect {
               x: rect.x as f32 / scale,
               y: rect.y as f32 / scale,
               width: rect.width as f32 / scale,
               height: rect.height as f32 / scale,
            },
         });
      }
   }
   components
}

fn device_state() -> DeviceState
{
   DeviceState {
      thermal_state: 0,
      low_power_mode: false,
      maximum_frames_per_second: 120,
      range_minimum_frames_per_second: 120.0,
      range_maximum_frames_per_second: 120.0,
      range_preferred_frames_per_second: 120.0,
   }
}

fn serialize(record: &RunRecord<'_>) -> Value
{
   let mut bytes = Vec::new();
   assert!(write_record_json(record, &mut bytes).is_ok());
   serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn assert_schema_keys(value: &Value)
{
   assert_exact_keys(
      value,
      &[
         "schema",
         "schema_revision",
         "fixture",
         "run",
         "canvas",
         "geometry",
         "environment",
         "gesture",
         "display_link",
         "status",
         "failure",
      ],
   );
   assert_fixture_identity(&value["fixture"]);
}

fn assert_fixture_identity(value: &Value)
{
   assert_exact_keys(value, &["schema", "revision", "canonical_sha256", "canonical_byte_count"]);
   assert_eq!(value["schema"], contract::SCHEMA);
   assert_eq!(value["revision"], contract::REVISION);
   assert_eq!(value["canonical_sha256"], contract::EXPECTED_CANONICAL_SHA256);
   assert_eq!(value["canonical_byte_count"], contract::EXPECTED_CANONICAL_BYTE_COUNT);
}

fn assert_exact_keys(value: &Value, expected: &[&str])
{
   let Some(object) = value.as_object() else
   {
      panic!("schema value is not an object");
   };
   assert_eq!(object.len(), expected.len());
   for key in expected
   {
      assert!(object.contains_key(*key), "missing key {key}");
   }
}

fn assert_visible_components(value: &Value, state: StartState)
{
   let Some(components) = value["geometry"]["visible_components"].as_array() else
   {
      panic!("visible_components is not an array");
   };
   assert!(!components.is_empty());
   let mut prior = None;
   for component in components
   {
      let row = component["row_index"].as_u64().unwrap_or(u64::MAX);
      let kind = component["kind"].as_str().unwrap_or("");
      let kind_index = contract::COMPONENT_KINDS
         .iter()
         .position(|candidate| *candidate == kind)
         .unwrap_or(usize::MAX);
      if let Some((prior_row, prior_kind)) = prior
      {
         assert!((row, kind_index) > (prior_row, prior_kind));
      }
      prior = Some((row, kind_index));
      let content = &component["content_rect_px"];
      let clip = &component["viewport_clip_px"];
      assert!(content["width"].as_u64().unwrap_or(0) > 0);
      assert!(content["height"].as_u64().unwrap_or(0) > 0);
      assert!(clip["width"].as_u64().unwrap_or(0) > 0);
      assert!(clip["height"].as_u64().unwrap_or(0) > 0);
      assert!(clip["x"].as_u64().unwrap_or(u64::MAX) < 1_170);
      assert!(clip["y"].as_u64().unwrap_or(u64::MAX) < 2_532);
      assert!(component["id"].as_str().is_some_and(|id| id.ends_with(kind)));
   }
   assert_eq!(value["run"]["start_state"], state.as_str());
   assert_eq!(value["run"]["direction"], state.direction());
}
