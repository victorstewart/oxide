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
fn id_mask_cached_compositor_only_hit_matches_fresh_fields_and_pixels() {
    use oxide_renderer_metal::id_mask_compositor::{
        IdMaskCityStyle, IdMaskCompositorMode, IdMaskGpuCompositorPass, IdMaskGpuRasterPass,
        IdMaskPolishConfig, IdMaskRasterChunk, IdMaskRasterProjection, IdMaskRasterVertex,
        ID_MASK_MAX_CITY_STYLES, ID_MASK_MAX_NEIGHBORHOOD_COLORS,
    };

    fn render(
        renderer: &mut MetalRenderer,
        vertices: &[IdMaskRasterVertex],
        chunks: &[IdMaskRasterChunk],
        red: f32,
    ) -> Vec<u8> {
        let mut city_styles = [IdMaskCityStyle::default(); ID_MASK_MAX_CITY_STYLES];
        city_styles[1].edge_rgb = [red, 0.25, 0.75];
        let pass = IdMaskGpuCompositorPass {
            raster: IdMaskGpuRasterPass {
                viewport: api::RectF::new(0.0, 0.0, 64.0, 64.0),
                mask_width: 64,
                mask_height: 64,
                mask_scale: 1.0,
                vertex_revision: 9,
                vertices,
                chunks,
                projection: IdMaskRasterProjection::screen_px(),
            },
            city_styles,
            neighborhood_colors: [[0.0; 3]; ID_MASK_MAX_NEIGHBORHOOD_COLORS],
            mode: IdMaskCompositorMode::CityIdMask,
            glow_enabled: false,
            darken_background_alpha: 0.0,
            polish: IdMaskPolishConfig::default(),
        };
        let token = renderer.begin_frame(&api::FrameTarget, None);
        renderer.encode_id_mask_gpu_compositor(&pass).expect("encode ID-mask snapshot");
        renderer.submit(token).expect("submit ID-mask snapshot");
        let (_, _, pixels) = renderer.readback_bgra8().expect("read ID-mask target");
        pixels
    }

    let vertices = [
        IdMaskRasterVertex::new([0.0, 0.0], 1, 2),
        IdMaskRasterVertex::new([64.0, 0.0], 1, 2),
        IdMaskRasterVertex::new([0.0, 64.0], 1, 2),
        IdMaskRasterVertex::new([0.0, 64.0], 1, 2),
        IdMaskRasterVertex::new([64.0, 0.0], 1, 2),
        IdMaskRasterVertex::new([64.0, 64.0], 1, 2),
    ];
    let chunks = [IdMaskRasterChunk { content_hash: 0x1234, first_vertex: 0, vertex_count: 6 }];
    let mut cached = MetalRenderer::new_default().expect("create cached renderer");
    cached.resize(64, 64, 1.0).expect("resize cached renderer");
    let _ = render(&mut cached, &vertices, &chunks, 0.2);
    let cached_pixels = render(&mut cached, &vertices, &chunks, 0.9);
    let cached_fields = cached.readback_id_mask_snapshot().expect("read cached ID-mask fields");
    assert_eq!(cached.last_stats().id_mask_cache_hits, 1);
    assert_eq!(cached.last_stats().render_passes, 1);

    let mut fresh = MetalRenderer::new_default().expect("create fresh renderer");
    fresh.resize(64, 64, 1.0).expect("resize fresh renderer");
    let fresh_pixels = render(&mut fresh, &vertices, &chunks, 0.9);
    let fresh_fields = fresh.readback_id_mask_snapshot().expect("read fresh ID-mask fields");
    assert_eq!(fresh.last_stats().id_mask_cache_misses, 1);
    assert_eq!(fresh.last_stats().render_passes, 9);
    assert_eq!(cached_pixels, fresh_pixels);
    assert_eq!(cached_fields, fresh_fields);
    cached.purge_id_mask_field_cache();
    assert!(cached.readback_id_mask_snapshot().is_none());
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
fn prepared_snapshot_reuses_mixed_buffers_and_matches_flat_output()
{
   let width = 128_u32;
   let height = 96_u32;
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(width, height, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(
      2,
      2,
      &[
         0, 0, 255, 255, 0, 255, 0, 255,
         255, 0, 0, 255, 255, 255, 255, 255,
      ],
      8,
   );
   let atlas = renderer.image_create_a8(2, 2, &[255, 255, 255, 255], 2);
   let snapshot = prepared_mixed_snapshot(image, atlas, 1);
   let updated_snapshot = prepared_mixed_snapshot_with_properties(
      image,
      atlas,
      1,
      [1.0, 0.0, 0.0, 1.0, 5.0, 3.0],
      0.5,
   );

   let first = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("encode first prepared snapshot");
   renderer.submit(first).expect("submit first prepared snapshot");
   let first_stats = renderer.last_stats();
   let (_, _, first_pixels) = renderer.readback_bgra8().expect("read first prepared snapshot");
   assert_eq!(first_stats.backend_cache_hits, 0);
   assert_eq!(first_stats.backend_cache_misses, 4);
   assert_eq!(first_stats.chunks_prepared, 4);
   assert!(first_stats.buffer_upload_bytes > 0);
   assert!(renderer.prepared_cache_resident_bytes() > 0);
   assert_eq!(first_stats.memory.prepared_cache_bytes, renderer.prepared_cache_resident_bytes());

   let second = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&updated_snapshot).expect("encode dynamic prepared snapshot");
   renderer.submit(second).expect("submit clean prepared snapshot");
   let second_stats = renderer.last_stats();
   let (_, _, second_pixels) = renderer.readback_bgra8().expect("read clean prepared snapshot");
   assert_ne!(first_pixels, second_pixels);
   assert_eq!(second_stats.backend_cache_hits, 4);
   assert_eq!(second_stats.backend_cache_misses, 0);
   assert_eq!(second_stats.chunks_prepared, 0);
   assert_eq!(second_stats.commands_traversed, 0);
   assert_eq!(second_stats.geometry_bytes_copied, 0);
   assert_eq!(second_stats.buffer_upload_bytes, 0);
   assert_eq!(second_stats.vb_bytes, 0);
   assert_eq!(second_stats.ib_bytes, 0);
   assert_eq!(second_stats.ub_bytes, 4 * 48);

   let mut flat = api::DrawList::default();
   updated_snapshot.flatten_into(&mut flat).expect("flatten prepared reference");
   let mut reference = MetalRenderer::new_default().expect("reference metal");
   reference.resize(width, height, 1.0).expect("reference resize");
   let reference_image = reference.image_create_rgba8(
      2,
      2,
      &[
         0, 0, 255, 255, 0, 255, 0, 255,
         255, 0, 0, 255, 255, 255, 255, 255,
      ],
      8,
   );
   let reference_atlas = reference.image_create_a8(2, 2, &[255, 255, 255, 255], 2);
   assert_eq!((reference_image, reference_atlas), (image, atlas));
   let token = reference.begin_frame(&api::FrameTarget, None);
   reference.encode_pass(&flat);
   reference.submit(token).expect("submit flat reference");
   let (_, _, reference_pixels) = reference.readback_bgra8().expect("read flat reference");
   assert_eq!(second_pixels, reference_pixels);
}

#[test]
fn prepared_property_ring_uploads_only_changed_instance_records_after_warmup()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(96, 96, 1.0).expect("resize");
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(990),
      api::RenderChunkRevisions::default(),
      api::DrawList {
         items: vec![api::DrawCmd::RRect {
            rect: api::RectF::new(0.0, 0.0, 24.0, 24.0),
            radii: [4.0; 4],
            color: api::Color::rgba(0.2, 0.5, 0.9, 1.0),
         }],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[],
   ).unwrap();
   let property = api::RenderPropertySlotId::dynamic(1, 1).unwrap();
   let snapshot = |revision: u64, tx: f32| {
      let mut instance = api::RenderChunkInstance::new(chunk.clone(), [20.0, 20.0]);
      instance.property_slots = vec![property].into();
      api::RenderSnapshot::new(
         vec![instance],
         vec![api::RenderPropertySlot {
            id: property,
            revision,
            value: api::RenderPropertyValue::Transform([1.0, 0.0, 0.0, 1.0, tx, 0.0]),
         }],
         api::Damage { rects: Vec::new() },
      ).unwrap()
   };
   let depth = renderer.frame_resource_depth_for_snapshot();
   for frame_index in 0..depth
   {
      let frame = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_snapshot(&snapshot(1, 2.0)).unwrap();
      renderer.submit(frame).unwrap();
      renderer.readback_bgra8().expect("complete property warmup frame");
      assert_eq!(renderer.last_stats().property_upload_bytes, 48);
      assert_eq!(renderer.last_stats().property_records_updated, 1);
      if frame_index > 0
      {
         assert_eq!(renderer.last_stats().geometry_bytes_copied, 0);
      }
   }
   let warm = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot(1, 2.0)).unwrap();
   renderer.submit(warm).unwrap();
   renderer.readback_bgra8().expect("complete unchanged property frame");
   assert_eq!(renderer.last_stats().property_upload_bytes, 0);
   assert_eq!(renderer.last_stats().property_records_updated, 0);
   assert_eq!(renderer.last_stats().buffer_upload_bytes, 0);

   let changed = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot(2, 7.0)).unwrap();
   renderer.submit(changed).unwrap();
   renderer.readback_bgra8().expect("complete changed property frame");
   assert_eq!(renderer.last_stats().property_upload_bytes, 48);
   assert_eq!(renderer.last_stats().property_records_updated, 1);
   assert_eq!(renderer.last_stats().buffer_upload_bytes, 0);
}

