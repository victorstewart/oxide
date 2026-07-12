use oxide_snapshot_runner::parity::{
   ParityLayout, ParityScene, SequenceKind, BACKEND_CPU, BACKEND_METAL, BACKEND_WEBGPU,
   PARITY_CASES, SEQUENCE_CASES,
};
use std::collections::HashSet;

#[test]
fn parity_manifest_covers_every_required_scene_family()
{
   for scene in [
      ParityScene::PrimitiveAtlas,
      ParityScene::GlyphA8,
      ParityScene::GlyphSdf,
      ParityScene::IdMaskAsymmetric,
      ParityScene::NestedClipLayerEffects,
      ParityScene::Scene3dViewportCull,
      ParityScene::ImageCropMinify,
      ParityScene::TransformOpacity,
      ParityScene::PrimitiveAtlasMsaa4x,
      ParityScene::PrimitiveAtlasEdr,
   ]
   {
      assert!(PARITY_CASES.iter().any(|case| case.scene == scene), "missing {scene:?}");
   }
   assert!(PARITY_CASES.iter().all(|case| case.tolerance.max_channel_error <= 3));
   assert!(PARITY_CASES.iter().all(|case| case.tolerance.mean_squared_error <= 0.02));
}

#[test]
fn id_mask_cross_backend_matrix_is_five_layouts_by_three_dprs()
{
   let id_mask = PARITY_CASES
      .iter()
      .filter(|case| case.scene == ParityScene::IdMaskAsymmetric)
      .collect::<Vec<_>>();
   assert_eq!(id_mask.len(), 15);
   for layout in [
      ParityLayout::Square,
      ParityLayout::Wide,
      ParityLayout::Portrait,
      ParityLayout::MultiDraw,
      ParityLayout::ProjectionChanged,
   ]
   {
      for dpr in 1..=3
      {
         let case = id_mask
            .iter()
            .find(|case| case.layout == layout && case.dpr == dpr)
            .unwrap_or_else(|| panic!("missing {layout:?} DPR {dpr}"));
         assert_eq!(case.backends, BACKEND_CPU | BACKEND_METAL | BACKEND_WEBGPU);
         assert_eq!(case.width_px % u32::from(dpr), 0);
         assert_eq!(case.height_px % u32::from(dpr), 0);
      }
   }
   assert_eq!(id_mask.iter().map(|case| case.id).collect::<HashSet<_>>().len(), 15);
}

#[test]
fn sequence_manifest_freezes_every_required_transition()
{
   assert_eq!(SEQUENCE_CASES.len(), 5);
   for kind in [
      SequenceKind::FullDirectThenPartialDamage,
      SequenceKind::MemoryWarningPurgeThenRebuild,
      SequenceKind::Resize,
      SequenceKind::DeviceLossThenRecreate,
      SequenceKind::AtlasEviction,
   ]
   {
      let case = SEQUENCE_CASES
         .iter()
         .find(|case| case.kind == kind)
         .unwrap_or_else(|| panic!("missing {kind:?}"));
      assert!(case.frames >= 2);
      assert_ne!(case.backends & BACKEND_METAL, 0);
      assert!(case.tolerance.max_channel_error <= 3);
   }
}
