use oxide_renderer_api as api;
use oxide_renderer_api::Renderer;
use oxide_renderer_web::{
    color_cache_key, color_to_css, layer_physical_dimension, packed_rgba_to_css, sanitize_scale,
    WebGpuTimestampSample, WebRenderer,
};

#[path = "../src/solid_color.rs"]
mod solid_color;

fn source_without_whitespace(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn source_block<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_idx = source.find(start).expect("source block start");
    let tail = &source[start_idx..];
    let end_idx = tail.find(end).expect("source block end");
    &tail[..end_idx]
}

fn compact_source_block(source: &str, start: &str, end: &str) -> String {
    source_without_whitespace(source_block(source, start, end))
}

#[test]
fn color_conversion_clamps_channels() {
    let css = color_to_css(api::Color::rgba(1.4, -0.2, 0.5, 2.0));
    assert_eq!(css, "rgba(255, 0, 128, 1.000)");
    assert_eq!(packed_rgba_to_css(0x8040_2010), "rgba(16, 32, 64, 0.502)");
    assert_eq!(color_cache_key(api::Color::rgba(1.0, 0.0, 0.5, 0.25)), 0x4080_00FF);
}

#[test]
fn sanitize_scale_rejects_invalid_values() {
    assert_eq!(sanitize_scale(2.0), 2.0);
    assert_eq!(sanitize_scale(0.0), 1.0);
    assert_eq!(sanitize_scale(f32::NAN), 1.0);
}

#[test]
fn layer_physical_dimension_is_bounded_and_positive() {
    assert_eq!(layer_physical_dimension(12.25, 2.0), 25);
    assert_eq!(layer_physical_dimension(0.0, 2.0), 1);
    assert_eq!(layer_physical_dimension(f32::NAN, 2.0), 1);
    assert_eq!(layer_physical_dimension(100_000.0, 2.0), 16_384);
}

#[test]
fn native_stub_tracks_frame_shape_and_reports_unsupported_submit() {
    let mut renderer = WebRenderer::new_for_tests(100, 50, 2.0);
    let damage = api::Damage { rects: vec![api::RectI::new(0, 0, 10, 10)] };
    let token = renderer.begin_frame(&api::FrameTarget, Some(&damage));
    renderer.encode_pass(&api::DrawList::default());
    let stats = renderer.last_stats();
    assert_eq!(stats.frame_id, 1);
    assert_eq!(stats.width, 100);
    assert_eq!(stats.height, 50);
    assert_eq!(stats.scale, 2.0);
    assert_eq!(stats.damage_rects, 1);
    assert!(matches!(renderer.submit(token), Err(api::RenderError::Unsupported(_))));
}

#[test]
fn native_stub_ignores_web_camera_background_commands() {
    let mut renderer = WebRenderer::new_for_tests(100, 50, 1.0);
    let mut list = api::DrawList::default();
    list.items.push(api::DrawCmd::CameraBg {
        rect: api::RectF::new(0.0, 0.0, 100.0, 50.0),
        tint: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
        alpha: 1.0,
        grayscale: false,
        blur: false,
        sigma: 0.0,
    });

    renderer.begin_frame(&api::FrameTarget, None);
    renderer.encode_pass(&list);
    let stats = renderer.last_stats();

    assert_eq!(stats.draws, 0);
    assert_eq!(stats.camera_bg_draws, 0);
}

#[test]
fn web_renderer_has_no_topomap_specific_command_hook() {
    let api_source = include_str!("../../renderer-api/src/lib.rs");
    let web_source = include_str!("../src/lib.rs");
    let webgpu_source = include_str!("../src/wasm/webgpu.rs");

    for source in [api_source, web_source, webgpu_source] {
        assert!(!source.contains("TopomapGlobe"));
        assert!(!source.contains("TopomapGlobeWebApp"));
        assert!(!source.contains("topomap_globe"));
        assert!(!source.contains("topomap_app_"));
        assert!(!source.contains("draw_topomap_globe"));
        assert!(!source.contains("DrawCmd::TopomapGlobe"));
    }
}

#[test]
fn webgpu_surface_config_uses_premultiplied_alpha() {
    let webgpu_source = include_str!("../src/wasm/webgpu.rs");

    assert!(webgpu_source.contains("config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;"));
}

#[test]
fn wasm_webgpu_runtime_images_are_explicitly_reclaimable_without_arena_tombstones()
{
   let source = include_str!("../src/wasm/webgpu.rs");
   let compact = source_without_whitespace(source);
   let slots = source_without_whitespace(include_str!("../src/wasm/image_slots.rs"));

   assert_eq!(
      source
         .matches("pub fn image_release(&mut self, handle: api::ImageHandle) -> bool")
         .count(),
      2
   );
   assert!(compact.contains("self.inner.image_release(handle)"));
   assert!(compact.contains("images:ImageSlots<GpuImage>"));
   assert!(compact.contains("self.images.remove(handle.0).is_some()"));
   assert!(compact.contains("self.images.get(handle.0)"));
   assert!(slots.contains("slots:Vec<Slot<T>>"));
   assert!(slots.contains("free:Vec<u16>"));
   assert!(slots.contains("(encoded_slot!=0&&generation!=0).then(||"));
   assert!(!slots.contains("(encoded_slot!=0&&generation!=0).then_some("));
   assert!(slots.contains("filter(|slot|slot.generation==generation)"));
   assert!(slots.contains("ifletSome(next_generation)=slot.generation.checked_add(1)"));
   assert!(!compact.contains("Vec<Option<GpuImage>>"));
   assert!(!compact.contains("self.images.push(Some(image))"));
}

#[test]
fn wasm_public_exports_are_webgpu_only() {
    let source = include_str!("../src/lib.rs");
    assert!(source.contains("pub use wasm::{bench_canvas_indexed_quads, BrowserRenderer, WebGpuRenderer};"));
    assert!(!source.contains("pub use wasm::{BrowserRenderer, WebGpuRenderer, WebRenderer};"));
    assert!(source.contains("pub fn bench_canvas_indexed_quads("));
    assert!(source.contains("fn canvas_indexed_quad_draw_list"));
    assert!(source.contains("expected_image_meshes=1"));
    assert!(source.contains("expected_image_draws={quad_count}"));
}