#[test]
fn prepared_opaque_rects_match_fractionally_translated_flat_output()
{
   let width = 128_u32;
   let height = 96_u32;
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(width, height, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let snapshot = prepared_fractional_opaque_snapshot(image);

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("encode fractional prepared snapshot");
   renderer.submit(token).expect("submit fractional prepared snapshot");
   let prepared_stats = renderer.last_stats();
   let (_, _, prepared_pixels) = renderer.readback_bgra8().expect("read fractional prepared snapshot");

   let mut flat = api::DrawList::default();
   snapshot.flatten_into(&mut flat).expect("flatten fractional reference");
   let mut reference = MetalRenderer::new_default().expect("reference metal");
   reference.resize(width, height, 1.0).expect("reference resize");
   let reference_image = reference.image_create_rgba8(2, 2, &[255; 16], 8);
   assert_eq!(reference_image, image);
   let token = reference.begin_frame(&api::FrameTarget, None);
   reference.encode_pass(&flat);
   reference.submit(token).expect("submit fractional flat reference");
   let reference_stats = reference.last_stats();
   let (_, _, reference_pixels) = reference.readback_bgra8().expect("read fractional flat reference");
   assert_eq!(prepared_pixels, reference_pixels);
   assert_eq!((prepared_stats.draws, prepared_stats.instanced), (2, 128));
   assert_eq!((reference_stats.draws, reference_stats.instanced), (2, 128));
}

#[test]
fn prepared_image_mesh_matches_flat_transform_opacity_output()
{
   let width = 96_u32;
   let height = 80_u32;
   let pixels = [
      0, 0, 255, 255, 0, 255, 0, 255,
      255, 0, 0, 255, 255, 255, 255, 255,
   ];
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(width, height, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(2, 2, &pixels, 8);
   let snapshot = prepared_image_mesh_snapshot(image);
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("encode prepared image mesh");
   renderer.submit(token).expect("submit prepared image mesh");
   let prepared_stats = renderer.last_stats();
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("replay prepared image mesh");
   renderer.submit(token).expect("submit replayed image mesh");
   let replay_stats = renderer.last_stats();
   let (_, _, prepared_pixels) = renderer.readback_bgra8().expect("read prepared image mesh");

   let mut flat = api::DrawList::default();
   snapshot.flatten_into(&mut flat).expect("flatten image mesh reference");
   let mut reference = MetalRenderer::new_default().expect("reference metal");
   reference.resize(width, height, 1.0).expect("reference resize");
   let reference_image = reference.image_create_rgba8(2, 2, &pixels, 8);
   assert_eq!(reference_image, image);
   let token = reference.begin_frame(&api::FrameTarget, None);
   reference.encode_pass(&flat);
   reference.submit(token).expect("submit flat image mesh");
   let (_, _, reference_pixels) = reference.readback_bgra8().expect("read flat image mesh");
   assert_eq!(prepared_pixels, reference_pixels);
   assert_eq!(prepared_stats.backend_cache_misses, 1);
   assert_eq!(prepared_stats.commands_copied, 0);
   assert_eq!(prepared_stats.draws, 1);
   assert_eq!(replay_stats.backend_cache_hits, 1);
   assert_eq!(replay_stats.backend_cache_misses, 0);
   assert_eq!(replay_stats.commands_copied, 0);
   assert_eq!(replay_stats.buffer_upload_bytes, 0);
}

#[test]
fn prepared_small_damage_queries_one_glyph_or_mesh_without_vertex_scans()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(1_024, 64, 1.0).expect("resize");
   renderer.set_damage_options(true, 0.70, 0.30);
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let atlas = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let snapshot = prepared_spatial_snapshot(image, atlas, 512);

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("warm spatial snapshot");
   renderer.submit(token).expect("submit spatial warmup");
   let (_, _, expected) = renderer.readback_bgra8().expect("read spatial warmup");

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("replay full spatial snapshot");
   renderer.submit(token).expect("submit full spatial replay");
   let full_stats = renderer.last_stats();
   assert_eq!(full_stats.prepared_plan_reuses, 1);
   assert_eq!(full_stats.backend_cache_hits, 512);
   assert_eq!(full_stats.backend_cache_misses, 0);
   assert_eq!(full_stats.geometry_bytes_copied, 0);
   assert_eq!(full_stats.buffer_upload_bytes, 0);

   for damage in [api::RectI::new(500, 10, 2, 2), api::RectI::new(520, 10, 2, 2)]
   {
      let frame_damage = api::Damage { rects: vec![damage] };
      let token = renderer.begin_frame(&api::FrameTarget, Some(&frame_damage));
      renderer.encode_snapshot(&snapshot).expect("encode spatial damage");
      renderer.submit(token).expect("submit spatial damage");
      let stats = renderer.last_stats();
      let (_, _, actual) = renderer.readback_bgra8().expect("read spatial damage");
      assert_eq!(actual, expected);
      assert!(stats.damage_instances_visited <= 2, "visited {} instances", stats.damage_instances_visited);
      assert_eq!(stats.damage_instances_matched, 1);
      assert_eq!(stats.damage_commands_visited, 1);
      assert_eq!(stats.damage_commands_matched, 1);
      assert_eq!(stats.damage_vertices_visited, 0);
      assert_eq!(stats.draws, 1);
      assert_eq!(stats.backend_cache_hits, 1);
      assert_eq!(stats.backend_cache_misses, 0);
      assert_eq!(stats.geometry_bytes_copied, 0);
      assert_eq!(stats.buffer_upload_bytes, 0);
      assert_eq!(stats.shaded_damage_px, 4);
   }
}

#[test]
fn prepared_snapshot_rebuilds_only_the_dirty_chunk()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(128, 96, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let atlas = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let warm = prepared_mixed_snapshot(image, atlas, 1);
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&warm).expect("warm prepared snapshot");
   renderer.submit(token).expect("submit warm snapshot");
   let _ = renderer.readback_bgra8().expect("drain warm snapshot");
   assert_eq!(renderer.prepared_cache_entry_count(), 4);

   let dirty = prepared_mixed_snapshot(image, atlas, 2);
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&dirty).expect("encode one dirty chunk");
   renderer.submit(token).expect("submit one dirty chunk");
   let stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("drain dirty snapshot");
   assert_eq!(stats.backend_cache_hits, 3);
   assert_eq!(stats.backend_cache_misses, 1);
   assert_eq!(stats.chunks_prepared, 1);
   assert_eq!(stats.commands_traversed, 3);
   assert!(stats.buffer_upload_bytes > 0);
   assert_eq!(renderer.prepared_cache_entry_count(), 4);
}

