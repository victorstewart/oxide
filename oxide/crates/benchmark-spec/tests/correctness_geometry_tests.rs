use oxide_benchmark_spec::{decode_macos_correctness_geometry, validate_macos_correctness_geometry_pair, MacOsCorrectnessGeometryEvidence, MacOsCorrectnessGeometryNode, MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE, MACOS_CANONICAL_CORRECTNESS_SCALE};

#[test]
fn canonical_runtime_geometry_round_trips_with_exact_profile_and_scale()
{
   let bytes = br#"{
      "schema_version": 1,
      "coordinate_space": "logical-points",
      "capture_profile": "macos-canonical-srgb8-3x-v1",
      "canonical_scale": 3,
      "source": "appkit-view-tree",
      "position_tolerance_points": 0.0,
      "size_tolerance_points": 0.0,
      "root": {"x": 0.0, "y": 0.0, "width": 844.0, "height": 390.0},
      "nodes": [{"ordinal": 0, "kind": "text", "role": "heading", "identifier": "title", "bounds": {"x": 16.0, "y": 20.0, "width": 200.0, "height": 48.0}, "text_line_bounds": [{"x": 16.0, "y": 20.0, "width": 200.0, "height": 48.0}]}]
   }"#;
   let geometry = decode_macos_correctness_geometry(bytes).expect("canonical runtime geometry");
   assert_eq!(geometry.capture_profile, MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE);
   assert_eq!(geometry.canonical_scale, MACOS_CANONICAL_CORRECTNESS_SCALE);
   assert_eq!(geometry.root.width, 844.0);
   assert_eq!(geometry.root.height, 390.0);
}

#[test]
fn canonical_runtime_geometry_accepts_live_swift_camel_case_keys()
{
   let bytes = br#"{
      "schemaVersion": 1,
      "coordinateSpace": "logical-points",
      "captureProfile": "macos-canonical-srgb8-3x-v1",
      "canonicalScale": 3,
      "source": "appkit-view-tree",
      "positionTolerancePoints": 0.0,
      "sizeTolerancePoints": 0.0,
      "root": {"x": 0.0, "y": 0.0, "width": 390.0, "height": 844.0},
      "nodes": [{"ordinal": 0, "kind": "text", "role": "heading", "identifier": "title", "bounds": {"x": 16.0, "y": 20.0, "width": 200.0, "height": 48.0}, "textLineBounds": [{"x": 16.0, "y": 20.0, "width": 200.0, "height": 48.0}]}]
   }"#;
   let geometry = decode_macos_correctness_geometry(bytes).expect("live Swift geometry");
   assert_eq!(geometry.nodes[0].text_line_bounds.len(), 1);
}

#[test]
fn noncanonical_profile_scale_origin_and_pixel_grid_fail_closed()
{
   for bytes in [
      geometry("other-profile", 3, 0.0, 390.0),
      geometry(MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE, 2, 0.0, 390.0),
      geometry(MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE, 3, 1.0, 390.0),
      geometry(MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE, 3, 0.0, 390.1),
   ]
   {
      assert!(decode_macos_correctness_geometry(&bytes).is_err());
   }
}

#[test]
fn paired_semantic_geometry_accepts_one_pixel_text_rounding_but_rejects_layout_drift()
{
   let native = evidence("appkit-view-tree");
   let mut oxide = evidence("oxide-semantic-tree");
   validate_macos_correctness_geometry_pair(&native, &oxide).expect("exact semantic geometry");

   oxide.nodes[0].text_line_bounds[0].y += 1.0 / 3.0;
   validate_macos_correctness_geometry_pair(&native, &oxide).expect("one canonical text pixel");
   oxide.nodes[0].text_line_bounds[0].y -= 1.0 / 3.0;
   oxide.nodes[0].bounds.x += 1.0;
   assert!(validate_macos_correctness_geometry_pair(&native, &oxide).is_err());
   oxide.nodes[0].bounds.x -= 1.0;
   oxide.nodes[0].text_line_bounds[0].height += 1.0;
   assert!(validate_macos_correctness_geometry_pair(&native, &oxide).is_err());
}

fn geometry(profile: &str, scale: u32, x: f64, width: f64) -> Vec<u8>
{
   let mut evidence = evidence("appkit-view-tree");
   evidence.capture_profile = String::from(profile);
   evidence.canonical_scale = scale;
   evidence.root.x = x;
   evidence.root.width = width;
   serde_json::to_vec(&evidence).expect("encode runtime geometry")
}

fn evidence(source: &str) -> MacOsCorrectnessGeometryEvidence
{
   MacOsCorrectnessGeometryEvidence {
      schema_version: 1,
      coordinate_space: String::from("logical-points"),
      capture_profile: String::from(MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE),
      canonical_scale: MACOS_CANONICAL_CORRECTNESS_SCALE,
      source: String::from(source),
      position_tolerance_points: 0.0,
      size_tolerance_points: 0.0,
      root: oxide_benchmark_spec::LogicalRect {x: 0.0, y: 0.0, width: 390.0, height: 844.0},
      nodes: vec![MacOsCorrectnessGeometryNode {
         ordinal: 0,
         kind: String::from("text"),
         role: String::from("heading"),
         identifier: Some(String::from("title")),
         bounds: oxide_benchmark_spec::LogicalRect {x: 16.0, y: 20.0, width: 200.0, height: 48.0},
         text_line_bounds: vec![oxide_benchmark_spec::LogicalRect {x: 16.0, y: 20.0, width: 200.0, height: 48.0}],
      }],
   }
}
