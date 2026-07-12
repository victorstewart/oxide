#[test]
fn renderer_loads_build_time_metallib_instead_of_runtime_source() {
    let source = include_str!("../src/lib.rs");
    assert!(
        source.contains("const DEFAULT_METALLIB")
            && source.contains("include_bytes!(concat!(env!(\"OUT_DIR\"), \"/default.metallib\"))")
            && source.contains("new_library_with_data(DEFAULT_METALLIB)")
            && !source.contains("new_library_with_source"),
        "renderer-metal must load the build-time metallib and avoid runtime shader source compilation"
    );
}

#[test]
fn build_script_fails_apple_metallib_generation_instead_of_placeholder_fallback() {
    let source = include_str!("../build.rs");
    assert!(
        source.contains("target_is_apple")
            && source.contains("Metal toolchain not found")
            && source.contains("metal compile failed")
            && source.contains("metallib link failed")
            && !source.contains("Metal toolchain not found; emitting placeholder metallib")
            && !source.contains("metallib link failed; emitting placeholder metallib"),
        "renderer-metal build.rs must not emit placeholder metallibs for Apple renderer builds"
    );
}

#[test]
fn per_frame_reuse_never_waits_for_gpu_completion() {
    let source = include_str!("../src/lib.rs");
    let start = source.find("fn prepare_for_encode").expect("prepare_for_encode function");
    let tail = &source[start..];
    let end = tail.find("fn mark_submitted").expect("mark_submitted function");
    let prepare_for_encode = &tail[..end];
    assert!(
        !prepare_for_encode.contains("wait_until_completed"),
        "normal frame-ring reuse must not block the CPU on an in-flight Metal command buffer"
    );
    assert!(
        source.contains("frame_backpressure_skipped")
            && source.contains(".find(|slot| self.frames[*slot].is_available())"),
        "renderer-metal must select an available frame-ring slot or skip instead of blocking"
    );
}

#[test]
fn blocking_gpu_waits_are_limited_to_explicit_readback_helpers() {
    let source = include_str!("../src/lib.rs");
    let total_waits = source.matches("wait_until_completed").count();
    let readback_texture = source_block(
        source,
        "fn readback_texture_bytes",
        "fn readback_direct_live_camera_bgra8",
    );
    let readback_camera =
        source_block(source, "fn readback_direct_live_camera_bgra8", "pub fn readback_bgra8");
    let allowed_waits = readback_texture.matches("wait_until_completed").count()
        + readback_camera.matches("wait_until_completed").count();

    assert_eq!(
        total_waits, allowed_waits,
        "renderer-metal must keep blocking GPU waits out of frame hot paths"
    );
    assert_eq!(allowed_waits, 2, "readback helpers are the only allowed blocking waits");
}

#[test]
fn command_buffer_gpu_duration_is_enabled_on_macos_and_ios() {
    let source = include_str!("../src/lib.rs");
    assert!(
        source.contains("#[cfg(any(target_os = \"ios\", target_os = \"macos\"))]")
            && source.contains("GPUStartTime")
            && source.contains("GPUEndTime"),
        "direct command-buffer GPU duration must be compiled for both iOS device reports and macOS Metal A/B perf runs"
    );
}

#[test]
fn completed_gpu_duration_is_attributed_to_frame_id() {
    let source = include_str!("../src/lib.rs");
    assert!(
        source.contains("struct CompletedGpuStats")
            && source.contains("gpu_frame_id")
            && source.contains("frame_id,")
            && source.contains("stats.gpu_frame_id = gpu.frame_id"),
        "published GPU durations must carry the completed frame id so perf reports do not sample stale command-buffer timings"
    );
}