#[test]
fn prepared_snapshot_byte_budget_evicts_lru_chunks()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(64, 64, 1.0).expect("resize");
   let first = prepared_rrect_snapshot(911, 4.0);
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&first).expect("prepare first chunk");
   renderer.submit(token).expect("submit first chunk");
   let _ = renderer.readback_bgra8().expect("drain first chunk");
   let one_chunk_bytes = renderer.prepared_cache_resident_bytes();
   assert!(one_chunk_bytes > 0);
   renderer.set_prepared_cache_budget_bytes(one_chunk_bytes);

   let second = prepared_rrect_snapshot(912, 24.0);
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&second).expect("prepare second chunk");
   renderer.submit(token).expect("submit second chunk");
   let stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("drain second chunk");
   assert_eq!(renderer.prepared_cache_entry_count(), 1);
   assert!(renderer.prepared_cache_resident_bytes() <= one_chunk_bytes);
   assert_eq!(stats.cache_evictions, 1);

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&first).expect("reprepare evicted first chunk");
   renderer.submit(token).expect("submit reprepared first chunk");
   let stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("drain reprepared first chunk");
   assert_eq!(stats.backend_cache_hits, 0);
   assert_eq!(stats.backend_cache_misses, 1);
   assert_eq!(stats.chunks_prepared, 1);
   assert_eq!(renderer.prepared_cache_entry_count(), 1);

   renderer.purge_prepared_chunks();
   assert_eq!(renderer.prepared_cache_entry_count(), 0);
   assert_eq!(renderer.prepared_cache_resident_bytes(), 0);
}

#[test]
fn prepared_snapshot_fallback_reports_exact_flattened_work()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(64, 64, 1.0).expect("resize");
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(914),
      api::RenderChunkRevisions::default(),
      api::DrawList {
         items: vec![api::DrawCmd::Spinner { center: [32.0, 32.0], atom: 12.0, alpha: 1.0 }],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[],
   ).expect("spinner chunk");
   let snapshot = api::RenderSnapshot::new(
      vec![api::RenderChunkInstance::new(chunk, [0.0, 0.0])],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("spinner snapshot");

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("encode flat fallback");
   renderer.submit(token).expect("submit flat fallback");
   let stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("drain flat fallback");
   assert_eq!(stats.commands_copied, 1);
   assert_eq!(stats.geometry_bytes_copied, 0);
   assert_eq!(stats.backend_cache_hits, 0);
   assert_eq!(stats.backend_cache_misses, 0);
   assert_eq!(renderer.prepared_cache_entry_count(), 0);
}

#[test]
fn prepared_snapshot_invalidates_referenced_resource_generations()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(64, 64, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let first = prepared_image_snapshot(image, 1);
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&first).expect("prepare image generation one");
   renderer.submit(token).expect("submit generation one");
   let _ = renderer.readback_bgra8().expect("drain generation one");
   assert_eq!(renderer.prepared_cache_entry_count(), 1);

   renderer.image_update_rgba8(image, 0, 0, 2, 2, &[0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255], 8);
   assert_eq!(renderer.image_generation(image), Some(2));
   assert_eq!(renderer.prepared_cache_entry_count(), 0);
   assert_eq!(renderer.prepared_cache_resident_bytes(), 0);

   let second = prepared_image_snapshot(image, 2);
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&second).expect("prepare image generation two");
   renderer.submit(token).expect("submit generation two");
   let stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("drain generation two");
   assert_eq!(stats.backend_cache_hits, 0);
   assert_eq!(stats.backend_cache_misses, 1);
   assert_eq!(stats.chunks_prepared, 1);
   assert_eq!(stats.cache_evictions, 1);
   assert_eq!(renderer.prepared_cache_entry_count(), 1);
}

#[test]
fn append_only_glyph_upload_preserves_unrelated_prepared_chunks()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(128, 96, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let atlas = renderer.image_create_a8(8, 8, &[255; 64], 8);
   let snapshot = prepared_mixed_snapshot(image, atlas, 1);

   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("prepare atlas chunks");
   renderer.submit(frame).expect("submit atlas chunks");
   let _ = renderer.readback_bgra8().expect("drain atlas chunks");
   assert_eq!(renderer.prepared_cache_entry_count(), 4);

   renderer.image_append_a8(atlas, 6, 6, 2, 2, &[192; 4], 2);
   assert_eq!(renderer.image_generation(atlas), Some(1));
   assert_eq!(renderer.prepared_cache_entry_count(), 4);

   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("reuse append-only atlas chunks");
   renderer.submit(frame).expect("submit reused atlas chunks");
   let stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("drain reused atlas chunks");
   assert_eq!(stats.backend_cache_hits, 4);
   assert_eq!(stats.backend_cache_misses, 0);
   assert_eq!(stats.chunks_prepared, 0);
}

#[test]
fn recycling_one_glyph_page_rebuilds_only_its_prepared_chunk()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(96, 64, 1.0).expect("resize");
   let first_page = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let stable_page = renderer.image_create_a8(2, 2, &[192; 4], 2);
   let first = two_glyph_page_snapshot(first_page, stable_page);

   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&first).expect("prepare two glyph pages");
   renderer.submit(frame).expect("submit two glyph pages");
   let _ = renderer.readback_bgra8().expect("drain two glyph pages");
   assert_eq!(renderer.prepared_cache_entry_count(), 2);

   renderer.image_release(first_page);
   assert_eq!(renderer.prepared_cache_entry_count(), 1);
   let replacement_page = renderer.image_create_a8(2, 2, &[128; 4], 2);
   let recycled = two_glyph_page_snapshot(replacement_page, stable_page);
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&recycled).expect("encode recycled glyph page");
   renderer.submit(frame).expect("submit recycled glyph page");
   let stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("drain recycled glyph page");
   assert_eq!(stats.backend_cache_hits, 1);
   assert_eq!(stats.backend_cache_misses, 1);
   assert_eq!(stats.chunks_prepared, 1);
   assert_eq!(renderer.prepared_cache_entry_count(), 2);
}

