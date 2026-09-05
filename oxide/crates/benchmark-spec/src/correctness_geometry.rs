use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::LogicalRect;

pub const MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE: &str = "macos-canonical-srgb8-3x-v1";
pub const MACOS_CANONICAL_CORRECTNESS_SCALE: u32 = 3;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsCorrectnessGeometryEvidence
{
   #[serde(alias = "schemaVersion")]
   pub schema_version: u32,
   #[serde(alias = "coordinateSpace")]
   pub coordinate_space: String,
   #[serde(alias = "captureProfile")]
   pub capture_profile: String,
   #[serde(alias = "canonicalScale")]
   pub canonical_scale: u32,
   pub source: String,
   #[serde(alias = "positionTolerancePoints")]
   pub position_tolerance_points: f64,
   #[serde(alias = "sizeTolerancePoints")]
   pub size_tolerance_points: f64,
   pub root: LogicalRect,
   pub nodes: Vec<MacOsCorrectnessGeometryNode>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsCorrectnessGeometryNode
{
   pub ordinal: u32,
   pub kind: String,
   pub role: String,
   pub identifier: Option<String>,
   pub bounds: LogicalRect,
   #[serde(alias = "textLineBounds")]
   pub text_line_bounds: Vec<LogicalRect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MacOsCorrectnessGeometryPairEvidence
{
   #[serde(alias = "schemaVersion")]
   pub schema_version: u32,
   pub native: MacOsCorrectnessGeometryEvidence,
   pub oxide: MacOsCorrectnessGeometryEvidence,
}

pub fn decode_macos_correctness_geometry(bytes: &[u8]) -> Result<MacOsCorrectnessGeometryEvidence>
{
   let geometry = serde_json::from_slice::<MacOsCorrectnessGeometryEvidence>(bytes).context("decoding macOS correctness geometry")?;
   validate_macos_correctness_geometry(&geometry)?;
   Ok(geometry)
}

pub fn decode_macos_correctness_geometry_pair(bytes: &[u8]) -> Result<MacOsCorrectnessGeometryPairEvidence>
{
   let pair = serde_json::from_slice::<MacOsCorrectnessGeometryPairEvidence>(bytes).context("decoding paired macOS correctness geometry")?;
   ensure!(pair.schema_version == 1, "paired macOS correctness geometry has unsupported schema version {}", pair.schema_version);
   validate_macos_correctness_geometry(&pair.native)?;
   validate_macos_correctness_geometry(&pair.oxide)?;
   validate_macos_correctness_geometry_pair(&pair.native, &pair.oxide)?;
   Ok(pair)
}

pub fn validate_macos_correctness_geometry(geometry: &MacOsCorrectnessGeometryEvidence) -> Result<()>
{
   ensure!(geometry.schema_version == 1, "macOS correctness geometry has unsupported schema version {}", geometry.schema_version);
   ensure!(geometry.coordinate_space == "logical-points", "macOS correctness geometry has a noncanonical coordinate space");
   ensure!(geometry.capture_profile == MACOS_CANONICAL_CORRECTNESS_CAPTURE_PROFILE, "macOS correctness geometry has a noncanonical capture profile");
   ensure!(geometry.canonical_scale == MACOS_CANONICAL_CORRECTNESS_SCALE, "macOS correctness geometry has canonical scale {}, expected {}", geometry.canonical_scale, MACOS_CANONICAL_CORRECTNESS_SCALE);
   ensure!(matches!(geometry.source.as_str(), "appkit-view-tree" | "oxide-semantic-tree"), "macOS correctness geometry has an unsupported runtime source");
   ensure!(geometry.position_tolerance_points == 0.0 && geometry.size_tolerance_points == 0.0, "macOS correctness geometry must declare exact point tolerances");
   ensure!(geometry.root.x == 0.0 && geometry.root.y == 0.0, "macOS correctness geometry root does not start at the logical origin");
   ensure!(geometry.root.width.is_finite() && geometry.root.height.is_finite(), "macOS correctness geometry root has non-finite dimensions");
   ensure!(geometry.root.width > 0.0 && geometry.root.height > 0.0, "macOS correctness geometry root has nonpositive dimensions");
   ensure!(geometry.root.width.fract() == 0.0 && geometry.root.height.fract() == 0.0, "macOS correctness geometry root dimensions must be whole logical points");
   validate_scaled_dimension(geometry.root.width, geometry.canonical_scale, "width")?;
   validate_scaled_dimension(geometry.root.height, geometry.canonical_scale, "height")?;
   ensure!(!geometry.nodes.is_empty(), "macOS correctness geometry has no runtime nodes");
   let mut identifiers = std::collections::BTreeSet::new();
   for (ordinal, node) in geometry.nodes.iter().enumerate()
   {
      ensure!(node.ordinal == ordinal as u32, "macOS correctness geometry node ordinals are not contiguous");
      ensure!(!node.kind.is_empty(), "macOS correctness geometry node has an empty kind");
      ensure!(!node.role.is_empty(), "macOS correctness geometry node has an empty semantic role");
      if let Some(identifier) = &node.identifier
      {
         ensure!(!identifier.is_empty(), "macOS correctness geometry node has an empty identifier");
         ensure!(identifiers.insert(identifier), "macOS correctness geometry node identifiers are not unique");
      }
      validate_rect(&node.bounds, "node")?;
      for line in &node.text_line_bounds
      {
         validate_rect(line, "text line")?;
      }
   }
   ensure!(geometry.source != "oxide-semantic-tree" || geometry.nodes.iter().all(|node| node.identifier.is_some()), "Oxide semantic geometry requires a stable identifier for every node");
   Ok(())
}

pub fn validate_macos_correctness_geometry_pair(native: &MacOsCorrectnessGeometryEvidence, oxide: &MacOsCorrectnessGeometryEvidence) -> Result<()>
{
   ensure!(native.source == "appkit-view-tree", "native macOS correctness geometry does not come from the AppKit view tree");
   ensure!(oxide.source == "oxide-semantic-tree", "Oxide macOS correctness geometry does not come from semantic scene composition");
   ensure!(native.capture_profile == oxide.capture_profile, "macOS correctness geometry capture profiles differ");
   ensure!(native.canonical_scale == oxide.canonical_scale, "macOS correctness geometry scales differ");
   ensure!(native.position_tolerance_points == oxide.position_tolerance_points && native.size_tolerance_points == oxide.size_tolerance_points, "macOS correctness geometry tolerances differ");
   ensure!(native.root == oxide.root, "macOS correctness geometry roots differ");
   let native_nodes = identified_nodes(native);
   let oxide_nodes = identified_nodes(oxide);
   ensure!(!native_nodes.is_empty(), "native macOS correctness geometry has no identified semantic nodes");
   ensure!(native_nodes.len() == native.nodes.iter().filter(|node| node.identifier.is_some()).count(), "native macOS correctness geometry identifiers are not unique");
   ensure!(oxide_nodes.len() == oxide.nodes.len(), "Oxide macOS correctness geometry identifiers are not unique");
   ensure!(native_nodes.keys().eq(oxide_nodes.keys()), "macOS correctness semantic node identifiers differ");
   for (identifier, native_node) in native_nodes
   {
      let oxide_node = oxide_nodes[identifier];
      ensure!(native_node.bounds == oxide_node.bounds, "macOS correctness geometry differs for semantic node {identifier}");
      ensure!(text_line_bounds_match(native_node, oxide_node, native.canonical_scale), "macOS correctness text-line geometry differs by more than one canonical pixel for semantic node {identifier}");
   }
   Ok(())
}

fn identified_nodes(geometry: &MacOsCorrectnessGeometryEvidence) -> std::collections::BTreeMap<&str, &MacOsCorrectnessGeometryNode>
{
   geometry.nodes.iter().filter_map(|node| node.identifier.as_deref().map(|identifier| (identifier, node))).collect()
}

fn text_line_bounds_match(native: &MacOsCorrectnessGeometryNode, oxide: &MacOsCorrectnessGeometryNode, scale: u32) -> bool
{
   native.text_line_bounds.len() == oxide.text_line_bounds.len()
      && native.text_line_bounds.iter().zip(&oxide.text_line_bounds).all(|(native, oxide)|
      {
         [native.x - oxide.x, native.y - oxide.y, native.width - oxide.width, native.height - oxide.height]
            .iter()
            .all(|delta| delta.abs() * f64::from(scale) <= 1.000_001)
      })
}

fn validate_rect(rect: &LogicalRect, label: &str) -> Result<()>
{
   ensure!(rect.x.is_finite() && rect.y.is_finite() && rect.width.is_finite() && rect.height.is_finite(), "macOS correctness geometry {label} has non-finite coordinates");
   ensure!(rect.width > 0.0 && rect.height > 0.0, "macOS correctness geometry {label} has nonpositive dimensions");
   Ok(())
}

fn validate_scaled_dimension(points: f64, scale: u32, label: &str) -> Result<()>
{
   let pixels = points * f64::from(scale);
   ensure!(pixels <= f64::from(u32::MAX) && pixels.fract() == 0.0, "macOS correctness geometry {label} does not map exactly to the canonical pixel grid");
   Ok(())
}
