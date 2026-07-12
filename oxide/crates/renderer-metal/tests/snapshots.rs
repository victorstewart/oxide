#![cfg(all(
    feature = "snapshot-tests",
    any(target_os = "macos", all(target_os = "ios", not(target_abi = "sim")))
))]

use oxide_renderer_api::{self as api, Renderer};
use oxide_renderer_metal::scene3d::{self, Instance3d, Mesh3dData, Pass3d, Vertex3d};
use oxide_renderer_metal::{CameraRenderMode, CameraTextureSource, MetalRenderer};

fn approx_eq(a: u8, b: u8, tol: u8) -> bool {
    let d = a.abs_diff(b);
    d <= tol
}

fn mat4_identity() -> scene3d::Mat4 {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

#[test]
fn snapshot_rrect_basic() {
    // Arrange
    let mut r = MetalRenderer::new_default().expect("metal");
    let w = 128u32;
    let h = 64u32;
    let scale = 1.0f32;
    r.resize(w, h, scale).unwrap();

    let mut list = api::DrawList::default();
    let rect = api::RectF::new(16.0, 12.0, 96.0, 40.0);
    let radii = [8.0, 8.0, 8.0, 8.0];
    let color = api::Color::rgba(1.0, 0.0, 0.0, 1.0); // pure red
    list.items.push(api::DrawCmd::RRect { rect, radii, color });

    // Act
    let fb = &api::FrameTarget;
    let token = r.begin_frame(fb, None);
    r.encode_pass(&list);
    r.submit(token).unwrap();
    let (rw, rh, bgra) = r.readback_bgra8().expect("readback");
    assert_eq!((rw, rh), (w, h));

    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let idx = ((y * w + x) * 4) as usize;
        [bgra[idx], bgra[idx + 1], bgra[idx + 2], bgra[idx + 3]]
    };

    let center = pixel((rect.x + rect.w * 0.5) as u32, (rect.y + rect.h * 0.5) as u32);
    assert!(
        center[2] > 220 && center[0] < 30 && center[1] < 30,
        "center pixel not red: {center:?}"
    );
    assert!(center[3] > 240, "center alpha too low: {}", center[3]);

    let top_left = pixel(2, 2);
    assert!(approx_eq(top_left[0], 0, 8));
    assert!(approx_eq(top_left[1], 0, 8));
    assert!(approx_eq(top_left[2], 0, 8));
    assert!(approx_eq(top_left[3], 255, 0));

    let mut red_pixels = 0usize;
    let mut soft_edge_found = false;
    for px in bgra.chunks_exact(4) {
        let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
        if r > 200 && g < 80 && b < 80 {
            red_pixels += 1;
        }
        if a > 0 && a < 255 {
            soft_edge_found = true;
        }
    }
    assert!(soft_edge_found, "expected antialiased edge pixels");
    assert!(red_pixels > 2800 && red_pixels < 4500, "unexpected red area: {red_pixels}");
}

#[test]
fn snapshot_rrect_instanced_batch_draws_consecutive_rects() {
    let mut renderer = MetalRenderer::new_default().expect("metal");
    let width = 128u32;
    let height = 96u32;
    renderer.resize(width, height, 1.0).expect("resize");

    let mut list = api::DrawList::default();
    list.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(10.0, 10.0, 28.0, 28.0),
        radii: [6.0; 4],
        color: api::Color::rgba(1.0, 0.0, 0.0, 1.0),
    });
    list.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(50.0, 24.0, 28.0, 28.0),
        radii: [6.0; 4],
        color: api::Color::rgba(0.0, 1.0, 0.0, 1.0),
    });
    list.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(90.0, 58.0, 28.0, 28.0),
        radii: [6.0; 4],
        color: api::Color::rgba(0.0, 0.0, 1.0, 1.0),
    });

    let token = renderer.begin_frame(&api::FrameTarget, None);
    renderer.encode_pass(&list);
    renderer.submit(token).expect("submit");
    let (_rw, _rh, bgra) = renderer.readback_bgra8().expect("readback");

    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let idx = ((y * width + x) * 4) as usize;
        [bgra[idx], bgra[idx + 1], bgra[idx + 2], bgra[idx + 3]]
    };

    let red = pixel(24, 24);
    assert!(red[2] > 220 && red[1] < 40 && red[0] < 40, "expected first instance red, got {red:?}");
    let green = pixel(64, 38);
    assert!(
        green[1] > 220 && green[2] < 40 && green[0] < 40,
        "expected second instance green, got {green:?}"
    );
    let blue = pixel(104, 72);
    assert!(
        blue[0] > 220 && blue[1] < 40 && blue[2] < 40,
        "expected third instance blue, got {blue:?}"
    );
}