#[test]
fn wasm_webgpu_submits_directly_to_surface_without_backdrop_effects() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let renderer_impl =
        source.split("impl api::Renderer for WebGpuRenderer").nth(1).expect("webgpu renderer impl");
    let submit = renderer_impl.split("fn resize(&mut self").next().expect("webgpu submit body");

    assert!(submit.contains("self.render_layer_passes(&mut encoder);"));
    let compact_submit = source_without_whitespace(submit);
    assert!(compact_submit.contains(
        "ifself.target_uses_backdrop(None,0,self.frame.draws.len())||!self.direct_surface_enabled"
    ));
    assert!(submit.contains("self.render_scene_with_effects(&mut encoder);"));
    assert!(submit.contains("self.render_present(&mut encoder, &surface_view);"));
    assert!(submit.contains("self.render_direct(&mut encoder, &surface_view);"));
    assert!(source.contains("direct_surface_enabled: bool"));
    assert!(source.contains("direct_surface_enabled: true"));
    assert!(source.contains("pub fn set_direct_surface_enabled_for_benchmark"));
}

#[test]
fn wasm_webgpu_present_quad_uploads_are_cached_across_frames() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let cache = source
        .split("fn ensure_present_buffers")
        .nth(1)
        .expect("present buffer cache helper")
        .split("fn render_present")
        .next()
        .expect("present buffer cache body");
    let present = source
        .split("fn render_present")
        .nth(1)
        .expect("present pass body")
        .split("fn canvas_by_id")
        .next()
        .expect("present pass end");

    assert!(cache.contains("self.present_width == self.width"));
    assert!(cache.contains("self.queue.write_buffer(&self.present_vertex_buffer"));
    assert!(cache.contains("self.queue.write_buffer(&self.present_index_buffer"));
    assert!(present.contains("self.ensure_present_buffers();"));
    assert!(!present.contains("queue.write_buffer"));
}

#[test]
fn wasm_webgpu_timestamp_samples_are_bounded_and_drainable()
{
   let source = include_str!("../src/wasm/webgpu.rs");
   let sample = WebGpuTimestampSample::default();

   assert_eq!(sample.frame_id, 0);
   assert_eq!(sample.total_ns, 0);
   assert!(source.contains("const TIMESTAMP_COMPLETED_CAPACITY: usize = 4_096;"));
   assert!(source.contains("completed: Option<Box<VecDeque<WebGpuTimestampSample>>>"));
   assert!(source.contains("VecDeque::with_capacity(TIMESTAMP_COMPLETED_CAPACITY)"));
   assert!(source.contains("if completed.len() == TIMESTAMP_COMPLETED_CAPACITY"));
   assert!(source.contains("completed.pop_front();"));
   assert!(source.contains("completed.push_back(summary.sample());"));
   assert!(source.contains("set_timestamp_readback_interval_for_benchmark"));
   assert!(source.contains("drain_completed_timestamp_samples_into"));
   assert!(source.contains("cpu_submit_timing_enabled"));
   assert!(source.contains("set_cpu_submit_timing_enabled_for_benchmark"));
   assert!(source.contains("cpu_submit_timing_end(&mut self.cpu_submit_timing.upload_ms"));
   assert!(source.contains("cpu_submit_timing_end(&mut self.cpu_submit_timing.command_encoding_ms"));
   assert!(source.contains("cpu_submit_timing_end(&mut self.cpu_submit_timing.queue_submit_ms"));
}

#[test]
fn wasm_webgpu_unindexed_quad_vertices_emit_two_triangles() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let helper = source
        .split("fn append_gpu_vertices")
        .nth(1)
        .expect("append helper")
        .split("fn logical_dimension")
        .next()
        .expect("append helper end");

    assert!(helper.contains("vertices.len() == 4"));
    assert!(helper.contains("[base, base + 1, base + 2, base + 2, base + 1, base + 3]"));
}

#[test]
fn wasm_webgpu_solid_vertex_colors_decode_aabbggrr_and_interpolate()
{
   let source = include_str!("../src/wasm/webgpu.rs");
   let solid = compact_source_block(source, "fn encode_solid(", "fn encode_image(");
   let shader = compact_source_block(source, "struct VertexIn", "@fragment\nfn fs_rgba");

   assert!(solid.contains("vertex.rgba,color"));
   assert!(solid.matches("color,true").count() >= 3);
   assert!(shader.contains("out.color=input.color"));
   assert!(shader.contains("fnfs_solid(input:VertexOut)->@location(0)vec4<f32>{returninput.color;"));

   let uniform = api::Color::rgba(0.25, 0.5, 0.75, 1.0);
   assert_eq!(solid_color::resolve_vertex_color(0, uniform), uniform);
   assert_eq!(
      solid_color::resolve_vertex_color(0x8040_2010, uniform),
      api::Color::rgba(16.0 / 255.0, 32.0 / 255.0, 64.0 / 255.0, 128.0 / 255.0)
   );
}

#[test]
fn canvas_colored_solids_accept_only_six_vertex_axis_aligned_edge_gradients()
{
   let source = include_str!("../src/lib.rs");
   let draw = compact_source_block(source, "fn draw_solid_span(", "fn fill_triangle(");

   assert!(draw.contains("ifib.len!=0{return;"));
   assert!(draw.contains("colored_quad(vertices,color)"));
   assert!(draw.contains("create_linear_gradient"));
   assert!(draw.contains("fill_rect"));
   assert!(draw.contains("ifvertices.iter().any(|vertex|vertex.rgba!=0)"));
   assert!(draw.contains("letcss=color_to_css(color)"));
}

fn colored_quad_vertices(colors: [u32; 4]) -> [api::Vertex; 6]
{
   let vertex = |x, y, rgba| api::Vertex { x, y, u: 0.0, v: 0.0, rgba };
   [
      vertex(2.0, 3.0, colors[0]),
      vertex(12.0, 3.0, colors[1]),
      vertex(2.0, 9.0, colors[2]),
      vertex(2.0, 9.0, colors[2]),
      vertex(12.0, 3.0, colors[1]),
      vertex(12.0, 9.0, colors[3]),
   ]
}