#[test]
fn layer_cache_uses_one_plan_and_reports_single_ownership()
{
   use oxide_renderer_metal::PerfStats;

   let stats = PerfStats {
      layer_body_commands_scanned: 1,
      layer_body_commands_copied: 2,
      layer_texture_creates: 3,
      layer_cache_hits: 4,
      layer_cache_misses: 5,
      layer_offscreen_draws: 6,
      layer_inline_draws: 7,
      layer_double_render_prevented: 8,
      ..PerfStats::default()
   };
   assert_eq!(stats.layer_body_commands_scanned, 1);
   assert_eq!(stats.layer_body_commands_copied, 2);
   assert_eq!(stats.layer_texture_creates, 3);
   assert_eq!(stats.layer_cache_hits, 4);
   assert_eq!(stats.layer_cache_misses, 5);
   assert_eq!(stats.layer_offscreen_draws, 6);
   assert_eq!(stats.layer_inline_draws, 7);
   assert_eq!(stats.layer_double_render_prevented, 8);

   let source = include_str!("../src/lib.rs");
   assert!(source.contains("struct LayerPlan"));
   assert!(source.contains("self.build_layer_plans(list);"));
   assert!(source.contains("if !plan.refresh"));
   assert!(source.contains("entry.generation = plan.generation;"));
   assert!(source.contains("entry.generation == plan.generation"));
   assert!(source.contains("parent.dirty = true;"));
   assert!(source.contains(".filter(|entry| entry.w == w && entry.h == h)"));
   assert!(source.contains("pso_layer_composite_aligned"));
   assert!(source.contains("let pixel_aligned = !plan.refresh"));
   assert!(source.contains("(pixels - pixels.round()).abs() <= f32::EPSILON"));
   let shader = include_str!("../shaders/ui.metal");
   assert!(shader.contains("fragment float4 f_layer_composite_aligned"));
   assert!(shader.contains("coord::pixel"));
   assert!(shader.contains("filter::nearest"));
   assert!(source.contains("self.acc_layer_double_render_prevented.saturating_add(1)"));
   assert_eq!(source.matches("build_layer_sublist(").count(), 2);
   assert!(!source.contains("DefaultHasher"));
}

#[test]
fn image_argument_tables_are_immutable_per_frame_and_report_reuse()
{
   use oxide_renderer_metal::PerfStats;

   let stats = PerfStats {
      image_argument_encodes: 1,
      image_argument_binds: 2,
      image_argument_tables_finalized: 3,
      image_argument_table_reuses: 4,
      image_argument_bytes: 5,
      image_argument_buffer_grows: 6,
      ..PerfStats::default()
   };
   assert_eq!(stats.image_argument_encodes, 1);
   assert_eq!(stats.image_argument_binds, 2);
   assert_eq!(stats.image_argument_tables_finalized, 3);
   assert_eq!(stats.image_argument_table_reuses, 4);
   assert_eq!(stats.image_argument_bytes, 5);
   assert_eq!(stats.image_argument_buffer_grows, 6);

   let source = include_str!("../src/lib.rs");
   assert!(source.contains("struct ImageArgTable"));
   assert!(source.contains("fn ensure_image_argument_capacity"));
   assert!(source.contains("fn prepare_image_argument_buffers"));
   assert!(source.contains("retained_high_water"));
   assert!(source.contains("argument_encoder.set_argument_buffer(buffer, offset as u64)"));
   assert!(source.contains("let needed = r.img_arg_used.saturating_add(r.img_arg_stride)"));
   assert!(source.contains("r.image_arg_table_count < IMAGE_ARG_SMALL_TABLE_COUNT"));
   assert!(source.contains("r.image_arg_table_index"));
   assert!(source.contains("r.image_arg_tables[*index].handles == r.image_arg_handles"));
   assert!(source.contains("r.acc_image_argument_table_reuses.saturating_add(1)"));
   assert_eq!(source.matches("argument_encoder.set_argument_buffer(buffer, offset as u64)").count(), 1);
   assert!(!source.contains("argument_encoder.set_argument_buffer(buffer, 0)"));
}

#[test]
fn neon_marker_instance_abi_is_explicit_and_chunks_inline_uploads()
{
   let source = include_str!("../src/neon_marker_gpu.rs");
   assert!(source.contains("#[repr(C, align(8))]"));
   assert!(source.contains("const _: [(); 72] = [(); core::mem::size_of::<MarkerGpuInstance>()]"));
   assert!(source.contains("offset_of!(MarkerGpuInstance, core_color)") && source.contains("const _: [(); 36]"));
   assert!(source.contains("offset_of!(MarkerGpuInstance, ring_color)") && source.contains("const _: [(); 52]"));
   assert!(source.contains("offset_of!(MarkerGpuInstance, _tail_pad)") && source.contains("const _: [(); 68]"));
   assert!(source.contains("METAL_SET_BYTES_LIMIT / core::mem::size_of::<MarkerGpuInstance>()"));
   assert!(source.contains("markers[..marker_count].chunks(instances_per_draw)"));
   assert!(source.contains("enc.set_vertex_bytes(1, marker_bytes as u64, markers.as_ptr().cast())"));
   assert!(source.contains("enc.set_fragment_bytes(1, marker_bytes as u64, markers.as_ptr().cast())"));

   let shader = include_str!("../shaders/neon_marker.metal");
   assert!(shader.contains("packed_float4 core_color;"));
   assert!(shader.contains("packed_float4 ring_color;"));
   assert!(shader.contains("uint _tail_pad;"));
}

