use oxide_renderer_metal::id_mask_compositor::{
    IdMaskGpuRasterPass, IdMaskRasterChunk, IdMaskRasterProjection, IdMaskRasterVertex,
    SemanticMaskRegionStyle, SEMANTIC_MASK_MAX_REGION_STYLES, SEMANTIC_MASK_MAX_SUBREGION_COLORS,
};

#[test]
fn id_mask_gpu_raster_rejects_empty_or_non_triangle_vertices() {
    let empty = IdMaskGpuRasterPass {
        viewport: oxide_renderer_api::RectF::new(0.0, 0.0, 10.0, 10.0),
        mask_width: 8,
        mask_height: 8,
        mask_scale: 1.0,
        vertex_revision: 0,
        vertices: &[],
        chunks: &[],
        projection: IdMaskRasterProjection::screen_px(),
    };
    assert!(!empty.valid_triangle_vertex_count());

    let vertices = [IdMaskRasterVertex::new([0.0, 0.0], 1, 2)];
    let partial_chunk = [IdMaskRasterChunk { content_hash: 1, first_vertex: 0, vertex_count: 1 }];
    let partial = IdMaskGpuRasterPass { vertices: &vertices, chunks: &partial_chunk, ..empty };
    assert!(!partial.valid_triangle_vertex_count());
}

#[test]
fn id_mask_gpu_raster_accepts_triangle_vertices_and_generic_style_alias() {
    let vertices = [
        IdMaskRasterVertex::new([0.0, 0.0], 1, 2),
        IdMaskRasterVertex::new([4.0, 0.0], 1, 2),
        IdMaskRasterVertex::new([0.0, 4.0], 1, 2),
    ];
    let chunks = [IdMaskRasterChunk { content_hash: 7, first_vertex: 0, vertex_count: 3 }];
    let pass = IdMaskGpuRasterPass {
        viewport: oxide_renderer_api::RectF::new(0.0, 0.0, 10.0, 10.0),
        mask_width: 8,
        mask_height: 8,
        mask_scale: 1.0,
        vertex_revision: 7,
        vertices: &vertices,
        chunks: &chunks,
        projection: IdMaskRasterProjection::screen_px(),
    };

    let style = SemanticMaskRegionStyle::default();
    assert!(pass.valid_triangle_vertex_count());
    assert_eq!(pass.vertex_count(), 3);
    assert_eq!(SEMANTIC_MASK_MAX_REGION_STYLES, 4);
    assert_eq!(SEMANTIC_MASK_MAX_SUBREGION_COLORS, 32);
    assert_eq!(style.fill_rgb, [1.0, 1.0, 1.0]);
}

#[test]
fn id_mask_gpu_upload_cache_is_content_hash_chunk_keyed() {
    let renderer_source = include_str!("../src/lib.rs");
    let gpu_source = include_str!("../src/id_mask_gpu.rs");
    assert!(
        renderer_source.contains("id_mask_vertex_caches: alloc::vec::Vec<IdMaskVertexUploadCache>")
            && renderer_source.contains("struct IdMaskVertexUploadKey")
            && renderer_source.contains("content_hash: u64")
            && renderer_source.contains("byte_len: usize"),
        "Metal id-mask raster upload cache must be keyed by stable content hash plus byte size"
    );
    assert!(
        gpu_source.contains("fn id_mask_vertex_cache_index")
            && gpu_source.contains("chunk.content_hash")
            && gpu_source.contains("self.id_mask_vertex_caches.push(IdMaskVertexUploadCache"),
        "Metal id-mask raster chunks should stay in content-hash keyed GPU buffers"
    );
}

