use oxide_renderer_api::{
    self as gfx, Color, DrawCmd, DrawList, GlyphRun, ImageHandle, IndexSpan, Insets, RectF, RectI,
    Vertex, VertexSpan, VisualEffect,
};
use oxide_ui_core::draw_replay::{replay_drawlist, replay_render_chunk};

#[derive(Default)]
struct RecordingEncoder {
    clips: Vec<RectI>,
    solids: Vec<Vec<Vertex>>,
    images: Vec<(ImageHandle, RectF, RectF)>,
    image_meshes: Vec<(ImageHandle, Vec<Vertex>, Vec<u16>, f32)>,
    rrects: Vec<(RectF, [f32; 4], Color)>,
    nine_slices: Vec<(ImageHandle, RectF, Insets, f32)>,
    backdrops: Vec<(RectF, f32, Color, f32)>,
    visual_effects: Vec<(RectF, VisualEffect)>,
    camera_bgs: Vec<(RectF, Color, f32, bool, bool, f32)>,
    spinners: Vec<([f32; 2], f32, f32)>,
    glyph_runs: usize,
    resolved_glyph_runs: Vec<(Vec<Vertex>, Vec<u16>)>,
}

impl gfx::RenderEncoder for RecordingEncoder {
    fn set_viewport(&mut self, _vp: RectF) {}

    fn set_clip(&mut self, scissor: RectI) {
        self.clips.push(scissor);
    }

    fn draw_solid(&mut self, verts: &[Vertex], _color: Color) {
        self.solids.push(verts.to_vec());
    }

    fn draw_image(&mut self, img: ImageHandle, dst: RectF, src: RectF) {
        self.images.push((img, dst, src));
    }

    fn draw_image_mesh(
        &mut self,
        img: ImageHandle,
        vertices: &[Vertex],
        indices: &[u16],
        alpha: f32,
    ) {
        self.image_meshes.push((img, vertices.to_vec(), indices.to_vec(), alpha));
    }

    fn draw_rrect(&mut self, rect: RectF, radii: [f32; 4], color: Color) {
        self.rrects.push((rect, radii, color));
    }

    fn draw_nine_slice(&mut self, img: ImageHandle, rect: RectF, slice: Insets, alpha: f32) {
        self.nine_slices.push((img, rect, slice, alpha));
    }

    fn draw_backdrop(&mut self, rect: RectF, sigma: f32, tint: Color, alpha: f32) {
        self.backdrops.push((rect, sigma, tint, alpha));
    }

    fn draw_visual_effect(&mut self, rect: RectF, effect: VisualEffect) {
        self.visual_effects.push((rect, effect));
    }

    fn draw_camera_bg(
        &mut self,
        rect: RectF,
        tint: Color,
        alpha: f32,
        grayscale: bool,
        blur: bool,
        sigma: f32,
    ) {
        self.camera_bgs.push((rect, tint, alpha, grayscale, blur, sigma));
    }

    fn draw_spinner(&mut self, center: [f32; 2], atom: f32, alpha: f32) {
        self.spinners.push((center, atom, alpha));
    }

    fn draw_glyph_run(&mut self, _run: &GlyphRun) {
        self.glyph_runs = self.glyph_runs.saturating_add(1);
    }

    fn draw_glyph_run_resolved(&mut self, _run: &GlyphRun, vertices: &[Vertex], indices: &[u16]) {
        self.resolved_glyph_runs.push((vertices.to_vec(), indices.to_vec()));
    }
}