#[cfg(target_os = "macos")]
#[test]
fn layer_cache_clean_and_dirty_frames_have_single_body_owner()
{
   use oxide_renderer_api::{self as api, Renderer};
   use oxide_renderer_metal::{MetalInitError, MetalRenderer};

   let mut renderer = match MetalRenderer::new_default()
   {
      Ok(renderer) => renderer,
      Err(MetalInitError::NoDevice) => panic!("macOS layer-cache contract requires Metal"),
      Err(error) => panic!("create Metal renderer: {error}"),
   };
   renderer.resize(96, 96, 1.0).expect("resize renderer");

   for (frame, dirty) in [false, false, true].into_iter().enumerate()
   {
      let mut list = api::DrawList::default();
      list.items.push(api::DrawCmd::LayerBegin {
         id: 7,
         rect: api::RectF::new(8.0, 8.0, 64.0, 64.0),
         dirty,
      });
      list.items.push(api::DrawCmd::RRect {
         rect: api::RectF::new(12.0, 12.0, 48.0, 48.0),
         radii: [6.0; 4],
         color: api::Color::rgba(0.2, 0.5, 0.9, 0.75),
      });
      list.items.push(api::DrawCmd::LayerEnd);

      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(&list);
      renderer.submit(token).expect("submit layer frame");
      let stats = renderer.last_stats();

      assert_eq!(stats.layer_body_commands_scanned, 2);
      assert_eq!(stats.layer_inline_draws, 0);
      if frame == 1
      {
         assert_eq!(stats.layer_body_commands_copied, 0);
         assert_eq!(stats.layer_texture_creates, 0);
         assert_eq!(stats.layer_cache_hits, 1);
         assert_eq!(stats.layer_cache_misses, 0);
         assert_eq!(stats.layer_offscreen_draws, 0);
         assert_eq!(stats.layer_double_render_prevented, 0);
         assert_eq!(stats.draws + stats.instanced, 1);
      }
      else
      {
         assert_eq!(stats.layer_body_commands_copied, 1);
         assert_eq!(stats.layer_texture_creates, if frame == 0 { 1 } else { 0 });
         assert_eq!(stats.layer_cache_hits, 0);
         assert_eq!(stats.layer_cache_misses, 1);
         assert_eq!(stats.layer_offscreen_draws, 1);
         assert_eq!(stats.layer_double_render_prevented, 1);
         assert_eq!(stats.draws + stats.instanced, 2);
      }
   }

   let mut unsupported = api::DrawList::default();
   unsupported.items.push(api::DrawCmd::LayerBegin {
      id: 8,
      rect: api::RectF::new(4.0, 4.0, 80.0, 80.0),
      dirty: true,
   });
   unsupported.items.push(api::DrawCmd::LayerBegin {
      id: 9,
      rect: api::RectF::new(8.0, 8.0, 64.0, 64.0),
      dirty: true,
   });
   unsupported.items.push(api::DrawCmd::RRect {
      rect: api::RectF::new(12.0, 12.0, 48.0, 48.0),
      radii: [6.0; 4],
      color: api::Color::rgba(0.2, 0.5, 0.9, 0.75),
   });
   unsupported.items.push(api::DrawCmd::Backdrop {
      rect: api::RectF::new(16.0, 16.0, 32.0, 32.0),
      sigma: 4.0,
      tint: api::Color::rgba(1.0, 1.0, 1.0, 1.0),
      alpha: 0.25,
   });
   unsupported.items.push(api::DrawCmd::LayerEnd);
   unsupported.items.push(api::DrawCmd::LayerEnd);

   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(&unsupported);
   renderer.submit(token).expect("submit unsupported layer frame");
   let stats = renderer.last_stats();
   assert_eq!(stats.layer_texture_creates, 0);
   assert_eq!(stats.layer_cache_hits, 0);
   assert_eq!(stats.layer_cache_misses, 0);
   assert_eq!(stats.layer_offscreen_draws, 0);
   assert_eq!(stats.layer_inline_draws, 2);
   assert_eq!(stats.layer_double_render_prevented, 0);
}

