use oxide_renderer_metal::id_mask_compositor::{
    IdMaskGpuRasterPass, IdMaskRasterVertex, SemanticMaskRegionStyle,
    SEMANTIC_MASK_MAX_REGION_STYLES, SEMANTIC_MASK_MAX_SUBREGION_COLORS,
};

#[test]
fn id_mask_gpu_raster_rejects_empty_or_non_triangle_vertices() {
    let empty = IdMaskGpuRasterPass {
        viewport: oxide_renderer_api::RectF::new(0.0, 0.0, 10.0, 10.0),
        mask_width: 8,
        mask_height: 8,
        mask_scale: 1.0,
        vertices: &[],
    };
    assert!(!empty.valid_triangle_vertex_count());

    let vertices = [IdMaskRasterVertex::new([0.0, 0.0], 1, 2)];
    let partial = IdMaskGpuRasterPass { vertices: &vertices, ..empty };
    assert!(!partial.valid_triangle_vertex_count());
}

#[test]
fn id_mask_gpu_raster_accepts_triangle_vertices_and_generic_style_alias() {
    let vertices = [
        IdMaskRasterVertex::new([0.0, 0.0], 1, 2),
        IdMaskRasterVertex::new([4.0, 0.0], 1, 2),
        IdMaskRasterVertex::new([0.0, 4.0], 1, 2),
    ];
    let pass = IdMaskGpuRasterPass {
        viewport: oxide_renderer_api::RectF::new(0.0, 0.0, 10.0, 10.0),
        mask_width: 8,
        mask_height: 8,
        mask_scale: 1.0,
        vertices: &vertices,
    };

    let style = SemanticMaskRegionStyle::default();
    assert!(pass.valid_triangle_vertex_count());
    assert_eq!(pass.vertex_count(), 3);
    assert_eq!(SEMANTIC_MASK_MAX_REGION_STYLES, 4);
    assert_eq!(SEMANTIC_MASK_MAX_SUBREGION_COLORS, 32);
    assert_eq!(style.fill_rgb, [1.0, 1.0, 1.0]);
}