#[test]
fn prepared_layer_clean_hit_composites_without_body_work_and_matches_flat_pixels()
{
   let width = 160_u32;
   let height = 128_u32;
   let image_pixels = [
      0, 0, 255, 255, 0, 255, 0, 255,
      255, 0, 0, 255, 255, 255, 255, 255,
   ];
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(width, height, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(2, 2, &image_pixels, 8);
   let atlas = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let snapshot = prepared_layer_snapshot(image, atlas, 1, 1, 1, false, true, None);

   let first = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("encode cold prepared layer");
   renderer.submit(first).expect("submit cold prepared layer");
   let first_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("complete cold prepared layer");
   assert_eq!(first_stats.layer_cache_hits, 0);
   assert_eq!(first_stats.layer_cache_misses, 1);
   assert_eq!(first_stats.layer_texture_creates, 1);
   assert_eq!(first_stats.layer_offscreen_draws, 5);
   assert_eq!(first_stats.layer_body_commands_scanned, 0);
   assert_eq!(first_stats.layer_body_commands_copied, 0);
   assert_eq!(first_stats.render_passes, 2);

   let clean = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).expect("encode clean prepared layer");
   renderer.submit(clean).expect("submit clean prepared layer");
   let clean_stats = renderer.last_stats();
   let (_, _, clean_pixels) = renderer.readback_bgra8().expect("read clean prepared layer");
   assert_eq!(clean_stats.layer_cache_hits, 1);
   assert_eq!(clean_stats.layer_cache_misses, 0);
   assert_eq!(clean_stats.layer_texture_creates, 0);
   assert_eq!(clean_stats.layer_offscreen_draws, 0);
   assert_eq!(clean_stats.layer_body_commands_scanned, 0);
   assert_eq!(clean_stats.layer_body_commands_copied, 0);
   assert_eq!(clean_stats.commands_traversed, 0);
   assert_eq!(clean_stats.commands_copied, 0);
   assert_eq!(clean_stats.geometry_bytes_copied, 0);
   assert_eq!(clean_stats.buffer_upload_bytes, 0);
   assert_eq!(clean_stats.render_passes, 1);
   assert_eq!(clean_stats.draws, 1);

   let mut reference = MetalRenderer::new_default().expect("reference metal");
   reference.resize(width, height, 1.0).expect("reference resize");
   let reference_image = reference.image_create_rgba8(2, 2, &image_pixels, 8);
   let reference_atlas = reference.image_create_a8(2, 2, &[255; 4], 2);
   assert_eq!((reference_image, reference_atlas), (image, atlas));
   let baseline = prepared_layer_snapshot(
      reference_image,
      reference_atlas,
      1,
      1,
      1,
      false,
      true,
      None,
   );
   let mut flat = api::DrawList::default();
   baseline.flatten_into(&mut flat).expect("flatten layer reference");
   let frame = reference.begin_frame(&api::FrameTarget, None);
   reference.encode_pass(&flat);
   reference.submit(frame).expect("submit flat layer reference");
   let (_, _, flat_pixels) = reference.readback_bgra8().expect("read flat layer reference");
   let differences = clean_pixels.iter().zip(&flat_pixels).enumerate()
      .filter(|(_, (prepared, flat))| prepared != flat)
      .take(16)
      .map(|(index, (prepared, flat))| (index, *prepared, *flat))
      .collect::<Vec<_>>();
   assert!(differences.is_empty(), "prepared layer pixel differences: {differences:?}");
}

#[test]
fn prepared_layer_main_format_matches_flat_translucent_rrect_pixels()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(96, 96, 1.0).expect("resize");
   let layered = prepared_rrect_layer_snapshot();
   for label in ["cold", "clean"]
   {
      let frame = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_snapshot(&layered).unwrap_or_else(|error| panic!("{label}: {error}"));
      renderer.submit(frame).unwrap_or_else(|error| panic!("{label}: {error}"));
      if label == "cold"
      {
         let _ = renderer.readback_bgra8().expect("complete cold RRect layer");
      }
   }
   let stats = renderer.last_stats();
   let (_, _, actual) = renderer.readback_bgra8().expect("read clean RRect layer");
   assert_eq!(stats.layer_cache_hits, 1);
   assert_eq!(stats.layer_cache_misses, 0);
   assert_eq!(stats.layer_offscreen_draws, 0);
   assert_eq!(stats.buffer_upload_bytes, 0);

   let mut reference = MetalRenderer::new_default().expect("reference metal");
   reference.resize(96, 96, 1.0).expect("reference resize");
   let baseline = prepared_rrect_layer_snapshot();
   let mut flat = api::DrawList::default();
   baseline.flatten_into(&mut flat).expect("flatten RRect layer reference");
   let frame = reference.begin_frame(&api::FrameTarget, None);
   reference.encode_pass(&flat);
   reference.submit(frame).expect("submit flat RRect layer reference");
   let (_, _, expected) = reference.readback_bgra8().expect("read flat RRect layer reference");
   let differences = actual.iter().zip(&expected).enumerate()
      .filter(|(_, (candidate, reference))| candidate != reference)
      .take(16)
      .map(|(offset, (candidate, reference))| (offset, *candidate, *reference))
      .collect::<Vec<_>>();
   assert!(differences.is_empty(), "main-format prepared layer pixel differences: {differences:?}");
}