#[test]
fn canvas_colored_quad_classifies_flat_and_opposing_edge_colors()
{
   let uniform = api::Color::rgba(0.25, 0.5, 0.75, 1.0);
   let flat = solid_color::colored_quad(&colored_quad_vertices([0xFF00_00FF; 4]), uniform)
      .expect("flat colored quad");
   assert_eq!(flat.3, 0xFF00_00FF);
   assert_eq!(flat.4, flat.3);

   let horizontal = solid_color::colored_quad(
      &colored_quad_vertices([0xFF00_00FF, 0xFFFF_0000, 0xFF00_00FF, 0xFFFF_0000]),
      uniform,
   )
   .expect("horizontal edge gradient");
   assert_eq!(horizontal.1, [2.0, 3.0]);
   assert_eq!(horizontal.2, [12.0, 3.0]);

   let vertical = solid_color::colored_quad(
      &colored_quad_vertices([0xFF00_00FF, 0xFF00_00FF, 0xFFFF_0000, 0xFFFF_0000]),
      uniform,
   )
   .expect("vertical edge gradient");
   assert_eq!(vertical.1, [2.0, 3.0]);
   assert_eq!(vertical.2, [2.0, 9.0]);

   let inherited = solid_color::colored_quad(&colored_quad_vertices([0; 4]), uniform)
      .expect("uniform inherited quad");
   assert_eq!(inherited.3, uniform.pack_rgba8());
   assert_eq!(inherited.4, inherited.3);
}

#[test]
fn canvas_colored_quad_rejects_other_nonzero_topologies()
{
   let uniform = api::Color::rgba(1.0, 1.0, 1.0, 1.0);
   let four_vertices = colored_quad_vertices([0xFFFF_FFFF; 4]);
   assert!(solid_color::colored_quad(&four_vertices[..4], uniform).is_none());
   assert!(solid_color::colored_quad(
      &colored_quad_vertices([0xFF00_00FF, 0xFF00_FF00, 0xFFFF_0000, 0xFFFF_FFFF]),
      uniform,
   )
   .is_none());

   let mut skewed = colored_quad_vertices([0xFFFF_FFFF; 4]);
   skewed[5].x = 11.0;
   assert!(solid_color::colored_quad(&skewed, uniform).is_none());

   let mut mismatched_duplicate = colored_quad_vertices([0xFFFF_FFFF; 4]);
   mismatched_duplicate[3].rgba = 0xFF00_00FF;
   assert!(solid_color::colored_quad(&mismatched_duplicate, uniform).is_none());
}

#[test]
fn wasm_webgpu_id_mask_vertex_cache_is_content_hash_keyed_and_inflight_safe() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let cache_key = source
        .split("struct IdMaskVertexCacheKey")
        .nth(1)
        .expect("id-mask vertex cache key")
        .split("struct IdMaskVertexCache")
        .next()
        .expect("id-mask vertex cache key end");
    let helper = source
        .split("fn id_mask_vertex_cache_index")
        .nth(1)
        .expect("id-mask vertex cache helper")
        .split("fn logical_dimension")
        .next()
        .expect("id-mask vertex cache helper end");
    let reusable = source
        .split("fn id_mask_reusable_vertex_cache_index")
        .nth(1)
        .expect("id-mask reusable vertex cache helper")
        .split("fn ensure_id_mask_vertex_cache_uploaded")
        .next()
        .expect("id-mask reusable vertex cache helper end");

    assert!(cache_key.contains("content_hash: u64"));
    assert!(cache_key.contains("len: usize"));
    assert!(!cache_key.contains("ptr: usize"));
    assert!(helper.contains("IdMaskVertexCacheKey { content_hash, len: vertices.len() }"));
    assert!(helper.contains("IdMaskVertexCacheKey"));
    assert!(helper.contains("fn id_mask_reusable_vertex_cache_index"));
    assert!(helper.contains("write_id_mask_raster_vertex_bytes(vertices, &mut cache.bytes);"));
    assert!(helper.contains("cache.uploaded = false;"));
    assert!(reusable.contains("'caches: for index in 0..self.id_mask_vertex_caches.len()"));
    assert!(reusable.contains("for entry in &self.id_mask_draw_chunk_indices"));
    assert!(reusable.contains("continue 'caches;"));
}

#[test]
fn wasm_webgpu_draw_encoding_reuses_scratch_storage() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let encode_solid = source
        .split("fn encode_solid")
        .nth(1)
        .expect("encode_solid")
        .split("fn encode_image")
        .next()
        .expect("encode_solid end");
    let encode_image_mesh = source
        .split("fn encode_image_mesh")
        .nth(1)
        .expect("encode_image_mesh")
        .split("fn encode_glyph_vertices")
        .next()
        .expect("encode_image_mesh end");
    let encode_glyph_vertices = source
        .split("fn encode_glyph_vertices")
        .nth(1)
        .expect("encode_glyph_vertices")
        .split("fn encode_rrect")
        .next()
        .expect("encode_glyph_vertices end");
    let encode_rrect = source
        .split("fn encode_rrect")
        .nth(1)
        .expect("encode_rrect")
        .split("fn encode_nine_slice")
        .next()
        .expect("encode_rrect end");

    assert!(source.contains("scratch_vertices: Vec<GpuVertex>"));
    assert!(source.contains("scratch_indices: Vec<u32>"));
    assert!(source.contains("scratch_points: Vec<(f32, f32)>"));
    assert!(source.contains("fn push_scratch_draw"));
    for section in [encode_solid, encode_image_mesh, encode_glyph_vertices, encode_rrect] {
        assert!(section.contains("self.clear_scratch_draw();"));
        assert!(section.contains("self.push_scratch_draw("));
        assert!(!section.contains("Vec::new()"));
        assert!(!section.contains("Vec::with_capacity"));
    }
}

