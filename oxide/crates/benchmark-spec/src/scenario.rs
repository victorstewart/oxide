use serde::{Deserialize, Serialize};

use crate::schema::BENCHMARK_SPEC_SCHEMA_VERSION;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactIdentity
{
   pub path: String,
   pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AssetManifest
{
   pub schema_version: u32,
   pub id: String,
   pub artifacts: Vec<AssetFile>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub inline_text_atlas: Option<InlineTextAtlas>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AssetFile
{
   pub role: String,
   pub artifact: ArtifactIdentity,
   pub media_type: String,
   pub color_space: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InlineTextAtlas
{
   pub columns: u32,
   pub rows: u32,
   pub source_repository: String,
   pub source_commit: String,
   pub license: ArtifactIdentity,
   pub variants: Vec<InlineTextAtlasVariant>,
   pub entries: Vec<InlineTextAsset>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InlineTextAtlasVariant
{
   pub artifact_role: String,
   pub pixel_width: u32,
   pub pixel_height: u32,
   pub em_pixels: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InlineTextAsset
{
   pub grapheme: String,
   pub column: u32,
   pub row: u32,
   pub advance_millionths: u32,
   pub top_from_baseline_millionths: i32,
   pub width_millionths: u32,
   pub height_millionths: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FontPackIdentity
{
   pub id: String,
   pub manifest: String,
   pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FontPackManifest
{
   pub schema_version: u32,
   pub id: String,
   pub fonts: Vec<FontPackFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FontPackFile
{
   pub role: String,
   pub artifact: ArtifactIdentity,
   pub variation_axes: Vec<FontVariationAxis>,
   pub source_repository: String,
   pub source_commit: String,
   pub source_path: String,
   pub license: ArtifactIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FontVariationAxis
{
   pub tag: String,
   pub value_millionths: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SceneContract
{
   pub roles: Vec<String>,
   pub style_tokens: ArtifactIdentity,
   pub layout_assertions: ArtifactIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParityCheckpoint
{
   pub id: String,
   pub phase_id: String,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub at_us: Option<u64>,
   pub state: ArtifactIdentity,
   pub accessibility: ArtifactIdentity,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub geometry: Option<ArtifactIdentity>,
   pub screenshot: ArtifactIdentity,
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub recapture_status: Option<String>,
   pub expected_visible_role_counts: Vec<RoleCount>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioPhase
{
   pub id: String,
   pub measured: bool,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub duration_ms: Option<u64>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub trace: Option<ArtifactIdentity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FairnessContract
{
   pub locale: String,
   pub timezone: String,
   pub direction: String,
   pub logical_viewport_width: u32,
   pub logical_viewport_height: u32,
   pub expected_visible_role_counts: Vec<RoleCount>,
   pub schedule_tolerance_us: u64,
   pub coordinate_tolerance_microunits: u32,
   pub elapsed_time_driven: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RoleCount
{
   pub role: String,
   pub count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScenarioSpec
{
   pub schema_version: u32,
   pub id: String,
   pub fixture: ArtifactIdentity,
   pub assets: ArtifactIdentity,
   pub font_pack: FontPackIdentity,
   pub viewport_class: String,
   pub scene: SceneContract,
   pub phases: Vec<ScenarioPhase>,
   pub primary_metric: String,
   pub required_metrics: Vec<String>,
   pub optional_metrics: Vec<String>,
   pub parity_checkpoints: Vec<ParityCheckpoint>,
   pub fairness_contract: FairnessContract,
}

impl ScenarioSpec
{
   pub fn is_v1(&self) -> bool
   {
      self.schema_version == BENCHMARK_SPEC_SCHEMA_VERSION
   }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TraceOperation
{
   PointerDown,
   PointerMove,
   PointerUp,
   PointerCancel,
   Wheel,
   KeyDown,
   KeyUp,
   CommitText,
   ImeStart,
   ImeUpdate,
   ImeEnd,
   Focus,
   Resize,
   Orientation,
   Theme,
   Scale,
   Mutate,
   Navigate,
   Background,
   Foreground,
   ResourceArrival,
   Pressure,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TraceEvent
{
   pub at_us: u64,
   pub op: TraceOperation,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub pointer: Option<u32>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub x_millionths: Option<i32>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub y_millionths: Option<i32>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub delta_x_millionths: Option<i32>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub delta_y_millionths: Option<i32>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub target: Option<String>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub value: Option<TraceValue>,
   #[serde(skip_serializing_if = "Option::is_none")]
   pub state_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum TraceValue
{
   Boolean(bool),
   Integer(i64),
   Text(String),
}
