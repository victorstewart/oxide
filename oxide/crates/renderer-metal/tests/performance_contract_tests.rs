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
fn analytic_instance_families_stream_through_frame_rings()
{
   let source = include_str!("../src/lib.rs");
   assert!(!source.contains("METAL_SET_BYTES_LIMIT"));
   assert!(!source.contains("max_instances_per_set_bytes"));
   assert!(source.contains("reserve_analytic_instance_slice::<RRectGpuParams>"));
   assert!(source.contains("reserve_analytic_instance_slice::<NineSliceGpuParams>"));
   assert_eq!(
      source.matches("reserve_analytic_instance_pair::<[f32; 4], ImageGpuParams>").count(),
      2,
   );
   assert!(source.contains("reserve_analytic_instance_pair::<[f32; 4], SpinnerGpuParams>"));
   assert_eq!(
      source.matches("reserve_analytic_instance_slice::<[f32; 8]>").count(),
      2,
      "backdrop and visual-effect runs must share one frame-ring record per instance",
   );

   let ui_shader = include_str!("../shaders/ui.metal");
   for params in ["RRectParams", "NineSliceParams", "SpinnerParams", "ImageParams"]
   {
      assert!(
         ui_shader.contains(&format!("const device {params}*")),
         "{params} arrays must support ring slices larger than constant address-space limits",
      );
   }
   let effects_shader = include_str!("../shaders/effects.metal");
   assert!(effects_shader.contains("const device BackdropParams*"));
   assert!(effects_shader.contains("const device VisualEffectParams*"));
}

#[test]
fn glyphs_use_compact_instances_without_per_frame_indirect_commands()
{
   let renderer = include_str!("../src/lib.rs");
   let prepared = include_str!("../src/prepared.rs");
   let shader = include_str!("../shaders/text.metal");
   assert!(renderer.contains("size_of::<GlyphGpuInstance>() == 48"));
   assert!(renderer.contains("draw_primitives_instanced"));
   assert!(renderer.contains("MTLPrimitiveType::TriangleStrip"));
   assert!(prepared.contains("draw_primitives_instanced_base_instance"));
   assert!(!renderer.contains("new_indirect_command_buffer_with_descriptor"));
   assert!(!renderer.contains("OXIDE_GLYPH_USE_ICB"));
   assert!(shader.contains("struct GlyphGpuInstance"));
   assert!(shader.contains("vertex GlyphVSOut v_glyph"));
   assert!(shader.contains("vertex GlyphVSOut v_prepared_glyph"));
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
    let end = tail.find("struct Ring").expect("Ring source block");
    let prepare_for_encode = &tail[..end];
    assert!(
        !prepare_for_encode.contains("wait_until_completed"),
        "normal frame-ring reuse must not block the CPU on an in-flight Metal command buffer"
    );
    assert!(
        source.contains("frame_backpressure_skipped")
            && source.contains("let busy_slots = self.frame_in_flight.load(Ordering::Acquire)")
            && source.contains("busy_slots & frame_slot_bit(candidate) == 0"),
        "renderer-metal must select an available frame-ring slot or skip instead of blocking"
    );
}

#[test]
fn visible_and_offscreen_frame_resource_modes_have_explicit_depths()
{
   use oxide_renderer_metal::MetalRendererConfig;

   assert_eq!(MetalRendererConfig::visible_host().frame_resource_depth, 3);
   assert_eq!(MetalRendererConfig::default().frame_resource_depth, 8);

   let source = include_str!("../src/lib.rs");
   assert!(source.contains("frame_in_flight.fetch_or(submitted_slot_bit, Ordering::Release)"));
   assert!(source.contains("frame_in_flight.fetch_and(!submitted_slot_bit, Ordering::Release)"));
   let per_frame = source
      .split_once("struct PerFrame")
      .and_then(|(_, tail)| tail.split_once("struct Ring"))
      .map(|(body, _)| body)
      .expect("PerFrame source block");
   assert!(!per_frame.contains("submitted: Option<CommandBuffer>"));
   assert!(!per_frame.contains("AtomicBool"));
   assert!(!source.contains("maximumDrawableCount"));
}