#[test]
fn wasm_webgpu_effect_path_avoids_redundant_hot_work() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let target_uses_backdrop = source
        .split("fn target_uses_backdrop")
        .nth(1)
        .expect("target_uses_backdrop")
        .split("fn backdrop_sample_rect")
        .next()
        .expect("target_uses_backdrop end");
    let prepare_effect_uniforms = source
        .split("fn prepare_effect_uniforms")
        .nth(1)
        .expect("prepare_effect_uniforms")
        .split("fn ensure_effect_uniform_capacity")
        .next()
        .expect("prepare_effect_uniforms end");
    let single_uniform_slot = prepare_effect_uniforms
        .split("if self.frame.effect_single_uniform_slot")
        .nth(1)
        .expect("single effect uniform slot")
        .split("self.ensure_effect_uniform_capacity(effect_count);")
        .next()
        .expect("single effect uniform slot end");

    assert!(target_uses_backdrop.contains("draw.target == target"));
    assert!(target_uses_backdrop.contains("matches!(draw.kind, DrawKind::Backdrop { .. })"));
    assert!(
        single_uniform_slot.contains("self.queue.write_buffer(&self.effect_buffer, 0, &bytes);")
    );
    assert!(!single_uniform_slot.contains("self.effect_uniform_bytes.clear();"));
    assert!(!single_uniform_slot.contains("self.effect_uniform_bytes.extend_from_slice"));
    assert!(source.contains("Backdrop { rect: api::RectF, sigma: f32 }"));
    assert!(source.contains("fn backdrop_sample_rect("));
    assert!(source
        .contains("fn backdrop_batch_end(&self, start: usize, target: Option<u32>, limit: usize)"));
    assert!(source.contains("self.backdrop_batch_enabled"));
    assert!(source.contains("fn render_draw_target_with_effects("));
    assert!(source_without_whitespace(source)
        .contains("self.render_draw_range(encoder,target_view,start,end,target"));
    assert!(source.contains("set_backdrop_batch_enabled_for_benchmark"));
}

#[test]
fn wasm_webgpu_scene3d_render_does_not_clone_draw_lists() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let render_scene3d = source
        .split("fn render_scene3d(")
        .nth(1)
        .expect("render_scene3d")
        .split("fn render_scene3d_overlay")
        .next()
        .expect("render_scene3d end");
    let render_scene3d_overlay = source
        .split("fn render_scene3d_overlay")
        .nth(1)
        .expect("render_scene3d_overlay")
        .split("fn render_id_mask_compositors")
        .next()
        .expect("render_scene3d_overlay end");

    assert!(!render_scene3d.contains(".clone()"));
    assert!(!render_scene3d_overlay.contains(".clone()"));
    assert!(render_scene3d.contains("for draw_index in 0..self.scene3d_draws.len()"));
    assert!(
        render_scene3d_overlay.contains("for draw_index in 0..self.scene3d_overlay_draws.len()")
    );
}

#[test]
fn wasm_webgpu_backend_packet_vocabulary_is_frozen() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let draw_kind = compact_source_block(
        source,
        "enum DrawKind {",
        "#[derive(Clone, Copy, PartialEq, Eq)]\nenum DrawPipelineKey",
    );
    let gpu_draw = compact_source_block(
        source,
        "struct GpuDraw {",
        "#[derive(Clone, Copy)]\nstruct FrameLayerPass",
    );
    let coalescible = compact_source_block(
        source,
        "fn coalescible_draw_kind",
        "#[derive(Clone, Copy)]\nenum TimestampPassFamily",
    );
    let encode_draw_cmd = compact_source_block(
        source,
        "fn encode_draw_cmd",
        "fn encode_solid",
    );

    assert_eq!(
        draw_kind,
        "enumDrawKind{Solid,Rgba{image:u32},A8{image:u32},Sdf{image:u32},Layer{id:u32},Backdrop{rect:api::RectF,sigma:f32},}"
    );
    assert_eq!(
        gpu_draw,
        "structGpuDraw{kind:DrawKind,first_index:u32,index_count:u32,clip:api::RectI,effect_uniform_offset:u32,target:Option<u32>,}"
    );
    for pattern in [
        "(DrawKind::Solid,DrawKind::Solid)=>true",
        "(DrawKind::Rgba{image:a},DrawKind::Rgba{image:b})=>a==b",
        "(DrawKind::A8{image:a},DrawKind::A8{image:b})=>a==b",
        "(DrawKind::Sdf{image:a},DrawKind::Sdf{image:b})=>a==b",
        "(DrawKind::Layer{id:a},DrawKind::Layer{id:b})=>a==b",
        "_=>false",
    ] {
        assert!(coalescible.contains(pattern), "missing coalescing packet rule {pattern}");
    }
    for pattern in [
        "api::DrawCmd::Solid{vb,ib,color}=>self.encode_solid(list,*vb,*ib,*color)",
        "api::DrawCmd::Image{tex,dst,src,alpha}=>{self.encode_image(*tex,*dst,*src,*alpha,false)}",
        "api::DrawCmd::ImageMesh{tex,vb,ib,alpha}=>{self.encode_image_mesh(list,*tex,*vb,*ib,*alpha)}",
        "api::DrawCmd::GlyphRun{run}=>self.encode_glyph_run(list,run)",
        "api::DrawCmd::RRect{rect,radii,color}=>self.encode_rrect(*rect,*radii,*color)",
        "api::DrawCmd::NineSlice{tex,rect,slice,alpha}=>{self.encode_nine_slice(*tex,*rect,*slice,*alpha)}",
        "api::DrawCmd::Backdrop{rect,sigma,tint,alpha}=>{self.stats.backdrop_draws=self.stats.backdrop_draws.saturating_add(1);self.encode_backdrop(*rect,*sigma,*tint,*alpha)}",
        "api::DrawCmd::VisualEffect{rect,effect}=>{lettint=effect.tint();self.stats.visual_effect_draws=self.stats.visual_effect_draws.saturating_add(1);self.encode_backdrop(*rect,effect.blur_intensity()*72.0,tint,tint.a);}",
        "api::DrawCmd::CameraBg{..}=>{}",
        "api::DrawCmd::Spinner{center,atom,alpha}=>{self.stats.spinner_draws=self.stats.spinner_draws.saturating_add(1);self.encode_spinner(*center,*atom,*alpha)}",
        "api::DrawCmd::ClipPush{rect}=>{self.clip_stack.push(*rect);self.stats.clip_depth_peak=self.stats.clip_depth_peak.max(self.clip_stack.len()asu32);}",
        "api::DrawCmd::ClipPop=>{let_=self.clip_stack.pop();}",
    ] {
        assert!(encode_draw_cmd.contains(pattern), "missing WebGPU lowering rule {pattern}");
    }
}

