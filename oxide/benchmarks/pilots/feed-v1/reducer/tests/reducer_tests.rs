use oxide_feed_v1_reducer::{
   frozen_components, frozen_order_index, strict_validate_failure_json, strict_validate_run_json,
};
use serde_json::{json, Value};

fn valid_run_json() -> Result<Value, String>
{
   run_json(
      "primary-s00-p00-o0-uikit-idiomatic-forward-test",
      "primary",
      0,
      0,
      "uikit-idiomatic",
      "forward",
      0,
   )
}
#[test]
fn strict_success_schema_round_trips_and_rejects_unknown_fields() -> Result<(), String>
{
   let valid = valid_run_json()?;
   let bytes = serde_json::to_vec(&valid).map_err(|error| error.to_string())?;
   strict_validate_run_json(&bytes)?;

   let mut invalid = valid;
   let object = invalid.as_object_mut().ok_or_else(|| "run fixture is not an object".to_string())?;
   object.insert("unfrozen_extra".to_string(), json!(true));
   let bytes = serde_json::to_vec(&invalid).map_err(|error| error.to_string())?;
   assert!(strict_validate_run_json(&bytes).is_err());

   let mut invalid_start = valid_run_json()?;
   invalid_start["gesture"]["start_offset_points"] = json!(1.0);
   let bytes = serde_json::to_vec(&invalid_start).map_err(|error| error.to_string())?;
   assert!(strict_validate_run_json(&bytes).is_err());

   for (order_index, treatment) in ["uikit-idiomatic", "uikit-optimized", "oxide"].iter().enumerate()
   {
      let mut tiny_travel = valid_run_json()?;
      tiny_travel["run"]["nonce"] = json!(format!("primary-s00-p00-o{order_index}-{treatment}-forward-test"));
      tiny_travel["run"]["order_index"] = json!(order_index);
      tiny_travel["run"]["treatment"] = json!(treatment);
      tiny_travel["gesture"]["end_offset_points"] = json!(100.0);
      tiny_travel["gesture"]["signed_travel_points"] = json!(100.0);
      tiny_travel["gesture"]["travel_distance_points"] = json!(100.0);
      let bytes = serde_json::to_vec(&tiny_travel).map_err(|error| error.to_string())?;
      assert!(strict_validate_run_json(&bytes).is_err());
   }

   let mut transient_environment_change = valid_run_json()?;
   transient_environment_change["environment"]["thermal_state_change_count"] = json!(1);
   let bytes = serde_json::to_vec(&transient_environment_change).map_err(|error| error.to_string())?;
   assert!(strict_validate_run_json(&bytes).is_err());

   let mut drag_only = valid_run_json()?;
   drag_only["gesture"]["inertia_observed"] = json!(false);
   let bytes = serde_json::to_vec(&drag_only).map_err(|error| error.to_string())?;
   assert!(strict_validate_run_json(&bytes).is_err());
   Ok(())
}

#[test]
fn strict_failure_schema_round_trips_and_rejects_unknown_fields() -> Result<(), String>
{
   let mut failure = json!({
      "schema": "oxide.feed-v1.failure",
      "schema_revision": 1,
      "fixture": {
         "schema": "oxide.feed-v1.fixture",
         "revision": 1,
         "canonical_sha256": "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473",
         "canonical_byte_count": 717745
      },
      "nonce": "smoke-s00-p00-o0-oxide-forward-test",
      "treatment": "oxide",
      "stage": "launch",
      "message": "deliberate test failure"
   });
   strict_validate_failure_json(&serde_json::to_vec(&failure).map_err(|error| error.to_string())?)?;
   failure["nonce"] = Value::Null;
   strict_validate_failure_json(&serde_json::to_vec(&failure).map_err(|error| error.to_string())?)?;
   failure["nonce"] = json!("smoke-s00-p00-o0-oxide-forward-test");
   failure.as_object_mut().ok_or_else(|| "failure fixture is not an object".to_string())?
      .insert("unfrozen_extra".to_string(), json!(true));
   assert!(strict_validate_failure_json(&serde_json::to_vec(&failure).map_err(|error| error.to_string())?).is_err());
   Ok(())
}