fn build_test_drawlist() -> DrawList {
    let mut list = DrawList::default();
    list.vertices.extend_from_slice(&[
        Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: u32::MAX },
        Vertex { x: 2.0, y: 0.0, u: 1.0, v: 0.0, rgba: u32::MAX },
        Vertex { x: 0.0, y: 2.0, u: 0.0, v: 1.0, rgba: u32::MAX },
        Vertex { x: 2.0, y: 2.0, u: 1.0, v: 1.0, rgba: u32::MAX },
    ]);
    list.indices.extend_from_slice(&[0, 1, 2, 2, 1, 3]);
    list.items.push(DrawCmd::ClipPush { rect: RectI::new(1, 2, 30, 40) });
    list.items.push(DrawCmd::Solid {
        vb: VertexSpan { offset: 0, len: 4 },
        ib: IndexSpan { offset: 0, len: 6 },
        color: Color::rgba(1.0, 0.0, 0.0, 1.0),
    });
    list.items.push(DrawCmd::Image {
        tex: ImageHandle(7),
        dst: RectF::new(3.0, 4.0, 5.0, 6.0),
        src: RectF::new(0.0, 0.0, 1.0, 1.0),
        alpha: 0.75,
    });
    list.items.push(DrawCmd::ImageMesh {
        tex: ImageHandle(11),
        vb: VertexSpan { offset: 0, len: 4 },
        ib: IndexSpan { offset: 0, len: 6 },
        alpha: 0.5,
    });
    list.items.push(DrawCmd::RRect {
        rect: RectF::new(10.0, 11.0, 12.0, 13.0),
        radii: [1.0, 2.0, 3.0, 4.0],
        color: Color::rgba(0.2, 0.3, 0.4, 0.5),
    });
    list.items.push(DrawCmd::NineSlice {
        tex: ImageHandle(8),
        rect: RectF::new(20.0, 21.0, 22.0, 23.0),
        slice: Insets::new(1.0, 2.0, 3.0, 4.0),
        alpha: 0.25,
    });
    list.items.push(DrawCmd::Backdrop {
        rect: RectF::new(30.0, 31.0, 32.0, 33.0),
        sigma: 5.0,
        tint: Color::rgba(0.1, 0.2, 0.3, 0.4),
        alpha: 0.8,
    });
    list.items.push(DrawCmd::VisualEffect {
        rect: RectF::new(34.0, 35.0, 36.0, 37.0),
        effect: VisualEffect::DarkPopup {
            blur_intensity: 0.5,
            tint: Color::rgba(1.0, 1.0, 1.0, 0.9),
        },
    });
    list.items.push(DrawCmd::Spinner { center: [40.0, 41.0], atom: 18.0, alpha: 0.6 });
    list.items.push(DrawCmd::CameraBg {
        rect: RectF::new(50.0, 51.0, 52.0, 53.0),
        tint: Color::rgba(0.6, 0.5, 0.4, 0.3),
        alpha: 0.2,
        grayscale: true,
        blur: true,
        sigma: 4.0,
    });
    list.items.push(DrawCmd::GlyphRun {
        run: GlyphRun {
            atlas: ImageHandle(9),
            atlas_revision: 0,
            vb: VertexSpan { offset: 0, len: 4 },
            ib: IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: Color::rgba(0.9, 0.8, 0.7, 1.0),
        },
    });
    list.items.push(DrawCmd::ClipPop);
    list
}

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.001
}

