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
        renderer_source
            .contains("id_mask_vertex_caches: alloc::vec::Vec<IdMaskVertexUploadCache>")
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