#[cfg(target_os = "macos")]
#[test]
fn dirty_nested_child_refreshes_its_cached_parent_once()
{
   use oxide_renderer_api::{self as api, Renderer};
   use oxide_renderer_metal::{MetalInitError, MetalRenderer};

   let mut renderer = match MetalRenderer::new_default()
   {
      Ok(renderer) => renderer,
      Err(MetalInitError::NoDevice) => panic!("macOS nested-layer contract requires Metal"),
      Err(error) => panic!("create Metal renderer: {error}"),
   };
   renderer.resize(96, 96, 1.0).expect("resize renderer");

   for (frame, child_dirty) in [false, false, true].into_iter().enumerate()
   {
      let mut list = api::DrawList::default();
      list.items.push(api::DrawCmd::LayerBegin {
         id: 10,
         rect: api::RectF::new(4.0, 4.0, 80.0, 80.0),
         dirty: false,
      });
      list.items.push(api::DrawCmd::LayerBegin {
         id: 11,
         rect: api::RectF::new(8.0, 8.0, 64.0, 64.0),
         dirty: child_dirty,
      });
      list.items.push(api::DrawCmd::RRect {
         rect: api::RectF::new(12.0, 12.0, 48.0, 48.0),
         radii: [6.0; 4],
         color: api::Color::rgba(0.8, 0.3, 0.2, 0.75),
      });
      list.items.push(api::DrawCmd::LayerEnd);
      list.items.push(api::DrawCmd::LayerEnd);

      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(&list);
      renderer.submit(token).expect("submit nested layer frame");
      let stats = renderer.last_stats();

      assert_eq!(stats.layer_inline_draws, 0);
      if frame == 1
      {
         assert_eq!(stats.layer_body_commands_copied, 0);
         assert_eq!(stats.layer_cache_hits, 2);
         assert_eq!(stats.layer_cache_misses, 0);
         assert_eq!(stats.layer_offscreen_draws, 0);
         assert_eq!(stats.layer_double_render_prevented, 0);
      }
      else
      {
         assert_eq!(stats.layer_body_commands_copied, 4);
         assert_eq!(stats.layer_texture_creates, if frame == 0 { 2 } else { 0 });
         assert_eq!(stats.layer_cache_hits, 0);
         assert_eq!(stats.layer_cache_misses, 2);
         assert_eq!(stats.layer_offscreen_draws, 2);
         assert_eq!(stats.layer_double_render_prevented, 2);
      }
   }
}

#[test]
fn renderer_memory_schema_covers_omitted_resource_families_and_saturates()
{
   use oxide_renderer_metal::PerfMemoryStats;

   let memory = PerfMemoryStats {
      depth_target_bytes: 1,
      bloom_targets_bytes: 2,
      id_mask_target_bytes: 3,
      scene3d_mesh_buffer_bytes: 4,
      id_mask_vertex_buffer_bytes: 5,
      layer_cache_bytes: 6,
      ..PerfMemoryStats::default()
   };
   assert_eq!(memory.depth_target_bytes, 1);
   assert_eq!(memory.bloom_targets_bytes, 2);
   assert_eq!(memory.id_mask_target_bytes, 3);
   assert_eq!(memory.scene3d_mesh_buffer_bytes, 4);
   assert_eq!(memory.id_mask_vertex_buffer_bytes, 5);
   assert_eq!(memory.layer_cache_bytes, 6);

   let source = include_str!("../src/lib.rs");
   assert!(source.contains("fold(bytes_per_element, u64::saturating_mul)"));
   assert!(source.contains("memory_texture_seen: RefCell<HashSet<usize>>"));
   assert!(source.contains("memory_buffer_seen: RefCell<HashSet<usize>>"));
   assert!(source.contains("let mut buffer_seen = self.memory_buffer_seen.borrow_mut();"));
   assert!(source.contains("buffer_seen.clear();"));
   assert!(source.contains("pub fn set_memory_stats_enabled_for_benchmark"));
   assert!(source.contains("pub fn set_accounting_stats_enabled_for_benchmark"));
   assert!(source.contains("self.last_stats.memory = PerfMemoryStats::default();"));
}