#[test]
fn snapshot_clip_push_pop_scopes_draws() {
    let mut renderer = MetalRenderer::new_default().expect("metal");
    let width = 128u32;
    let height = 96u32;
    renderer.resize(width, height, 1.0).expect("resize");

    let mut list = api::DrawList::default();
    list.items.push(api::DrawCmd::ClipPush { rect: api::RectI::new(0, 0, 64, height as i32) });
    list.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(20.0, 36.0, 24.0, 24.0),
        radii: [6.0; 4],
        color: api::Color::rgba(0.0, 0.0, 1.0, 1.0),
    });
    list.items.push(api::DrawCmd::ClipPop);
    list.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(80.0, 36.0, 30.0, 24.0),
        radii: [6.0; 4],
        color: api::Color::rgba(0.0, 1.0, 0.0, 1.0),
    });

    let fb = &api::FrameTarget;
    let token = renderer.begin_frame(fb, None);
    renderer.encode_pass(&list);
    renderer.submit(token).expect("submit");
    let (rw, rh, bgra) = renderer.readback_bgra8().expect("readback");
    assert_eq!((rw, rh), (width, height));

    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let idx = ((y * width + x) * 4) as usize;
        [bgra[idx], bgra[idx + 1], bgra[idx + 2], bgra[idx + 3]]
    };

    let blue_center = pixel(32, 48);
    assert!(
        blue_center[0] > 180 && blue_center[1] < 80 && blue_center[2] < 80,
        "expected blue pixel inside clipped-left rect, got {blue_center:?}"
    );

    let rect_center = pixel(94, 48);
    assert!(
        rect_center[1] > 180 && rect_center[2] < 80 && rect_center[0] < 80,
        "expected green pixel at unclipped rect center, got {rect_center:?}"
    );
    assert!(rect_center[3] > 220, "expected opaque alpha, got {}", rect_center[3]);

    let left_side = pixel(64, 48);
    assert!(
        approx_eq(left_side[0], 0, 10)
            && approx_eq(left_side[1], 0, 10)
            && approx_eq(left_side[2], 0, 10),
        "expected black default clear on untouched area, got {left_side:?}"
    );
}

#[test]
fn snapshot_solid_rejects_non_triangle_index_counts() {
    let mut renderer = MetalRenderer::new_default().expect("metal");
    let width = 96u32;
    let height = 96u32;
    renderer.resize(width, height, 1.0).expect("resize");

    let mut list = api::DrawList::default();
    list.vertices.extend_from_slice(&[
        api::Vertex { x: 8.0, y: 8.0, u: 0.0, v: 0.0, rgba: u32::MAX },
        api::Vertex { x: 88.0, y: 8.0, u: 1.0, v: 0.0, rgba: u32::MAX },
        api::Vertex { x: 8.0, y: 88.0, u: 0.0, v: 1.0, rgba: u32::MAX },
        api::Vertex { x: 88.0, y: 88.0, u: 1.0, v: 1.0, rgba: u32::MAX },
    ]);
    list.indices.extend_from_slice(&[0, 1, 2, 3]);
    list.items.push(api::DrawCmd::Solid {
        vb: api::VertexSpan { offset: 0, len: 4 },
        ib: api::IndexSpan { offset: 0, len: 4 },
        color: api::Color::rgba(1.0, 0.0, 0.0, 1.0),
    });

    let token = renderer.begin_frame(&api::FrameTarget, None);
    renderer.encode_pass(&list);
    renderer.submit(token).expect("submit");
    let (_rw, _rh, bgra) = renderer.readback_bgra8().expect("readback");

    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let idx = ((y * width + x) * 4) as usize;
        [bgra[idx], bgra[idx + 1], bgra[idx + 2], bgra[idx + 3]]
    };

    for (x, y) in [(20_u32, 20_u32), (48, 48), (80, 80), (80, 20), (20, 80)] {
        let p = pixel(x, y);
        assert!(
            approx_eq(p[0], 0, 10) && approx_eq(p[1], 0, 10) && approx_eq(p[2], 0, 10),
            "expected untouched black default clear at ({x},{y}), got {p:?}"
        );
    }
}