#[cfg(all(target_os = "macos", feature = "snapshot-tests"))]
#[test]
fn id_mask_field_cache_hits_final_only_changes_and_evicts_complete_keys() {
    use oxide_renderer_api::{self as api, Renderer};
    use oxide_renderer_metal::id_mask_compositor::{
        IdMaskCityStyle, IdMaskCompositorMode, IdMaskGpuCompositorPass, IdMaskPolishConfig,
        ID_MASK_MAX_CITY_STYLES, ID_MASK_MAX_NEIGHBORHOOD_COLORS,
    };
    use oxide_renderer_metal::MetalRenderer;

    fn render(
        renderer: &mut MetalRenderer,
        vertices: &[IdMaskRasterVertex],
        chunks: &[IdMaskRasterChunk],
        revision: u64,
        projection_x: f32,
        final_only_variant: bool,
        mask_size: usize,
        mask_scale: f32,
        second_revision: Option<u64>,
    ) -> Vec<u8> {
        let mut projection = IdMaskRasterProjection::world_3d([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [projection_x, 0.0, 0.0, 1.0],
        ]);
        projection.model_to_world[0][0] = 1.0;
        let mut city_styles = [IdMaskCityStyle::default(); ID_MASK_MAX_CITY_STYLES];
        let mut neighborhood_colors = [[0.0; 3]; ID_MASK_MAX_NEIGHBORHOOD_COLORS];
        let mut polish = IdMaskPolishConfig::default();
        if final_only_variant {
            city_styles[1].fill_rgb = [0.25, 0.75, 0.35];
            neighborhood_colors[2] = [0.8, 0.2, 0.6];
            polish.smooth_radius_px = 1.25;
        }
        let pass = IdMaskGpuCompositorPass {
            raster: IdMaskGpuRasterPass {
                viewport: api::RectF::new(if final_only_variant { 1.0 } else { 0.0 }, 0.0, 64.0, 64.0),
                mask_width: mask_size,
                mask_height: mask_size,
                mask_scale,
                vertex_revision: revision,
                vertices,
                chunks,
                projection,
            },
            city_styles,
            neighborhood_colors,
            mode: if final_only_variant { IdMaskCompositorMode::CityIdMask } else { IdMaskCompositorMode::Beauty },
            glow_enabled: final_only_variant,
            darken_background_alpha: if final_only_variant { 0.35 } else { 0.0 },
            polish,
        };
        let token = renderer.begin_frame(&api::FrameTarget, None);
        renderer.encode_id_mask_gpu_compositor(&pass).expect("encode ID-mask frame");
        if let Some(revision) = second_revision {
            let mut second = pass;
            second.raster.vertex_revision = revision;
            renderer.encode_id_mask_gpu_compositor(&second).expect("encode second ID-mask frame");
        }
        renderer.submit(token).expect("submit ID-mask frame");
        renderer.readback_bgra8().expect("drain ID-mask frame").2
    }

    let vertices = [
        IdMaskRasterVertex::new_world([-1.0, -1.0, 0.0], 1, 2),
        IdMaskRasterVertex::new_world([1.0, -1.0, 0.0], 1, 2),
        IdMaskRasterVertex::new_world([-1.0, 1.0, 0.0], 1, 2),
        IdMaskRasterVertex::new_world([-1.0, 1.0, 0.0], 1, 2),
        IdMaskRasterVertex::new_world([1.0, -1.0, 0.0], 1, 2),
        IdMaskRasterVertex::new_world([1.0, 1.0, 0.0], 1, 2),
    ];
    let chunks = [IdMaskRasterChunk { content_hash: 0xabc, first_vertex: 0, vertex_count: 6 }];
    let mut renderer = MetalRenderer::new_default().expect("create Metal renderer");
    renderer.resize(64, 64, 1.0).expect("resize Metal renderer");

    render(&mut renderer, &vertices, &chunks, 1, 0.0, false, 64, 1.0, None);
    let cold = renderer.last_stats();
    assert_eq!((cold.id_mask_cache_hits, cold.id_mask_cache_misses), (0, 1));
    assert_eq!(cold.id_mask_raster_passes, 1);
    assert_eq!(cold.id_mask_field_seed_passes, 1);
    assert_eq!(cold.id_mask_field_jump_passes, 6);
    assert_eq!(cold.id_mask_compositor_passes, 1);
    assert_eq!(cold.render_passes, 9);
    assert_eq!(cold.id_mask_cache_entries, 1);
    assert!(cold.id_mask_cache_resident_bytes > 0);
    assert_eq!(cold.id_mask_target_creates, 1);
    assert_eq!(cold.id_mask_in_flight_generations, 1);
    assert_eq!(cold.id_mask_in_flight_target_bytes, cold.id_mask_cache_resident_bytes);
    assert_eq!(cold.id_mask_target_storage_bytes, cold.id_mask_cache_resident_bytes);

    render(&mut renderer, &vertices, &chunks, 1, 0.0, true, 64, 1.0, None);
    let final_only = renderer.last_stats();
    assert_eq!((final_only.id_mask_cache_hits, final_only.id_mask_cache_misses), (1, 0));
    assert_eq!(final_only.id_mask_raster_passes, 0);
    assert_eq!(final_only.id_mask_field_seed_passes, 0);
    assert_eq!(final_only.id_mask_field_jump_passes, 0);
    assert_eq!(final_only.id_mask_compositor_passes, 1);
    assert_eq!(final_only.render_passes, 1);
    assert_eq!(final_only.id_mask_target_creates, 0);

    render(&mut renderer, &vertices, &chunks, 1, 0.01, false, 64, 1.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_misses, 1);
    render(&mut renderer, &vertices, &chunks, 2, 0.0, false, 64, 1.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_misses, 1);
    render(&mut renderer, &vertices, &chunks, 1, 0.0, false, 64, 1.0, Some(2));
    assert_eq!(renderer.last_stats().id_mask_cache_hits, 2);
    assert_eq!(renderer.last_stats().id_mask_compositor_passes, 2);
    assert_eq!(renderer.last_stats().render_passes, 2);
    render(&mut renderer, &vertices, &chunks, 1, 0.0, false, 64, 1.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_hits, 1);
    assert_eq!(renderer.last_stats().id_mask_cache_entries, 3);

    let changed_chunks = [IdMaskRasterChunk { content_hash: 0xdef, ..chunks[0] }];
    render(&mut renderer, &vertices, &changed_chunks, 1, 0.0, false, 64, 1.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_misses, 1);
    assert_eq!(renderer.last_stats().id_mask_cache_entries, 4);

    let one_entry_budget = renderer.last_stats().id_mask_cache_resident_bytes / 4;
    renderer.set_id_mask_cache_budget_bytes(one_entry_budget);
    assert_eq!(renderer.last_stats().id_mask_cache_entries, 1);
    render(&mut renderer, &vertices, &chunks, 3, 0.0, false, 64, 1.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_misses, 1);
    assert!(renderer.last_stats().id_mask_cache_evictions >= 4);
    assert!(renderer.last_stats().id_mask_cache_resident_bytes
        <= renderer.last_stats().id_mask_cache_budget_bytes);
    render(&mut renderer, &vertices, &chunks, 3, 0.0, false, 64, 2.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_misses, 1);
    render(&mut renderer, &vertices, &chunks, 3, 0.0, false, 32, 2.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_misses, 1);
    render(&mut renderer, &vertices, &chunks, 1, 0.0, false, 64, 1.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_misses, 1);
    renderer.set_id_mask_cache_budget_bytes(0);
    render(&mut renderer, &vertices, &chunks, 1, 0.0, false, 64, 1.0, None);
    assert_eq!(renderer.last_stats().id_mask_cache_entries, 0);
    assert_eq!(renderer.last_stats().id_mask_cache_resident_bytes, 0);
    assert_eq!(renderer.last_stats().id_mask_target_creates, 0);
    assert!(renderer.last_stats().id_mask_in_flight_target_bytes > 0);
    assert_eq!(
        renderer.last_stats().id_mask_target_storage_bytes,
        renderer.last_stats().id_mask_in_flight_target_bytes,
    );
    render(&mut renderer, &vertices, &chunks, 2, 0.0, false, 64, 1.0, None);
    assert_eq!(renderer.last_stats().id_mask_target_creates, 0);
    renderer.purge_id_mask_field_cache();
    assert_eq!(renderer.last_stats().id_mask_cache_entries, 0);
    assert_eq!(renderer.last_stats().id_mask_cache_resident_bytes, 0);

    let mut bounded = MetalRenderer::new_default().expect("create bounded Metal renderer");
    bounded.resize(64, 64, 1.0).expect("resize bounded Metal renderer");
    let baseline_pixels = render(
        &mut bounded,
        &vertices,
        &chunks,
        10,
        0.0,
        false,
        64,
        1.0,
        None,
    );
    let generation_bytes = bounded.last_stats().id_mask_cache_resident_bytes;
    bounded.set_id_mask_cache_budget_bytes(generation_bytes);
    let busy_slot = bounded.current_frame_slot_for_snapshot();
    bounded.mark_frame_slot_busy_for_snapshot(busy_slot);
    let busy_pixels = render(
        &mut bounded,
        &vertices,
        &chunks,
        11,
        0.0,
        false,
        64,
        1.0,
        None,
    );
    let busy = bounded.last_stats();
    assert_eq!(busy.id_mask_target_creates, 1, "busy target was overwritten: {busy:?}");
    assert_eq!(busy.id_mask_target_reuse_blocked, 1);
    assert_eq!(busy.id_mask_in_flight_generations, 2);
    assert_eq!(busy.id_mask_target_storage_bytes, generation_bytes.saturating_mul(2));
    assert_eq!(busy.id_mask_target_peak_bytes, generation_bytes.saturating_mul(2));
    assert_eq!(busy.frame_backpressure_skipped, 0);
    assert_eq!(busy_pixels, baseline_pixels);

    bounded.release_frame_slot_for_snapshot(busy_slot);
    let reusable_pixels = render(
        &mut bounded,
        &vertices,
        &chunks,
        12,
        0.0,
        false,
        64,
        1.0,
        None,
    );
    let reusable = bounded.last_stats();
    assert_eq!(reusable.id_mask_target_creates, 0, "completed target was not recycled: {reusable:?}");
    assert_eq!(reusable.id_mask_target_reuse_blocked, 1);
    assert_eq!(reusable.id_mask_target_storage_bytes, generation_bytes);
    assert_eq!(reusable_pixels, baseline_pixels);
}
