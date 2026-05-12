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
    assert!(approx_eq(top_left[0], 255, 8));
    assert!(approx_eq(top_left[1], 255, 8));
    assert!(approx_eq(top_left[2], 255, 8));
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
        approx_eq(left_side[0], 255, 10)
            && approx_eq(left_side[1], 255, 10)
            && approx_eq(left_side[2], 255, 10),
        "expected white background on untouched area, got {left_side:?}"
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
            approx_eq(p[0], 255, 10) && approx_eq(p[1], 255, 10) && approx_eq(p[2], 255, 10),
            "expected untouched white background at ({x},{y}), got {p:?}"
        );
    }
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