#[test]
fn metal_draw_cmd_debug_capture_names_are_frozen() {
    let source = include_str!("../src/lib.rs");
    let mapping = source_without_whitespace(source_block(
        source,
        "fn draw_cmd_kind",
        "#[inline(always)]\nfn running_on_ios_simulator",
    ));
    let expected = [
        r#"api::DrawCmd::LayerBegin{..}=>"layer_begin""#,
        r#"api::DrawCmd::LayerEnd=>"layer_end""#,
        r#"api::DrawCmd::Solid{..}=>"solid""#,
        r#"api::DrawCmd::Image{..}=>"image""#,
        r#"api::DrawCmd::ImageMesh{..}=>"image_mesh""#,
        r#"api::DrawCmd::GlyphRun{..}=>"glyph_run""#,
        r#"api::DrawCmd::RRect{..}=>"rrect""#,
        r#"api::DrawCmd::NineSlice{..}=>"nine_slice""#,
        r#"api::DrawCmd::Backdrop{..}=>"backdrop""#,
        r#"api::DrawCmd::VisualEffect{..}=>"visual_effect""#,
        r#"api::DrawCmd::CameraBg{..}=>"camera_bg""#,
        r#"api::DrawCmd::Spinner{..}=>"spinner""#,
        r#"api::DrawCmd::ClipPush{..}=>"clip_push""#,
        r#"api::DrawCmd::ClipPop=>"clip_pop""#,
    ];
    let mut previous = 0usize;
    for pattern in expected {
        let offset = mapping[previous..]
            .find(pattern)
            .map(|relative| previous + relative)
            .unwrap_or_else(|| panic!("missing Metal draw command debug mapping {pattern}"));
        previous = offset + pattern.len();
    }
    assert_eq!(mapping.matches("api::DrawCmd::").count(), expected.len());
}

fn source_block<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_idx = source.find(start).expect("source block start");
    let tail = &source[start_idx..];
    let end_idx = tail.find(end).expect("source block end");
    &tail[..end_idx]
}

fn source_without_whitespace(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[cfg(target_os = "macos")]
#[test]
fn renderer_initializes_default_pipelines_from_embedded_metallib_on_macos() {
    use oxide_renderer_metal::{MetalInitError, MetalRenderer};

    match MetalRenderer::new_default() {
        Ok(_) => {}
        Err(MetalInitError::NoDevice) => {
            panic!("macOS Metal performance contract requires a real Metal device")
        }
        Err(err) => {
            panic!(
            "renderer must initialize from embedded default.metallib without runtime shader fallback: {err}"
         )
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn disabled_accounting_path_keeps_new_stats_zero()
{
   use oxide_renderer_api::{self as api, Renderer};
   use oxide_renderer_metal::{MetalInitError, MetalRenderer};

   let mut renderer = match MetalRenderer::new_default()
   {
      Ok(renderer) => renderer,
      Err(MetalInitError::NoDevice) => panic!("macOS accounting contract requires Metal"),
      Err(error) => panic!("create Metal renderer: {error}"),
   };
   renderer.set_accounting_stats_enabled_for_benchmark(false);
   renderer.resize(64, 64, 1.0).expect("resize renderer");
   let mut list = api::DrawList::default();
   list.items.push(api::DrawCmd::RRect {
      rect: api::RectF::new(8.0, 8.0, 48.0, 48.0),
      radii: [4.0; 4],
      color: api::Color::rgba(0.2, 0.4, 0.8, 1.0),
   });
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(&list);
   renderer.submit(token).expect("submit frame");
   let stats = renderer.last_stats();

   assert_eq!(stats.commands_traversed, 0);
   assert_eq!(stats.render_passes, 0);
   assert_eq!(stats.command_buffers, 0);
   assert_eq!(stats.actual_submissions, 0);
   assert_eq!(stats.memory.logical_total_bytes, 0);
   assert_eq!(stats.memory.total_bytes, 0);
}