#[test]
fn replay_translates_primitives_and_restores_fallback_clip() {
    let list = build_test_drawlist();
    let fallback = RectI::new(0, 0, 100, 100);
    let mut encoder = RecordingEncoder::default();
    replay_drawlist(&list, &mut encoder, fallback, [5.0, -3.0]);

    assert_eq!(encoder.clips.len(), 3);
    assert_eq!(encoder.clips[0], RectI::new(5, -3, 100, 100));
    assert_eq!(encoder.clips[1], RectI::new(6, -1, 30, 40));
    assert_eq!(encoder.clips[2], RectI::new(5, -3, 100, 100));

    assert_eq!(encoder.solids.len(), 1);
    let solid = &encoder.solids[0];
    assert_eq!(solid.len(), 4);
    assert!(approx_eq(solid[0].x, 5.0));
    assert!(approx_eq(solid[0].y, -3.0));
    assert!(approx_eq(solid[3].x, 7.0));
    assert!(approx_eq(solid[3].y, -1.0));

    assert_eq!(encoder.images.len(), 1);
    let (_, image_dst, _) = encoder.images[0];
    assert!(approx_eq(image_dst.x, 8.0));
    assert!(approx_eq(image_dst.y, 1.0));

    assert_eq!(encoder.image_meshes.len(), 1);
    let (mesh_tex, mesh_vertices, mesh_indices, mesh_alpha) = &encoder.image_meshes[0];
    assert_eq!(*mesh_tex, ImageHandle(11));
    assert_eq!(mesh_vertices.len(), 4);
    assert_eq!(mesh_indices, &[0, 1, 2, 2, 1, 3]);
    assert!(approx_eq(*mesh_alpha, 0.5));
    assert!(approx_eq(mesh_vertices[0].x, 5.0));
    assert!(approx_eq(mesh_vertices[0].y, -3.0));
    assert!(approx_eq(mesh_vertices[3].x, 7.0));
    assert!(approx_eq(mesh_vertices[3].y, -1.0));

    assert_eq!(encoder.rrects.len(), 1);
    let (rrect_rect, _, _) = encoder.rrects[0];
    assert!(approx_eq(rrect_rect.x, 15.0));
    assert!(approx_eq(rrect_rect.y, 8.0));

    assert_eq!(encoder.nine_slices.len(), 1);
    let (_, nine_slice_rect, _, _) = encoder.nine_slices[0];
    assert!(approx_eq(nine_slice_rect.x, 25.0));
    assert!(approx_eq(nine_slice_rect.y, 18.0));

    assert_eq!(encoder.backdrops.len(), 1);
    let (first_backdrop_rect, _, _, _) = encoder.backdrops[0];
    assert!(approx_eq(first_backdrop_rect.x, 35.0));
    assert!(approx_eq(first_backdrop_rect.y, 28.0));

    assert_eq!(encoder.visual_effects.len(), 1);
    let (visual_effect_rect, visual_effect) = encoder.visual_effects[0];
    assert!(approx_eq(visual_effect_rect.x, 39.0));
    assert!(approx_eq(visual_effect_rect.y, 32.0));
    assert!(matches!(
        visual_effect,
        VisualEffect::DarkPopup {
            blur_intensity,
            tint,
        } if approx_eq(blur_intensity, 0.5)
            && tint == Color::rgba(1.0, 1.0, 1.0, 0.9)
    ));

    assert_eq!(encoder.camera_bgs.len(), 1);
    let (camera_rect, _, _, grayscale, blur, sigma) = encoder.camera_bgs[0];
    assert!(approx_eq(camera_rect.x, 55.0));
    assert!(approx_eq(camera_rect.y, 48.0));
    assert!(grayscale);
    assert!(blur);
    assert!(approx_eq(sigma, 4.0));

    assert_eq!(encoder.spinners.len(), 1);
    let (center, atom, alpha) = encoder.spinners[0];
    assert!(approx_eq(center[0], 45.0));
    assert!(approx_eq(center[1], 38.0));
    assert!(approx_eq(atom, 18.0));
    assert!(approx_eq(alpha, 0.6));

    assert_eq!(encoder.glyph_runs, 0);
    assert_eq!(encoder.resolved_glyph_runs.len(), 1);
    let (glyph_vertices, glyph_indices) = &encoder.resolved_glyph_runs[0];
    assert_eq!(glyph_vertices.len(), 4);
    assert_eq!(glyph_indices, &[0, 1, 2, 2, 1, 3]);
    assert!(approx_eq(glyph_vertices[0].x, 5.0));
    assert!(approx_eq(glyph_vertices[0].y, -3.0));
    assert!(approx_eq(glyph_vertices[3].x, 7.0));
    assert!(approx_eq(glyph_vertices[3].y, -1.0));
}

#[test]
fn replay_skips_invalid_solid_vertex_span() {
    let mut list = DrawList::default();
    list.items.push(DrawCmd::Solid {
        vb: VertexSpan { offset: 999, len: 4 },
        ib: IndexSpan { offset: 0, len: 6 },
        color: Color::rgba(1.0, 1.0, 1.0, 1.0),
    });
    let mut encoder = RecordingEncoder::default();
    replay_drawlist(&list, &mut encoder, RectI::new(0, 0, 10, 10), [0.0, 0.0]);

    assert!(encoder.solids.is_empty());
    assert_eq!(encoder.clips, vec![RectI::new(0, 0, 10, 10)]);
}