#[cfg(all(target_os = "macos", feature = "snapshot-tests"))]
#[test]
fn visible_frame_resources_cover_measured_high_water_and_skip_only_when_busy()
{
   use oxide_renderer_api::{self as api, Renderer};
   use oxide_renderer_metal::{MetalRenderer, MetalRendererConfig};

   let mut renderer = MetalRenderer::new_with_config(MetalRendererConfig::visible_host())
      .expect("create visible Metal renderer");
   assert_eq!(renderer.frame_resource_depth_for_snapshot(), 3);
   for slot in 0..3
   {
      assert_eq!(renderer.frame_ring_capacities_for_snapshot(slot), Some([524_288, 65_536, 73_728]));
      renderer.mark_frame_slot_busy_for_snapshot(slot);
   }

   let blocked = renderer.begin_frame(&api::FrameTarget, None);
   assert_eq!(renderer.last_stats().frame_backpressure_skipped, 1);
   renderer.submit(blocked).expect("coalesce blocked frame");

   renderer.release_frame_slot_for_snapshot(2);
   let resumed = renderer.begin_frame(&api::FrameTarget, None);
   assert_eq!(renderer.last_stats().frame_backpressure_skipped, 0);
   assert_eq!(renderer.current_frame_slot_for_snapshot(), 2);
   renderer.submit(resumed).expect("submit resumed empty frame");
}