#[test]
fn snapshot_solid_vertex_color_interpolates_and_zero_inherits_uniform()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   let width = 96_u32;
   let height = 64_u32;
   renderer.resize(width, height, 1.0).expect("resize");

   let red = api::Color::rgba(1.0, 0.0, 0.0, 1.0).pack_rgba8();
   let blue = api::Color::rgba(0.0, 0.0, 1.0, 1.0).pack_rgba8();
   let vertex = |x, y, rgba| api::Vertex { x, y, u: 0.0, v: 0.0, rgba };
   let mut list = api::DrawList::default();
   list.vertices.extend_from_slice(&[
      vertex(8.0, 8.0, red),
      vertex(88.0, 8.0, blue),
      vertex(8.0, 28.0, red),
      vertex(8.0, 28.0, red),
      vertex(88.0, 8.0, blue),
      vertex(88.0, 28.0, blue),
      vertex(8.0, 36.0, 0),
      vertex(88.0, 36.0, 0),
      vertex(8.0, 56.0, 0),
      vertex(8.0, 56.0, 0),
      vertex(88.0, 36.0, 0),
      vertex(88.0, 56.0, 0),
   ]);
   list.items.extend_from_slice(&[
      api::DrawCmd::Solid {
         vb: api::VertexSpan { offset: 0, len: 6 },
         ib: api::IndexSpan { offset: 0, len: 0 },
         color: api::Color::rgba(0.0, 1.0, 0.0, 1.0),
      },
      api::DrawCmd::Solid {
         vb: api::VertexSpan { offset: 6, len: 6 },
         ib: api::IndexSpan { offset: 0, len: 0 },
         color: api::Color::rgba(0.0, 1.0, 0.0, 1.0),
      },
   ]);

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(&list);
   renderer.submit(token).expect("submit");
   let (_, _, bgra) = renderer.readback_bgra8().expect("readback");
   let pixel = |x: u32, y: u32| -> [u8; 4] {
      let index = ((y * width + x) * 4) as usize;
      [bgra[index], bgra[index + 1], bgra[index + 2], bgra[index + 3]]
   };

   let left = pixel(8, 18);
   assert!(left[2] > 240 && left[0] < 40 && left[1] < 20, "red endpoint: {left:?}");
   let middle = pixel(48, 18);
   assert!(middle[2] > 100 && middle[0] > 100 && middle[1] < 20, "interpolation: {middle:?}");
   let right = pixel(87, 18);
   assert!(right[0] > 240 && right[2] < 40 && right[1] < 20, "blue endpoint: {right:?}");
   let inherited = pixel(48, 46);
   assert_eq!(inherited, [0, 255, 0, 255], "zero rgba uniform byte identity");
}