#[test]
fn retained_chunk_replay_uses_canonical_local_indices()
{
   let mut list = DrawList::default();
   list.vertices.extend_from_slice(&[
      Vertex { x: -2.0, y: -2.0, u: 0.0, v: 0.0, rgba: 0 },
      Vertex { x: -1.0, y: -2.0, u: 0.0, v: 0.0, rgba: 0 },
      Vertex { x: -1.0, y: -1.0, u: 0.0, v: 0.0, rgba: 0 },
      Vertex { x: -2.0, y: -1.0, u: 0.0, v: 0.0, rgba: 0 },
      Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
      Vertex { x: 2.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
      Vertex { x: 2.0, y: 2.0, u: 1.0, v: 1.0, rgba: 0 },
      Vertex { x: 0.0, y: 2.0, u: 0.0, v: 1.0, rgba: 0 },
   ]);
   list.indices.extend_from_slice(&[4, 5, 6, 4, 6, 7]);
   list.items.push(DrawCmd::ImageMesh {
      tex: ImageHandle(44),
      vb: VertexSpan { offset: 4, len: 4 },
      ib: IndexSpan { offset: 0, len: 6 },
      alpha: 1.0,
   });
   let chunk = gfx::RenderChunk::new(
      gfx::RenderChunkId(1),
      gfx::RenderChunkRevisions::default(),
      list,
      gfx::ChunkIndexMode::Absolute,
      &[gfx::RenderResourceDependency { image: ImageHandle(44), generation: 9 }],
   ).unwrap();
   assert_eq!(chunk.draw_list().indices, [0, 1, 2, 0, 2, 3]);

   let mut encoder = RecordingEncoder::default();
   replay_render_chunk(&chunk, &mut encoder, RectI::new(0, 0, 20, 20), [3.0, 4.0]);
   assert_eq!(encoder.image_meshes.len(), 1);
   assert_eq!(encoder.image_meshes[0].2, [0, 1, 2, 0, 2, 3]);
   assert_eq!(encoder.image_meshes[0].1[0].x, 3.0);
   assert_eq!(encoder.image_meshes[0].1[0].y, 4.0);
}

#[test]
fn replay_rebases_absolute_image_mesh_indices_to_translated_span() {
    let mut list = DrawList::default();
    list.vertices.extend_from_slice(&[
        Vertex { x: -100.0, y: -100.0, u: 0.0, v: 0.0, rgba: u32::MAX },
        Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: u32::MAX },
        Vertex { x: 4.0, y: 0.0, u: 1.0, v: 0.0, rgba: u32::MAX },
        Vertex { x: 0.0, y: 4.0, u: 0.0, v: 1.0, rgba: u32::MAX },
        Vertex { x: 4.0, y: 4.0, u: 1.0, v: 1.0, rgba: u32::MAX },
    ]);
    list.indices.extend_from_slice(&[1, 2, 3, 3, 2, 4]);
    list.items.push(DrawCmd::ImageMesh {
        tex: ImageHandle(22),
        vb: VertexSpan { offset: 1, len: 4 },
        ib: IndexSpan { offset: 0, len: 6 },
        alpha: 0.65,
    });

    let mut encoder = RecordingEncoder::default();
    replay_drawlist(&list, &mut encoder, RectI::new(0, 0, 20, 20), [6.0, 7.0]);

    assert_eq!(encoder.image_meshes.len(), 1);
    let (tex, vertices, indices, alpha) = &encoder.image_meshes[0];
    assert_eq!(*tex, ImageHandle(22));
    assert_eq!(indices, &[0, 1, 2, 2, 1, 3]);
    assert!(approx_eq(*alpha, 0.65));
    assert_eq!(vertices.len(), 4);
    assert!(approx_eq(vertices[0].x, 6.0));
    assert!(approx_eq(vertices[0].y, 7.0));
    assert!(approx_eq(vertices[3].x, 10.0));
    assert!(approx_eq(vertices[3].y, 11.0));
}

#[test]
fn replay_recovers_from_unbalanced_clip_stack() {
    let mut list = DrawList::default();
    list.items.push(DrawCmd::ClipPush { rect: RectI::new(2, 3, 4, 5) });
    let mut encoder = RecordingEncoder::default();
    replay_drawlist(&list, &mut encoder, RectI::new(10, 20, 30, 40), [1.0, 2.0]);

    assert_eq!(encoder.clips.len(), 3);
    assert_eq!(encoder.clips[0], RectI::new(11, 22, 30, 40));
    assert_eq!(encoder.clips[1], RectI::new(3, 5, 4, 5));
    assert_eq!(encoder.clips[2], RectI::new(11, 22, 30, 40));
}