#[cfg(all(target_os = "macos", feature = "snapshot-tests"))]
#[test]
fn offscreen_frame_resources_retain_deeper_completion_protected_mode()
{
   use oxide_renderer_api::{self as api, Renderer};
   use oxide_renderer_metal::{MetalRenderer, MetalRendererConfig};

   let mut config = MetalRendererConfig::default();
   config.frame_resource_depth = usize::MAX;
   let mut renderer = MetalRenderer::new_with_config(config)
      .expect("create offscreen Metal renderer");
   assert_eq!(renderer.frame_resource_depth_for_snapshot(), 8);
   for slot in 0..8
   {
      renderer.mark_frame_slot_busy_for_snapshot(slot);
   }
   let blocked = renderer.begin_frame(&api::FrameTarget, None);
   assert_eq!(renderer.last_stats().frame_backpressure_skipped, 1);
   renderer.submit(blocked).expect("coalesce saturated offscreen frame");
   renderer.release_frame_slot_for_snapshot(7);

   let mut shallow = MetalRendererConfig::default();
   shallow.frame_resource_depth = 0;
   let shallow = MetalRenderer::new_with_config(shallow).expect("clamp shallow renderer depth");
   assert_eq!(shallow.frame_resource_depth_for_snapshot(), 1);
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
   assert!(source.contains("entry.w == w"));
   assert!(source.contains("entry.h == h"));
   assert!(source.contains("entry.tex.pixel_format() == self.color_format"));
   assert!(source.contains("entry.prepared_key.is_none()"));
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
fn neon_marker_instance_abi_is_explicit()
{
   let source = include_str!("../src/neon_marker_gpu.rs");
   assert!(source.contains("#[repr(C, align(8))]"));
   assert!(source.contains("const _: [(); 72] = [(); core::mem::size_of::<MarkerGpuInstance>()]"));
   assert!(source.contains("offset_of!(MarkerGpuInstance, core_color)") && source.contains("const _: [(); 36]"));
   assert!(source.contains("offset_of!(MarkerGpuInstance, ring_color)") && source.contains("const _: [(); 52]"));
   assert!(source.contains("offset_of!(MarkerGpuInstance, _tail_pad)") && source.contains("const _: [(); 68]"));

   let shader = include_str!("../shaders/neon_marker.metal");
   assert!(shader.contains("packed_float4 core_color;"));
   assert!(shader.contains("packed_float4 ring_color;"));
   assert!(shader.contains("uint _tail_pad;"));
}

#[test]
fn neon_marker_instances_stream_once_through_the_frame_ring()
{
   let source = include_str!("../src/neon_marker_gpu.rs");
   assert!(source.contains("let marker_offset = align_up_usize("));
   assert!(source.contains("self.ub.ensure_capacity(&self.device, slot, marker_offset + marker_bytes)"));
   assert_eq!(source.matches("core::ptr::copy_nonoverlapping(").count(), 1);
   assert!(source.contains("enc.set_vertex_buffer(1, Some(self.ub.buffer(slot)), marker_offset as u64)"));
   assert!(source.contains("enc.set_fragment_buffer(1, Some(self.ub.buffer(slot)), marker_offset as u64)"));
   assert!(!source.contains("enc.set_vertex_bytes(1,"));
   assert!(!source.contains("enc.set_fragment_bytes(1,"));
   assert!(source.contains("self.acc_draws.saturating_add(1)"));
   assert!(source.contains("self.acc_instanced.saturating_add(marker_count as u32)"));
}

#[test]
fn auxiliary_encoders_use_the_selected_frame_slot()
{
   let neon = include_str!("../src/neon_marker_gpu.rs");
   let id_mask = include_str!("../src/id_mask_gpu.rs");
   for source in [neon, id_mask]
   {
      assert!(!source.contains("frame_id % FRAME_RING_SIZE"));
      assert!(!source.contains("frame_id % FRAME_RING_SIZE as u64"));
   }
   assert!(neon.contains("let slot = self.current_frame_slot();"));
   assert_eq!(id_mask.matches("let slot = self.current_frame_slot();").count(), 2);

   let renderer = include_str!("../src/lib.rs");
   assert!(renderer.contains("mark_next_preferred_frame_slot_busy_for_snapshot"));
   assert!(renderer.contains("current_frame_command_buffer_slot_for_snapshot"));
   assert!(renderer.contains("frame_slot_has_command_buffer_for_snapshot"));
   assert!(!renderer.contains("id_mask_targets: alloc::vec::Vec<Option"));
   assert!(renderer.contains("id_mask_snapshot_target: Option"));
   assert!(renderer.contains("id_mask_in_flight_generations:"));
   assert!(renderer.contains("!self.id_mask_generation_in_flight(entry.serial)"));
   assert!(renderer.contains("self.clear_completed_id_mask_generations(busy_slots)"));
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
         assert_eq!((stats.draws, stats.instanced), (1, 0));
      }
      else
      {
         assert_eq!(stats.layer_body_commands_copied, 1);
         assert_eq!(stats.layer_texture_creates, if frame == 0 { 1 } else { 0 });
         assert_eq!(stats.layer_cache_hits, 0);
         assert_eq!(stats.layer_cache_misses, 1);
         assert_eq!(stats.layer_offscreen_draws, 1);
         assert_eq!(stats.layer_double_render_prevented, 1);
         assert_eq!((stats.draws, stats.instanced), (2, 1));
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
fn layer_cache_budget_falls_back_exactly_and_recycles_compatible_textures()
{
   use oxide_renderer_api::{self as api, Renderer};
   use oxide_renderer_metal::{MetalInitError, MetalRenderer};

   fn layer_list(id: u32, size: f32) -> api::DrawList
   {
      let mut list = api::DrawList::default();
      list.items.push(api::DrawCmd::LayerBegin {
         id,
         rect: api::RectF::new(0.0, 0.0, size, size),
         dirty: true,
      });
      list.items.push(api::DrawCmd::RRect {
         rect: api::RectF::new(1.0, 1.0, size - 2.0, size - 2.0),
         radii: [2.0; 4],
         color: api::Color::rgba(0.2, 0.5, 0.9, 1.0),
      });
      list.items.push(api::DrawCmd::LayerEnd);
      list
   }

   fn render(renderer: &mut MetalRenderer, list: &api::DrawList)
   {
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(list);
      renderer.submit(token).expect("submit layer budget frame");
   }

   let mut renderer = match MetalRenderer::new_default()
   {
      Ok(renderer) => renderer,
      Err(MetalInitError::NoDevice) => panic!("macOS layer budget contract requires Metal"),
      Err(error) => panic!("create Metal renderer: {error}"),
   };
   renderer.resize(96, 96, 1.0).expect("resize renderer");
   renderer.set_layer_cache_budget_bytes(0);
   render(&mut renderer, &layer_list(70, 32.0));
   let fallback = renderer.last_stats();
   assert_eq!(fallback.layer_cache_budget_bytes, 0);
   assert_eq!(fallback.layer_cache_resident_bytes, 0);
   assert_eq!(fallback.layer_texture_creates, 0);
   assert_eq!(fallback.layer_inline_draws, 1);

   renderer.set_layer_cache_budget_bytes(32 * 1024 * 1024);
   render(&mut renderer, &layer_list(71, 32.0));
   let first = renderer.last_stats();
   assert_eq!(first.layer_texture_creates, 1);
   assert!(first.layer_cache_resident_bytes > 0);
   assert!(first.layer_cache_resident_bytes + first.layer_cache_pool_bytes
      <= first.layer_cache_budget_bytes);

   let empty = api::DrawList::default();
   render(&mut renderer, &empty);
   render(&mut renderer, &empty);
   assert_eq!(renderer.last_stats().layer_cache_resident_bytes, first.layer_cache_resident_bytes);

   render(&mut renderer, &layer_list(71, 64.0));
   let resized = renderer.last_stats();
   assert_eq!(resized.layer_texture_creates, 1);
   assert!(resized.layer_cache_pool_bytes > 0);
   render(&mut renderer, &layer_list(71, 32.0));
   let recycled = renderer.last_stats();
   assert_eq!(recycled.layer_texture_creates, 0);
   assert!(recycled.layer_cache_pool_reuses > resized.layer_cache_pool_reuses);
   render(&mut renderer, &layer_list(72, 32.0));
   let navigation = renderer.last_stats();
   assert_eq!(navigation.layer_texture_creates, 0);
   assert!(navigation.layer_cache_pool_reuses > recycled.layer_cache_pool_reuses);

   renderer.purge_layer_cache_for_memory_warning();
   let purged = renderer.last_stats();
   assert_eq!(purged.layer_cache_resident_bytes, 0);
   assert_eq!(purged.layer_cache_pool_bytes, 0);
   assert_eq!(purged.layer_cache_last_purge_reason, 2);
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

#[cfg(all(target_os = "macos", feature = "snapshot-tests"))]
#[test]
fn effect_targets_follow_the_declared_pass_plan_and_purge()
{
   use oxide_renderer_api::{self as api, Renderer};
   use oxide_renderer_metal::{MetalInitError, MetalRenderer};

   fn render(effect: Option<api::DrawCmd>) -> MetalRenderer
   {
      let mut renderer = match MetalRenderer::new_default()
      {
         Ok(renderer) => renderer,
         Err(MetalInitError::NoDevice) => panic!("macOS effect-target contract requires Metal"),
         Err(error) => panic!("create Metal renderer: {error}"),
      };
      renderer.resize(1_200, 800, 1.0).expect("resize renderer");
      let mut list = api::DrawList::default();
      list.items.push(api::DrawCmd::RRect {
         rect: api::RectF::new(0.0, 0.0, 1_200.0, 800.0),
         radii: [0.0; 4],
         color: api::Color::rgba(0.15, 0.25, 0.45, 1.0),
      });
      if let Some(effect) = effect
      {
         list.items.push(effect);
      }
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(&list);
      renderer.submit(token).expect("submit first effect frame");
      renderer
   }

   let direct = render(None);
   assert_eq!(direct.effect_target_presence_for_snapshot(), [false; 8]);
   assert_eq!(direct.last_stats().resource_creates, 0);
   assert_eq!(direct.last_stats().memory.effect_targets_bytes, 0);
   assert_eq!(direct.last_stats().memory.bloom_targets_bytes, 0);

   let zero = render(Some(api::DrawCmd::Backdrop {
      rect: api::RectF::new(200.0, 160.0, 800.0, 480.0),
      sigma: 0.0,
      tint: api::Color::rgba(0.2, 0.2, 0.2, 0.3),
      alpha: 1.0,
   }));
   assert_eq!(
      zero.effect_target_presence_for_snapshot(),
      [true, false, false, false, false, false, false, false],
   );
   assert_eq!(zero.last_stats().resource_creates, 1);
   assert!(zero.last_stats().memory.effect_prepass_bytes > 0);
   assert_eq!(zero.last_stats().memory.effect_blur_chain_bytes, 0);

   let quarter_effect = api::DrawCmd::VisualEffect {
      rect: api::RectF::new(200.0, 160.0, 800.0, 480.0),
      effect: api::VisualEffect::DarkPopup {
         blur_intensity: 0.5,
         tint: api::Color::rgba(0.1, 0.1, 0.1, 0.8),
      },
   };
   let mut quarter = render(Some(quarter_effect.clone()));
   assert_eq!(
      quarter.effect_target_presence_for_snapshot(),
      [true, true, true, true, false, false, false, false],
   );
   assert_eq!(quarter.last_stats().resource_creates, 4);
   assert!(quarter.last_stats().memory.effect_blur_chain_bytes > 0);
   let quarter_bytes = quarter.last_stats().memory.effect_targets_bytes;

   let mut warm = api::DrawList::default();
   warm.items.push(api::DrawCmd::RRect {
      rect: api::RectF::new(0.0, 0.0, 1_200.0, 800.0),
      radii: [0.0; 4],
      color: api::Color::rgba(0.15, 0.25, 0.45, 1.0),
   });
   warm.items.push(quarter_effect);
   let token = quarter.begin_frame(&api::FrameTarget, None);
   quarter.encode_pass(&warm);
   quarter.submit(token).expect("submit warm quarter effect frame");
   assert_eq!(quarter.last_stats().resource_creates, 0);

   let eighth = render(Some(api::DrawCmd::VisualEffect {
      rect: api::RectF::new(200.0, 160.0, 800.0, 480.0),
      effect: api::VisualEffect::DarkPopup {
         blur_intensity: 1.0,
         tint: api::Color::rgba(0.1, 0.1, 0.1, 0.8),
      },
   }));
   assert_eq!(
      eighth.effect_target_presence_for_snapshot(),
      [true, true, true, false, true, true, false, false],
   );
   assert_eq!(eighth.last_stats().resource_creates, 5);
   assert!(eighth.last_stats().memory.effect_targets_bytes < quarter_bytes);

   let mut high = api::DrawList::default();
   high.items.push(api::DrawCmd::RRect {
      rect: api::RectF::new(0.0, 0.0, 1_200.0, 800.0),
      radii: [0.0; 4],
      color: api::Color::rgba(0.15, 0.25, 0.45, 1.0),
   });
   high.items.push(api::DrawCmd::VisualEffect {
      rect: api::RectF::new(200.0, 160.0, 800.0, 480.0),
      effect: api::VisualEffect::DarkPopup {
         blur_intensity: 1.0,
         tint: api::Color::rgba(0.1, 0.1, 0.1, 0.8),
      },
   });
   let token = quarter.begin_frame(&api::FrameTarget, None);
   quarter.encode_pass(&high);
   quarter.submit(token).expect("submit quarter-to-eighth transition");
   assert_eq!(
      quarter.effect_target_presence_for_snapshot(),
      [true, true, true, false, true, true, false, false],
   );
   assert_eq!(quarter.last_stats().resource_creates, 2);

   let mut prepass = api::DrawList::default();
   prepass.items.push(api::DrawCmd::RRect {
      rect: api::RectF::new(0.0, 0.0, 1_200.0, 800.0),
      radii: [0.0; 4],
      color: api::Color::rgba(0.15, 0.25, 0.45, 1.0),
   });
   prepass.items.push(api::DrawCmd::Backdrop {
      rect: api::RectF::new(200.0, 160.0, 800.0, 480.0),
      sigma: 0.0,
      tint: api::Color::rgba(0.2, 0.2, 0.2, 0.3),
      alpha: 1.0,
   });
   let token = quarter.begin_frame(&api::FrameTarget, None);
   quarter.encode_pass(&prepass);
   quarter.submit(token).expect("submit eighth-to-prepass transition");
   assert_eq!(
      quarter.effect_target_presence_for_snapshot(),
      [true, false, false, false, false, false, false, false],
   );
   assert_eq!(quarter.last_stats().resource_creates, 0);

   quarter.purge_effect_targets();
   assert_eq!(quarter.effect_target_presence_for_snapshot(), [false; 8]);
}