#[test]
fn prepared_layer_main_format_image_text_mesh_and_solid_match_flat_pixels()
{
   let pixels = [
      0, 0, 255, 255, 0, 255, 0, 255,
      255, 0, 0, 255, 255, 255, 255, 255,
   ];
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(160, 128, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(2, 2, &pixels, 8);
   let atlas = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let snapshot = prepared_layer_snapshot_content(
      image, atlas, 1, 1, 1, false, true, None, false,
   );
   for label in ["cold", "clean"]
   {
      let frame = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_snapshot(&snapshot).unwrap_or_else(|error| panic!("{label}: {error}"));
      renderer.submit(frame).unwrap_or_else(|error| panic!("{label}: {error}"));
      if label == "cold"
      {
         let _ = renderer.readback_bgra8().expect("complete cold non-RRect layer");
      }
   }
   let stats = renderer.last_stats();
   let (_, _, actual) = renderer.readback_bgra8().expect("read clean non-RRect layer");
   assert_eq!(stats.layer_cache_hits, 1);
   assert_eq!(stats.layer_cache_misses, 0);
   assert_eq!(stats.layer_offscreen_draws, 0);
   assert_eq!(stats.buffer_upload_bytes, 0);

   let mut reference = MetalRenderer::new_default().expect("reference metal");
   reference.resize(160, 128, 1.0).expect("reference resize");
   let reference_image = reference.image_create_rgba8(2, 2, &pixels, 8);
   let reference_atlas = reference.image_create_a8(2, 2, &[255; 4], 2);
   assert_eq!((reference_image, reference_atlas), (image, atlas));
   let baseline = prepared_layer_snapshot_content(
      reference_image, reference_atlas, 1, 1, 1, false, true, None, false,
   );
   let mut flat = api::DrawList::default();
   baseline.flatten_into(&mut flat).expect("flatten non-RRect layer reference");
   let frame = reference.begin_frame(&api::FrameTarget, None);
   reference.encode_pass(&flat);
   reference.submit(frame).expect("submit flat non-RRect layer reference");
   let (_, _, expected) = reference.readback_bgra8().expect("read flat non-RRect layer reference");
   let differences = actual.iter().zip(&expected).enumerate()
      .filter(|(_, (prepared, flat))| prepared != flat)
      .take(16)
      .map(|(offset, (prepared, flat))| (offset, *prepared, *flat))
      .collect::<Vec<_>>();
   assert!(differences.is_empty(), "non-RRect prepared layer pixel differences: {differences:?}");
}

#[test]
fn prepared_layer_invalidates_once_for_dirty_nested_resource_scale_and_purge_changes()
{
   let mut renderer = MetalRenderer::new_default().expect("metal");
   renderer.resize(160, 128, 1.0).expect("resize");
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let atlas = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let base = prepared_layer_snapshot(image, atlas, 1, 1, 1, false, true, None);
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&base).expect("warm prepared layer");
   renderer.submit(frame).expect("submit warm prepared layer");
   let (_, _, base_pixels) = renderer.readback_bgra8().expect("read warm prepared layer");

   let dirty = prepared_layer_snapshot(image, atlas, 1, 1, 1, true, true, None);
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&dirty).expect("encode dirty prepared layer");
   renderer.submit(frame).expect("submit dirty prepared layer");
   let dirty_stats = renderer.last_stats();
   let (_, _, dirty_pixels) = renderer.readback_bgra8().expect("read dirty prepared layer");
   assert_eq!(dirty_pixels, base_pixels);
   assert_eq!(dirty_stats.layer_cache_misses, 1);
   assert_eq!(dirty_stats.layer_cache_hits, 0);
   assert_eq!(dirty_stats.layer_offscreen_draws, 5);
   assert_eq!(dirty_stats.layer_texture_creates, 0);
   assert_eq!(dirty_stats.chunks_prepared, 0);
   assert_eq!(dirty_stats.commands_traversed, 0);
   assert_eq!(dirty_stats.render_passes, 2);

   let first_instance = dirty.instance(0).expect("first dirty layer instance");
   let mut second_instance = first_instance.clone();
   second_instance.origin = [52.0, 28.0];
   let duplicated_dirty = api::RenderSnapshot::new(
      vec![first_instance, second_instance],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("duplicated dirty layer snapshot");
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&duplicated_dirty).expect("encode duplicated dirty layer");
   renderer.submit(frame).expect("submit duplicated dirty layer");
   let duplicated_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("read duplicated dirty layer");
   assert_eq!(duplicated_stats.layer_cache_misses, 1);
   assert_eq!(duplicated_stats.layer_cache_hits, 1);
   assert_eq!(duplicated_stats.layer_offscreen_draws, 5);
   assert_eq!(duplicated_stats.layer_texture_creates, 0);
   assert_eq!(duplicated_stats.render_passes, 2);
   assert_eq!(duplicated_stats.draws, 7);

   let translated = prepared_layer_snapshot(
      image,
      atlas,
      1,
      1,
      1,
      false,
      true,
      Some([1.0, 0.0, 0.0, 1.0, 8.0, 4.0]),
   );
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&translated).expect("move clean prepared layer");
   renderer.submit(frame).expect("submit moved prepared layer");
   let translated_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("read moved prepared layer");
   assert_eq!(translated_stats.layer_cache_hits, 1);
   assert_eq!(translated_stats.layer_cache_misses, 0);
   assert_eq!(translated_stats.layer_offscreen_draws, 0);
   assert_eq!(translated_stats.buffer_upload_bytes, 0);

   let scaled = prepared_layer_snapshot(
      image,
      atlas,
      1,
      1,
      1,
      false,
      true,
      Some([1.5, 0.0, 0.0, 1.25, 0.0, 0.0]),
   );
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&scaled).expect("scale prepared layer");
   renderer.submit(frame).expect("submit scaled prepared layer");
   let scaled_stats = renderer.last_stats();
   let (_, _, scaled_pixels) = renderer.readback_bgra8().expect("read scaled prepared layer");
   assert_eq!(scaled_stats.layer_cache_misses, 1);
   assert_eq!(scaled_stats.layer_offscreen_draws, 5);
   assert_eq!(scaled_stats.layer_texture_creates, 1);

   let mut reference = MetalRenderer::new_default().expect("scaled reference metal");
   reference.resize(160, 128, 1.0).expect("scaled reference resize");
   let reference_image = reference.image_create_rgba8(2, 2, &[255; 16], 8);
   let reference_atlas = reference.image_create_a8(2, 2, &[255; 4], 2);
   assert_eq!((reference_image, reference_atlas), (image, atlas));
   let baseline_scaled = prepared_layer_snapshot(
      reference_image,
      reference_atlas,
      1,
      1,
      1,
      false,
      false,
      Some([1.5, 0.0, 0.0, 1.25, 0.0, 0.0]),
   );
   let frame = reference.begin_frame(&api::FrameTarget, None);
   reference.encode_snapshot(&baseline_scaled).expect("encode scaled direct reference");
   reference.submit(frame).expect("submit scaled direct reference");
   let (_, _, direct_scaled_pixels) = reference.readback_bgra8().expect("read scaled direct reference");
   let differences = scaled_pixels.iter().zip(&direct_scaled_pixels).enumerate()
      .filter(|(_, (prepared, direct))| prepared != direct)
      .take(16)
      .map(|(index, (prepared, direct))| (index, *prepared, *direct))
      .collect::<Vec<_>>();
   assert!(differences.is_empty(), "scaled prepared layer pixel differences: {differences:?}");

   renderer.resize(192, 160, 1.0).expect("resize same scale");
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&scaled).expect("encode resized prepared layer");
   renderer.submit(frame).expect("submit resized prepared layer");
   let resized_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("read resized prepared layer");
   assert_eq!(resized_stats.layer_cache_hits, 1);
   assert_eq!(resized_stats.layer_cache_misses, 0);
   assert_eq!(resized_stats.layer_offscreen_draws, 0);

   renderer.resize(384, 320, 2.0).expect("resize target scale");
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&scaled).expect("encode target-scale layer");
   renderer.submit(frame).expect("submit target-scale layer");
   let target_scale_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("read target-scale layer");
   assert_eq!(target_scale_stats.layer_cache_hits, 0);
   assert_eq!(target_scale_stats.layer_cache_misses, 1);
   assert_eq!(target_scale_stats.layer_offscreen_draws, 5);
   assert_eq!(target_scale_stats.layer_texture_creates, 1);

   let nested = prepared_layer_snapshot(
      image,
      atlas,
      1,
      2,
      1,
      false,
      true,
      Some([1.5, 0.0, 0.0, 1.25, 0.0, 0.0]),
   );
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&nested).expect("invalidate nested generation");
   renderer.submit(frame).expect("submit nested generation");
   let nested_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("read nested generation");
   assert_eq!(nested_stats.layer_cache_misses, 1);
   assert_eq!(nested_stats.layer_offscreen_draws, 5);
   assert_eq!(nested_stats.layer_texture_creates, 0);
   assert_eq!(nested_stats.chunks_prepared, 1);

   renderer.purge_prepared_chunks();
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&nested).expect("encode after purge");
   renderer.submit(frame).expect("submit after purge");
   let purge_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("read after purge");
   assert_eq!(purge_stats.layer_cache_misses, 1);
   assert_eq!(purge_stats.layer_offscreen_draws, 5);
   assert_eq!(purge_stats.layer_texture_creates, 0);
   assert_eq!(purge_stats.chunks_prepared, 1);

   renderer.image_update_rgba8(
      image,
      0,
      0,
      2,
      2,
      &[
         0, 0, 255, 255, 0, 0, 255, 255,
         0, 0, 255, 255, 0, 0, 255, 255,
      ],
      8,
   );
   renderer.image_update_a8(atlas, 0, 0, 2, 2, &[192; 4], 2);
   let resource = prepared_layer_snapshot(
      image,
      atlas,
      1,
      2,
      2,
      false,
      true,
      Some([1.5, 0.0, 0.0, 1.25, 0.0, 0.0]),
   );
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&resource).expect("invalidate layer resource generation");
   renderer.submit(frame).expect("submit resource generation");
   let resource_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("read resource generation");
   assert_eq!(resource_stats.layer_cache_misses, 1);
   assert_eq!(resource_stats.layer_offscreen_draws, 5);
   assert_eq!(resource_stats.layer_texture_creates, 0);
   assert_eq!(resource_stats.chunks_prepared, 1);

   let content = prepared_layer_snapshot(
      image,
      atlas,
      2,
      2,
      2,
      false,
      true,
      Some([1.5, 0.0, 0.0, 1.25, 0.0, 0.0]),
   );
   let frame = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&content).expect("invalidate layer content generation");
   renderer.submit(frame).expect("submit content generation");
   let content_stats = renderer.last_stats();
   let _ = renderer.readback_bgra8().expect("read content generation");
   assert_eq!(content_stats.layer_cache_misses, 1);
   assert_eq!(content_stats.layer_offscreen_draws, 5);
   assert_eq!(content_stats.layer_texture_creates, 0);
   assert_eq!(content_stats.chunks_prepared, 1);
}

