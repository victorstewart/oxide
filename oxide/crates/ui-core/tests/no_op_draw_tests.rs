use oxide_renderer_api as gfx;
use oxide_ui_core::DrawListBuilder;
use proptest::prelude::*;

fn color(alpha: f32) -> gfx::Color
{
   gfx::Color::rgba(0.2, 0.4, 0.8, alpha)
}

fn quad() -> [gfx::Vertex; 4]
{
   [
      gfx::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: u32::MAX },
      gfx::Vertex { x: 8.0, y: 0.0, u: 1.0, v: 0.0, rgba: u32::MAX },
      gfx::Vertex { x: 0.0, y: 8.0, u: 0.0, v: 1.0, rgba: u32::MAX },
      gfx::Vertex { x: 8.0, y: 8.0, u: 1.0, v: 1.0, rgba: u32::MAX },
   ]
}

fn glyph(atlas: gfx::ImageHandle, vb_len: u32, alpha: f32) -> gfx::GlyphRun
{
   gfx::GlyphRun {
      atlas,
      atlas_revision: 1,
      vb: gfx::VertexSpan { offset: 0, len: vb_len },
      ib: gfx::IndexSpan { offset: 0, len: 6 },
      sdf: false,
      color: color(alpha),
   }
}

#[test]
fn builder_discards_intrinsic_noops_from_every_draw_family()
{
   let mut builder = DrawListBuilder::new();
   let rect = gfx::RectF::new(0.0, 0.0, 20.0, 20.0);
   let src = gfx::RectF::new(0.0, 0.0, 1.0, 1.0);
   let transparent = color(0.0);
   builder.solid(
      gfx::VertexSpan { offset: 0, len: 0 },
      gfx::IndexSpan { offset: 0, len: 0 },
      color(1.0),
   );
   builder.image(gfx::ImageHandle(0), rect, src, 1.0);
   builder.image(gfx::ImageHandle(1), rect, src, 0.0);
   builder.image_mesh(gfx::ImageHandle(0), &quad(), &[0, 1, 2, 2, 1, 3], 1.0);
   builder.image_mesh(gfx::ImageHandle(1), &quad(), &[0, 1, 2, 2, 1, 3], 0.0);
   builder.glyph_run(glyph(gfx::ImageHandle(0), 4, 1.0));
   builder.glyph_run(glyph(gfx::ImageHandle(1), 0, 1.0));
   builder.rrect(rect, [2.0; 4], transparent);
   builder.nine_slice(gfx::ImageHandle(0), rect, gfx::Insets::new(1.0, 1.0, 1.0, 1.0), 1.0);
   builder.backdrop(rect, 0.0, transparent, 0.0);
   builder.visual_effect(
      rect,
      gfx::VisualEffect::DarkPopup { blur_intensity: 0.0, tint: transparent },
   );
   builder.camera_bg(rect, color(1.0), 0.0, false, false, 0.0);
   builder.spinner([10.0, 10.0], 0.0, 1.0);

   assert!(builder.drawlist().items.is_empty());
   assert!(builder.drawlist().vertices.is_empty());
   assert!(builder.drawlist().indices.is_empty());
}

#[test]
fn builder_discards_nonfinite_parameters_from_every_draw_family()
{
   let mut builder = DrawListBuilder::new();
   let rect = gfx::RectF::new(0.0, 0.0, 20.0, 20.0);
   let src = gfx::RectF::new(0.0, 0.0, 1.0, 1.0);
   let nan_color = gfx::Color::rgba(f32::NAN, 0.4, 0.8, 1.0);
   builder.solid(
      gfx::VertexSpan { offset: 0, len: 3 },
      gfx::IndexSpan { offset: 0, len: 0 },
      nan_color,
   );
   builder.image(
      gfx::ImageHandle(1),
      gfx::RectF::new(f32::NAN, 0.0, 20.0, 20.0),
      src,
      1.0,
   );
   let mut nan_quad = quad();
   nan_quad[0].u = f32::NAN;
   builder.image_mesh(gfx::ImageHandle(1), &nan_quad, &[0, 1, 2], 1.0);
   builder.glyph_run(gfx::GlyphRun { color: nan_color, ..glyph(gfx::ImageHandle(1), 4, 1.0) });
   builder.rrect(rect, [0.0, f32::NAN, 0.0, 0.0], color(1.0));
   builder.nine_slice(
      gfx::ImageHandle(1),
      rect,
      gfx::Insets::new(1.0, f32::NAN, 1.0, 1.0),
      1.0,
   );
   builder.backdrop(rect, f32::NAN, color(1.0), 1.0);
   builder.visual_effect(
      rect,
      gfx::VisualEffect::DarkPopup { blur_intensity: f32::NAN, tint: color(1.0) },
   );
   builder.camera_bg(rect, color(1.0), 1.0, false, true, f32::NAN);
   builder.spinner([f32::NAN, 10.0], 8.0, 1.0);

   assert!(builder.drawlist().items.is_empty());
   assert!(builder.drawlist().vertices.is_empty());
   assert!(builder.drawlist().indices.is_empty());
}