#[test]
fn snapshot_scene3d_mixes_with_2d_overlay() {
    let mut renderer = MetalRenderer::new_default().expect("metal");
    let width = 128u32;
    let height = 128u32;
    renderer.resize(width, height, 1.0).expect("resize");

    let fill_vertices = [
        Vertex3d { position: [-0.70, -0.55, 0.10] },
        Vertex3d { position: [0.10, -0.60, 0.10] },
        Vertex3d { position: [-0.45, 0.15, 0.10] },
    ];
    let fill_indices = [0_u32, 1, 2];
    let fill = renderer
        .mesh3d_create(&Mesh3dData {
            vertices: &fill_vertices,
            indices: &fill_indices,
            topology: scene3d::MeshTopology::Triangles,
        })
        .expect("create fill mesh");

    let line_vertices = [
        Vertex3d { position: [-0.85, 0.0, 0.0] },
        Vertex3d { position: [0.85, 0.0, 0.0] },
        Vertex3d { position: [0.0, -0.85, 0.0] },
        Vertex3d { position: [0.0, 0.85, 0.0] },
    ];
    let line_indices = [0_u32, 1, 2, 3];
    let lines = renderer
        .mesh3d_create(&Mesh3dData {
            vertices: &line_vertices,
            indices: &line_indices,
            topology: scene3d::MeshTopology::Lines,
        })
        .expect("create line mesh");

    let mut line_instance =
        Instance3d::new(lines, mat4_identity(), api::Color::rgba(0.98, 0.30, 0.46, 1.0));
    line_instance.cull = scene3d::CullMode3d::None;
    line_instance.depth_write = false;
    let instances = [
        Instance3d::new(fill, mat4_identity(), api::Color::rgba(0.18, 0.72, 1.0, 1.0)),
        line_instance,
    ];
    let scene = Pass3d {
        viewport: None,
        clear_color: Some(api::Color::rgba(0.08, 0.09, 0.13, 1.0)),
        clear_depth: true,
        view_proj: mat4_identity(),
        instances: &instances,
        bloom: None,
    };

    let mut overlay = api::DrawList::default();
    overlay.items.push(api::DrawCmd::RRect {
        rect: api::RectF::new(10.0, 10.0, 28.0, 18.0),
        radii: [4.0; 4],
        color: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
    });

    let token = renderer.begin_frame(&api::FrameTarget, None);
    renderer.encode_scene3d(&scene).expect("encode scene3d");
    renderer.encode_pass(&overlay);
    renderer.submit(token).expect("submit");
    let (_rw, _rh, bgra) = renderer.readback_bgra8().expect("readback");

    let pixel = |x: u32, y: u32| -> [u8; 4] {
        let idx = ((y * width + x) * 4) as usize;
        [bgra[idx], bgra[idx + 1], bgra[idx + 2], bgra[idx + 3]]
    };

    let overlay_px = pixel(20, 18);
    assert!(
        overlay_px[0] > 235 && overlay_px[1] > 235 && overlay_px[2] > 235,
        "expected 2D overlay to remain visible over scene3d, got {overlay_px:?}"
    );

    let fill_px = pixel(38, 74);
    assert!(
        fill_px[0] > 180 && fill_px[1] > 120 && fill_px[2] < 120,
        "expected scene3d fill color in the lower-left quadrant, got {fill_px:?}"
    );

    let background_px = pixel(118, 118);
    assert!(
        background_px[2] < 140 && background_px[1] < 140 && background_px[0] < 140,
        "expected clear color to survive on untouched pixels, got {background_px:?}"
    );
}

fn render_camera_preview(mode: CameraRenderMode) -> Vec<u8> {
    let mut renderer = MetalRenderer::new_default().expect("metal");
    let width = 128u32;
    let height = 128u32;
    renderer.set_camera_texture_source(CameraTextureSource::SyntheticBenchmark);
    renderer.set_camera_render_mode(mode);
    renderer.resize(width, height, 1.0).expect("resize");

    let mut list = api::DrawList::default();
    list.items.push(api::DrawCmd::CameraBg {
        rect: api::RectF::new(0.0, 0.0, width as f32, height as f32),
        tint: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
        alpha: 1.0,
        grayscale: false,
        blur: false,
        sigma: 0.0,
    });

    let token = renderer.begin_frame(&api::FrameTarget, None);
    renderer.encode_pass(&list);
    renderer.submit(token).expect("submit");
    let (_rw, _rh, bgra) = renderer.readback_bgra8().expect("readback");
    bgra
}