#[test]
fn wasm_webgpu_id_mask_uniform_bytes_reuse_scratch_storage() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let render_id_mask = source
        .split("fn render_id_mask_compositors")
        .nth(1)
        .expect("render_id_mask_compositors")
        .split("fn render_draw_range")
        .next()
        .expect("render_id_mask_compositors end");
    let raster_writer = source
        .split("fn write_id_mask_raster_uniform_bytes")
        .nth(1)
        .expect("raster uniform writer")
        .split("fn id_mask_field_uniform_bytes")
        .next()
        .expect("raster uniform writer end");
    let compositor_writer = source
        .split("fn write_id_mask_compositor_uniform_bytes")
        .nth(1)
        .expect("compositor uniform writer")
        .split("fn push_scene3d_uniform")
        .next()
        .expect("compositor uniform writer end");

    assert!(source.contains("id_mask_raster_uniform_bytes: Vec<u8>"));
    assert!(source.contains("id_mask_compositor_uniform_bytes: Vec<u8>"));
    assert!(render_id_mask.contains("write_id_mask_raster_uniform_bytes("));
    assert!(render_id_mask.contains("write_id_mask_compositor_uniform_bytes("));
    assert!(!render_id_mask.contains("id_mask_raster_uniform_bytes(width"));
    assert!(!render_id_mask.contains("id_mask_compositor_uniform_bytes(&draw)"));
    assert!(raster_writer.contains("out.clear();"));
    assert!(raster_writer.contains("out.reserve(ID_MASK_RASTER_UNIFORM_SIZE_BYTES);"));
    assert!(!raster_writer.contains("Vec::with_capacity"));
    assert!(compositor_writer.contains("out.clear();"));
    assert!(compositor_writer.contains("out.reserve("));
    assert!(!compositor_writer.contains("Vec::with_capacity"));
}