#[test]
fn builder_discards_zero_and_negative_geometry_from_every_geometric_family()
{
   let mut builder = DrawListBuilder::new();
   let rect = gfx::RectF::new(0.0, 0.0, 20.0, 20.0);
   let src = gfx::RectF::new(0.0, 0.0, 1.0, 1.0);
   let mut flat_quad = quad();
   for vertex in &mut flat_quad
   {
      vertex.y = 4.0;
   }
   builder.image(
      gfx::ImageHandle(1),
      gfx::RectF::new(0.0, 0.0, -20.0, 20.0),
      src,
      1.0,
   );
   builder.image(
      gfx::ImageHandle(1),
      rect,
      gfx::RectF::new(0.0, 0.0, 1.0, 0.0),
      1.0,
   );
   builder.image_mesh(gfx::ImageHandle(1), &flat_quad, &[0, 1, 2], 1.0);
   builder.glyph_run_resolved(
      glyph(gfx::ImageHandle(1), 4, 1.0),
      &flat_quad,
      &[0, 1, 2],
   );
   builder.rrect(
      gfx::RectF::new(0.0, 0.0, 20.0, -20.0),
      [0.0; 4],
      color(1.0),
   );
   builder.nine_slice(
      gfx::ImageHandle(1),
      gfx::RectF::new(0.0, 0.0, 0.0, 20.0),
      gfx::Insets::new(1.0, 1.0, 1.0, 1.0),
      1.0,
   );
   builder.backdrop(gfx::RectF::new(0.0, 0.0, -20.0, 20.0), 2.0, color(1.0), 1.0);
   builder.visual_effect(gfx::RectF::new(0.0, 0.0, 20.0, 0.0), gfx::VisualEffect::UIKitDark);
   builder.camera_bg(
      gfx::RectF::new(0.0, 0.0, -20.0, -20.0),
      color(1.0),
      1.0,
      false,
      false,
      0.0,
   );
   builder.spinner([10.0, 10.0], -1.0, 1.0);

   assert!(builder.drawlist().items.is_empty());
   assert!(builder.drawlist().vertices.is_empty());
   assert!(builder.drawlist().indices.is_empty());
}

#[test]
fn builder_keeps_visible_variants_from_every_draw_family()
{
   let mut builder = DrawListBuilder::new();
   let rect = gfx::RectF::new(0.0, 0.0, 20.0, 20.0);
   let src = gfx::RectF::new(0.0, 0.0, 1.0, 1.0);
   builder.solid(
      gfx::VertexSpan { offset: 0, len: 3 },
      gfx::IndexSpan { offset: 0, len: 0 },
      color(1.0),
   );
   builder.image(gfx::ImageHandle(1), rect, src, 1.0);
   builder.image_mesh(gfx::ImageHandle(1), &quad(), &[0, 1, 2, 2, 1, 3], 1.0);
   builder.glyph_run(glyph(gfx::ImageHandle(1), 4, 1.0));
   builder.rrect(rect, [2.0; 4], color(1.0));
   builder.nine_slice(
      gfx::ImageHandle(1),
      rect,
      gfx::Insets::new(1.0, 1.0, 1.0, 1.0),
      1.0,
   );
   builder.backdrop(rect, 4.0, color(0.0), 0.0);
   builder.visual_effect(
      rect,
      gfx::VisualEffect::DarkPopup { blur_intensity: 0.5, tint: color(0.0) },
   );
   builder.camera_bg(rect, color(1.0), 1.0, false, false, 0.0);
   builder.spinner([10.0, 10.0], 8.0, 1.0);

   assert_eq!(builder.drawlist().items.len(), 10);
}