#[test]
fn prepared_layer_effect_nested_and_unsupported_content_preserve_flat_fallback_pixels()
{
   let cases = [
      (
         "effect outset",
         api::DrawList {
            items: vec![
               api::DrawCmd::RRect {
                  rect: api::RectF::new(8.0, 8.0, 56.0, 56.0),
                  radii: [8.0; 4],
                  color: api::Color::rgba(0.2, 0.5, 0.9, 1.0),
               },
               api::DrawCmd::VisualEffect {
                  rect: api::RectF::new(16.0, 16.0, 40.0, 40.0),
                  effect: api::VisualEffect::DarkPopup {
                     blur_intensity: 0.25,
                     tint: api::Color::rgba(0.1, 0.1, 0.1, 0.2),
                  },
               },
            ],
            ..api::DrawList::default()
         },
      ),
      (
         "nested layer",
         api::DrawList {
            items: vec![
               api::DrawCmd::LayerBegin {
                  id: 8_102,
                  rect: api::RectF::new(12.0, 12.0, 48.0, 48.0),
                  dirty: false,
               },
               api::DrawCmd::RRect {
                  rect: api::RectF::new(12.0, 12.0, 48.0, 48.0),
                  radii: [7.0; 4],
                  color: api::Color::rgba(0.8, 0.3, 0.2, 1.0),
               },
               api::DrawCmd::LayerEnd,
            ],
            ..api::DrawList::default()
         },
      ),
      (
         "unsupported spinner",
         api::DrawList {
            items: vec![api::DrawCmd::Spinner {
               center: [36.0, 36.0],
               atom: 18.0,
               alpha: 1.0,
            }],
            ..api::DrawList::default()
         },
      ),
      (
         "mixed RRect precision",
         api::DrawList {
            items: vec![
               api::DrawCmd::RRect {
                  rect: api::RectF::new(8.0, 8.0, 48.0, 40.0),
                  radii: [7.0; 4],
                  color: api::Color::rgba(0.9, 0.2, 0.1, 1.0),
               },
               api::DrawCmd::RRect {
                  rect: api::RectF::new(24.0, 20.0, 44.0, 44.0),
                  radii: [9.0; 4],
                  color: api::Color::rgba(0.1, 0.7, 0.9, 0.72),
               },
            ],
            ..api::DrawList::default()
         },
      ),
   ];
   for (index, (label, list)) in cases.into_iter().enumerate()
   {
      let snapshot = fallback_layer_snapshot(list, 8_200 + index as u64);
      let mut renderer = MetalRenderer::new_default().expect("metal");
      renderer.resize(96, 96, 1.0).expect("resize");
      let frame = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_snapshot(&snapshot).unwrap_or_else(|error| panic!("{label}: {error}"));
      renderer.submit(frame).unwrap_or_else(|error| panic!("{label}: {error}"));
      let stats = renderer.last_stats();
      let (_, _, actual) = renderer.readback_bgra8().expect("read fallback layer");
      assert_eq!(stats.chunks_prepared, 0, "{label}: {stats:?}");
      assert!(stats.commands_copied > 0, "{label}: {stats:?}");

      let mut flat = api::DrawList::default();
      snapshot.flatten_into(&mut flat).unwrap_or_else(|error| panic!("{label}: {error}"));
      let mut reference = MetalRenderer::new_default().expect("reference metal");
      reference.resize(96, 96, 1.0).expect("reference resize");
      let frame = reference.begin_frame(&api::FrameTarget, None);
      reference.encode_pass(&flat);
      reference.submit(frame).unwrap_or_else(|error| panic!("{label}: {error}"));
      let (_, _, expected) = reference.readback_bgra8().expect("read fallback reference");
      let differences = actual.iter().zip(&expected).enumerate()
         .filter(|(_, (candidate, reference))| candidate != reference)
         .take(16)
         .map(|(offset, (candidate, reference))| (offset, *candidate, *reference))
         .collect::<Vec<_>>();
      assert!(differences.is_empty(), "{label} pixel differences: {differences:?}");
   }
}

fn fallback_layer_snapshot(list: api::DrawList, id: u64) -> api::RenderSnapshot
{
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(id),
      api::RenderChunkRevisions { structural: 1, geometry: 1, ..api::RenderChunkRevisions::default() },
      list,
      api::ChunkIndexMode::Local,
      &[],
   ).expect("fallback layer chunk");
   let mut instance = api::RenderChunkInstance::new(chunk, [12.0, 10.0]);
   instance.layer = Some(api::RenderLayerInstance {
      id: 8_100 + id as u32,
      rect: api::RectF::new(0.0, 0.0, 72.0, 72.0),
      dirty: false,
   });
   api::RenderSnapshot::new(
      vec![instance],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("fallback layer snapshot")
}

fn prepared_rrect_layer_snapshot() -> api::RenderSnapshot
{
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(8_300),
      api::RenderChunkRevisions { structural: 1, geometry: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![
            api::DrawCmd::RRect {
               rect: api::RectF::new(4.0, 4.0, 48.0, 40.0),
               radii: [7.0; 4],
               color: api::Color::rgba(0.9, 0.2, 0.1, 0.64),
            },
            api::DrawCmd::RRect {
               rect: api::RectF::new(24.0, 20.0, 44.0, 44.0),
               radii: [9.0; 4],
               color: api::Color::rgba(0.1, 0.7, 0.9, 0.72),
            },
         ],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[],
   ).expect("prepared RRect layer chunk");
   let mut instance = api::RenderChunkInstance::new(chunk, [12.0, 10.0]);
   instance.layer = Some(api::RenderLayerInstance {
      id: 8_300,
      rect: api::RectF::new(0.0, 0.0, 72.0, 72.0),
      dirty: false,
   });
   api::RenderSnapshot::new(
      vec![instance],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("prepared RRect layer snapshot")
}

fn prepared_layer_snapshot(image: api::ImageHandle, atlas: api::ImageHandle, geometry: u64, structural: u64, resource: u64, dirty: bool, layer: bool, transform: Option<[f32; 6]>) -> api::RenderSnapshot
{
   prepared_layer_snapshot_content(
      image, atlas, geometry, structural, resource, dirty, layer, transform, true,
   )
}

fn prepared_layer_snapshot_content(image: api::ImageHandle, atlas: api::ImageHandle, geometry: u64, structural: u64, resource: u64, dirty: bool, layer: bool, transform: Option<[f32; 6]>, include_rrect: bool) -> api::RenderSnapshot
{
   let vertices = vec![
      api::Vertex { x: 8.0, y: 52.0, u: 0.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 28.0, y: 52.0, u: 1.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 28.0, y: 72.0, u: 1.0, v: 1.0, rgba: 0 },
      api::Vertex { x: 8.0, y: 72.0, u: 0.0, v: 1.0, rgba: 0 },
      api::Vertex { x: 38.0, y: 52.0, u: 0.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 58.0, y: 52.0, u: 1.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 58.0, y: 72.0, u: 1.0, v: 1.0, rgba: 0 },
      api::Vertex { x: 38.0, y: 72.0, u: 0.0, v: 1.0, rgba: 0 },
      api::Vertex { x: 68.0, y: 72.0, u: 0.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 88.0, y: 52.0, u: 0.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 88.0, y: 72.0, u: 0.0, v: 0.0, rgba: 0 },
   ];
   let mut items = Vec::with_capacity(5);
   if include_rrect
   {
      items.push(api::DrawCmd::RRect {
         rect: api::RectF::new(8.0, 8.0, 28.0, 28.0),
         radii: [5.0; 4],
         color: api::Color::rgba(0.9, 0.2, 0.1, 1.0),
      });
   }
   items.extend([
      api::DrawCmd::Image {
         tex: image,
         dst: api::RectF::new(48.0, 8.0, 28.0, 28.0),
         src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
         alpha: 1.0,
      },
      api::DrawCmd::GlyphRun { run: api::GlyphRun {
         atlas,
         atlas_revision: resource,
         vb: api::VertexSpan { offset: 0, len: 4 },
         ib: api::IndexSpan { offset: 0, len: 6 },
         sdf: false,
         color: api::Color::rgba(0.2, 0.8, 1.0, 1.0),
      }},
      api::DrawCmd::ImageMesh {
         tex: image,
         vb: api::VertexSpan { offset: 4, len: 4 },
         ib: api::IndexSpan { offset: 6, len: 6 },
         alpha: 1.0,
      },
      api::DrawCmd::Solid {
         vb: api::VertexSpan { offset: 8, len: 3 },
         ib: api::IndexSpan { offset: 12, len: 3 },
         color: api::Color::rgba(0.9, 0.8, 0.1, 1.0),
      },
   ]);
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(929),
      api::RenderChunkRevisions {
         structural,
         geometry,
         resource,
         dynamic_properties: 1,
      },
      api::DrawList {
         items,
         vertices,
         indices: vec![0, 1, 2, 0, 2, 3, 0, 1, 2, 0, 2, 3, 0, 1, 2],
      },
      api::ChunkIndexMode::Local,
      &[
         api::RenderResourceDependency { image, generation: resource },
         api::RenderResourceDependency { image: atlas, generation: resource },
      ],
   ).expect("prepared layer chunk");
   let mut instance = api::RenderChunkInstance::new(chunk, [20.0, 16.0]);
   if layer
   {
      instance.layer = Some(api::RenderLayerInstance {
         id: 77,
         rect: api::RectF::new(0.0, 0.0, 96.0, 80.0),
         dirty,
      });
   }
   let mut properties = Vec::new();
   if let Some(transform) = transform
   {
      let id = api::RenderPropertySlotId::dynamic(1, 1).unwrap();
      instance.property_slots = vec![id].into();
      properties.push(api::RenderPropertySlot {
         id,
         revision: 1,
         value: api::RenderPropertyValue::Transform(transform),
      });
   }
   api::RenderSnapshot::new(
      vec![instance],
      properties,
      api::Damage { rects: Vec::new() },
   ).expect("prepared layer snapshot")
}