#[test]
fn wasm_webgpu_resource_counters_cover_uploads_and_passes() {
    let stats = include_str!("../src/lib.rs");
    let source = include_str!("../src/wasm/webgpu.rs");
    let host = include_str!("../../../host/web-app/oxide-host-web/src/lib.rs");

    for field in [
        "pub render_passes: u32",
        "pub command_buffers: u32",
        "pub buffer_upload_bytes: u64",
        "pub texture_upload_bytes: u64",
        "pub buffer_grows: u32",
        "pub texture_creates: u32",
        "pub bind_group_creates: u32",
        "pub pipeline_creates: u32",
        "pub sampler_creates: u32",
        "pub mesh3d_creates: u32",
        "pub draw_buffer_grows: u32",
        "pub image_texture_creates: u32",
        "pub image_bind_group_creates: u32",
        "pub target_texture_creates: u32",
        "pub target_bind_group_creates: u32",
        "pub scene3d_buffer_grows: u32",
        "pub scene3d_bind_group_creates: u32",
        "pub effect_buffer_grows: u32",
        "pub effect_bind_group_creates: u32",
        "pub id_mask_texture_creates: u32",
        "pub id_mask_buffer_grows: u32",
        "pub id_mask_bind_group_creates: u32",
        "pub image_upload_temp_allocs: u32",
        "pub image_upload_temp_bytes: u64",
        "pub image_upload_scratch_bytes: u64",
        "pub image_upload_scratch_grows: u32",
        "pub draw_items: u32",
        "pub draw_items_coalesced: u32",
        "pub draw_pipeline_binds: u32",
        "pub draw_bind_group_binds: u32",
        "pub draw_scissor_sets: u32",
        "pub image_mesh_draws: u32",
        "pub nine_slice_draws: u32",
        "pub sdf_glyph_quads: u32",
        "pub clip_depth_peak: u32",
        "pub cpu_scratch_bytes: u64",
        "pub cpu_scratch_grows: u32",
        "pub cpu_scratch_growth_bytes: u64",
        "pub cpu_draw_scratch_bytes: u64",
        "pub cpu_draw_scratch_grows: u32",
        "pub cpu_draw_scratch_growth_bytes: u64",
        "pub cpu_scene3d_scratch_bytes: u64",
        "pub cpu_scene3d_scratch_grows: u32",
        "pub cpu_scene3d_scratch_growth_bytes: u64",
        "pub cpu_effect_scratch_bytes: u64",
        "pub cpu_effect_scratch_grows: u32",
        "pub cpu_effect_scratch_growth_bytes: u64",
        "pub cpu_id_mask_scratch_bytes: u64",
        "pub cpu_id_mask_scratch_grows: u32",
        "pub cpu_id_mask_scratch_growth_bytes: u64",
        "pub cpu_image_upload_scratch_bytes: u64",
        "pub cpu_image_upload_scratch_grows: u32",
        "pub cpu_image_upload_scratch_growth_bytes: u64",
        "pub cpu_resource_table_scratch_bytes: u64",
        "pub cpu_resource_table_scratch_grows: u32",
        "pub cpu_resource_table_scratch_growth_bytes: u64",
        "pub scene3d_draws: u32",
        "pub id_mask_draws: u32",
        "pub backdrop_draws: u32",
        "pub visual_effect_draws: u32",
        "pub effect_uniform_writes: u32",
        "pub effect_uniform_bytes: u64",
        "pub effect_uniform_slots: u32",
        "pub spinner_draws: u32",
        "pub camera_bg_draws: u32",
        "pub clear_passes: u32",
        "pub draw_passes: u32",
        "pub scene3d_passes: u32",
        "pub scene3d_overlay_passes: u32",
        "pub id_mask_raster_passes: u32",
        "pub id_mask_field_seed_passes: u32",
        "pub id_mask_field_jump_passes: u32",
        "pub id_mask_compositor_passes: u32",
        "pub present_passes: u32",
        "pub texture_copies: u32",
        "pub gpu_timestamp_query_supported: bool",
        "pub gpu_timestamp_frame_id: u64",
        "pub gpu_timestamp_passes: u32",
        "pub gpu_timestamp_total_ns: u64",
        "pub gpu_timestamp_clear_ns: u64",
        "pub gpu_timestamp_draw_ns: u64",
        "pub gpu_timestamp_scene3d_ns: u64",
        "pub gpu_timestamp_scene3d_overlay_ns: u64",
        "pub gpu_timestamp_id_mask_raster_ns: u64",
        "pub gpu_timestamp_id_mask_field_seed_ns: u64",
        "pub gpu_timestamp_id_mask_field_jump_ns: u64",
        "pub gpu_timestamp_id_mask_compositor_ns: u64",
        "pub gpu_timestamp_present_ns: u64",
        "pub gpu_timestamp_max_pass_ns: u64",
        "pub gpu_timestamp_readback_skips: u32",
    ] {
        assert!(stats.contains(field), "missing WebRendererStats field {field}");
    }

    assert!(source.contains("wgpu::Features::TIMESTAMP_QUERY"));
    assert!(source.contains("wgpu::QueryType::Timestamp"));
    assert!(source.contains("wgpu::RenderPassTimestampWrites"));
    assert!(source.contains("encoder.resolve_query_set"));
    assert!(source.contains("encoder.copy_buffer_to_buffer"));
    assert!(source.contains("slot.buffer.map_async"));
    assert!(source.contains("const TIMESTAMP_READBACK_SLOTS: usize = 48;"));
    assert!(source.contains("fn pending_timestamp_readbacks(&self) -> u32"));
    assert!(source.contains("fn pending_count(&self) -> u32"));
    assert!(source.contains("collect_timestamp_readbacks"));
    assert!(source
        .contains("self.stats.command_buffers = self.stats.command_buffers.saturating_add(1);"));
    assert!(
        source.contains("self.stats.render_passes = self.stats.render_passes.saturating_add(1);")
    );
    assert!(source.contains("self.stats.clear_passes"));
    assert!(source.contains("self.stats.draw_passes"));
    assert!(source.contains("self.stats.scene3d_passes"));
    assert!(source.contains("self.stats.scene3d_overlay_passes"));
    assert!(source.contains("self.stats.id_mask_raster_passes"));
    assert!(source.contains("self.stats.id_mask_field_seed_passes"));
    assert!(source.contains("self.stats.id_mask_field_jump_passes"));
    assert!(source.contains("self.stats.id_mask_compositor_passes"));
    assert!(source.contains("self.stats.present_passes"));
    assert!(source.contains("self.stats.texture_copies"));
    assert!(source.contains("self.stats.buffer_upload_bytes"));
    assert!(source.contains("self.stats.texture_upload_bytes"));
    assert!(source.contains("self.stats.buffer_grows"));
    assert!(source.contains("self.stats.texture_creates"));
    assert!(source.contains("self.stats.bind_group_creates"));
    assert!(!source.contains("self.stats.pipeline_creates"));
    for field in [
        "self.stats.draw_buffer_grows",
        "self.stats.image_texture_creates",
        "self.stats.image_bind_group_creates",
        "self.stats.target_texture_creates",
        "self.stats.target_bind_group_creates",
        "self.stats.scene3d_buffer_grows",
        "self.stats.scene3d_bind_group_creates",
        "self.stats.effect_buffer_grows",
        "self.stats.effect_bind_group_creates",
        "self.stats.id_mask_texture_creates",
        "self.stats.id_mask_buffer_grows",
        "self.stats.id_mask_bind_group_creates",
    ] {
        assert!(source.contains(field), "missing WebGPU resource attribution {field}");
    }
    assert!(source.contains("draw_state_cache_enabled: bool"));
    assert!(source.contains("image_upload_scratch: Vec<u8>"));
    assert!(source.contains("image_upload_scratch_enabled: bool"));
    assert!(source.contains("set_image_upload_scratch_enabled_for_benchmark"));
    let compact_source = source_without_whitespace(source);
    assert!(compact_source.contains("copy_a8_rows_to_rgba_into(&mutself.image_upload_scratch"));
    assert!(compact_source.contains("copy_rgba_rows_into(&mutself.image_upload_scratch"));
    assert!(!source.contains("core::mem::take(&mut self.image_upload_scratch)"));
    assert!(source.contains("fn update_image_from_upload_scratch("));
    assert!(source.contains("fn write_image_update("));
    assert!(source.contains("fn record_image_upload_scratch(&mut self, grew: bool)"));
    assert!(source.contains("fn record_image_upload_temp(&mut self, bytes: usize, allocs: u32)"));
    assert!(!source.contains("fn validate_image_update("));
    assert!(source.contains("self.stats.image_upload_temp_allocs"));
    assert!(source.contains("self.stats.image_upload_temp_bytes"));
    assert!(source.contains("self.stats.image_upload_scratch_bytes"));
    assert!(source.contains("self.stats.image_upload_scratch_grows"));
    assert!(source.contains("DrawStateKey"));
    assert!(source.contains("fn draw_state_key(&self, draw: GpuDraw) -> Option<DrawStateKey>"));
    assert!(source.contains("set_draw_state_cache_enabled_for_benchmark"));
    assert!(source.contains("draw_item_coalescing_enabled: bool"));
    assert!(source.contains("draw_item_coalescing_enabled: true"));
    assert!(source.contains("set_draw_item_coalescing_enabled_for_benchmark"));
    assert!(source.contains("fn coalescible_draw_kind"));
    assert!(source.contains("fn try_coalesce_draw_item"));
    assert!(source.contains("bound_pipeline"));
    assert!(source.contains("bound_bind"));
    assert!(source.contains("bound_clip"));
    assert!(source.contains("self.stats.draw_items"));
    assert!(source.contains("self.stats.draw_items_coalesced"));
    assert!(source.contains("self.stats.draw_pipeline_binds"));
    assert!(source.contains("self.stats.draw_bind_group_binds"));
    assert!(source.contains("self.stats.draw_scissor_sets"));
    assert_eq!(source.matches("device.create_sampler(").count(), 1);
    assert!(source.contains("let sampler = device.create_sampler(&wgpu::SamplerDescriptor"));
    assert!(source.contains("self.stats.mesh3d_creates"));
    assert!(source.contains("struct ScratchCapacityBreakdown"));
    assert!(source.contains("fn scratch_capacity_breakdown(&self) -> ScratchCapacityBreakdown"));
    assert!(source.contains(
        "fn apply_scratch_capacity_stats(&mut self, capacity: ScratchCapacityBreakdown)"
    ));
    assert!(source.contains("fn record_scratch_growth_stats(&mut self)"));
    assert!(source.contains("self.stats.cpu_scratch_grows"));
    assert!(source.contains("self.stats.cpu_scratch_growth_bytes"));
    for field in [
        "self.stats.cpu_draw_scratch_grows",
        "self.stats.cpu_draw_scratch_growth_bytes",
        "self.stats.cpu_scene3d_scratch_grows",
        "self.stats.cpu_scene3d_scratch_growth_bytes",
        "self.stats.cpu_effect_scratch_grows",
        "self.stats.cpu_effect_scratch_growth_bytes",
        "self.stats.cpu_id_mask_scratch_grows",
        "self.stats.cpu_id_mask_scratch_growth_bytes",
        "self.stats.cpu_image_upload_scratch_grows",
        "self.stats.cpu_image_upload_scratch_growth_bytes",
        "self.stats.cpu_resource_table_scratch_grows",
        "self.stats.cpu_resource_table_scratch_growth_bytes",
    ] {
        assert!(source.contains(field), "missing WebGPU scratch growth attribution {field}");
    }
    assert!(source.contains("self.stats.layer_draws = self.stats.layer_draws.saturating_add(1);"));
    assert!(source
        .contains("self.stats.layer_cache_hits = self.stats.layer_cache_hits.saturating_add(1);"));
    assert!(source.contains(
        "self.stats.layer_cache_misses = self.stats.layer_cache_misses.saturating_add(1);"
    ));
    assert!(source.contains("self.stats.layer_cache_skipped_draws"));
    assert!(source.contains("self.stats.layer_passes = self.stats.layer_passes.saturating_add(1);"));
    assert!(source.contains("self.stats.layer_texture_creates"));
    assert!(source.contains("self.stats.layer_bind_group_creates"));
    assert!(
        source.contains("self.stats.scene3d_draws = self.stats.scene3d_draws.saturating_add(1);")
    );
    assert!(
        source.contains("self.stats.id_mask_draws = self.stats.id_mask_draws.saturating_add(1);")
    );
    assert!(
        source.contains("self.stats.backdrop_draws = self.stats.backdrop_draws.saturating_add(1);")
    );
    assert!(source.contains("self.stats.visual_effect_draws"));
    assert!(
        source.contains("self.stats.spinner_draws = self.stats.spinner_draws.saturating_add(1);")
    );
    assert!(source.contains("api::DrawCmd::CameraBg { .. } => {}"));
    assert!(!source.contains("set_camera_background_rgba8"));
    assert!(!source.contains("camera_background:"));
    assert!(!source
        .contains("self.stats.camera_bg_draws = self.stats.camera_bg_draws.saturating_add(1);"));
    assert!(source.contains("self.stats.clip_depth_peak ="));
    assert!(source.contains("self.stats.clip_depth_peak.max(self.clip_stack.len() as u32)"));
    assert!(source
        .contains("self.stats.image_mesh_draws = self.stats.image_mesh_draws.saturating_add(1);"));
    assert!(source
        .contains("self.stats.nine_slice_draws = self.stats.nine_slice_draws.saturating_add(1);"));
    assert!(
        source.contains("self.stats.sdf_glyph_quads = self.stats.sdf_glyph_quads.saturating_add")
    );
    assert!(source.contains("encoded_render_passes"));
    assert!(source.contains("encoded_buffer_upload_bytes"));
    assert!(source.contains("fn ensure_buffer("));
    assert!(source.contains(") -> bool"));
    assert!(source.contains("fn prepare_effect_uniforms"));
    assert!(source.contains("fn ensure_effect_uniform_capacity"));
    assert!(source.contains("set_effect_uniform_batch_enabled_for_benchmark"));
    assert!(source.contains("DrawBindKey::Effect { offset"));
    assert!(source.contains("has_dynamic_offset: true"));
    assert!(source.contains("pub fn image_update_rgba8("));
    assert!(source.contains("self.inner.try_image_update_rgba8"));

    assert!(host.contains("fn renderer_stats_metrics"));
    assert!(host.contains("renderer_stats_metrics(current.stats, \"current\")"));
    assert!(host.contains("renderer_stats_metrics(legacy.stats, \"legacy\")"));
    assert!(host.contains("{key_prefix}draw_items={}"));
    assert!(host.contains("{key_prefix}draw_items_coalesced={}"));
    assert!(host.contains("{key_prefix}draw_pipeline_binds={}"));
    assert!(host.contains("{key_prefix}draw_bind_group_binds={}"));
    assert!(host.contains("{key_prefix}draw_scissor_sets={}"));
    assert!(host.contains("{key_prefix}layer_draws={}"));
    assert!(host.contains("{key_prefix}layer_cache_hits={}"));
    assert!(host.contains("{key_prefix}layer_cache_misses={}"));
    assert!(host.contains("{key_prefix}layer_cache_skipped_draws={}"));
    assert!(host.contains("{key_prefix}layer_passes={}"));
    assert!(host.contains("{key_prefix}image_mesh_draws={}"));
    assert!(host.contains("{key_prefix}nine_slice_draws={}"));
    assert!(host.contains("{key_prefix}sdf_glyph_quads={}"));
    assert!(host.contains("{key_prefix}scene3d_draws={}"));
    assert!(host.contains("{key_prefix}id_mask_draws={}"));
    assert!(host.contains("{key_prefix}backdrop_draws={}"));
    assert!(host.contains("{key_prefix}effect_uniform_writes={}"));
    assert!(host.contains("{key_prefix}effect_uniform_bytes={}"));
    assert!(host.contains("{key_prefix}effect_uniform_slots={}"));
    assert!(host.contains("{key_prefix}sampler_creates={}"));
    assert!(host.contains("{key_prefix}mesh3d_creates={}"));
    assert!(host.contains("{key_prefix}draw_buffer_grows={}"));
    assert!(host.contains("{key_prefix}image_texture_creates={}"));
    assert!(host.contains("{key_prefix}image_bind_group_creates={}"));
    assert!(host.contains("{key_prefix}target_texture_creates={}"));
    assert!(host.contains("{key_prefix}target_bind_group_creates={}"));
    assert!(host.contains("{key_prefix}layer_texture_creates={}"));
    assert!(host.contains("{key_prefix}layer_bind_group_creates={}"));
    assert!(host.contains("{key_prefix}scene3d_buffer_grows={}"));
    assert!(host.contains("{key_prefix}scene3d_bind_group_creates={}"));
    assert!(host.contains("{key_prefix}effect_buffer_grows={}"));
    assert!(host.contains("{key_prefix}effect_bind_group_creates={}"));
    assert!(host.contains("{key_prefix}id_mask_texture_creates={}"));
    assert!(host.contains("{key_prefix}id_mask_buffer_grows={}"));
    assert!(host.contains("{key_prefix}id_mask_bind_group_creates={}"));
    assert!(host.contains("{key_prefix}image_upload_temp_allocs={}"));
    assert!(host.contains("{key_prefix}image_upload_temp_bytes={}"));
    assert!(host.contains("{key_prefix}image_upload_scratch_bytes={}"));
    assert!(host.contains("{key_prefix}image_upload_scratch_grows={}"));
    assert!(host.contains("{key_prefix}clear_passes={}"));
    assert!(host.contains("{key_prefix}draw_passes={}"));
    assert!(host.contains("{key_prefix}id_mask_field_jump_passes={}"));
    assert!(host.contains("{key_prefix}texture_copies={}"));
    assert!(host.contains("{key_prefix}gpu_timestamp_query_supported={}"));
    assert!(host.contains("{key_prefix}gpu_timestamp_total_ns={}"));
    assert!(host.contains("{key_prefix}gpu_timestamp_id_mask_field_jump_ns={}"));
    assert!(host.contains("settle_renderer_timestamps"));
    assert!(host.contains("fn timestamp_stats_cover_row"));
    assert!(host.contains("stats.gpu_timestamp_frame_id > after_frame_id"));
    assert!(host.contains("stats.gpu_timestamp_passes == stats.render_passes"));
    assert!(host.contains("WebGPU timestamp readback did not settle for row"));
    assert!(host.contains("{key_prefix}cpu_scratch_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_scratch_grows={}"));
    assert!(host.contains("{key_prefix}cpu_scratch_growth_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_draw_scratch_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_draw_scratch_grows={}"));
    assert!(host.contains("{key_prefix}cpu_draw_scratch_growth_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_scene3d_scratch_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_scene3d_scratch_grows={}"));
    assert!(host.contains("{key_prefix}cpu_scene3d_scratch_growth_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_effect_scratch_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_effect_scratch_grows={}"));
    assert!(host.contains("{key_prefix}cpu_effect_scratch_growth_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_id_mask_scratch_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_id_mask_scratch_grows={}"));
    assert!(host.contains("{key_prefix}cpu_id_mask_scratch_growth_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_image_upload_scratch_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_image_upload_scratch_grows={}"));
    assert!(host.contains("{key_prefix}cpu_image_upload_scratch_growth_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_resource_table_scratch_bytes={}"));
    assert!(host.contains("{key_prefix}cpu_resource_table_scratch_grows={}"));
    assert!(host.contains("{key_prefix}cpu_resource_table_scratch_growth_bytes={}"));
    assert!(host.contains("{key_prefix}buffer_upload_bytes={}"));
}