#[test]
fn empty_effective_clip_discards_only_nested_drawing()
{
   let mut builder = DrawListBuilder::new();
   builder.clip_push(gfx::RectI::new(0, 0, 10, 10));
   builder.clip_push(gfx::RectI::new(20, 20, 5, 5));
   builder.rrect(
      gfx::RectF::new(20.0, 20.0, 5.0, 5.0),
      [0.0; 4],
      color(1.0),
   );
   builder.clip_pop();
   builder.rrect(gfx::RectF::new(1.0, 1.0, 5.0, 5.0), [0.0; 4], color(1.0));
   builder.clip_pop();

   assert_eq!(builder.drawlist().items.len(), 5);
   assert!(matches!(builder.drawlist().items[3], gfx::DrawCmd::RRect { .. }));
}

#[test]
fn layer_and_clip_structure_survives_filtered_contents()
{
   let mut builder = DrawListBuilder::new();
   builder.layer_begin(7, gfx::RectF::new(0.0, 0.0, 40.0, 40.0), true);
   builder.clip_push(gfx::RectI::new(0, 0, 0, 40));
   builder.rrect(
      gfx::RectF::new(0.0, 0.0, 20.0, 20.0),
      [0.0; 4],
      color(1.0),
   );
   builder.clip_pop();
   builder.backdrop(gfx::RectF::new(0.0, 0.0, 20.0, 20.0), 0.0, color(0.0), 0.0);
   builder.layer_end();

   assert_eq!(builder.drawlist().items.len(), 4);
   assert!(matches!(builder.drawlist().items[0], gfx::DrawCmd::LayerBegin { .. }));
   assert!(matches!(builder.drawlist().items[1], gfx::DrawCmd::ClipPush { .. }));
   assert!(matches!(builder.drawlist().items[2], gfx::DrawCmd::ClipPop));
   assert!(matches!(builder.drawlist().items[3], gfx::DrawCmd::LayerEnd));
}

#[test]
fn invalid_mesh_and_resolved_glyph_do_not_append_backing_geometry()
{
   let mut invalid_quad = quad();
   invalid_quad[2].x = f32::NAN;
   let mut builder = DrawListBuilder::new();
   builder.image_mesh(gfx::ImageHandle(1), &invalid_quad, &[0, 1, 2], 1.0);
   builder.glyph_run_resolved(
      glyph(gfx::ImageHandle(1), 4, 0.0),
      &quad(),
      &[0, 1, 2, 2, 1, 3],
   );
   assert!(builder.drawlist().items.is_empty());
   assert!(builder.drawlist().vertices.is_empty());
   assert!(builder.drawlist().indices.is_empty());
}

#[test]
fn appended_noops_are_filtered_before_span_validation()
{
   let mut source = gfx::DrawList::default();
   source.items.push(gfx::DrawCmd::Solid {
      vb: gfx::VertexSpan { offset: u32::MAX, len: 0 },
      ib: gfx::IndexSpan { offset: u32::MAX, len: 0 },
      color: color(1.0),
   });
   source.items.push(gfx::DrawCmd::RRect {
      rect: gfx::RectF::new(0.0, 0.0, 0.0, 10.0),
      radii: [0.0; 4],
      color: color(1.0),
   });
   let mut builder = DrawListBuilder::new();
   assert!(builder.append_drawlist(&source));
   assert!(builder.drawlist().items.is_empty());
}

proptest!
{
   #[test]
   fn rrect_emission_matches_finite_positive_geometry_and_alpha(
      x in any::<f32>(),
      y in any::<f32>(),
      w in any::<f32>(),
      h in any::<f32>(),
      alpha in any::<f32>(),
   )
   {
      let mut builder = DrawListBuilder::new();
      builder.rrect(gfx::RectF::new(x, y, w, h), [0.0; 4], color(alpha));
      let expected = x.is_finite()
         && y.is_finite()
         && w.is_finite()
         && h.is_finite()
         && w > 0.0
         && h > 0.0
         && alpha.is_finite()
         && alpha > 0.0;
      prop_assert_eq!(builder.drawlist().items.len(), usize::from(expected));
   }
}