#[test]
fn snapshot_camera_nv12_optimized_tracks_bgra_benchmark() {
    let optimized = render_camera_preview(CameraRenderMode::Nv12Optimized);
    let legacy = render_camera_preview(CameraRenderMode::Nv12Legacy);
    let bgra = render_camera_preview(CameraRenderMode::BgraBenchmark);

    let mut optimized_diff = 0u64;
    let mut legacy_diff = 0u64;
    let mut sample_count = 0u64;
    for ((opt_px, legacy_px), bgra_px) in
        optimized.chunks_exact(4).zip(legacy.chunks_exact(4)).zip(bgra.chunks_exact(4))
    {
        for channel in 0..3 {
            optimized_diff += opt_px[channel].abs_diff(bgra_px[channel]) as u64;
            legacy_diff += legacy_px[channel].abs_diff(bgra_px[channel]) as u64;
            sample_count += 1;
        }
    }

    let optimized_mean = optimized_diff as f64 / sample_count as f64;
    let legacy_mean = legacy_diff as f64 / sample_count as f64;
    assert!(
        optimized_mean < 6.0,
        "optimized NV12 preview drifted too far from BGRA reference: {optimized_mean:.3}"
    );
    assert!(
        legacy_mean > optimized_mean * 1.8,
        "legacy NV12 path no longer meaningfully diverges from BGRA reference: optimized={optimized_mean:.3} legacy={legacy_mean:.3}"
    );
}

fn solid_image(renderer: &mut MetalRenderer, bgra: [u8; 4]) -> api::ImageHandle
{
   let pixels = [bgra, bgra, bgra, bgra].concat();
   renderer.image_create_rgba8(2, 2, &pixels, 8)
}

fn readback_pixel(bgra: &[u8], width: u32, x: u32, y: u32) -> [u8; 4]
{
   let index = ((y * width + x) * 4) as usize;
   [bgra[index], bgra[index + 1], bgra[index + 2], bgra[index + 3]]
}

fn assert_pixel_eq(actual: [u8; 4], expected: [u8; 4], label: &str)
{
   assert_eq!(actual, expected, "{label}");
}

#[test]
fn snapshot_image_argument_tables_survive_separators_layers_and_effects()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   let width = 128_u32;
   renderer.resize(width, 32, 1.0).expect("resize");
   let red = solid_image(&mut renderer, [0, 0, 255, 255]);
   let green = solid_image(&mut renderer, [0, 255, 0, 255]);
   let blue = solid_image(&mut renderer, [255, 0, 0, 255]);
   let image = |tex, x| api::DrawCmd::Image {
      tex,
      dst: api::RectF::new(x, 4.0, 16.0, 16.0),
      src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
      alpha: 1.0,
   };
   let mut list = api::DrawList::default();
   list.items.extend_from_slice(&[
      image(red, 0.0),
      api::DrawCmd::RRect {
         rect: api::RectF::new(16.0, 24.0, 4.0, 4.0),
         radii: [1.0; 4],
         color: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
      },
      api::DrawCmd::ClipPush { rect: api::RectI::new(20, 0, 16, 32) },
      image(green, 20.0),
      api::DrawCmd::ClipPop,
      api::DrawCmd::LayerBegin {
         id: 6_001,
         rect: api::RectF::new(40.0, 4.0, 16.0, 16.0),
         dirty: true,
      },
      image(blue, 40.0),
      api::DrawCmd::LayerEnd,
      image(red, 60.0),
      api::DrawCmd::VisualEffect {
         rect: api::RectF::new(80.0, 4.0, 16.0, 16.0),
         effect: api::VisualEffect::DarkPopup {
            blur_intensity: 0.25,
            tint: api::Color::rgba(0.1, 0.1, 0.1, 0.2),
         },
      },
      image(green, 104.0),
   ]);

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(&list);
   renderer.submit(token).expect("submit");
   let (_, _, pixels) = renderer.readback_bgra8().expect("readback");
   for (x, expected, label) in [
      (8, [0, 0, 255, 255], "first table before rrect"),
      (28, [0, 255, 0, 255], "table inside clip"),
      (48, [255, 0, 0, 255], "table inside cached layer"),
      (68, [0, 0, 255, 255], "reused table after layer"),
      (112, [0, 255, 0, 255], "reused table after effect"),
   ]
   {
      assert_pixel_eq(readback_pixel(&pixels, width, x, 12), expected, label);
   }
   let stats = renderer.last_stats();
   assert!(stats.render_passes > 1, "fixture must exercise multiple Metal passes: {stats:?}");
   assert!(
      stats.image_argument_tables_finalized >= 3,
      "expected distinct immutable image tables: {stats:?}",
   );
   assert!(
      stats.image_argument_table_reuses >= 2,
      "expected identical tables to be reused without re-encoding: {stats:?}",
   );
   assert!(
      stats.image_argument_binds > stats.image_argument_tables_finalized,
      "expected reused immutable tables to remain bindable: {stats:?}",
   );
}