fn prepared_image_snapshot(image: api::ImageHandle, generation: u64) -> api::RenderSnapshot
{
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(913),
      api::RenderChunkRevisions { resource: generation, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::Image {
            tex: image,
            dst: api::RectF::new(8.0, 8.0, 32.0, 32.0),
            src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
            alpha: 1.0,
         }],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image, generation }],
   ).expect("image chunk");
   api::RenderSnapshot::new(
      vec![api::RenderChunkInstance::new(chunk, [0.0, 0.0])],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("image snapshot")
}

fn two_glyph_page_snapshot(
   first_page: api::ImageHandle,
   second_page: api::ImageHandle,
) -> api::RenderSnapshot
{
   let chunk = |id, atlas, x| api::RenderChunk::new(
      api::RenderChunkId(id),
      api::RenderChunkRevisions { resource: 1, geometry: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::GlyphRun { run: api::GlyphRun {
            atlas,
            atlas_revision: 1,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: api::Color::rgba(0.3, 0.8, 1.0, 1.0),
         }}],
         vertices: vec![
            api::Vertex { x, y: 8.0, u: 0.0, v: 0.0, rgba: 0 },
            api::Vertex { x: x + 24.0, y: 8.0, u: 1.0, v: 0.0, rgba: 0 },
            api::Vertex { x, y: 32.0, u: 0.0, v: 1.0, rgba: 0 },
            api::Vertex { x: x + 24.0, y: 32.0, u: 1.0, v: 1.0, rgba: 0 },
         ],
         indices: vec![0, 1, 2, 2, 1, 3],
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image: atlas, generation: 1 }],
   ).expect("glyph page chunk");
   api::RenderSnapshot::new(
      vec![
         api::RenderChunkInstance::new(chunk(921, first_page, 8.0), [0.0, 0.0]),
         api::RenderChunkInstance::new(chunk(922, second_page, 48.0), [0.0, 0.0]),
      ],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("two glyph page snapshot")
}

fn prepared_image_mesh_snapshot(image: api::ImageHandle) -> api::RenderSnapshot
{
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(917),
      api::RenderChunkRevisions { resource: 1, geometry: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::ImageMesh {
            tex: image,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            alpha: 0.8,
         }],
         vertices: vec![
            api::Vertex { x: 8.0, y: 8.0, u: 0.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 40.0, y: 8.0, u: 1.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 40.0, y: 40.0, u: 1.0, v: 1.0, rgba: 0 },
            api::Vertex { x: 8.0, y: 40.0, u: 0.0, v: 1.0, rgba: 0 },
         ],
         indices: vec![0, 1, 2, 0, 2, 3],
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image, generation: 1 }],
   ).expect("image mesh chunk");
   let transform = api::RenderPropertySlotId::dynamic(1, 1).unwrap();
   let opacity = api::RenderPropertySlotId::dynamic(2, 1).unwrap();
   let mut instance = api::RenderChunkInstance::new(chunk, [12.0, 6.0]);
   instance.property_slots = vec![transform, opacity].into();
   api::RenderSnapshot::new(
      vec![instance],
      vec![
         api::RenderPropertySlot {
            id: transform,
            revision: 1,
            value: api::RenderPropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 3.0, 2.0]),
         },
         api::RenderPropertySlot {
            id: opacity,
            revision: 1,
            value: api::RenderPropertyValue::Opacity(0.7),
         },
      ],
      api::Damage { rects: Vec::new() },
   ).expect("image mesh snapshot")
}

fn prepared_spatial_snapshot(image: api::ImageHandle, atlas: api::ImageHandle, count: usize) -> api::RenderSnapshot
{
   let vertices = vec![
      api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 8.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 8.0, y: 10.0, u: 1.0, v: 1.0, rgba: 0 },
      api::Vertex { x: 0.0, y: 10.0, u: 0.0, v: 1.0, rgba: 0 },
   ];
   let indices = vec![0, 1, 2, 0, 2, 3];
   let mesh = api::RenderChunk::new(
      api::RenderChunkId(918),
      api::RenderChunkRevisions { resource: 1, geometry: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::ImageMesh {
            tex: image,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            alpha: 1.0,
         }],
         vertices: vertices.clone(),
         indices: indices.clone(),
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image, generation: 1 }],
   ).expect("spatial mesh chunk");
   let glyph = api::RenderChunk::new(
      api::RenderChunkId(919),
      api::RenderChunkRevisions { resource: 1, geometry: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::GlyphRun { run: api::GlyphRun {
            atlas,
            atlas_revision: 1,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: api::Color::rgba(0.3, 0.7, 1.0, 1.0),
         }}],
         vertices,
         indices,
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image: atlas, generation: 1 }],
   ).expect("spatial glyph chunk");
   let instances = (0..count).map(|index| {
      api::RenderChunkInstance::new(
         if index & 1 == 0 { mesh.clone() } else { glyph.clone() },
         [index as f32 * 20.0, 8.0],
      )
   }).collect();
   api::RenderSnapshot::new(
      instances,
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("spatial snapshot")
}

fn prepared_fractional_opaque_snapshot(image: api::ImageHandle) -> api::RenderSnapshot
{
   let mut rrects = api::DrawList::default();
   let mut images = api::DrawList::default();
   for index in 0..64_u32
   {
      let rect = api::RectF::new(index as f32 * 0.75, 0.0, 0.625, 28.0);
      rrects.items.push(api::DrawCmd::RRect {
         rect,
         radii: [0.25; 4],
         color: api::Color::rgba(0.2, 0.6, 0.9, 1.0),
      });
      images.items.push(api::DrawCmd::Image {
         tex: image,
         dst: rect,
         src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
         alpha: 1.0,
      });
   }
   let rrects = api::RenderChunk::new(
      api::RenderChunkId(915),
      api::RenderChunkRevisions::default(),
      rrects,
      api::ChunkIndexMode::Local,
      &[],
   ).expect("fractional rrect chunk");
   let images = api::RenderChunk::new(
      api::RenderChunkId(916),
      api::RenderChunkRevisions { resource: 1, ..api::RenderChunkRevisions::default() },
      images,
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image, generation: 1 }],
   ).expect("fractional image chunk");
   let mut instances = vec![
      api::RenderChunkInstance::new(rrects, [8.0, 8.0]),
      api::RenderChunkInstance::new(images, [8.0, 48.0]),
   ];
   for instance in &mut instances
   {
      instance.property_slots = vec![api::RenderPropertySlotId(1)].into();
   }
   api::RenderSnapshot::new(
      instances,
      vec![api::RenderPropertySlot {
         id: api::RenderPropertySlotId(1),
         revision: 1,
         value: api::RenderPropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 0.25, 0.0]),
      }],
      api::Damage { rects: Vec::new() },
   ).expect("fractional opaque snapshot")
}