#[test]
fn frozen_recipe_matches_contract_extent_and_component_states() -> Result<(), String>
{
   let top = frozen_components("top")?;
   let bottom = frozen_components("bottom")?;
   assert!(!top.is_empty());
   assert!(!bottom.is_empty());
   assert_eq!(top[0].id, "feed-v1-row-0000/row");
   assert!(bottom[0].row_index > 1_900);
   assert!(top.iter().all(|component| component.viewport_clip_px.y >= 0));
   assert!(bottom.iter().all(|component| component.viewport_clip_px.y >= 0));
   Ok(())
}
#[test]
fn frozen_order_maps_each_treatment_and_both_directions_share_it() -> Result<(), String>
{
   assert_eq!(frozen_order_index("smoke", 0, 0, "uikit-idiomatic")?, 0);
   assert_eq!(frozen_order_index("smoke", 0, 0, "uikit-optimized")?, 1);
   assert_eq!(frozen_order_index("smoke", 0, 0, "oxide")?, 2);
   assert_eq!(frozen_order_index("primary", 0, 1, "uikit-optimized")?, 0);
   assert_eq!(frozen_order_index("primary", 0, 1, "oxide")?, 1);
   assert_eq!(frozen_order_index("primary", 0, 1, "uikit-idiomatic")?, 2);
   Ok(())
}
fn run_json(nonce: &str, phase: &str, session_index: u32, pair_index: u32, treatment: &str, direction: &str, order_index: u32) -> Result<Value, String>
{
   let start_state = if direction == "forward" { "top" } else { "bottom" };
   let captured_offset = if direction == "forward" { 0.0 } else { 236_616.0 };
   let signed_travel = if direction == "forward" { 2_000.0 } else { -2_000.0 };
   let end_offset = captured_offset + signed_travel;
   Ok(json!({
      "schema": "oxide.feed-v1.run",
      "schema_revision": 1,
      "fixture": {
         "schema": "oxide.feed-v1.fixture",
         "revision": 1,
         "canonical_sha256": "a1de9b4a914734fe21d21e9b6f8a9b61970f7e22e0fa4ef0103031e399881473",
         "canonical_byte_count": 717745
      },
      "run": {
         "nonce": nonce,
         "phase": phase,
         "session_index": session_index,
         "pair_index": pair_index,
         "order_index": order_index,
         "treatment": treatment,
         "start_state": start_state,
         "direction": direction
      },
      "canvas": {
         "host_width_points": 440,
         "host_height_points": 956,
         "surface_origin_x_points": 25,
         "surface_origin_y_points": 56,
         "surface_width_points": 390,
         "surface_height_points": 844,
         "scale": 3
      },
      "geometry": {
         "row_count": 2000,
         "manifest_component_count": 12000,
         "content_extent_points": 237460.0,
         "maximum_content_offset_points": 236616.0,
         "captured_content_offset_points": captured_offset,
         "viewport_clip_px": { "x": 0, "y": 0, "width": 1170, "height": 2532 },
         "visible_components": frozen_components(start_state)?
      },
      "environment": {
         "before": {
            "thermal_state": "nominal",
            "low_power_mode": false,
            "maximum_frames_per_second": 120,
            "configured_frame_rate": { "minimum": 120.0, "maximum": 120.0, "preferred": 120.0 }
         },
         "after": {
            "thermal_state": "nominal",
            "low_power_mode": false,
            "maximum_frames_per_second": 120,
            "configured_frame_rate": { "minimum": 120.0, "maximum": 120.0, "preferred": 120.0 }
         },
         "thermal_state_change_count": 0,
         "low_power_mode_change_count": 0
      },
      "gesture": {
         "start_offset_points": captured_offset,
         "end_offset_points": end_offset,
         "signed_travel_points": signed_travel,
         "travel_distance_points": 2000.0,
         "duration_seconds": 1.0,
         "inertia_observed": true,
         "settled": true
      },
      "display_link": {
         "clock": "CADisplayLink.timestamp/targetTimestamp",
         "callback_only": true,
         "samples": [
            { "timestamp_seconds": 100.0, "target_timestamp_seconds": 100.008333333333 },
            { "timestamp_seconds": 100.008333333333, "target_timestamp_seconds": 100.016666666666 }
         ]
      },
      "status": "complete",
      "failure": null
   }))
}