#[test]
fn snapshot_image_argument_tables_split_more_than_128_textures()
{
   const IMAGE_COUNT: usize = 130;
   let mut renderer = MetalRenderer::new_default().expect("metal");
   let width = IMAGE_COUNT as u32 * 4;
   renderer.resize(width, 8, 1.0).expect("resize");
   let mut expected = Vec::with_capacity(IMAGE_COUNT);
   let mut list = api::DrawList::default();
   for index in 0..IMAGE_COUNT
   {
      let color = [
         (index as u8).wrapping_mul(31),
         (index as u8).wrapping_mul(47),
         (index as u8).wrapping_mul(61),
         255,
      ];
      let texture = solid_image(&mut renderer, color);
      expected.push(color);
      list.items.push(api::DrawCmd::Image {
         tex: texture,
         dst: api::RectF::new(index as f32 * 4.0, 0.0, 4.0, 8.0),
         src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
         alpha: 1.0,
      });
   }

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(&list);
   renderer.submit(token).expect("submit");
   let (_, _, pixels) = renderer.readback_bgra8().expect("readback");
   for (index, expected) in expected.into_iter().enumerate()
   {
      assert_pixel_eq(
         readback_pixel(&pixels, width, index as u32 * 4 + 2, 4),
         expected,
         &format!("unique image {index}"),
      );
   }
   let stats = renderer.last_stats();
   assert_eq!(stats.image_argument_tables_finalized, 2, "128-slot split changed: {stats:?}");
   assert_eq!(stats.image_argument_binds, 2, "each table should bind once: {stats:?}");
   assert_eq!(stats.image_argument_table_reuses, 0, "unique tables cannot be reused: {stats:?}");
}

#[test]
fn snapshot_image_argument_table_growth_preserves_bound_slices_and_warms_up()
{
   const TABLE_COUNT: usize = 24;
   let mut renderer = MetalRenderer::new_default().expect("metal");
   let width = (TABLE_COUNT as u32 + 1) * 8;
   renderer.resize(width, 16, 1.0).expect("resize");
   let mut expected = Vec::with_capacity(TABLE_COUNT + 1);
   let mut list = api::DrawList::default();
   let mut first_texture = None;
   for index in 0..TABLE_COUNT
   {
      let color = [
         (index as u8).wrapping_mul(17),
         (index as u8).wrapping_mul(37),
         (index as u8).wrapping_mul(67),
         255,
      ];
      expected.push(color);
      let texture = solid_image(&mut renderer, color);
      first_texture.get_or_insert(texture);
      list.items.push(api::DrawCmd::Image {
         tex: texture,
         dst: api::RectF::new(index as f32 * 8.0, 0.0, 6.0, 8.0),
         src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
         alpha: 1.0,
      });
      list.items.push(api::DrawCmd::RRect {
         rect: api::RectF::new(index as f32 * 8.0, 12.0, 2.0, 2.0),
         radii: [0.0; 4],
         color: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
      });
   }
   let first_color = expected[0];
   expected.push(first_color);
   list.items.push(api::DrawCmd::Image {
      tex: first_texture.unwrap(),
      dst: api::RectF::new(TABLE_COUNT as f32 * 8.0, 0.0, 6.0, 8.0),
      src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
      alpha: 1.0,
   });

   for frame in 0..9
   {
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(&list);
      renderer.submit(token).expect("submit");
      let (_, _, pixels) = renderer.readback_bgra8().expect("readback");
      for (index, expected) in expected.iter().copied().enumerate()
      {
         assert_pixel_eq(
            readback_pixel(&pixels, width, index as u32 * 8 + 3, 4),
            expected,
            &format!("frame {frame} immutable table {index}"),
         );
      }
      let stats = renderer.last_stats();
      assert_eq!(stats.image_argument_tables_finalized, TABLE_COUNT as u32, "table count changed: {stats:?}");
      assert_eq!(stats.image_argument_binds, TABLE_COUNT as u32 + 1, "bind count changed: {stats:?}");
      assert_eq!(stats.image_argument_table_reuses, 1, "indexed reuse changed: {stats:?}");
      if frame < 8
      {
         assert!(stats.image_argument_buffer_grows > 0, "cold ring slot must grow: {stats:?}");
      }
      else
      {
         assert_eq!(stats.image_argument_buffer_grows, 0, "warm frame allocated: {stats:?}");
      }
   }
}