#[test]
fn wasm_webgpu_static_pipelines_are_created_before_frame_encoding() {
    let source = include_str!("../src/wasm/webgpu.rs");
    let programs = source
        .split("struct GpuPrograms")
        .nth(1)
        .expect("gpu programs struct")
        .split("fn image_for_update")
        .next()
        .expect("gpu programs struct end");
    let create_programs = source
        .split("fn create_programs")
        .nth(1)
        .expect("create_programs")
        .split("fn alpha_color_target")
        .next()
        .expect("create_programs end");

    assert!(!source.contains("Option<wgpu::RenderPipeline>"));
    assert!(!source.contains("fn ensure_solid_pipeline"));
    assert!(!source.contains("fn ensure_rgba_pipeline"));
    assert!(!source.contains("fn ensure_scene3d_pipeline"));
    assert!(!source.contains("fn ensure_id_mask_raster_pipeline"));
    assert!(!source.contains("fn ensure_draw_pipeline"));
    assert!(!source.contains("self.stats.pipeline_creates"));
    for field in [
        "solid_pipeline: wgpu::RenderPipeline",
        "rgba_pipeline: wgpu::RenderPipeline",
        "a8_pipeline: wgpu::RenderPipeline",
        "sdf_pipeline: wgpu::RenderPipeline",
        "effect_pipeline: wgpu::RenderPipeline",
        "scene3d_color_tri_depth_read_pipeline: wgpu::RenderPipeline",
        "scene3d_color_tri_depth_write_pipeline: wgpu::RenderPipeline",
        "scene3d_color_tri_pipeline: wgpu::RenderPipeline",
        "scene3d_color_tri_add_depth_read_pipeline: wgpu::RenderPipeline",
        "scene3d_color_tri_add_depth_write_pipeline: wgpu::RenderPipeline",
        "scene3d_color_tri_add_pipeline: wgpu::RenderPipeline",
        "id_mask_raster_pipeline: wgpu::RenderPipeline",
        "id_mask_field_seed_pipeline: wgpu::RenderPipeline",
        "id_mask_field_jump_pipeline: wgpu::RenderPipeline",
        "id_mask_compositor_pipeline: wgpu::RenderPipeline",
    ] {
        assert!(programs.contains(field), "missing eager pipeline field {field}");
    }
    for local in [
        "let solid_pipeline = create_pipeline(",
        "let rgba_pipeline = create_pipeline(",
        "let a8_pipeline = create_pipeline(",
        "let sdf_pipeline = create_pipeline(",
        "let effect_pipeline = create_pipeline(",
        "let scene3d_color_tri_depth_read_pipeline = create_scene3d_pipeline(",
        "let scene3d_color_tri_depth_write_pipeline = create_scene3d_pipeline(",
        "let scene3d_color_tri_pipeline = create_scene3d_pipeline(",
        "let scene3d_color_tri_add_depth_read_pipeline = create_scene3d_pipeline(",
        "let scene3d_color_tri_add_depth_write_pipeline = create_scene3d_pipeline(",
        "let scene3d_color_tri_add_pipeline = create_scene3d_pipeline(",
        "let id_mask_raster_pipeline = create_id_mask_raster_pipeline(",
        "let id_mask_field_seed_pipeline = create_id_mask_field_pipeline(",
        "let id_mask_field_jump_pipeline = create_id_mask_field_pipeline(",
        "let id_mask_compositor_pipeline = create_id_mask_compositor_pipeline(",
    ] {
        assert!(create_programs.contains(local), "missing eager pipeline creation {local}");
    }
}
