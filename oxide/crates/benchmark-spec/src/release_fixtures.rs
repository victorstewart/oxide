use serde::{Deserialize, Serialize};

use crate::scenario::ArtifactIdentity;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GridColumnContract
{
   pub viewport_class: String,
   pub columns: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GridThumbnailRecipe
{
   pub base_tile_count: u32,
   pub variant_count: u32,
   pub base_tile_index: String,
   pub variant_index: String,
   pub variants: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GridFixture
{
   pub schema_version: u32,
   pub id: String,
   pub tile_count: u32,
   pub thumbnail_count: u32,
   pub tile_identity: String,
   pub tile_title: String,
   pub detail_tile_index: u32,
   pub column_contracts: Vec<GridColumnContract>,
   pub thumbnail_recipe: GridThumbnailRecipe,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EffectsAnimationFixture
{
   pub duration_ms: u32,
   pub timing_curve: String,
   pub opacity_from_millionths: u32,
   pub opacity_to_millionths: u32,
   pub translation_x_points: i32,
   pub translation_y_points: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EffectsFixture
{
   pub schema_version: u32,
   pub id: String,
   pub card_count: u32,
   pub card_identity: String,
   pub clip_count: u32,
   pub shadow_count: u32,
   pub shadow_index_formula: String,
   pub backdrop_blur_count: u32,
   pub backdrop_blur_radius: u32,
   pub corner_radius: u32,
   pub dirty_layer_index: u32,
   pub animation: EffectsAnimationFixture,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MutationClassFixture
{
   pub id: String,
   pub percent: u32,
   pub changed_node_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MutationSelectionFormula
{
   pub kind: String,
   pub seed: u32,
   pub multiplier: u32,
   pub increment: u32,
   pub modulus: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MutationFixture
{
   pub schema_version: u32,
   pub id: String,
   pub node_count: u32,
   pub node_identity: String,
   pub mutation_repetitions: u32,
   pub mutation_classes: Vec<MutationClassFixture>,
   pub selection_formula: MutationSelectionFormula,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextCategoryFixture
{
   pub id: String,
   pub count: u32,
   pub direction: String,
   pub font_chain: Vec<String>,
   pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextFixture
{
   pub schema_version: u32,
   pub id: String,
   pub label_count: u32,
   pub label_identity: String,
   pub categories: Vec<TextCategoryFixture>,
   pub wrap_width_rotation: Vec<u32>,
   pub scale_change_millionths: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeChangeFixture
{
   pub orientation: String,
   pub width: u32,
   pub height: u32,
   pub theme: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeViewportFixture
{
   pub width: u32,
   pub height: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResizeFixture
{
   pub schema_version: u32,
   pub id: String,
   pub dashboard_fixture: ArtifactIdentity,
   pub initial_orientation: String,
   pub initial_theme: String,
   pub initial_viewport: ResizeViewportFixture,
   pub change_count: u32,
   pub changes: Vec<ResizeChangeFixture>,
}