#[test]
fn snapshot_neon_marker_instance_arrays_match_distinctive_colors()
{
   use oxide_renderer_metal::neon_marker::{NeonMarker, NeonMarkerPass};

   let mut renderer = MetalRenderer::new_default().expect("metal");
   let width = 208_u32;
   let height = 112_u32;
   renderer.resize(width, height, 1.0).expect("resize");
   let colors = [
      (api::Color::rgba(1.0, 0.0, 0.0, 1.0), [0, 0, 252, 249]),
      (api::Color::rgba(0.0, 1.0, 0.0, 1.0), [0, 252, 0, 249]),
      (api::Color::rgba(0.0, 0.0, 1.0, 1.0), [252, 0, 0, 249]),
      (api::Color::rgba(1.0, 1.0, 0.0, 1.0), [0, 252, 252, 249]),
      (api::Color::rgba(1.0, 0.0, 1.0, 1.0), [252, 0, 252, 249]),
      (api::Color::rgba(0.0, 1.0, 1.0, 1.0), [252, 252, 0, 249]),
      (api::Color::rgba(1.0, 1.0, 1.0, 1.0), [252, 252, 252, 249]),
   ];

   for count in [1_usize, 2, 51, 52, 60, 61, 128]
   {
      let markers = (0..count)
         .map(|index| {
            let column = index % 16;
            let row = index / 16;
            NeonMarker {
               center: [8.0 + column as f32 * 12.0, 8.0 + row as f32 * 12.0],
               core_radius_px: 2.5,
               ring_radius_px: 3.0,
               ring_width_px: 1.0,
               halo_radius_px: 4.0,
               halo_sigma_px: 2.0,
               core_color: colors[index % colors.len()].0,
               ring_color: colors[(index + 3) % colors.len()].0,
               halo_alpha_max: 0.0,
               ring_alpha_max: 1.0,
            }
         })
         .collect::<Vec<_>>();
      let preferred_slot = renderer.mark_next_preferred_frame_slot_busy_for_snapshot();
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let selected_slot = renderer.current_frame_slot_for_snapshot();
      assert_ne!(selected_slot, preferred_slot, "busy preferred slot was selected");
      assert_eq!(renderer.last_stats().frame_backpressure_skipped, 0, "one busy slot caused backpressure");
      renderer.release_frame_slot_for_snapshot(preferred_slot);
      renderer
         .encode_neon_markers(&NeonMarkerPass {
            viewport: api::RectF::new(0.0, 0.0, width as f32, height as f32),
            markers: &markers,
         })
         .expect("encode neon markers");
      assert_eq!(
         renderer.current_frame_command_buffer_slot_for_snapshot(),
         Some(selected_slot),
         "neon markers encoded outside the selected frame slot",
      );
      assert!(
         !renderer.frame_slot_has_command_buffer_for_snapshot(preferred_slot),
         "neon markers created a command buffer on the busy preferred slot",
      );
      renderer.submit(token).expect("submit");
      let (_, _, pixels) = renderer.readback_bgra8().expect("readback");
      for (index, marker) in markers.iter().enumerate()
      {
         assert_pixel_eq(
            readback_pixel(&pixels, width, marker.center[0] as u32, marker.center[1] as u32),
            colors[index % colors.len()].1,
            &format!("marker count {count}, instance {index}"),
         );
      }
      let stats = renderer.last_stats();
      assert_eq!(stats.draws, count as u32, "marker draw count changed: {stats:?}");
   }
}