fn prepared_rrect_snapshot(id: u64, x: f32) -> api::RenderSnapshot
{
   let chunk = api::RenderChunk::new(
      api::RenderChunkId(id),
      api::RenderChunkRevisions::default(),
      api::DrawList {
         items: vec![api::DrawCmd::RRect {
            rect: api::RectF::new(x, 8.0, 24.0, 24.0),
            radii: [4.0; 4],
            color: api::Color::rgba(0.2, 0.6, 0.9, 1.0),
         }],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[],
   ).expect("rrect chunk");
   api::RenderSnapshot::new(
      vec![api::RenderChunkInstance::new(chunk, [0.0, 0.0])],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("rrect snapshot")
}

fn prepared_mixed_snapshot(image: api::ImageHandle, atlas: api::ImageHandle, geometry_revision: u64) -> api::RenderSnapshot
{
   prepared_mixed_snapshot_with_properties(
      image,
      atlas,
      geometry_revision,
      [1.0, 0.0, 0.0, 1.0, 2.0, 1.0],
      0.75,
   )
}

fn prepared_mixed_snapshot_with_properties(image: api::ImageHandle, atlas: api::ImageHandle, geometry_revision: u64, transform: [f32; 6], opacity: f32) -> api::RenderSnapshot
{
   let rrect = api::RenderChunk::new(
      api::RenderChunkId(901),
      api::RenderChunkRevisions { geometry: geometry_revision, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![
            api::DrawCmd::ClipPush { rect: api::RectI::new(0, 0, 46, 40) },
            api::DrawCmd::RRect {
               rect: api::RectF::new(0.0, 0.0, 52.0, 34.0),
               radii: [6.0; 4],
               color: api::Color::rgba(0.9, 0.2, 0.1, 1.0),
            },
            api::DrawCmd::ClipPop,
         ],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[],
   ).expect("rrect chunk");
   let image_chunk = api::RenderChunk::new(
      api::RenderChunkId(902),
      api::RenderChunkRevisions { resource: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::Image {
            tex: image,
            dst: api::RectF::new(50.0, 4.0, 32.0, 32.0),
            src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
            alpha: 0.8,
         }],
         ..api::DrawList::default()
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image, generation: 1 }],
   ).expect("image chunk");
   let glyph_chunk = api::RenderChunk::new(
      api::RenderChunkId(903),
      api::RenderChunkRevisions { resource: 1, geometry: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::GlyphRun { run: api::GlyphRun {
            atlas,
            atlas_revision: 1,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: api::Color::rgba(0.2, 0.8, 1.0, 1.0),
         }}],
         vertices: vec![
            api::Vertex { x: 6.0, y: 50.0, u: 0.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 38.0, y: 50.0, u: 1.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 38.0, y: 82.0, u: 1.0, v: 1.0, rgba: 0 },
            api::Vertex { x: 6.0, y: 82.0, u: 0.0, v: 1.0, rgba: 0 },
         ],
         indices: vec![0, 1, 2, 0, 2, 3],
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image: atlas, generation: 1 }],
   ).expect("glyph chunk");
   let solid_chunk = api::RenderChunk::new(
      api::RenderChunkId(904),
      api::RenderChunkRevisions { geometry: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::Solid {
            vb: api::VertexSpan { offset: 0, len: 3 },
            ib: api::IndexSpan { offset: 0, len: 3 },
            color: api::Color::rgba(0.9, 0.8, 0.1, 1.0),
         }],
         vertices: vec![
            api::Vertex { x: 88.0, y: 50.0, u: 0.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 118.0, y: 82.0, u: 0.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 82.0, y: 82.0, u: 0.0, v: 0.0, rgba: 0 },
         ],
         indices: vec![0, 1, 2],
      },
      api::ChunkIndexMode::Local,
      &[],
   ).expect("solid chunk");
   let mut instances = vec![
      api::RenderChunkInstance::new(rrect, [4.0, 3.0]),
      api::RenderChunkInstance::new(image_chunk, [0.0, 0.0]),
      api::RenderChunkInstance::new(glyph_chunk, [0.0, 0.0]),
      api::RenderChunkInstance::new(solid_chunk, [0.0, 0.0]),
   ];
   for instance in &mut instances
   {
      instance.property_slots = vec![api::RenderPropertySlotId(1), api::RenderPropertySlotId(2)].into();
   }
   api::RenderSnapshot::new(
      instances,
      vec![
         api::RenderPropertySlot {
            id: api::RenderPropertySlotId(1),
            revision: u64::from(transform != [1.0, 0.0, 0.0, 1.0, 2.0, 1.0]),
            value: api::RenderPropertyValue::Transform(transform),
         },
         api::RenderPropertySlot {
            id: api::RenderPropertySlotId(2),
            revision: u64::from(opacity != 0.75),
            value: api::RenderPropertyValue::Opacity(opacity),
         },
      ],
      api::Damage { rects: Vec::new() },
   ).expect("prepared snapshot")
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
      assert_eq!(stats.draws, 1, "marker draw count changed: {stats:?}");
      assert_eq!(stats.instanced, count as u32, "marker instance count changed: {stats:?}");
      assert_eq!(stats.ub_bytes, (count * 72) as u64, "marker upload bytes changed: {stats:?}");
      assert_eq!(stats.resource_grows, 0, "warm marker ring grew: {stats:?}");
   }
}

#[test]
fn snapshot_neon_marker_batches_keep_nonoverlapping_ring_slices()
{
   use oxide_renderer_metal::neon_marker::{NeonMarker, NeonMarkerPass};

   let mut renderer = MetalRenderer::new_default().expect("metal");
   let size = 260_u32;
   renderer.resize(size, size, 1.0).expect("resize");
   let colors = [
      (api::Color::rgba(1.0, 0.0, 0.0, 1.0), [0, 0, 252, 249]),
      (api::Color::rgba(0.0, 1.0, 0.0, 1.0), [0, 252, 0, 249]),
      (api::Color::rgba(0.0, 0.0, 1.0, 1.0), [252, 0, 0, 249]),
      (api::Color::rgba(1.0, 1.0, 0.0, 1.0), [0, 252, 252, 249]),
      (api::Color::rgba(1.0, 0.0, 1.0, 1.0), [252, 0, 252, 249]),
      (api::Color::rgba(0.0, 1.0, 1.0, 1.0), [252, 252, 0, 249]),
      (api::Color::rgba(1.0, 1.0, 1.0, 1.0), [252, 252, 252, 249]),
      (api::Color::rgba(1.0, 0.0, 0.0, 1.0), [0, 0, 252, 249]),
   ];
   let markers = (0..1_024_usize)
      .map(|index| {
         let color = colors[index / 128].0;
         NeonMarker {
            center: [4.0 + (index % 32) as f32 * 8.0, 4.0 + (index / 32) as f32 * 8.0],
            core_radius_px: 2.5,
            ring_radius_px: 3.0,
            ring_width_px: 0.5,
            halo_radius_px: 3.5,
            halo_sigma_px: 2.0,
            core_color: color,
            ring_color: color,
            halo_alpha_max: 0.0,
            ring_alpha_max: 1.0,
         }
      })
      .collect::<Vec<_>>();

   let token = renderer.begin_frame(&api::FrameTarget, None);
   for markers in markers.chunks(128)
   {
      renderer
         .encode_neon_markers(&NeonMarkerPass {
            viewport: api::RectF::new(0.0, 0.0, size as f32, size as f32),
            markers,
         })
         .expect("encode neon marker batch");
   }
   renderer.submit(token).expect("submit");
   let (_, _, pixels) = renderer.readback_bgra8().expect("readback");
   for (index, marker) in markers.iter().enumerate()
   {
      assert_pixel_eq(
         readback_pixel(&pixels, size, marker.center[0] as u32, marker.center[1] as u32),
         colors[index / 128].1,
         &format!("marker batch {}, instance {}", index / 128, index % 128),
      );
   }
   let stats = renderer.last_stats();
   assert_eq!(stats.draws, 8, "expected one draw per marker batch: {stats:?}");
   assert_eq!(stats.instanced, 1_024, "marker instance count changed: {stats:?}");
   assert_eq!(stats.ub_bytes, 1_024 * 72, "marker ring bytes changed: {stats:?}");
   assert_eq!(stats.resource_grows, 0, "marker ring grew: {stats:?}");
}
