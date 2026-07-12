pub const BACKEND_CPU: u8 = 1 << 0;
pub const BACKEND_METAL: u8 = 1 << 1;
pub const BACKEND_WEBGPU: u8 = 1 << 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParityScene
{
   PrimitiveAtlas,
   GlyphA8,
   GlyphSdf,
   IdMaskAsymmetric,
   NestedClipLayerEffects,
   Scene3dViewportCull,
   ImageCropMinify,
   TransformOpacity,
   PrimitiveAtlasMsaa4x,
   PrimitiveAtlasEdr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParityLayout
{
   Square,
   Wide,
   Portrait,
   MultiDraw,
   ProjectionChanged,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelTolerance
{
   pub differing_pixels: u64,
   pub max_channel_error: u8,
   pub mean_squared_error: f64,
}

impl PixelTolerance
{
   pub const EXACT: Self =
      Self { differing_pixels: 0, max_channel_error: 0, mean_squared_error: 0.0 };
   pub const ANTIALIASED: Self =
      Self { differing_pixels: 16, max_channel_error: 3, mean_squared_error: 0.02 };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParityCase
{
   pub id: &'static str,
   pub scene: ParityScene,
   pub layout: ParityLayout,
   pub width_px: u32,
   pub height_px: u32,
   pub dpr: u8,
   pub backends: u8,
   pub tolerance: PixelTolerance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SequenceKind
{
   FullDirectThenPartialDamage,
   MemoryWarningPurgeThenRebuild,
   Resize,
   DeviceLossThenRecreate,
   AtlasEviction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SequenceCase
{
   pub id: &'static str,
   pub kind: SequenceKind,
   pub frames: u8,
   pub backends: u8,
   pub tolerance: PixelTolerance,
}

const ALL_BACKENDS: u8 = BACKEND_CPU | BACKEND_METAL | BACKEND_WEBGPU;
const CPU_METAL: u8 = BACKEND_CPU | BACKEND_METAL;

pub const PARITY_CASES: &[ParityCase] = &[
   core_case("primitive_atlas", ParityScene::PrimitiveAtlas),
   core_case("glyph_a8", ParityScene::GlyphA8),
   core_case("glyph_sdf", ParityScene::GlyphSdf),
   core_case("nested_clip_layer_effects", ParityScene::NestedClipLayerEffects),
   core_case("scene3d_viewport_cull", ParityScene::Scene3dViewportCull),
   core_case("image_crop_minify", ParityScene::ImageCropMinify),
   core_case("transform_opacity", ParityScene::TransformOpacity),
   capability_case("primitive_atlas_msaa4x", ParityScene::PrimitiveAtlasMsaa4x),
   capability_case("primitive_atlas_edr", ParityScene::PrimitiveAtlasEdr),
   id_mask_case("id_mask_square_dpr1", ParityLayout::Square, 1),
   id_mask_case("id_mask_square_dpr2", ParityLayout::Square, 2),
   id_mask_case("id_mask_square_dpr3", ParityLayout::Square, 3),
   id_mask_case("id_mask_wide_dpr1", ParityLayout::Wide, 1),
   id_mask_case("id_mask_wide_dpr2", ParityLayout::Wide, 2),
   id_mask_case("id_mask_wide_dpr3", ParityLayout::Wide, 3),
   id_mask_case("id_mask_portrait_dpr1", ParityLayout::Portrait, 1),
   id_mask_case("id_mask_portrait_dpr2", ParityLayout::Portrait, 2),
   id_mask_case("id_mask_portrait_dpr3", ParityLayout::Portrait, 3),
   id_mask_case("id_mask_multi_draw_dpr1", ParityLayout::MultiDraw, 1),
   id_mask_case("id_mask_multi_draw_dpr2", ParityLayout::MultiDraw, 2),
   id_mask_case("id_mask_multi_draw_dpr3", ParityLayout::MultiDraw, 3),
   id_mask_case("id_mask_projection_changed_dpr1", ParityLayout::ProjectionChanged, 1),
   id_mask_case("id_mask_projection_changed_dpr2", ParityLayout::ProjectionChanged, 2),
   id_mask_case("id_mask_projection_changed_dpr3", ParityLayout::ProjectionChanged, 3),
];

pub const SEQUENCE_CASES: &[SequenceCase] = &[
   SequenceCase {
      id: "full_direct_then_partial_damage",
      kind: SequenceKind::FullDirectThenPartialDamage,
      frames: 2,
      backends: BACKEND_METAL | BACKEND_WEBGPU,
      tolerance: PixelTolerance::EXACT,
   },
   SequenceCase {
      id: "memory_warning_purge_then_rebuild",
      kind: SequenceKind::MemoryWarningPurgeThenRebuild,
      frames: 3,
      backends: BACKEND_METAL | BACKEND_WEBGPU,
      tolerance: PixelTolerance::EXACT,
   },
   SequenceCase {
      id: "resize",
      kind: SequenceKind::Resize,
      frames: 2,
      backends: ALL_BACKENDS,
      tolerance: PixelTolerance::ANTIALIASED,
   },
   SequenceCase {
      id: "device_loss_then_recreate",
      kind: SequenceKind::DeviceLossThenRecreate,
      frames: 2,
      backends: BACKEND_METAL | BACKEND_WEBGPU,
      tolerance: PixelTolerance::EXACT,
   },
   SequenceCase {
      id: "atlas_eviction",
      kind: SequenceKind::AtlasEviction,
      frames: 3,
      backends: CPU_METAL | BACKEND_WEBGPU,
      tolerance: PixelTolerance::ANTIALIASED,
   },
];

const fn core_case(id: &'static str, scene: ParityScene) -> ParityCase
{
   ParityCase {
      id,
      scene,
      layout: ParityLayout::Square,
      width_px: 192,
      height_px: 192,
      dpr: 1,
      backends: ALL_BACKENDS,
      tolerance: PixelTolerance::ANTIALIASED,
   }
}

const fn capability_case(id: &'static str, scene: ParityScene) -> ParityCase
{
   ParityCase {
      id,
      scene,
      layout: ParityLayout::Square,
      width_px: 192,
      height_px: 192,
      dpr: 1,
      backends: CPU_METAL,
      tolerance: PixelTolerance::ANTIALIASED,
   }
}

const fn id_mask_case(id: &'static str, layout: ParityLayout, dpr: u8) -> ParityCase
{
   let (width_dp, height_dp) = match layout
   {
      ParityLayout::Square => (192, 192),
      ParityLayout::Wide => (320, 192),
      ParityLayout::Portrait => (192, 320),
      ParityLayout::MultiDraw => (256, 192),
      ParityLayout::ProjectionChanged => (256, 192),
   };
   ParityCase {
      id,
      scene: ParityScene::IdMaskAsymmetric,
      layout,
      width_px: width_dp * dpr as u32,
      height_px: height_dp * dpr as u32,
      dpr,
      backends: ALL_BACKENDS,
      tolerance: PixelTolerance::EXACT,
   }
}
