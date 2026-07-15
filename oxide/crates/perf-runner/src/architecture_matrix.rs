use super::*;
use core_graphics_types::geometry::CGSize;
use foreign_types::ForeignTypeRef;
use metal_rs::{Device, MetalLayer, MTLPixelFormat};
use std::collections::HashSet;

struct RetainedScenario
{
   surface: ui::UiSurface,
   leaf: ui::NodeId,
   mixed_sequences: Vec<api::RenderChunkSequence>,
   dirty: bool,
   phase: usize,
}

pub(super) fn push_architecture_matrix_cases(cases: &mut Vec<PerfCaseResult>, smoke: bool) -> Result<()>
{
   for depth in [16_usize, 32]
   {
      for dirty in [false, true]
      {
         let id = format!("cpu.architecture.retained.depth_{depth}.{}", if dirty { "dirty_leaf" } else { "clean" });
         if perf_case_allowed(&id)
         {
            cases.push(retained_tree_case(&id, smoke, depth, dirty));
         }
      }
   }
   for churn in [false, true]
   {
      let suffix = if churn { "one_use_churn" } else { "hot_reuse" };
      let id = format!("cpu.architecture.retained.cache_pressure.{suffix}");
      if perf_case_allowed(&id)
      {
         cases.push(retained_cache_pressure_case(&id, smoke, churn));
      }
   }

   push_if_allowed(cases, "cpu.architecture.animation.surface_300", || animation_surface_case(smoke));
   push_if_allowed(cases, "cpu.architecture.spatial_metadata.glyph_mesh_10000", || {
      retained_spatial_query_case("cpu.architecture.spatial_metadata.glyph_mesh_10000", smoke)
   });
   push_if_allowed(cases, "cpu.architecture.damage.retained_surface_idle_10000", || {
      retained_surface_idle_case(smoke)
   });
   push_if_allowed(cases, "cpu.architecture.damage.retained_surface_dirty_leaf_10000", || {
      retained_surface_dirty_case(
         "cpu.architecture.damage.retained_surface_dirty_leaf_10000",
         "architecture",
         smoke,
      )
   });
   if perf_case_allowed("gpu.architecture.damage.retained_surface_dirty_leaf_10000")
   {
      cases.push(metal_retained_surface_dirty_case(smoke)?);
   }
   push_if_allowed(cases, "cpu.architecture.text.warm_labels_1000", || text_warm_labels_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.new_labels_200", || text_new_labels_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.script_fallback_matrix", || text_script_matrix_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.scale_sdf_matrix", || text_scale_sdf_matrix_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.atlas_eviction", || text_atlas_eviction_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.paged_atlas_locality", || {
      text_paged_atlas_locality_case(smoke)
   });
   push_if_allowed(cases, "cpu.architecture.text.bitmap_options", || {
      text_bitmap_options_case(smoke)
   });
   let id = "gpu.architecture.text.new_labels_200";
   if perf_case_allowed(id)
   {
      let frame_scoped = std::env::var("OXIDE_C43_TEXT_FRAME_SCOPED")
         .map_or(true, |value| value != "0");
      cases.push(metal_text_new_labels_case(id, smoke, frame_scoped)?);
   }
   let id = "gpu.architecture.text.paged_atlas_locality";
   if perf_case_allowed(id)
   {
      cases.push(metal_text_paged_atlas_locality_case(id, smoke)?);
   }
   let id = "gpu.architecture.text.glyph_instances_1000";
   if perf_case_allowed(id)
   {
      cases.push(metal_text_glyph_instances_case(id, smoke)?);
   }
   push_if_allowed(cases, "cpu.architecture.layers.matrix", || layer_matrix_case(smoke));

   for (name, count) in [
      ("rrect_1", 1_usize),
      ("rrect_64", 64),
      ("rrect_1024", 1_024),
      ("spinner_1", 1),
      ("spinner_64", 64),
      ("spinner_512", 512),
      ("neon_64", 64),
      ("neon_1024", 1_024),
      ("nine_slice_64", 64),
      ("nine_slice_512", 512),
   ]
   {
      let id = format!("cpu.architecture.web_primitive.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(primitive_case(&id, smoke, name, count));
      }
   }

   for family in ["rrect", "image", "nine_slice", "spinner", "backdrop", "visual_effect"]
   {
      for count in [1_usize, 64, 1_024, 10_000]
      {
         let id = format!("gpu.architecture.analytic_instances.{family}_{count}");
         if perf_case_allowed(&id)
         {
            cases.push(metal_analytic_instance_case(&id, smoke, family, count)?);
         }
      }
   }

   for name in [
      "backdrop_separated_48",
      "backdrop_coalescible_12",
      "blur_fullscreen",
      "blur_mixed_sigma",
      "blur_edges_corners",
      "nested_layer_effects",
   ]
   {
      let id = format!("cpu.architecture.effects.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(effect_case(&id, smoke, name));
      }
   }

   for name in ["auxiliary_direct", "partial_damage"]
   {
      let id = format!("gpu.architecture.final_target.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_final_target_case(&id, smoke, name)?);
      }
   }

   for name in ["clean_100x100", "dirty_one", "resize", "navigation_churn", "nested", "backdrop_dependency", "memory_warning"]
   {
      let id = format!("gpu.architecture.layers.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_layer_case(&id, smoke, name)?);
      }
   }

   for name in [
      "backdrop_separated_48",
      "backdrop_coalescible_12",
      "blur_fullscreen",
      "blur_mixed_sigma",
      "blur_edges_corners",
      "nested_layer_effects",
      "blur_sigma_2_local",
      "blur_sigma_8_local",
      "blur_sigma_16_fullscreen",
      "blur_sigma_32_fullscreen",
      "blur_sigma_64_fullscreen",
      "target_plan_direct",
      "target_plan_prepass",
      "target_plan_quarter",
      "target_plan_eighth",
   ]
   {
      let id = format!("gpu.architecture.effects.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_effect_case(&id, smoke, name)?);
      }
   }

   for name in [
      "caret_blink",
      "isolated_mutation_10000",
      "moving_node",
      "removed_node",
      "damage_5pct",
      "damage_25pct",
      "damage_100pct",
      "full_direct_then_partial",
   ]
   {
      let id = format!("cpu.architecture.damage.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(damage_case(&id, smoke, name));
      }
   }

   for name in ["transparent_containers", "zero_area"]
   {
      let cpu_id = format!("cpu.architecture.noop.{name}");
      if perf_case_allowed(&cpu_id)
      {
         cases.push(noop_case(&cpu_id, smoke, name));
      }
      let gpu_id = format!("gpu.architecture.noop.{name}");
      if perf_case_allowed(&gpu_id)
      {
         cases.push(metal_noop_case(&gpu_id, smoke, name)?);
      }
   }

   for name in [
      "caret_blink",
      "isolated_mutation_10000",
      "moving_node",
      "removed_node",
      "damage_5pct",
      "damage_25pct",
      "damage_100pct",
      "full_direct_then_partial",
   ]
   {
      let id = format!("gpu.architecture.damage.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_damage_case(&id, smoke, name)?);
      }
   }

   for (name, count) in [
      ("icons_100", 100_usize),
      ("icons_1000", 1_000),
      ("icons_10000", 10_000),
      ("contain_3x", 1_000),
      ("cover_3x", 1_000),
      ("zoom_3x", 1_000),
      ("decode_display_size", 1_000),
      ("release_reuse", 1_000),
      ("minification_mips", 1_000),
   ]
   {
      let id = format!("cpu.architecture.images.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(image_case(&id, smoke, name, count));
      }
   }

   for (name, count) in [
      ("icons_100", 100_usize),
      ("icons_1000", 1_000),
      ("icons_10000", 10_000),
      ("contain_3x", 1_000),
      ("cover_3x", 1_000),
      ("zoom_3x", 1_000),
      ("decode_display_size", 1_000),
      ("release_reuse", 1_000),
      ("minification_mips", 1_000),
   ]
   {
      let id = format!("gpu.architecture.images.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_image_case(&id, smoke, name, count)?);
      }
   }

   for name in [
      "immutable_large_shared",
      "immutable_large_private",
      "immutable_large_auto",
      "immutable_minified_shared",
      "immutable_minified_private_nomip",
      "immutable_minified_shared_mipmapped",
      "immutable_minified_mipmapped",
      "immutable_minified_auto",
      "immutable_small_one_use_shared",
      "immutable_small_one_use_private",
      "immutable_small_one_use_auto",
   ]
   {
      let id = format!("gpu.architecture.images.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_immutable_image_case(&id, smoke, name)?);
      }
   }

   for count in [100_usize, 1_000]
   {
      let name = format!("image_view_cover_grid_{count}");
      let cpu_id = format!("cpu.authoring.image_view_grid.cover_{count}");
      if perf_case_allowed(&cpu_id)
      {
         cases.push(image_case(&cpu_id, smoke, &name, count));
      }
      let gpu_id = format!("gpu.authoring.image_view_grid.cover_{count}");
      if perf_case_allowed(&gpu_id)
      {
         cases.push(metal_image_case(&gpu_id, smoke, &name, count)?);
      }
   }
   let immutable_authoring_id = "gpu.authoring.image_view_grid.immutable_minified";
   if perf_case_allowed(immutable_authoring_id)
   {
      cases.push(metal_immutable_image_case(
         immutable_authoring_id,
         smoke,
         "immutable_minified_auto",
      )?);
   }

   for count in [1_usize, 51, 52, 60, 61, 128, 1_024]
   {
      let id = format!("gpu.architecture.neon_markers.count_{count}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_neon_marker_case(&id, smoke, count)?);
      }
   }

   for change in ["static", "style", "viewport", "projection", "content"]
   {
      for size in [512_usize, 1_024, 2_048]
      {
         for chunk_count in [1_usize, 16, 256]
         {
            let id = format!("gpu.architecture.id_mask.{change}.size_{size}.chunks_{chunk_count}");
            if perf_case_allowed(&id)
            {
               cases.push(id_mask_matrix_case(&id, smoke, change, size, chunk_count)?);
            }
         }
      }
   }
   let multiple_map_id = "gpu.architecture.id_mask.multiple_map.size_512.chunks_16";
   if perf_case_allowed(multiple_map_id)
   {
      cases.push(id_mask_matrix_case(multiple_map_id, smoke, "multiple_map", 512, 16)?);
   }

   for instances in [96_usize, 1_000, 10_000]
   {
      for feature in ["compatible", "one_mesh", "many_meshes", "alpha_order", "viewport_25pct", "culling", "bloom_1", "bloom_3"]
      {
         let id = format!("gpu.architecture.scene3d.instances_{instances}.{feature}");
         if perf_case_allowed(&id)
         {
            cases.push(scene3d_matrix_case(&id, smoke, instances, feature)?);
         }
      }
   }
   for feature in ["bloom_viewport_25pct", "bloom_overlay"]
   {
      let id = format!("gpu.architecture.scene3d.instances_96.{feature}");
      if perf_case_allowed(&id)
      {
         cases.push(scene3d_matrix_case(&id, smoke, 96, feature)?);
      }
   }
   let scene3d_endurance_id = "gpu.architecture.scene3d.create_release_endurance";
   if perf_case_allowed(scene3d_endurance_id)
   {
      cases.push(scene3d_create_release_endurance_case(scene3d_endurance_id, smoke)?);
   }

   for name in ["visible_high_water", "offscreen_growth_stress"]
   {
      let id = format!("gpu.architecture.frame_resources.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_frame_resource_case(&id, smoke, name)?);
      }
   }

   for dirty in [false, true]
   {
      let name = if dirty { "one_dirty" } else { "clean_mixed" };
      let id = format!("gpu.architecture.prepared_chunks.{name}");
      if perf_case_allowed(&id)
      {
         cases.push(metal_prepared_chunk_case(&id, smoke, dirty)?);
      }
   }

   for (id, dirty) in [
      ("gpu.architecture.prepared_layers.clean_100x100", false),
      ("gpu.architecture.prepared_layers.one_dirty_100x100", true),
      ("gpu.authoring.retained_snapshot.prepared_layers_clean_100x100", false),
   ]
   {
      if perf_case_allowed(id)
      {
         cases.push(metal_prepared_layer_case(id, smoke, dirty)?);
      }
   }

   let dynamic_property_id = "gpu.architecture.animation.dynamic_properties_300";
   if perf_case_allowed(dynamic_property_id)
   {
      cases.push(metal_dynamic_property_case(dynamic_property_id, smoke)?);
   }

   for full_damage in [false, true]
   {
      let id = if full_damage
      {
         "gpu.architecture.spatial_metadata.full_damage_glyph_mesh_10000"
      }
      else
      {
         "gpu.architecture.spatial_metadata.small_damage_glyph_mesh_10000"
      };
      if perf_case_allowed(id)
      {
         cases.push(metal_spatial_damage_case(id, smoke, full_damage)?);
      }
   }

   push_if_allowed(cases, "cpu.architecture.idle.static_foreground", || idle_case(smoke));
   Ok(())
}

pub(super) fn retained_spatial_query_case(id: &str, smoke: bool) -> PerfCaseResult
{
   let instance_count = if smoke { 512 } else { 10_000 };
   let instances = spatial_glyph_mesh_instances(api::ImageHandle(1), api::ImageHandle(2), instance_count);
   let snapshot = api::RenderSnapshot::new(
      instances,
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("spatial query snapshot");
   let mut queried = Vec::new();
   let probe = snapshot.query_damage_instances(api::RectI::new(6_002, 26, 2, 2), &mut queried);
   let metadata_bytes = snapshot.metadata_byte_size();
   let family = if id.contains(".authoring.") { "authoring" } else { "architecture" };
   let mut phase = 0_u32;
   let mut case = measure_cpu_case(
      id,
      family,
      smoke,
      true,
      0.20,
      1,
      vec![String::from(
         "Compact retained-instance spatial queries preserve paint order over ten thousand alternating glyph and image-mesh instances without revisiting geometry.",
      )],
      move || {
         let index = phase as usize % instance_count.min(100);
         phase = phase.wrapping_add(1);
         let stats = snapshot.query_damage_instances(
            api::RectI::new((index * 12 + 2) as i32, 26, 2, 2),
            &mut queried,
         );
         stats.entries_visited
            .wrapping_add(stats.entries_matched.rotate_left(11))
            .wrapping_add(queried.first().copied().unwrap_or(0) as u64)
      },
   );
   case.metrics.insert(String::from("instance_count"), instance_count as f64);
   case.metrics.insert(String::from("damage_instances_visited"), probe.entries_visited as f64);
   case.metrics.insert(String::from("damage_instances_matched"), probe.entries_matched as f64);
   case.metrics.insert(String::from("damage_vertices_visited"), 0.0);
   case.metrics.insert(String::from("snapshot_metadata_bytes"), metadata_bytes as f64);
   case
}

pub(super) fn metal_spatial_damage_case(id: &str, smoke: bool, full_damage: bool) -> Result<PerfCaseResult>
{
   let instance_count = if smoke { 512 } else { 10_000 };
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating spatial Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing spatial Metal renderer")?;
   renderer.set_damage_options(true, 0.70, 0.30);
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let atlas = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let snapshot = api::RenderSnapshot::new(
      spatial_glyph_mesh_instances(image, atlas, instance_count),
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("spatial Metal snapshot");
   let initial = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&snapshot).context("warming spatial Metal snapshot")?;
   renderer.submit(initial).context("submitting spatial Metal warmup")?;
   let _ = last_metal_stats_after_submit(&renderer, initial.0);

   let warmups = if smoke { 3 } else { 8 };
   let frames = if smoke { 4 } else { 24 };
   let raw_samples = std::env::var_os("OXIDE_C27_RAW_SAMPLES").is_some();
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut query_samples = Vec::with_capacity(frames);
   let mut instances_visited = 0_u64;
   let mut instances_matched = 0_u64;
   let mut commands_visited = 0_u64;
   let mut commands_matched = 0_u64;
   let mut vertices_visited = 0_u64;
   let mut plan_reuses = 0_u64;
   let mut draws = 0_u64;
   let mut geometry_copied = 0_u64;
   let mut uploads = 0_u64;
   let mut shaded_pixels = 0_u64;
   for frame in 0..warmups + frames
   {
      let selected = frame % 100;
      let damage = api::Damage {
         rects: vec![api::RectI::new((selected * 12 + 2) as i32, 26, 2, 2)],
      };
      let started_at = Instant::now();
      let token = renderer.begin_frame(
         &api::FrameTarget,
         if full_damage { None } else { Some(&damage) },
      );
      renderer.encode_snapshot(&snapshot).with_context(|| format!("encoding {id}"))?;
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, token.0);
      if frame < warmups
      {
         continue;
      }
      frame_samples.push(started_at.elapsed().as_secs_f64() * 1_000.0);
      encode_samples.push(stats.encode_ms);
      gpu_samples.push(stats.gpu_ms);
      query_samples.push(stats.damage_query_ms);
      instances_visited = instances_visited.saturating_add(stats.damage_instances_visited);
      instances_matched = instances_matched.saturating_add(stats.damage_instances_matched);
      commands_visited = commands_visited.saturating_add(stats.damage_commands_visited);
      commands_matched = commands_matched.saturating_add(stats.damage_commands_matched);
      vertices_visited = vertices_visited.saturating_add(stats.damage_vertices_visited);
      plan_reuses = plan_reuses.saturating_add(stats.prepared_plan_reuses);
      draws = draws.saturating_add(u64::from(stats.draws));
      geometry_copied = geometry_copied.saturating_add(stats.geometry_bytes_copied);
      uploads = uploads.saturating_add(stats.buffer_upload_bytes);
      shaded_pixels = shaded_pixels.saturating_add(stats.shaded_damage_px);
   }
   let summary = summarize(&frame_samples);
   let measured = frames as f64;
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_distribution_metrics(&mut metrics, "damage_query_ms", &query_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("instance_count"), instance_count as f64);
   metrics.insert(String::from("damage_instances_visited_avg"), instances_visited as f64 / measured);
   metrics.insert(String::from("damage_instances_matched_avg"), instances_matched as f64 / measured);
   metrics.insert(String::from("damage_commands_visited_avg"), commands_visited as f64 / measured);
   metrics.insert(String::from("damage_commands_matched_avg"), commands_matched as f64 / measured);
   metrics.insert(String::from("damage_vertices_visited_avg"), vertices_visited as f64 / measured);
   metrics.insert(String::from("prepared_plan_reuses_avg"), plan_reuses as f64 / measured);
   metrics.insert(String::from("draws_avg"), draws as f64 / measured);
   metrics.insert(String::from("geometry_bytes_copied_avg"), geometry_copied as f64 / measured);
   metrics.insert(String::from("buffer_upload_bytes_avg"), uploads as f64 / measured);
   metrics.insert(String::from("shaded_damage_pixels_avg"), shaded_pixels as f64 / measured);
   if raw_samples
   {
      for (index, value) in frame_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c27_raw_frame_ms_{index:04}"), value);
      }
      for (index, value) in encode_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c27_raw_encode_ms_{index:04}"), value);
      }
      for (index, value) in gpu_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c27_raw_gpu_ms_{index:04}"), value);
      }
   }
   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from(if id.contains(".authoring.") { "authoring" } else { "architecture" }),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(if full_damage {
         "Full retained glyph/image-mesh damage bypasses the spatial query and remains one linear ordered replay."
      } else {
         "Two-pixel damage queries compact retained metadata, visit no source vertices, and replay only intersecting glyph/image-mesh instances."
      })],
      metrics,
   })
}

pub(super) fn metal_prepared_chunk_case(id: &str, smoke: bool, dirty: bool) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing Metal renderer")?;
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let atlas = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let snapshot_a = prepared_chunk_snapshot(image, atlas, 1, [0.0, 0.0]);
   let snapshot_b = prepared_chunk_snapshot(image, atlas, if dirty { 2 } else { 1 }, [0.25, 0.0]);
   let flat_control = std::env::var_os("OXIDE_C24_FLAT_CONTROL").is_some();
   let mut flat_a = api::DrawList::default();
   let mut flat_b = api::DrawList::default();
   if flat_control
   {
      snapshot_a.flatten_into(&mut flat_a).context("flattening C24 control A")?;
      snapshot_b.flatten_into(&mut flat_b).context("flattening C24 control B")?;
   }
   let warmups = std::env::var("OXIDE_ARCHITECTURE_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups > 0)
      .unwrap_or(if smoke { 2 } else { 8 });
   let frames = std::env::var("OXIDE_ARCHITECTURE_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 4 } else { 24 });
   let raw_samples = std::env::var_os("OXIDE_C24_RAW_SAMPLES").is_some();
   let mut warmup_frame_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut warmup_encode_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut warmup_gpu_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut uploads = 0_u64;
   let mut dynamic_uniform_uploads = 0_u64;
   let mut geometry_copied = 0_u64;
   let mut commands_traversed = 0_u64;
   let mut draws = 0_u64;
   let mut binds = 0_u64;
   let mut hits = 0_u64;
   let mut misses = 0_u64;
   let mut prepared = 0_u64;
   let mut reused = 0_u64;
   let mut evictions = 0_u64;
   let mut prepared_bytes_peak = 0_u64;
   let mut renderer_bytes_peak = 0_u64;

   for frame in 0..(warmups + frames)
   {
      let snapshot = if frame & 1 == 0 { &snapshot_a } else { &snapshot_b };
      let frame_started_at = Instant::now();
      let token = renderer.begin_frame(&api::FrameTarget, Some(snapshot.damage()));
      let frame_id = token.0;
      if flat_control
      {
         renderer.encode_pass(if frame & 1 == 0 { &flat_a } else { &flat_b });
      }
      else
      {
         renderer.encode_snapshot(snapshot).with_context(|| format!("encoding {id}"))?;
      }
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame >= warmups
      {
         frame_samples.push(frame_started_at.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         uploads = uploads.saturating_add(stats.buffer_upload_bytes);
         dynamic_uniform_uploads = dynamic_uniform_uploads.saturating_add(stats.ub_bytes);
         geometry_copied = geometry_copied.saturating_add(stats.geometry_bytes_copied);
         commands_traversed = commands_traversed.saturating_add(stats.commands_traversed);
         draws = draws.saturating_add(u64::from(stats.draws));
         binds = binds.saturating_add(u64::from(stats.image_argument_binds));
         hits = hits.saturating_add(stats.backend_cache_hits);
         misses = misses.saturating_add(stats.backend_cache_misses);
         prepared = prepared.saturating_add(stats.chunks_prepared);
         reused = reused.saturating_add(stats.chunks_reused);
         evictions = evictions.saturating_add(u64::from(stats.cache_evictions));
         prepared_bytes_peak = prepared_bytes_peak.max(renderer.prepared_cache_resident_bytes());
         renderer_bytes_peak = renderer_bytes_peak.max(stats.memory.total_bytes);
      }
      else if raw_samples
      {
         warmup_frame_samples.push(frame_started_at.elapsed().as_secs_f64() * 1_000.0);
         warmup_encode_samples.push(stats.encode_ms);
         warmup_gpu_samples.push(stats.gpu_ms);
      }
   }

   let summary = summarize(&frame_samples);
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("chunk_count"), 256.0);
   metrics.insert(String::from("dirty_chunks_per_frame"), if dirty { 1.0 } else { 0.0 });
   metrics.insert(String::from("buffer_upload_bytes_avg"), uploads as f64 / frames as f64);
   metrics.insert(String::from("dynamic_uniform_upload_bytes_avg"), dynamic_uniform_uploads as f64 / frames as f64);
   metrics.insert(String::from("geometry_bytes_copied_avg"), geometry_copied as f64 / frames as f64);
   metrics.insert(String::from("commands_traversed_avg"), commands_traversed as f64 / frames as f64);
   metrics.insert(String::from("draws_avg"), draws as f64 / frames as f64);
   metrics.insert(String::from("image_argument_binds_avg"), binds as f64 / frames as f64);
   metrics.insert(String::from("backend_cache_hits_avg"), hits as f64 / frames as f64);
   metrics.insert(String::from("backend_cache_misses_avg"), misses as f64 / frames as f64);
   metrics.insert(String::from("chunks_prepared_avg"), prepared as f64 / frames as f64);
   metrics.insert(String::from("chunks_reused_avg"), reused as f64 / frames as f64);
   metrics.insert(String::from("cache_evictions_total"), evictions as f64);
   metrics.insert(String::from("prepared_cache_bytes_peak"), prepared_bytes_peak as f64);
   metrics.insert(String::from("renderer_bytes_peak"), renderer_bytes_peak as f64);
   if raw_samples
   {
      for (index, value) in warmup_frame_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c24_warmup_frame_ms_{index:04}"), value);
      }
      for (index, value) in warmup_encode_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c24_warmup_encode_ms_{index:04}"), value);
      }
      for (index, value) in warmup_gpu_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c24_warmup_gpu_ms_{index:04}"), value);
      }
      for (index, value) in frame_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c24_raw_frame_ms_{index:04}"), value);
      }
      for (index, value) in encode_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c24_raw_encode_ms_{index:04}"), value);
      }
      for (index, value) in gpu_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c24_raw_gpu_ms_{index:04}"), value);
      }
   }

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from(if id.contains(".authoring.") { "authoring" } else { "architecture" }),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(if dirty {
         "Persistent Metal buffers rebuild exactly one alternating geometry revision while the other 255 chunks remain reusable."
      } else {
         "Persistent Metal buffers replay 256 mixed immutable chunks while a dynamic transform property changes without geometry uploads."
      })],
      metrics,
   })
}

fn prepared_layer_matrix_snapshot(first_geometry_revision: u64, first_dirty: bool) -> api::RenderSnapshot
{
   let mut instances = Vec::with_capacity(100);
   for layer in 0..100_usize
   {
      let mut list = api::DrawList::default();
      list.items.reserve(100);
      for draw in 0..100_usize
      {
         list.items.push(api::DrawCmd::RRect {
            rect: api::RectF::new(
               (draw % 10) as f32 * 9.0 + 2.0,
               (draw / 10) as f32 * 6.0 + 2.0,
               7.0,
               4.0,
            ),
            radii: [1.0; 4],
            color: api::Color::rgba(
               0.18 + (layer % 5) as f32 * 0.07,
               0.48,
               0.88,
               0.92,
            ),
         });
      }
      let chunk = api::RenderChunk::new(
         api::RenderChunkId(29_000 + layer as u64),
         api::RenderChunkRevisions {
            structural: 1,
            geometry: if layer == 0 { first_geometry_revision } else { 1 },
            resource: 0,
            dynamic_properties: 0,
         },
         list,
         api::ChunkIndexMode::Local,
         &[],
      ).expect("prepared layer benchmark chunk");
      let mut instance = api::RenderChunkInstance::new(
         chunk,
         [
            (layer % 10) as f32 * 116.0 + 10.0,
            (layer / 10) as f32 * 76.0 + 10.0,
         ],
      );
      instance.layer = Some(api::RenderLayerInstance {
         id: 29_000 + layer as u32,
         rect: api::RectF::new(0.0, 0.0, 96.0, 64.0),
         dirty: first_dirty && layer == 0,
      });
      instances.push(instance);
   }
   api::RenderSnapshot::new(
      instances,
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("prepared layer benchmark snapshot")
}

fn metal_prepared_layer_case(id: &str, smoke: bool, dirty: bool) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating prepared-layer Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing prepared-layer Metal renderer")?;
   renderer.set_memory_stats_enabled_for_benchmark(true);
   let snapshot_a = prepared_layer_matrix_snapshot(1, dirty);
   let snapshot_b = prepared_layer_matrix_snapshot(1, dirty);
   let flat_control = std::env::var_os("OXIDE_C29_FLAT_CONTROL").is_some();
   let mut flat_a = api::DrawList::default();
   let mut flat_b = api::DrawList::default();
   if flat_control
   {
      snapshot_a.flatten_into(&mut flat_a).context("flattening C29 control A")?;
      snapshot_b.flatten_into(&mut flat_b).context("flattening C29 control B")?;
   }
   let warmups = std::env::var("OXIDE_ARCHITECTURE_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups > 0)
      .unwrap_or(if smoke { 2 } else { 8 });
   let frames = std::env::var("OXIDE_ARCHITECTURE_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 4 } else { 24 });
   let raw_samples = std::env::var_os("OXIDE_C29_RAW_SAMPLES").is_some();
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut body_scans = 0_u64;
   let mut body_copies = 0_u64;
   let mut geometry_copies = 0_u64;
   let mut uploads = 0_u64;
   let mut texture_creates = 0_u64;
   let mut cache_hits = 0_u64;
   let mut cache_misses = 0_u64;
   let mut offscreen_draws = 0_u64;
   let mut render_passes = 0_u64;
   let mut draws = 0_u64;
   let mut chunks_prepared = 0_u64;
   let mut layer_bytes_peak = 0_u64;

   for frame in 0..warmups.saturating_add(frames)
   {
      let use_b = dirty && frame & 1 == 1;
      let snapshot = if use_b { &snapshot_b } else { &snapshot_a };
      let started_at = Instant::now();
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      if flat_control
      {
         renderer.encode_pass(if use_b { &flat_b } else { &flat_a });
      }
      else
      {
         renderer.encode_snapshot(snapshot).with_context(|| format!("encoding {id}"))?;
      }
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame < warmups
      {
         continue;
      }
      frame_samples.push(started_at.elapsed().as_secs_f64() * 1_000.0);
      encode_samples.push(stats.encode_ms);
      gpu_samples.push(stats.gpu_ms);
      body_scans = body_scans.saturating_add(stats.layer_body_commands_scanned);
      body_copies = body_copies.saturating_add(stats.layer_body_commands_copied);
      geometry_copies = geometry_copies.saturating_add(stats.geometry_bytes_copied);
      uploads = uploads.saturating_add(stats.buffer_upload_bytes);
      texture_creates = texture_creates.saturating_add(u64::from(stats.layer_texture_creates));
      cache_hits = cache_hits.saturating_add(u64::from(stats.layer_cache_hits));
      cache_misses = cache_misses.saturating_add(u64::from(stats.layer_cache_misses));
      offscreen_draws = offscreen_draws.saturating_add(stats.layer_offscreen_draws);
      render_passes = render_passes.saturating_add(u64::from(stats.render_passes));
      draws = draws.saturating_add(u64::from(stats.draws));
      chunks_prepared = chunks_prepared.saturating_add(stats.chunks_prepared);
      layer_bytes_peak = layer_bytes_peak.max(stats.memory.layer_cache_bytes);
   }

   let summary = summarize(&frame_samples);
   let measured = frames.max(1) as f64;
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("layers"), 100.0);
   metrics.insert(String::from("draws_per_layer"), 100.0);
   metrics.insert(String::from("dirty_layers_per_frame"), if dirty { 1.0 } else { 0.0 });
   metrics.insert(String::from("layer_body_commands_scanned_avg"), body_scans as f64 / measured);
   metrics.insert(String::from("layer_body_commands_copied_avg"), body_copies as f64 / measured);
   metrics.insert(String::from("geometry_bytes_copied_avg"), geometry_copies as f64 / measured);
   metrics.insert(String::from("buffer_upload_bytes_avg"), uploads as f64 / measured);
   metrics.insert(String::from("layer_texture_creates_avg"), texture_creates as f64 / measured);
   metrics.insert(String::from("layer_cache_hits_avg"), cache_hits as f64 / measured);
   metrics.insert(String::from("layer_cache_misses_avg"), cache_misses as f64 / measured);
   metrics.insert(String::from("layer_offscreen_draws_avg"), offscreen_draws as f64 / measured);
   metrics.insert(String::from("render_passes_avg"), render_passes as f64 / measured);
   metrics.insert(String::from("draws_avg"), draws as f64 / measured);
   metrics.insert(String::from("chunks_prepared_avg"), chunks_prepared as f64 / measured);
   metrics.insert(String::from("layer_cache_bytes_peak"), layer_bytes_peak as f64);
   metrics.insert(String::from("flat_control"), if flat_control { 1.0 } else { 0.0 });
   if raw_samples
   {
      insert_indexed_samples(&mut metrics, "c29_raw_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "c29_raw_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "c29_raw_gpu_ms", &gpu_samples);
   }
   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from(if id.contains(".authoring.") { "authoring" } else { "architecture" }),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(if dirty {
         "One of one hundred retained layers is explicitly dirty and refreshes once from its prepared body while ninety-nine layers composite body-free."
      } else {
         "One hundred retained layers with one hundred draws each composite from generation-keyed Metal textures without body traversal."
      })],
      metrics,
   })
}

fn metal_dynamic_property_case(id: &str, smoke: bool) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating dynamic-property Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing dynamic-property Metal renderer")?;
   let image = renderer.image_create_rgba8(2, 2, &[255; 16], 8);
   let atlas = renderer.image_create_a8(2, 2, &[255; 4], 2);
   let instances = dynamic_property_instances(image, atlas);
   let snapshot_a = dynamic_property_snapshot(&instances, 0);
   let snapshot_b = dynamic_property_snapshot(&instances, 1);
   let warmups = std::env::var("OXIDE_ARCHITECTURE_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups >= 3)
      .unwrap_or(if smoke { 4 } else { 8 });
   let frames = std::env::var("OXIDE_ARCHITECTURE_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 4 } else { 24 });
   let raw_samples = std::env::var_os("OXIDE_C26_RAW_SAMPLES").is_some();
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut property_upload_bytes = 0_u64;
   let mut property_records_updated = 0_u64;
   let mut geometry_upload_bytes = 0_u64;
   let mut geometry_bytes_copied = 0_u64;
   let mut commands_traversed = 0_u64;
   let mut cache_hits = 0_u64;
   let mut cache_misses = 0_u64;
   let mut property_ring_bytes_peak = 0_u64;

   for frame in 0..warmups.saturating_add(frames)
   {
      let snapshot = if frame & 1 == 0 { &snapshot_a } else { &snapshot_b };
      let frame_started_at = Instant::now();
      let token = renderer.begin_frame(&api::FrameTarget, Some(snapshot.damage()));
      let frame_id = token.0;
      renderer.encode_snapshot(snapshot).with_context(|| format!("encoding {id}"))?;
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame < warmups
      {
         continue;
      }
      frame_samples.push(frame_started_at.elapsed().as_secs_f64() * 1_000.0);
      encode_samples.push(stats.encode_ms);
      gpu_samples.push(stats.gpu_ms);
      property_upload_bytes = property_upload_bytes.saturating_add(stats.property_upload_bytes);
      property_records_updated = property_records_updated.saturating_add(u64::from(stats.property_records_updated));
      geometry_upload_bytes = geometry_upload_bytes.saturating_add(stats.buffer_upload_bytes);
      geometry_bytes_copied = geometry_bytes_copied.saturating_add(stats.geometry_bytes_copied);
      commands_traversed = commands_traversed.saturating_add(stats.commands_traversed);
      cache_hits = cache_hits.saturating_add(stats.backend_cache_hits);
      cache_misses = cache_misses.saturating_add(stats.backend_cache_misses);
      property_ring_bytes_peak = property_ring_bytes_peak.max(stats.property_ring_bytes);
   }

   let summary = summarize(&frame_samples);
   let measured = frames.max(1) as f64;
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("animated_nodes"), 300.0);
   metrics.insert(String::from("text_nodes"), 200.0);
   metrics.insert(String::from("image_nodes"), 100.0);
   metrics.insert(String::from("property_records"), 300.0);
   metrics.insert(String::from("property_upload_bytes_avg"), property_upload_bytes as f64 / measured);
   metrics.insert(String::from("property_records_updated_avg"), property_records_updated as f64 / measured);
   metrics.insert(String::from("property_ring_bytes_peak"), property_ring_bytes_peak as f64);
   metrics.insert(String::from("buffer_upload_bytes_avg"), geometry_upload_bytes as f64 / measured);
   metrics.insert(String::from("geometry_bytes_copied_avg"), geometry_bytes_copied as f64 / measured);
   metrics.insert(String::from("commands_traversed_avg"), commands_traversed as f64 / measured);
   metrics.insert(String::from("backend_cache_hits_avg"), cache_hits as f64 / measured);
   metrics.insert(String::from("backend_cache_misses_avg"), cache_misses as f64 / measured);
   if raw_samples
   {
      for (index, value) in frame_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c26_raw_frame_ms_{index:04}"), value);
      }
      for (index, value) in encode_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c26_raw_encode_ms_{index:04}"), value);
      }
      for (index, value) in gpu_samples.iter().copied().enumerate()
      {
         metrics.insert(format!("c26_raw_gpu_ms_{index:04}"), value);
      }
   }
   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from("engine"),
      scenario: String::from("rendering-architecture"),
      variant: String::from("oxide"),
      cache_state: String::from("warm"),
      refresh_mode: String::from("offscreen"),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(
         "Three hundred retained text/image instances alternate full affine and opacity properties through the completion-safe Metal property ring after immutable geometry warmup.",
      )],
      metrics,
   })
}

fn dynamic_property_instances(image: api::ImageHandle, atlas: api::ImageHandle) -> Vec<api::RenderChunkInstance>
{
   let image_list = api::DrawList {
      items: vec![api::DrawCmd::Image {
         tex: image,
         dst: api::RectF::new(0.0, 0.0, 28.0, 28.0),
         src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
         alpha: 1.0,
      }],
      vertices: Vec::new(),
      indices: Vec::new(),
   };
   let image_chunk = api::RenderChunk::new(
      api::RenderChunkId(26_000),
      api::RenderChunkRevisions { resource: 1, ..api::RenderChunkRevisions::default() },
      image_list,
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image, generation: 1 }],
   ).expect("dynamic property image chunk");
   let text_chunk = api::RenderChunk::new(
      api::RenderChunkId(26_001),
      api::RenderChunkRevisions { resource: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::GlyphRun { run: api::GlyphRun {
            atlas,
            atlas_revision: 1,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: api::Color::rgba(0.92, 0.94, 1.0, 1.0),
         }}],
         vertices: vec![
            api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 28.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
            api::Vertex { x: 28.0, y: 28.0, u: 1.0, v: 1.0, rgba: 0 },
            api::Vertex { x: 0.0, y: 28.0, u: 0.0, v: 1.0, rgba: 0 },
         ],
         indices: vec![0, 1, 2, 0, 2, 3],
      },
      api::ChunkIndexMode::Local,
      &[api::RenderResourceDependency { image: atlas, generation: 1 }],
   ).expect("dynamic property glyph chunk");
   (0..300_usize).map(|index| {
      let chunk = if index < 200 { text_chunk.clone() } else { image_chunk.clone() };
      let origin = [
         (index % 20) as f32 * 40.0 + 24.0,
         (index / 20) as f32 * 40.0 + 24.0,
      ];
      let mut instance = api::RenderChunkInstance::new(chunk, origin);
      instance.property_slots = vec![
         api::RenderPropertySlotId::dynamic((index * 2 + 1) as u32, 1).expect("dynamic transform slot"),
         api::RenderPropertySlotId::dynamic((index * 2 + 2) as u32, 1).expect("dynamic opacity slot"),
      ].into();
      instance
   }).collect()
}

fn dynamic_property_snapshot(instances: &[api::RenderChunkInstance], phase: u64) -> api::RenderSnapshot
{
   let mut properties = Vec::with_capacity(instances.len().saturating_mul(2));
   for index in 0..instances.len()
   {
      let angle = if phase & 1 == 0 { -0.035 } else { 0.035 };
      let scale = if phase & 1 == 0 { 0.96 } else { 1.04 };
      let (sin, cos) = f32::sin_cos(angle + (index % 7) as f32 * 0.001);
      properties.push(api::RenderPropertySlot {
         id: api::RenderPropertySlotId::dynamic((index * 2 + 1) as u32, 1).expect("dynamic transform property"),
         revision: phase.saturating_add(1),
         value: api::RenderPropertyValue::Transform([
            cos * scale,
            sin * scale,
            -sin * scale,
            cos * scale,
            if phase & 1 == 0 { -1.25 } else { 1.25 },
            if phase & 1 == 0 { 0.75 } else { -0.75 },
         ]),
      });
      properties.push(api::RenderPropertySlot {
         id: api::RenderPropertySlotId::dynamic((index * 2 + 2) as u32, 1).expect("dynamic opacity property"),
         revision: phase.saturating_add(1),
         value: api::RenderPropertyValue::Opacity(if phase & 1 == 0 { 0.72 } else { 0.96 }),
      });
   }
   api::RenderSnapshot::new(
      instances.to_vec(),
      properties,
      api::Damage { rects: Vec::new() },
   ).expect("dynamic property snapshot")
}

fn spatial_glyph_mesh_instances(image: api::ImageHandle, atlas: api::ImageHandle, count: usize) -> Vec<api::RenderChunkInstance>
{
   let vertices = vec![
      api::Vertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 8.0, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
      api::Vertex { x: 8.0, y: 10.0, u: 1.0, v: 1.0, rgba: 0 },
      api::Vertex { x: 0.0, y: 10.0, u: 0.0, v: 1.0, rgba: 0 },
   ];
   let indices = vec![0, 1, 2, 0, 2, 3];
   let mesh = api::RenderChunk::new(
      api::RenderChunkId(27_000),
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
   ).expect("spatial image-mesh chunk");
   let glyph = api::RenderChunk::new(
      api::RenderChunkId(27_001),
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
   (0..count).map(|index| {
      api::RenderChunkInstance::new(
         if index & 1 == 0 { mesh.clone() } else { glyph.clone() },
         [index as f32 * 12.0, 24.0],
      )
   }).collect()
}

fn prepared_chunk_snapshot(image: api::ImageHandle, atlas: api::ImageHandle, dirty_revision: u64, translation: [f32; 2]) -> api::RenderSnapshot
{
   let mut instances = Vec::with_capacity(256);
   for index in 0..256_usize
   {
      let column = index % 16;
      let row = index / 16;
      let origin = [column as f32 * 72.0 + 8.0, row as f32 * 46.0 + 8.0];
      let id = api::RenderChunkId(24_000 + index as u64);
      let geometry = if index == 0 { dirty_revision } else { 1 };
      let (list, dependencies) = match index & 3
      {
         0 =>
         {
            let mut list = api::DrawList::default();
            for offset in 0..64
            {
               list.items.push(api::DrawCmd::RRect {
                  rect: api::RectF::new(offset as f32 * 0.75, 0.0, 0.625, 28.0),
                  radii: [0.25; 4],
                  color: api::Color::rgba(0.15 + (offset & 3) as f32 * 0.15, 0.45, 0.85, 1.0),
               });
            }
            (list, Vec::new())
         }
         1 =>
         {
            let mut list = api::DrawList::default();
            for offset in 0..64
            {
               list.items.push(api::DrawCmd::Image {
                  tex: image,
                  dst: api::RectF::new(offset as f32 * 0.75, 0.0, 0.625, 28.0),
                  src: api::RectF::new(0.0, 0.0, 2.0, 2.0),
                  alpha: 1.0,
               });
            }
            (list, vec![api::RenderResourceDependency { image, generation: 1 }])
         }
         2 =>
         {
            let mut vertices = Vec::with_capacity(256);
            let mut indices = Vec::with_capacity(384);
            for glyph in 0..64_u16
            {
               let x = f32::from(glyph) * 0.75;
               let base = glyph * 4;
               vertices.extend_from_slice(&[
                  api::Vertex { x, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
                  api::Vertex { x: x + 0.625, y: 0.0, u: 1.0, v: 0.0, rgba: 0 },
                  api::Vertex { x: x + 0.625, y: 28.0, u: 1.0, v: 1.0, rgba: 0 },
                  api::Vertex { x, y: 28.0, u: 0.0, v: 1.0, rgba: 0 },
               ]);
               indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            (api::DrawList {
               items: vec![api::DrawCmd::GlyphRun { run: api::GlyphRun {
                  atlas,
                  atlas_revision: 1,
                  vb: api::VertexSpan { offset: 0, len: 256 },
                  ib: api::IndexSpan { offset: 0, len: 384 },
                  sdf: false,
                  color: api::Color::rgba(0.9, 0.9, 1.0, 1.0),
               }}],
               vertices,
               indices,
            }, vec![api::RenderResourceDependency { image: atlas, generation: 1 }])
         }
         _ =>
         {
            let mut vertices = Vec::with_capacity(192);
            let mut indices = Vec::with_capacity(192);
            for triangle in 0..64_u16
            {
               let x = f32::from(triangle) * 0.75;
               let base = triangle * 3;
               vertices.extend_from_slice(&[
                  api::Vertex { x, y: 28.0, u: 0.0, v: 0.0, rgba: 0 },
                  api::Vertex { x: x + 0.3125, y: 0.0, u: 0.0, v: 0.0, rgba: 0 },
                  api::Vertex { x: x + 0.625, y: 28.0, u: 0.0, v: 0.0, rgba: 0 },
               ]);
               indices.extend_from_slice(&[base, base + 1, base + 2]);
            }
            (api::DrawList {
               items: vec![api::DrawCmd::Solid {
                  vb: api::VertexSpan { offset: 0, len: 192 },
                  ib: api::IndexSpan { offset: 0, len: 192 },
                  color: api::Color::rgba(0.9, 0.55, 0.1, 1.0),
               }],
               vertices,
               indices,
            }, Vec::new())
         }
      };
      let chunk = api::RenderChunk::new(
         id,
         api::RenderChunkRevisions { geometry, resource: u64::from(!dependencies.is_empty()), ..api::RenderChunkRevisions::default() },
         list,
         api::ChunkIndexMode::Local,
         &dependencies,
      ).expect("prepared perf chunk");
      let mut instance = api::RenderChunkInstance::new(chunk, origin);
      instance.property_slots = vec![api::RenderPropertySlotId(1)].into();
      instances.push(instance);
   }
   api::RenderSnapshot::new(
      instances,
      vec![api::RenderPropertySlot {
         id: api::RenderPropertySlotId(1),
         revision: u64::from(translation != [0.0, 0.0]),
         value: api::RenderPropertyValue::Transform([1.0, 0.0, 0.0, 1.0, translation[0], translation[1]]),
      }],
      api::Damage { rects: Vec::new() },
   ).expect("prepared perf snapshot")
}

fn frame_resource_drawlist(quads: usize) -> api::DrawList
{
   let mut list = api::DrawList::default();
   list.vertices.reserve(quads * 4);
   list.indices.reserve(quads * 6);
   for quad in 0..quads
   {
      let base = (quad * 4) as u16;
      let x = (quad % 128) as f32 * 8.0;
      let y = (quad / 128) as f32 * 8.0;
      list.vertices.extend_from_slice(&[
         api::Vertex { x, y, u: 0.0, v: 0.0, rgba: u32::MAX },
         api::Vertex { x: x + 7.0, y, u: 1.0, v: 0.0, rgba: u32::MAX },
         api::Vertex { x, y: y + 7.0, u: 0.0, v: 1.0, rgba: u32::MAX },
         api::Vertex { x: x + 7.0, y: y + 7.0, u: 1.0, v: 1.0, rgba: u32::MAX },
      ]);
      list.indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
   }
   list.items.push(api::DrawCmd::Solid {
      vb: api::VertexSpan { offset: 0, len: list.vertices.len() as u32 },
      ib: api::IndexSpan { offset: 0, len: list.indices.len() as u32 },
      color: api::Color::rgba(0.25, 0.55, 0.9, 1.0),
   });
   list
}

fn metal_frame_resource_case(id: &str, smoke: bool, name: &str) -> Result<PerfCaseResult>
{
   let visible = name == "visible_high_water";
   let config = if visible {
      metal::MetalRendererConfig::visible_host()
   } else {
      metal::MetalRendererConfig::default()
   };
   let quads = if visible { 4_096 } else { 8_192 };
   let warmups = config.frame_resource_depth;
   let frames = if smoke { 60 } else { 120 };
   let list = frame_resource_drawlist(quads);
   let mut renderer = metal::MetalRenderer::new_with_config(config)
      .context("creating frame-resource Metal renderer")?;
   renderer.resize(1_200, 800, 1.0).context("resizing frame-resource renderer")?;
   let mut cold_growths = 0_u64;
   let mut warm_growths = 0_u64;
   let mut skips = 0_u64;
   let mut ring_bytes_peak = 0_u64;
   let mut encode_samples = Vec::with_capacity(frames);
   let mut frame_samples = Vec::with_capacity(frames);
   let mut vb_bytes = 0_u64;
   let mut ib_bytes = 0_u64;
   let mut ub_bytes = 0_u64;

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      renderer.encode_pass(&list);
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      ring_bytes_peak = ring_bytes_peak.max(stats.memory.frame_ring_buffer_bytes);
      if frame < warmups
      {
         cold_growths = cold_growths.saturating_add(stats.resource_grows as u64);
      }
      else
      {
         frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         warm_growths = warm_growths.saturating_add(stats.resource_grows as u64);
         skips = skips.saturating_add(stats.frame_backpressure_skipped as u64);
         vb_bytes = stats.vb_bytes;
         ib_bytes = stats.ib_bytes;
         ub_bytes = stats.ub_bytes;
      }
   }

   let summary = summarize(&frame_samples);
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("frame_resource_depth"), config.frame_resource_depth as f64);
   metrics.insert(String::from("frame_ring_buffer_bytes_peak"), ring_bytes_peak as f64);
   metrics.insert(String::from("cold_resource_grows"), cold_growths as f64);
   metrics.insert(String::from("warm_resource_grows"), warm_growths as f64);
   metrics.insert(String::from("frame_backpressure_skips"), skips as f64);
   metrics.insert(String::from("vertex_upload_bytes"), vb_bytes as f64);
   metrics.insert(String::from("index_upload_bytes"), ib_bytes as f64);
   metrics.insert(String::from("uniform_upload_bytes"), ub_bytes as f64);
   metrics.insert(String::from("quads"), quads as f64);
   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from("engine"),
      scenario: String::from("rendering-architecture"),
      variant: String::from("oxide"),
      cache_state: String::from("warm"),
      refresh_mode: String::from(if visible { "visible-host" } else { "offscreen" }),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![if visible {
         String::from("Three-slot visible renderer at the measured 4,096-quad no-growth high-water workload.")
      } else {
         String::from("Eight-slot offscreen renderer grows every slot under 8,192-quad stress, then remains allocation-free when warm.")
      }],
      metrics,
   })
}

fn push_if_allowed<F>(cases: &mut Vec<PerfCaseResult>, id: &str, build: F)
where
   F: FnOnce() -> PerfCaseResult,
{
   if perf_case_allowed(id)
   {
      cases.push(build());
   }
}

fn measured_architecture_case<F>(id: &str, smoke: bool, notes: &str, mut run: F) -> PerfCaseResult
where
   F: FnMut() -> u64,
{
   measure_cpu_case(
      id,
      "architecture",
      smoke,
      true,
      0.20,
      1,
      vec![String::from(notes)],
      move || run(),
   )
}

fn retained_tree_case(id: &str, smoke: bool, depth: usize, dirty: bool) -> PerfCaseResult
{
   let mut scenario = build_retained_scenario(depth, dirty);
   let (_, probe) = run_retained_scenario(&mut scenario);
   let mut mixed_retained_bytes = 0_u64;
   for sequence in &scenario.mixed_sequences {
      sequence.visit_instances(|instance| {
         mixed_retained_bytes = mixed_retained_bytes.saturating_add(instance.chunk.byte_size());
      });
   }
   let traversed = scenario.surface.render_snapshot_retained(
      api::RenderChunkId(1),
      &scenario.mixed_sequences,
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("valid retained traversal probe");
   let mut commands_traversed = 0_u64;
   traversed.snapshot.visit_instances(|instance| {
      commands_traversed = commands_traversed
         .saturating_add(instance.chunk.draw_list().items.len() as u64);
   });
   let mut case = measured_architecture_case(
      id,
      smoke,
      "Per-node immutable UiSurface chunks composed with retained glyph and image chunks; clean frames copy zero geometry and a dirty leaf rebuilds exactly one chunk.",
      move || run_retained_scenario(&mut scenario).0,
   );
   case.metrics.insert(String::from("tree_depth"), depth as f64);
   case.metrics.insert(String::from("label_nodes"), 1_000.0);
   case.metrics.insert(String::from("image_nodes"), 500.0);
   case.metrics.insert(String::from("dirty_nodes"), if dirty { 1.0 } else { 0.0 });
   case.metrics.insert(String::from("layout_passes_expected"), if dirty { 1.0 } else { 0.0 });
   case.metrics.insert(String::from("chunks_reused"), probe.chunks_reused as f64);
   case.metrics.insert(String::from("chunks_rebuilt"), probe.chunks_rebuilt as f64);
   case.metrics.insert(String::from("sequences_reused"), probe.sequences_reused as f64);
   case.metrics.insert(String::from("sequences_rebuilt"), probe.sequences_rebuilt as f64);
   case.metrics.insert(String::from("commands_copied"), (probe.command_bytes_copied as usize / core::mem::size_of::<api::DrawCmd>()) as f64);
   case.metrics.insert(String::from("commands_traversed"), commands_traversed as f64);
   case.metrics.insert(String::from("command_bytes_copied"), probe.command_bytes_copied as f64);
   case.metrics.insert(String::from("vertex_bytes_copied"), probe.vertex_bytes_copied as f64);
   case.metrics.insert(String::from("index_bytes_copied"), probe.index_bytes_copied as f64);
   case.metrics.insert(String::from("retained_chunk_bytes"), probe.retained_bytes.saturating_add(mixed_retained_bytes) as f64);
   case.metrics.insert(String::from("retained_sequence_bytes"), probe.retained_sequence_bytes as f64);
   case.metrics.insert(String::from("flat_fallback_uses"), 0.0);
   case
}

struct RetainedCachePressureScenario
{
   surface: ui::UiSurface,
   content: Vec<api::RenderChunkSequence>,
   churn: bool,
}

fn retained_cache_pressure_case(id: &str, smoke: bool, churn: bool) -> PerfCaseResult
{
   let node_count = if smoke { 256 } else { 1_500 };
   let mut scenario = build_retained_cache_pressure_scenario(node_count, churn);
   let (_, probe) = run_retained_cache_pressure(&mut scenario);
   let notes = if churn {
      "One-use retained churn takes the explicit zero-budget direct rebuild path and retains zero cache bytes."
   } else {
      "Large reusable text/image content and a broad node tree remain retained under a working-set-sized hard budget."
   };
   let mut case = measured_architecture_case(id, smoke, notes, move || {
      run_retained_cache_pressure(&mut scenario).0
   });
   let accesses = probe.cache_hits.saturating_add(probe.cache_misses);
   case.metrics.insert(String::from("node_count"), node_count as f64);
   case.metrics.insert(String::from("cache_hits"), probe.cache_hits as f64);
   case.metrics.insert(String::from("cache_misses"), probe.cache_misses as f64);
   case.metrics.insert(
      String::from("cache_hit_rate"),
      if accesses == 0 { 0.0 } else { probe.cache_hits as f64 / accesses as f64 },
   );
   case.metrics.insert(String::from("cache_admissions"), probe.cache_admissions as f64);
   case.metrics.insert(String::from("cache_admission_rejections"), probe.cache_admission_rejections as f64);
   case.metrics.insert(String::from("cache_evictions"), probe.cache_evictions as f64);
   case.metrics.insert(String::from("cache_evicted_bytes"), probe.cache_evicted_bytes as f64);
   case.metrics.insert(String::from("cache_build_time_ns"), probe.cache_build_time_ns as f64);
   case.metrics.insert(String::from("retained_chunk_bytes"), probe.retained_chunk_bytes as f64);
   case.metrics.insert(String::from("retained_sequence_bytes"), probe.retained_sequence_bytes as f64);
   case.metrics.insert(String::from("prepared_gpu_bytes"), probe.prepared_gpu_bytes as f64);
   case.metrics.insert(String::from("flat_fallback_uses"), probe.flat_fallback_uses as f64);
   case.metrics.insert(String::from("hard_budget_bytes"), scenario_budget(churn) as f64);
   case.metrics.insert(String::from("cache_complete"), f64::from(probe.cache_complete));
   case
}

fn scenario_budget(churn: bool) -> u64
{
   if churn { 0 } else { 1024 * 1024 }
}

fn build_retained_cache_pressure_scenario(node_count: usize, churn: bool) -> RetainedCachePressureScenario
{
   let width = node_count as f32 * 20.0 + 16.0;
   let mut surface = ui::UiSurface::new(ui::NodeStyle {
      axis: ui::Axis::Row,
      size: ui::Size2D { w: ui::Dim::Px(width), h: ui::Dim::Px(48.0) },
      background: api::Color::rgba(0.0, 0.0, 0.0, 0.0),
      ..ui::NodeStyle::default()
   });
   let root = surface.root();
   for index in 0..node_count
   {
      surface.tree_mut().add_node(root, ui::NodeStyle {
         size: ui::Size2D { w: ui::Dim::Px(18.0), h: ui::Dim::Px(18.0) },
         background: api::Color::rgba(0.15 + (index % 7) as f32 * 0.03, 0.45, 0.75, 1.0),
         ..ui::NodeStyle::default()
      });
   }
   surface.layout(width, 48.0);
   surface.set_retained_cache_policy(ui::RetainedCachePolicy {
      cpu_budget_bytes: scenario_budget(churn),
      ..ui::RetainedCachePolicy::default()
   });
   let content = retained_mixed_sequences();
   let mut scenario = RetainedCachePressureScenario { surface, content, churn };
   let _ = run_retained_cache_pressure(&mut scenario);
   scenario
}

fn run_retained_cache_pressure(scenario: &mut RetainedCachePressureScenario) -> (u64, ui::RetainedNodeStats)
{
   if scenario.churn
   {
      scenario.surface.mark_dirty(ui::DirtyClass::Paint);
   }
   let rendered = scenario.surface.render_snapshot_retained(
      api::RenderChunkId(80),
      &scenario.content,
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("valid cache-pressure snapshot");
   let stats = scenario.surface.retained_node_stats();
   let checksum = rendered.snapshot.instance_count()
      .wrapping_add(stats.cache_hits)
      .wrapping_add(stats.cache_misses)
      .wrapping_add(stats.retained_chunk_bytes)
      .wrapping_add(stats.retained_sequence_bytes);
   (checksum, stats)
}

fn run_retained_scenario(scenario: &mut RetainedScenario) -> (u64, ui::SurfaceRenderChunkStats)
{
   if scenario.dirty
   {
      scenario.phase = scenario.phase.wrapping_add(1);
      let phase = (scenario.phase % 97) as f32 / 97.0;
      scenario.surface.edit_style(scenario.leaf, |style| {
         style.opacity = 0.55 + phase * 0.40;
      });
      scenario.surface.layout(1_200.0, 2_400.0);
   }
   let rendered = scenario.surface.render_snapshot_retained(
      api::RenderChunkId(1),
      &scenario.mixed_sequences,
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("valid retained mixed snapshot");
   let checksum = rendered.snapshot.instance_count()
      .wrapping_add(rendered.stats.chunks_reused)
      .wrapping_add(rendered.stats.chunks_rebuilt)
      .wrapping_add(rendered.stats.sequences_reused)
      .wrapping_add(rendered.stats.sequences_rebuilt);
   (checksum, rendered.stats)
}

fn build_retained_scenario(depth: usize, dirty: bool) -> RetainedScenario
{
   let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(1_200.0));
   let mut parents = Vec::with_capacity(depth);
   let mut parent = surface.root();
   for level in 0..depth
   {
      parent = surface.tree_mut().add_node(parent, ui::NodeStyle {
         axis: if level & 1 == 0 { ui::Axis::Row } else { ui::Axis::Column },
         size: ui::Size2D { w: ui::Dim::Auto, h: ui::Dim::Auto },
         opacity: 0.98,
         clip: level % 7 == 0,
         ..ui::NodeStyle::default()
      });
      parents.push(parent);
   }
   let mut leaf = parent;
   for index in 0..1_500
   {
      let semantic_parent = parents[index % parents.len()];
      let is_image = index >= 1_000;
      leaf = surface.tree_mut().add_node(semantic_parent, ui::NodeStyle {
         size: ui::Size2D { w: ui::Dim::Px(if is_image { 32.0 } else { 72.0 }), h: ui::Dim::Px(18.0) },
         background: if is_image { api::Color::rgba(0.18, 0.52, 0.92, 1.0) } else { api::Color::rgba(0.12, 0.12, 0.14, 1.0) },
         corner_radii: if is_image { [6.0; 4] } else { [2.0; 4] },
         ..ui::NodeStyle::default()
      });
   }
   surface.layout(1_200.0, 2_400.0);
   let mixed_sequences = retained_mixed_sequences();
   let _ = surface.render_snapshot_retained(
      api::RenderChunkId(1),
      &mixed_sequences,
      Vec::new(),
      api::Damage { rects: Vec::new() },
   );
   RetainedScenario { surface, leaf, mixed_sequences, dirty, phase: 0 }
}

struct RetainedSurfaceDamageScenario
{
   surface: ui::UiSurface,
   leaf: ui::NodeId,
   phase: u64,
   damage: Vec<api::RectI>,
}

#[derive(Clone, Copy)]
struct RetainedSurfaceDamageProbe
{
   render: ui::SurfaceRenderChunkStats,
   damage: ui::SurfaceDamageStats,
   submissions: u64,
}

fn build_retained_surface_damage_scenario() -> RetainedSurfaceDamageScenario
{
   let mut surface = ui::UiSurface::new(ui::NodeStyle {
      axis: ui::Axis::Column,
      size: ui::Size2D { w: ui::Dim::Px(1_000.0), h: ui::Dim::Px(700.0) },
      ..ui::NodeStyle::default()
   });
   let root = surface.root();
   let mut leaf = root;
   for row_index in 0..100
   {
      let row = surface.add_node(root, ui::NodeStyle {
         axis: ui::Axis::Row,
         size: ui::Size2D { w: ui::Dim::Px(1_000.0), h: ui::Dim::Px(7.0) },
         ..ui::NodeStyle::default()
      }).expect("damage row");
      for column in 0..100
      {
         let node = surface.add_node(row, ui::NodeStyle {
            size: ui::Size2D { w: ui::Dim::Px(10.0), h: ui::Dim::Px(7.0) },
            background: api::Color::rgba(0.15, 0.45, 0.85, 1.0),
            ..ui::NodeStyle::default()
         }).expect("damage cell");
         if row_index == 50 && column == 50
         {
            leaf = node;
         }
      }
   }
   assert_ne!(leaf, root);
   surface.layout(1_000.0, 700.0);
   let mut damage = Vec::with_capacity(8);
   for _ in 0..2
   {
      let _ = surface.render_snapshot_retained(
         api::RenderChunkId(28),
         &[],
         Vec::new(),
         api::Damage { rects: Vec::new() },
      ).expect("warm retained damage snapshot");
      surface.take_damage_into(&mut damage);
   }
   RetainedSurfaceDamageScenario { surface, leaf, phase: 0, damage }
}

fn run_retained_surface_dirty(scenario: &mut RetainedSurfaceDamageScenario) -> (u64, RetainedSurfaceDamageProbe)
{
   let rendered = render_retained_surface_dirty(scenario);
   let damage = scenario.surface.damage_stats();
   scenario.surface.take_damage_into(&mut scenario.damage);
   let checksum = rendered.snapshot.instance_count()
      .wrapping_add(rendered.stats.chunks_rebuilt)
      .wrapping_add(u64::from(damage.changed_paint_units))
      .wrapping_add(damage.damage_pixels);
   (
      checksum,
      RetainedSurfaceDamageProbe {
         render: rendered.stats,
         damage,
         submissions: u64::from(!scenario.damage.is_empty()),
      },
   )
}

fn render_retained_surface_dirty(scenario: &mut RetainedSurfaceDamageScenario) -> ui::SurfaceRenderSnapshot
{
   scenario.phase = scenario.phase.wrapping_add(1);
   let blue = 0.45 + (scenario.phase & 1) as f32 * 0.35;
   assert!(scenario.surface.edit_style(scenario.leaf, |style| {
      style.background = api::Color::rgba(0.15, 0.45, blue, 1.0);
   }));
   assert!(scenario.surface.needs_frame());
   scenario.damage.clear();
   scenario.surface.render_snapshot_retained(
      api::RenderChunkId(28),
      &[],
      Vec::new(),
      api::Damage { rects: core::mem::take(&mut scenario.damage) },
   ).expect("dirty retained damage snapshot")
}

fn retained_surface_idle_case(smoke: bool) -> PerfCaseResult
{
   let scenario = build_retained_surface_damage_scenario();
   assert!(!scenario.surface.needs_frame());
   let mut case = measured_architecture_case(
      "cpu.architecture.damage.retained_surface_idle_10000",
      smoke,
      "Static retained 10,000-cell UiSurface checks frame demand without constructing a builder, snapshot, backend frame, or submission.",
      move || u64::from(black_box(scenario.surface.needs_frame())),
   );
   case.metrics.insert(String::from("paint_nodes"), 10_000.0);
   case.metrics.insert(String::from("dirty_nodes"), 0.0);
   case.metrics.insert(String::from("damage_pixels"), 0.0);
   case.metrics.insert(String::from("commands_built"), 0.0);
   case.metrics.insert(String::from("commands_lowered"), 0.0);
   case.metrics.insert(String::from("submissions"), 0.0);
   case
}

pub(super) fn retained_surface_dirty_case(id: &str, family: &str, smoke: bool) -> PerfCaseResult
{
   let mut scenario = build_retained_surface_damage_scenario();
   let (_, probe) = run_retained_surface_dirty(&mut scenario);
   assert_eq!(probe.render.chunks_rebuilt, 1);
   assert_eq!(probe.damage.changed_paint_units, 1);
   assert!(!probe.damage.full_damage);
   let mut case = measure_cpu_case(
      id,
      family,
      smoke,
      true,
      0.20,
      1,
      vec![String::from(
         "One author-visible leaf mutation derives damage from retained old/new paint bounds and rebuilds one immutable chunk in a 10,000-cell surface.",
      )],
      move || run_retained_surface_dirty(&mut scenario).0,
   );
   case.metrics.insert(String::from("paint_nodes"), 10_000.0);
   case.metrics.insert(String::from("dirty_nodes"), 1.0);
   case.metrics.insert(String::from("changed_paint_units"), probe.damage.changed_paint_units as f64);
   case.metrics.insert(String::from("damage_rects"), probe.damage.damage_rects as f64);
   case.metrics.insert(String::from("damage_pixels"), probe.damage.damage_pixels as f64);
   case.metrics.insert(String::from("commands_built"), probe.render.chunks_rebuilt as f64);
   case.metrics.insert(String::from("commands_lowered_expected"), 1.0);
   case.metrics.insert(String::from("submissions"), probe.submissions as f64);
   case
}

fn metal_retained_surface_dirty_case(smoke: bool) -> Result<PerfCaseResult>
{
   let id = "gpu.architecture.damage.retained_surface_dirty_leaf_10000";
   let mut scenario = build_retained_surface_damage_scenario();
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating retained-damage Metal renderer")?);
   renderer.resize(1_000, 700, 1.0).context("resizing retained-damage Metal renderer")?;
   renderer.set_damage_options(true, DAMAGE_USE_THRESH, DAMAGE_PREFILTER_THRESH);
   scenario.surface.mark_dirty(ui::DirtyClass::Paint);
   scenario.damage.clear();
   let initial = scenario.surface.render_snapshot_retained(
      api::RenderChunkId(28),
      &[],
      Vec::new(),
      api::Damage { rects: core::mem::take(&mut scenario.damage) },
   ).expect("initial retained-damage snapshot");
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&initial.snapshot).context("encoding initial retained-damage snapshot")?;
   renderer.submit(token).context("submitting initial retained-damage snapshot")?;
   let _ = last_metal_stats_after_submit(&renderer, token.0);
   scenario.surface.take_damage_into(&mut scenario.damage);

   let warmups = if smoke { 1_usize } else { 3 };
   let frames = if smoke { 3_usize } else { 12 };
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut event_to_submit_samples = Vec::with_capacity(frames);
   let mut changed_units_sum = 0_u64;
   let mut damage_pixels_sum = 0_u64;
   let mut damage_rects_sum = 0_u64;
   let mut commands_built_sum = 0_u64;
   let mut commands_lowered_sum = 0_u64;
   let mut draws_sum = 0_u64;
   let mut chunks_reused_sum = 0_u64;
   let mut chunks_prepared_sum = 0_u64;
   for frame in 0..warmups.saturating_add(frames)
   {
      let started_at = Instant::now();
      let rendered = render_retained_surface_dirty(&mut scenario);
      let damage = scenario.surface.damage_stats();
      let token = renderer.begin_frame(&api::FrameTarget, Some(rendered.snapshot.damage()));
      renderer.encode_snapshot(&rendered.snapshot).with_context(|| format!("encoding {id}"))?;
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let event_to_submit_ms = started_at.elapsed().as_secs_f64() * 1_000.0;
      let stats = last_metal_stats_after_submit(&renderer, token.0);
      let frame_ms = started_at.elapsed().as_secs_f64() * 1_000.0;
      scenario.surface.take_damage_into(&mut scenario.damage);
      if frame < warmups
      {
         continue;
      }
      frame_samples.push(frame_ms);
      encode_samples.push(stats.encode_ms);
      gpu_samples.push(stats.gpu_ms);
      event_to_submit_samples.push(event_to_submit_ms);
      changed_units_sum = changed_units_sum.saturating_add(u64::from(damage.changed_paint_units));
      damage_pixels_sum = damage_pixels_sum.saturating_add(damage.damage_pixels);
      damage_rects_sum = damage_rects_sum.saturating_add(u64::from(damage.damage_rects));
      commands_built_sum = commands_built_sum.saturating_add(rendered.stats.chunks_rebuilt);
      commands_lowered_sum = commands_lowered_sum.saturating_add(stats.commands_traversed);
      draws_sum = draws_sum.saturating_add(u64::from(stats.draws));
      chunks_reused_sum = chunks_reused_sum.saturating_add(stats.chunks_reused);
      chunks_prepared_sum = chunks_prepared_sum.saturating_add(stats.chunks_prepared);
   }
   let summary = summarize(&frame_samples);
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_distribution_metrics(&mut metrics, "event_to_submit_ms", &event_to_submit_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("paint_nodes"), 10_000.0);
   metrics.insert(String::from("dirty_nodes_avg"), 1.0);
   metrics.insert(String::from("changed_paint_units_avg"), changed_units_sum as f64 / frames as f64);
   metrics.insert(String::from("damage_pixels_avg"), damage_pixels_sum as f64 / frames as f64);
   metrics.insert(String::from("damage_rects_avg"), damage_rects_sum as f64 / frames as f64);
   metrics.insert(String::from("commands_built_avg"), commands_built_sum as f64 / frames as f64);
   metrics.insert(String::from("commands_lowered_avg"), commands_lowered_sum as f64 / frames as f64);
   metrics.insert(String::from("draws_avg"), draws_sum as f64 / frames as f64);
   metrics.insert(String::from("submissions"), frames as f64);
   metrics.insert(String::from("chunks_reused_avg"), chunks_reused_sum as f64 / frames as f64);
   metrics.insert(String::from("chunks_prepared_avg"), chunks_prepared_sum as f64 / frames as f64);
   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from("engine"),
      scenario: String::from("rendering-architecture"),
      variant: String::from("oxide"),
      cache_state: String::from("warm"),
      refresh_mode: String::from("offscreen"),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(
         "One dirty leaf in a retained 10,000-cell UiSurface derives exact damage, rebuilds one chunk, and lowers only intersecting prepared paint into Metal.",
      )],
      metrics,
   })
}

fn retained_mixed_sequences() -> Vec<api::RenderChunkSequence>
{
   let mut text = authoring_text_replay_drawlist();
   let glyph = text.items[0].clone();
   for _ in 2..200
   {
      text.items.push(glyph.clone());
   }
   let text_chunk = api::RenderChunk::new(
      api::RenderChunkId(2),
      api::RenderChunkRevisions { resource: 1, ..api::RenderChunkRevisions::default() },
      text,
      api::ChunkIndexMode::Local,
      &[],
   ).expect("valid retained text chunk");
   let mut images = api::DrawList::default();
   for index in 0..100
   {
      images.items.push(api::DrawCmd::Image {
         tex: api::ImageHandle((index % 8 + 20) as u32),
         dst: api::RectF::new((index % 20) as f32 * 24.0, (index / 20) as f32 * 24.0, 20.0, 20.0),
         src: api::RectF::new(0.0, 0.0, 1.0, 1.0),
         alpha: 1.0,
      });
   }
   let image_dependencies = (20..28).map(|handle| api::RenderResourceDependency {
      image: api::ImageHandle(handle),
      generation: 1,
   }).collect::<Vec<_>>();
   let image_chunk = api::RenderChunk::new(
      api::RenderChunkId(3),
      api::RenderChunkRevisions { resource: 1, ..api::RenderChunkRevisions::default() },
      images,
      api::ChunkIndexMode::Local,
      &image_dependencies,
   ).expect("valid retained image chunk");
   let mixed_sequences = vec![
      api::RenderChunkSequence::new(vec![api::RenderChunkInstance::new(text_chunk, [0.0, 0.0])]),
      api::RenderChunkSequence::new(vec![api::RenderChunkInstance::new(image_chunk, [0.0, 0.0])]),
   ];
   mixed_sequences
}

fn animation_surface_case(smoke: bool) -> PerfCaseResult
{
   dynamic_property_surface_case("cpu.architecture.animation.surface_300", "architecture", smoke)
}

pub(super) fn authoring_dynamic_property_surface_case(smoke: bool) -> PerfCaseResult
{
   dynamic_property_surface_case("cpu.authoring.animation.dynamic_properties_300", "authoring", smoke)
}

fn dynamic_property_surface_case(id: &str, family: &str, smoke: bool) -> PerfCaseResult
{
   timing::testing::reset();
   let mut surface = ui::UiSurface::new(flat_rect_surface_root_style(1_000.0));
   let nodes = populate_flat_rect_surface(&mut surface, 300, 0).cells;
   for (index, node) in nodes.iter().copied().enumerate()
   {
      surface.edit_style(node, |style| {
         style.opacity = 0.85;
         style.clip = index % 11 == 0;
         style.transform.rot_rad = (index % 7) as f32 * 0.003;
      });
   }
   surface.layout(1_000.0, 900.0);
   for (index, node) in nodes.iter().copied().enumerate()
   {
      let transform = platform::Transform2D {
         tx: (index % 9) as f32,
         ty: (index % 5) as f32,
         sx: 1.0 + (index % 3) as f32 * 0.02,
         sy: 1.0,
         rot_rad: (index % 13) as f32 * 0.01,
      };
      surface.animator().start(node, platform::AnimDesc {
         id: 0,
         prop: platform::AnimProp::Transform2D,
         from: platform::AnimValue::Xform2D(ui::anim::helpers::identity_transform()),
         to: platform::AnimValue::Xform2D(transform),
         curve: platform::AnimCurve::Ease { ease: platform::Ease { kind: platform::EaseKind::CubicInOut } },
         duration_ms: 700,
         delay_ms: (index % 17) as u32,
         repeat: platform::Repeat::Forever,
      });
      surface.animator().start(node, platform::AnimDesc {
         id: 0,
         prop: platform::AnimProp::Opacity,
         from: platform::AnimValue::F32(0.55),
         to: platform::AnimValue::F32(1.0),
         curve: platform::AnimCurve::Ease { ease: platform::Ease { kind: platform::EaseKind::QuadInOut } },
         duration_ms: 500,
         delay_ms: 0,
         repeat: platform::Repeat::Forever,
      });
   }
   let start = timing::now_ms();
   let mut frame = 0_u64;
   let mixed_sequences = retained_mixed_sequences();
   surface.tick_at(start);
   let _ = surface.render_snapshot_retained(
      api::RenderChunkId(10),
      &mixed_sequences,
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("warm animated mixed snapshot");
   let mut operations = 0_u64;
   let mut chunks_rebuilt = 0_u64;
   let mut sequences_rebuilt = 0_u64;
   let mut command_bytes_copied = 0_u64;
   let mut vertex_bytes_copied = 0_u64;
   let mut index_bytes_copied = 0_u64;
   let mut property_records = 0_u64;
   let mut case = measure_cpu_case(
      id,
      family,
      smoke,
      true,
      0.20,
      1,
      vec![String::from(
         "Real 300-node UiSurface animation with Animator overrides, nested clips/opacity, transforms, retained encoding, hit testing, and accessibility dirtiness.",
      )],
      || {
         frame = frame.wrapping_add(1);
         surface.tick_at(start.saturating_add(frame * 8));
         let _ = surface.mark_node_dirty(nodes[frame as usize % nodes.len()], ui::DirtyClass::Accessibility);
         let rendered = surface.render_snapshot_retained(
            api::RenderChunkId(10),
            &mixed_sequences,
            Vec::new(),
            api::Damage { rects: Vec::new() },
         ).expect("valid animated mixed snapshot");
         operations = operations.saturating_add(1);
         chunks_rebuilt = chunks_rebuilt.saturating_add(rendered.stats.chunks_rebuilt);
         sequences_rebuilt = sequences_rebuilt.saturating_add(rendered.stats.sequences_rebuilt);
         command_bytes_copied = command_bytes_copied.saturating_add(rendered.stats.command_bytes_copied);
         vertex_bytes_copied = vertex_bytes_copied.saturating_add(rendered.stats.vertex_bytes_copied);
         index_bytes_copied = index_bytes_copied.saturating_add(rendered.stats.index_bytes_copied);
         property_records = property_records.saturating_add(rendered.snapshot.properties().len() as u64);
         let hit = surface.hit_test((frame % 800) as f32, (frame % 700) as f32).is_some() as u64;
         let mut draw_items = 0_u64;
         rendered.snapshot.visit_instances(|instance| {
            draw_items = draw_items.saturating_add(instance.chunk.draw_list().items.len() as u64);
         });
         draw_items
            + surface.overrides().len() as u64
            + hit
      },
   );
   case.metrics.insert(String::from("animated_nodes"), 300.0);
   case.metrics.insert(String::from("active_animations"), 600.0);
   case.metrics.insert(String::from("hit_tests_per_op"), 1.0);
   case.metrics.insert(String::from("accessibility_geometry_nodes"), 300.0);
   case.metrics.insert(String::from("label_nodes"), 200.0);
   case.metrics.insert(String::from("image_nodes"), 100.0);
   let operations = operations.max(1) as f64;
   case.metrics.insert(String::from("chunks_rebuilt_avg"), chunks_rebuilt as f64 / operations);
   case.metrics.insert(String::from("sequences_rebuilt_avg"), sequences_rebuilt as f64 / operations);
   case.metrics.insert(String::from("command_bytes_copied_avg"), command_bytes_copied as f64 / operations);
   case.metrics.insert(String::from("vertex_bytes_copied_avg"), vertex_bytes_copied as f64 / operations);
   case.metrics.insert(String::from("index_bytes_copied_avg"), index_bytes_copied as f64 / operations);
   case.metrics.insert(String::from("property_records_avg"), property_records as f64 / operations);
   case
}

fn text_warm_labels_case(smoke: bool) -> PerfCaseResult
{
   let mut text = perf_text_ctx();
   text.set_frame_stats_enabled(true);
   text.set_fallback_fonts(&[1]);
   let mut uploader = CpuUploader::default();
   let mut builder = ui::DrawListBuilder::new();
   let labels = (0..1_000).map(|index| format!("Warm label {index:04}")).collect::<Vec<_>>();
   text.begin_frame();
   for (index, label) in labels.iter().enumerate()
   {
      encode_matrix_label(label, index, 2.0, 18.0, &mut text, &mut uploader, &mut builder);
   }
   let _ = text.finish_frame(&mut uploader, &mut builder);
   let proof_stats = {
      builder.clear();
      text.begin_frame();
      for (index, label) in labels.iter().enumerate()
      {
         encode_matrix_label_profiled(label, index, 2.0, 18.0, &mut text, &mut uploader, &mut builder);
      }
      text.finish_frame(&mut uploader, &mut builder)
   };
   text.set_frame_stats_enabled(false);
   let frame_scoped = std::env::var("OXIDE_C43_TEXT_FRAME_SCOPED")
      .map_or(true, |value| value != "0");
   let mut case = measured_architecture_case(
      "cpu.architecture.text.warm_labels_1000",
      smoke,
      "One thousand already-shaped and atlas-resident labels encoded into one warm frame.",
      move || {
         builder.clear();
         if frame_scoped
         {
            text.begin_frame();
         }
         for (index, label) in labels.iter().enumerate()
         {
            encode_matrix_label(label, index, 2.0, 18.0, &mut text, &mut uploader, &mut builder);
         }
         let upload_calls;
         if frame_scoped
         {
            upload_calls = text.finish_frame(&mut uploader, &mut builder).atlas_upload_calls;
         }
         else
         {
            upload_calls = 0;
         }
         builder.drawlist().items.len() as u64
            + builder.drawlist().vertices.len() as u64
            + upload_calls
      },
   );
   case.metrics.insert(String::from("warm_labels"), 1_000.0);
   case.metrics.insert(String::from("device_scale"), 2.0);
   insert_text_frame_metrics(&mut case.metrics, proof_stats);
   case
}

fn text_new_labels_case(smoke: bool) -> PerfCaseResult
{
   let proof_stats = run_new_label_frame(0x43);
   let mut phase = 0_u64;
   let frame_scoped = std::env::var("OXIDE_C43_TEXT_FRAME_SCOPED")
      .map_or(true, |value| value != "0");
   let mut case = measured_architecture_case(
      "cpu.architecture.text.new_labels_200",
      smoke,
      "Two hundred previously unseen labels shaped, baked, uploaded, and encoded in one frame.",
      move || {
         phase = phase.wrapping_add(1);
         let mut text = perf_text_ctx();
         text.set_fallback_fonts(&[1]);
         let mut uploader = CpuUploader::default();
         let mut builder = ui::DrawListBuilder::new();
         if frame_scoped
         {
            text.begin_frame();
         }
         for index in 0..200
         {
            let label = format!("New {phase:08x} Latin 漢字 مرحبا 😀 {index:03}");
            encode_matrix_label(&label, index, 3.0, 20.0, &mut text, &mut uploader, &mut builder);
         }
         let upload_calls;
         if frame_scoped
         {
            upload_calls = text.finish_frame(&mut uploader, &mut builder).atlas_upload_calls;
         }
         else
         {
            upload_calls = 0;
         }
         builder.drawlist().items.len() as u64
            + builder.drawlist().vertices.len() as u64
            + text.atlas_revision()
            + upload_calls
      },
   );
   case.metrics.insert(String::from("new_labels"), 200.0);
   case.metrics.insert(String::from("device_scale"), 3.0);
   insert_text_frame_metrics(&mut case.metrics, proof_stats);
   case
}

#[derive(Default)]
struct BitmapOptionsEncoder
{
   solids: u64,
   rrects: u64,
   glyph_runs: u64,
}

impl BitmapOptionsEncoder
{
   fn clear(&mut self)
   {
      self.solids = 0;
      self.rrects = 0;
      self.glyph_runs = 0;
   }

   fn commands(&self) -> u64
   {
      self.solids + self.rrects + self.glyph_runs
   }
}

impl api::RenderEncoder for BitmapOptionsEncoder
{
   fn set_viewport(&mut self, _vp: api::RectF) {}
   fn set_clip(&mut self, _scissor: api::RectI) {}

   fn draw_solid(&mut self, _verts: &[api::Vertex], _color: api::Color)
   {
      self.solids = self.solids.wrapping_add(1);
   }

   fn draw_image(&mut self, _img: api::ImageHandle, _dst: api::RectF, _src: api::RectF) {}

   fn draw_rrect(&mut self, _rect: api::RectF, _radii: [f32; 4], _color: api::Color)
   {
      self.rrects = self.rrects.wrapping_add(1);
   }

   fn draw_nine_slice(
      &mut self,
      _img: api::ImageHandle,
      _rect: api::RectF,
      _slice: api::Insets,
      _alpha: f32,
   )
   {
   }

   fn draw_backdrop(
      &mut self,
      _rect: api::RectF,
      _sigma: f32,
      _tint: api::Color,
      _alpha: f32,
   )
   {
   }

   fn draw_spinner(&mut self, _center: [f32; 2], _atom: f32, _alpha: f32) {}

   fn draw_glyph_run(&mut self, _run: &api::GlyphRun)
   {
      self.glyph_runs = self.glyph_runs.wrapping_add(1);
   }
}

fn text_bitmap_options_case(smoke: bool) -> PerfCaseResult
{
   let layout = ui::text_input_options_layout(
      api::RectF::new(260.0, 80.0, 120.0, 44.0),
      api::RectF::new(0.0, 0.0, 640.0, 480.0),
      1.0,
      ui::TextInputOptionsConfig::all(),
      10.6,
   )
   .expect("text option layout");
   let style = ui::TextInputOptionsPopoverStyle {
      background: api::Color::rgba(0.01, 0.01, 0.01, 0.96),
      divider: api::Color::rgba(1.0, 1.0, 1.0, 0.78),
      text: api::Color::rgba(1.0, 1.0, 1.0, 0.96),
      text_px: 10.6,
   };
   let mut atlas = ui::bitmap_text::BitmapTextAtlas::new();
   atlas.set_handle(api::ImageHandle(1));
   let mut encoder = BitmapOptionsEncoder::default();
   assert!(ui::draw_text_input_options_popover(
      &mut encoder,
      &mut atlas,
      2.0,
      layout,
      style,
   ));
   let glyph_runs = encoder.glyph_runs;
   let solid_draws = encoder.solids;
   atlas.clear_dirty();

   let mut case = measured_architecture_case(
      "cpu.architecture.text.bitmap_options",
      smoke,
      "Warm text-field Cut/Copy/Select All/Paste options encoded through one explicit A8 atlas and four GlyphRuns.",
      move || {
         encoder.clear();
         let ready = ui::draw_text_input_options_popover(
            &mut encoder,
            &mut atlas,
            2.0,
            layout,
            style,
         );
         encoder.commands().wrapping_add(u64::from(ready))
      },
   );
   case.metrics.insert(String::from("option_labels"), 4.0);
   case.metrics.insert(String::from("glyph_run_draws"), glyph_runs as f64);
   case.metrics.insert(String::from("non_label_solid_draws"), solid_draws as f64);
   case.metrics.insert(String::from("label_solid_draws"), 0.0);
   case.metrics.insert(String::from("global_render_mutex_locks"), 0.0);
   case.metrics.insert(String::from("warm_atlas_upload_calls"), 0.0);
   case.metrics.insert(String::from("warm_atlas_upload_bytes"), 0.0);
   case
}

fn run_new_label_frame(phase: u64) -> ui::elements::TextFrameStats
{
   let mut text = perf_text_ctx();
   text.set_frame_stats_enabled(true);
   text.set_fallback_fonts(&[1]);
   let mut uploader = CpuUploader::default();
   let mut builder = ui::DrawListBuilder::new();
   text.begin_frame();
   for index in 0..200
   {
      let label = format!("New {phase:08x} Latin 漢字 مرحبا 😀 {index:03}");
      encode_matrix_label_profiled(&label, index, 3.0, 20.0, &mut text, &mut uploader, &mut builder);
   }
   text.finish_frame(&mut uploader, &mut builder)
}

fn insert_text_frame_metrics(
   metrics: &mut BTreeMap<String, f64>,
   stats: ui::elements::TextFrameStats,
)
{
   for (name, value) in [
      ("visible_labels", stats.visible_labels),
      ("shaping_calls", stats.shaping_calls),
      ("rasterizations", stats.rasterizations),
      ("layout_cache_hits", stats.layout_cache_hits),
      ("layout_cache_misses", stats.layout_cache_misses),
      ("glyph_cache_hits", stats.glyph_cache_hits),
      ("glyph_cache_misses", stats.glyph_cache_misses),
      ("atlas_upload_calls", stats.atlas_upload_calls),
      ("atlas_upload_pixels", stats.atlas_upload_pixels),
      ("atlas_upload_bytes", stats.atlas_upload_bytes),
      ("atlas_evictions", stats.atlas_evictions),
      ("invalidated_runs", stats.invalidated_runs),
   ]
   {
      metrics.insert(String::from(name), value as f64);
   }
}

struct ArchitectureTextMetalUploader
{
   renderer: *mut metal::MetalRenderer,
   creates: u64,
   updates: u64,
   upload_pixels: u64,
   upload_bytes: u64,
}

impl ui::elements::ImageUploader for ArchitectureTextMetalUploader
{
   fn create_a8(&mut self, w: u32, h: u32, data: &[u8], row_bytes: usize) -> api::ImageHandle
   {
      let pixels = u64::from(w).saturating_mul(u64::from(h));
      self.creates = self.creates.saturating_add(1);
      self.upload_pixels = self.upload_pixels.saturating_add(pixels);
      self.upload_bytes = self.upload_bytes.saturating_add(pixels);
      unsafe
      {
         (*self.renderer).image_create_a8(w, h, data, row_bytes)
      }
   }

   fn update_a8(
      &mut self,
      handle: api::ImageHandle,
      x: u32,
      y: u32,
      w: u32,
      h: u32,
      data: &[u8],
      row_bytes: usize,
   )
   {
      let pixels = u64::from(w).saturating_mul(u64::from(h));
      self.updates = self.updates.saturating_add(1);
      self.upload_pixels = self.upload_pixels.saturating_add(pixels);
      self.upload_bytes = self.upload_bytes.saturating_add(pixels);
      unsafe
      {
         (*self.renderer).image_update_a8(handle, x, y, w, h, data, row_bytes)
      }
   }

   fn append_a8(
      &mut self,
      handle: api::ImageHandle,
      x: u32,
      y: u32,
      w: u32,
      h: u32,
      data: &[u8],
      row_bytes: usize,
   )
   {
      let pixels = u64::from(w).saturating_mul(u64::from(h));
      self.updates = self.updates.saturating_add(1);
      self.upload_pixels = self.upload_pixels.saturating_add(pixels);
      self.upload_bytes = self.upload_bytes.saturating_add(pixels);
      unsafe
      {
         (*self.renderer).image_append_a8(handle, x, y, w, h, data, row_bytes)
      }
   }

   fn release_a8(&mut self, handle: api::ImageHandle)
   {
      unsafe
      {
         (*self.renderer).image_release(handle)
      }
   }
}

fn metal_text_new_labels_case(id: &str, smoke: bool, frame_scoped: bool) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating text-preparation Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing text-preparation Metal renderer")?;
   let renderer_ptr: *mut metal::MetalRenderer = &mut *renderer;
   let font_probe = perf_text_ctx();
   let mut supported = Vec::new();
   for scalar in 0x21_u32..0x300
   {
      let Some(ch) = char::from_u32(scalar) else
      {
         continue;
      };
      if ch.is_whitespace()
      {
         continue;
      }
      let unique = ch.to_string();
      if font_probe.fonts.font_supports_cluster(0, &unique)
      {
         supported.push(unique);
      }
   }
   assert!(!supported.is_empty(), "text-preparation case requires supported glyphs");
   let mut labels = Vec::with_capacity(200);
   for font_px in 12_u32..=20
   {
      for unique in &supported
      {
         labels.push((format!("Item {unique}"), font_px as f32));
         if labels.len() == 200
         {
            break;
         }
      }
      if labels.len() == 200
      {
         break;
      }
   }
   assert_eq!(labels.len(), 200, "text-preparation case requires 200 unique glyph/size keys");
   let proof_stats;
   if frame_scoped
   {
      let mut text = perf_text_ctx();
      text.set_fallback_fonts(&[1]);
      text.set_frame_stats_enabled(true);
      let mut uploader = CpuUploader::default();
      let mut builder = ui::DrawListBuilder::new();
      text.begin_frame();
      for (index, (label, font_px)) in labels.iter().enumerate()
      {
         encode_matrix_label_profiled(label, index, 1.0, *font_px, &mut text, &mut uploader, &mut builder);
      }
      proof_stats = text.finish_frame(&mut uploader, &mut builder);
   }
   else
   {
      proof_stats = ui::elements::TextFrameStats::default();
   }
   let warmups;
   let frames;
   if smoke
   {
      warmups = 1_usize;
      frames = 3_usize;
   }
   else
   {
      warmups = 3;
      frames = std::env::var("OXIDE_C43_METAL_FRAMES")
         .ok()
         .and_then(|value| value.parse::<usize>().ok())
         .filter(|frames| *frames >= 16)
         .unwrap_or(16);
   }
   let mut frame_samples = Vec::with_capacity(frames);
   let mut warmup_frame_samples = Vec::with_capacity(warmups);
   let mut prepare_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut creates = 0_u64;
   let mut updates = 0_u64;
   let mut upload_pixels = 0_u64;
   let mut upload_bytes = 0_u64;
   let mut draws = 0_u64;
   let mut buffer_upload_bytes = 0_u64;

   for frame in 0..warmups.saturating_add(frames)
   {
      let mut text = perf_text_ctx();
      text.set_fallback_fonts(&[1]);
      let mut uploader = ArchitectureTextMetalUploader {
         renderer: renderer_ptr,
         creates: 0,
         updates: 0,
         upload_pixels: 0,
         upload_bytes: 0,
      };
      let mut builder = ui::DrawListBuilder::new();
      let frame_started_at = Instant::now();
      if frame_scoped
      {
         text.begin_frame();
      }
      for (index, (label, font_px)) in labels.iter().enumerate()
      {
         encode_matrix_label(label, index, 1.0, *font_px, &mut text, &mut uploader, &mut builder);
      }
      if frame_scoped
      {
         let _ = text.finish_frame(&mut uploader, &mut builder);
      }
      let prepare_ms = frame_started_at.elapsed().as_secs_f64() * 1_000.0;
      ui::coalesce_adjacent_draws(builder.drawlist_mut());
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(builder.drawlist());
      renderer
         .submit(token)
         .with_context(|| format!("submitting {id}"))?;
      let metal_stats = last_metal_stats_after_submit(&renderer, token.0);
      let frame_ms = frame_started_at.elapsed().as_secs_f64() * 1_000.0;
      if frame < warmups
      {
         warmup_frame_samples.push(frame_ms);
         continue;
      }
      frame_samples.push(frame_ms);
      prepare_samples.push(prepare_ms);
      encode_samples.push(metal_stats.encode_ms);
      gpu_samples.push(metal_stats.gpu_ms);
      creates = creates.saturating_add(uploader.creates);
      updates = updates.saturating_add(uploader.updates);
      upload_pixels = upload_pixels.saturating_add(uploader.upload_pixels);
      upload_bytes = upload_bytes.saturating_add(uploader.upload_bytes);
      draws = draws.saturating_add(u64::from(metal_stats.draws));
      buffer_upload_bytes = buffer_upload_bytes.saturating_add(metal_stats.buffer_upload_bytes);
   }

   let summary = summarize(&frame_samples);
   let measured = frames.max(1) as f64;
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "text_prepare_ms", &prepare_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("new_labels"), 200.0);
   metrics.insert(String::from("frame_scoped_preparation"), u8::from(frame_scoped) as f64);
   metrics.insert(String::from("atlas_create_calls_avg"), creates as f64 / measured);
   metrics.insert(String::from("atlas_update_calls_avg"), updates as f64 / measured);
   metrics.insert(String::from("atlas_upload_calls_avg"), creates.saturating_add(updates) as f64 / measured);
   metrics.insert(String::from("atlas_upload_pixels_avg"), upload_pixels as f64 / measured);
   metrics.insert(String::from("atlas_upload_bytes_avg"), upload_bytes as f64 / measured);
   metrics.insert(String::from("draws_avg"), draws as f64 / measured);
   metrics.insert(String::from("buffer_upload_bytes_avg"), buffer_upload_bytes as f64 / measured);
   if frame_scoped
   {
      insert_text_frame_metrics(&mut metrics, proof_stats);
   }
   if std::env::var_os("OXIDE_C43_RAW_SAMPLES").is_some()
   {
      insert_indexed_samples(&mut metrics, "c43_warmup_frame_ms", &warmup_frame_samples);
      insert_indexed_samples(&mut metrics, "c43_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "c43_text_prepare_ms", &prepare_samples);
      insert_indexed_samples(&mut metrics, "c43_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "c43_gpu_ms", &gpu_samples);
   }
   let note;
   if frame_scoped
   {
      note = "Two hundred new mixed-script labels preflight into one frame-scoped A8 atlas publication before Metal encoding.";
   }
   else
   {
      note = "Control path: two hundred new mixed-script labels publish dirty A8 atlas state immediately while labels are encoded.";
   }
   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from("engine"),
      scenario: String::from("rendering-architecture"),
      variant: String::from("oxide"),
      cache_state: String::from("cold"),
      refresh_mode: String::from("offscreen"),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(note)],
      metrics,
   })
}

fn metal_text_glyph_instances_case(id: &str, smoke: bool) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating glyph-instance Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing glyph-instance Metal renderer")?;
   let renderer_ptr: *mut metal::MetalRenderer = &mut *renderer;
   let mut text = perf_text_ctx();
   text.atlas = oxide_text::PagedAtlas::new(128, 128, oxide_text::DEFAULT_GLYPH_ATLAS_PAGE_COUNT);
   text.set_fallback_fonts(&[1]);
   text.set_frame_stats_enabled(true);
   let mut supported = Vec::new();
   for scalar in 0x21_u32..0x3000
   {
      let Some(ch) = char::from_u32(scalar) else { continue };
      if !ch.is_whitespace() && text.fonts.font_supports_cluster(0, &ch.to_string())
      {
         supported.push(ch);
      }
   }
   assert!(!supported.is_empty(), "glyph-instance case requires supported glyphs");
   let labels = (0..1_000_usize).map(|index| {
      let unique = supported[index % supported.len()];
      match index % 11
      {
         0 => format!("Wrapped Latin {unique} words {index:04} across a narrow row"),
         1 => format!("RTL مرحبا {unique} {index:04}"),
         2 => format!("CJK 漢字 {unique} {index:04}"),
         3 => format!("Emoji 😀 {unique} {index:04}"),
         _ => format!("Glyph {unique} size variant {index:04}"),
      }
   }).collect::<Vec<_>>();
   let palette = [
      api::Color::rgba(0.10, 0.12, 0.16, 1.0),
      api::Color::rgba(0.80, 0.20, 0.12, 0.90),
      api::Color::rgba(0.12, 0.45, 0.85, 0.75),
      api::Color::rgba(0.20, 0.70, 0.35, 1.0),
   ];
   let mut uploader = ArchitectureTextMetalUploader {
      renderer: renderer_ptr,
      creates: 0,
      updates: 0,
      upload_pixels: 0,
      upload_bytes: 0,
   };
   let mut builder = ui::DrawListBuilder::new();
   text.begin_frame();
   for (index, label) in labels.iter().enumerate()
   {
      let font_px = 16.0 + (index % 20) as f32;
      let wrap = index % 11 == 0;
      ui::elements::encode_label_text(
         label,
         palette[index % palette.len()],
         ui::elements::Align::Left,
         wrap,
         0,
         font_px,
         api::RectF::new(
            (index % 5) as f32 * 238.0,
            (index % 40) as f32 * 20.0,
            if wrap { 180.0 } else { 232.0 },
            if wrap { 48.0 } else { 40.0 },
         ),
         1.0,
         &mut text,
         &mut uploader,
         &mut builder,
      );
   }
   let proof = text.finish_frame(&mut uploader, &mut builder);
   ui::coalesce_adjacent_draws(builder.drawlist_mut());
   let mut atlas_pages = HashSet::new();
   let mut has_bitmap = false;
   let mut has_sdf = false;
   for item in &builder.drawlist().items
   {
      if let api::DrawCmd::GlyphRun { run } = item
      {
         atlas_pages.insert(run.atlas);
         has_sdf |= run.sdf;
         has_bitmap |= !run.sdf;
      }
   }
   assert!(
      atlas_pages.len() >= 2,
      "glyph-instance case requires multiple atlas pages: referenced={} resident={} glyphs={} items={} rasterizations={}",
      atlas_pages.len(),
      text.atlas.page_count(),
      text.atlas.glyph_count(),
      builder.drawlist().items.len(),
      proof.rasterizations,
   );
   assert!(has_bitmap && has_sdf, "glyph-instance case requires bitmap and SDF text");

   let warmups = if smoke { 1_usize } else { 3 };
   let frames = if smoke { 3_usize } else {
      std::env::var("OXIDE_C45_METAL_FRAMES")
         .ok()
         .and_then(|value| value.parse::<usize>().ok())
         .filter(|frames| *frames >= 16)
         .unwrap_or(32)
   };
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut warmup_samples = Vec::with_capacity(warmups);
   let mut draws = 0_u64;
   let mut buffer_upload_bytes = 0_u64;
   let mut glyph_instance_bytes = 0_u64;
   let mut glyph_instance_binds = 0_u64;
   let mut glyph_instances = 0_u64;
   for frame in 0..warmups.saturating_add(frames)
   {
      let started_at = Instant::now();
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(builder.drawlist());
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, token.0);
      let frame_ms = started_at.elapsed().as_secs_f64() * 1_000.0;
      if frame < warmups
      {
         warmup_samples.push(frame_ms);
         continue;
      }
      frame_samples.push(frame_ms);
      encode_samples.push(stats.encode_ms);
      gpu_samples.push(stats.gpu_ms);
      draws = draws.saturating_add(u64::from(stats.draws));
      buffer_upload_bytes = buffer_upload_bytes.saturating_add(stats.buffer_upload_bytes);
      glyph_instance_bytes = glyph_instance_bytes.saturating_add(stats.glyph_instance_bytes);
      glyph_instance_binds = glyph_instance_binds.saturating_add(u64::from(stats.glyph_instance_buffer_binds));
      glyph_instances = glyph_instances.saturating_add(u64::from(stats.glyph_instances));
   }
   let summary = summarize(&frame_samples);
   let measured = frames.max(1) as f64;
   let average_instances = glyph_instances as f64 / measured;
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("labels"), labels.len() as f64);
   metrics.insert(String::from("atlas_pages"), atlas_pages.len() as f64);
   metrics.insert(String::from("bitmap_and_sdf"), 1.0);
   metrics.insert(String::from("draws_avg"), draws as f64 / measured);
   metrics.insert(String::from("buffer_upload_bytes_avg"), buffer_upload_bytes as f64 / measured);
   metrics.insert(String::from("glyph_instance_bytes_avg"), glyph_instance_bytes as f64 / measured);
   metrics.insert(String::from("glyph_instance_buffer_binds_avg"), glyph_instance_binds as f64 / measured);
   metrics.insert(String::from("glyph_instances_avg"), average_instances);
   metrics.insert(
      String::from("bytes_per_glyph_instance"),
      if average_instances == 0.0 { 0.0 } else { glyph_instance_bytes as f64 / measured / average_instances },
   );
   metrics.insert(
      String::from("glyph_buffer_upload_bytes_per_instance"),
      if average_instances == 0.0 { 0.0 } else { buffer_upload_bytes as f64 / measured / average_instances },
   );
   insert_text_frame_metrics(&mut metrics, proof);
   if std::env::var_os("OXIDE_C45_RAW_SAMPLES").is_some()
   {
      insert_indexed_samples(&mut metrics, "c45_warmup_frame_ms", &warmup_samples);
      insert_indexed_samples(&mut metrics, "c45_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "c45_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "c45_gpu_ms", &gpu_samples);
   }
   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from("engine"),
      scenario: String::from("rendering-architecture"),
      variant: String::from("oxide"),
      cache_state: String::from("warm"),
      refresh_mode: String::from("offscreen"),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(
         "One thousand wrapped, mixed-color, mixed-page bitmap/SDF labels with Latin, RTL, CJK, and emoji content encoded through the Metal glyph path.",
      )],
      metrics,
   })
}

fn glyph_page_snapshot(
   first_page: api::ImageHandle,
   first_generation: u64,
   second_page: api::ImageHandle,
   second_generation: u64,
) -> api::RenderSnapshot
{
   let chunk = |id, atlas, generation, x| api::RenderChunk::new(
      api::RenderChunkId(id),
      api::RenderChunkRevisions { resource: generation, geometry: 1, ..api::RenderChunkRevisions::default() },
      api::DrawList {
         items: vec![api::DrawCmd::GlyphRun { run: api::GlyphRun {
            atlas,
            atlas_revision: generation,
            vb: api::VertexSpan { offset: 0, len: 4 },
            ib: api::IndexSpan { offset: 0, len: 6 },
            sdf: false,
            color: api::Color::rgba(0.2, 0.8, 1.0, 1.0),
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
      &[api::RenderResourceDependency { image: atlas, generation }],
   ).expect("valid glyph page benchmark chunk");
   api::RenderSnapshot::new(
      vec![
         api::RenderChunkInstance::new(chunk(9441, first_page, first_generation, 8.0), [0.0, 0.0]),
         api::RenderChunkInstance::new(chunk(9442, second_page, second_generation, 48.0), [0.0, 0.0]),
      ],
      Vec::new(),
      api::Damage { rects: Vec::new() },
   ).expect("valid glyph page benchmark snapshot")
}

fn metal_text_paged_atlas_locality_case(id: &str, smoke: bool) -> Result<PerfCaseResult>
{
   let paged = std::env::var("OXIDE_C44_PAGED_ATLAS").map_or(true, |value| value != "0");
   let mut renderer = metal::MetalRenderer::new_default().context("creating glyph-page Metal renderer")?;
   renderer.resize(96, 64, 1.0).context("resizing glyph-page Metal renderer")?;
   let page_bytes = 64_u64 * 64;
   let mut first_page;
   let second_page;
   let mut first_generation = 1_u64;
   if paged
   {
      first_page = renderer.image_create_a8(64, 64, &[255; 64 * 64], 64);
      second_page = renderer.image_create_a8(64, 64, &[255; 64 * 64], 64);
   }
   else
   {
      first_page = renderer.image_create_a8(128, 64, &[255; 128 * 64], 128);
      second_page = first_page;
   }
   let warm = glyph_page_snapshot(first_page, first_generation, second_page, first_generation);
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_snapshot(&warm).context("preparing glyph-page benchmark")?;
   renderer.submit(token).context("submitting glyph-page benchmark warmup")?;
   let _ = last_metal_stats_after_submit(&renderer, token.0);

   let warmups = if smoke { 1_usize } else { 3 };
   let frames = if smoke {
      3_usize
   } else {
      std::env::var("OXIDE_C44_METAL_FRAMES")
         .ok()
         .and_then(|value| value.parse::<usize>().ok())
         .filter(|frames| *frames >= 16)
         .unwrap_or(32)
   };
   let mut frame_samples = Vec::with_capacity(frames);
   let mut warmup_frame_samples = Vec::with_capacity(warmups);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut cache_hits = 0_u64;
   let mut cache_misses = 0_u64;
   let mut chunks_prepared = 0_u64;
   let mut draws = 0_u64;

   for frame in 0..warmups.saturating_add(frames)
   {
      let started_at = Instant::now();
      if paged
      {
         renderer.image_release(first_page);
         first_page = renderer.image_create_a8(64, 64, &[255; 64 * 64], 64);
         first_generation = 1;
      }
      else
      {
         renderer.image_update_a8(first_page, 0, 0, 2, 2, &[255; 4], 2);
         first_generation = renderer.image_generation(first_page).unwrap_or(first_generation + 1);
      }
      let snapshot = glyph_page_snapshot(
         first_page,
         first_generation,
         second_page,
         if paged { 1 } else { first_generation },
      );
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_snapshot(&snapshot).with_context(|| format!("encoding {id}"))?;
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, token.0);
      if frame < warmups
      {
         warmup_frame_samples.push(started_at.elapsed().as_secs_f64() * 1_000.0);
         continue;
      }
      frame_samples.push(started_at.elapsed().as_secs_f64() * 1_000.0);
      encode_samples.push(stats.encode_ms);
      gpu_samples.push(stats.gpu_ms);
      cache_hits = cache_hits.saturating_add(stats.backend_cache_hits);
      cache_misses = cache_misses.saturating_add(stats.backend_cache_misses);
      chunks_prepared = chunks_prepared.saturating_add(stats.chunks_prepared);
      draws = draws.saturating_add(u64::from(stats.draws));
   }

   let summary = summarize(&frame_samples);
   let measured = frames.max(1) as f64;
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("paged_atlas"), u8::from(paged) as f64);
   metrics.insert(String::from("glyph_pages"), 2.0);
   metrics.insert(String::from("atlas_resident_bytes"), (page_bytes * 2) as f64);
   metrics.insert(String::from("atlas_upload_bytes_avg"), if paged { page_bytes as f64 } else { 4.0 });
   metrics.insert(String::from("invalidated_chunks_avg"), cache_misses as f64 / measured);
   metrics.insert(String::from("prepared_cache_hits_avg"), cache_hits as f64 / measured);
   metrics.insert(String::from("prepared_cache_misses_avg"), cache_misses as f64 / measured);
   metrics.insert(String::from("chunks_prepared_avg"), chunks_prepared as f64 / measured);
   metrics.insert(String::from("draws_avg"), draws as f64 / measured);
   if std::env::var_os("OXIDE_C44_RAW_SAMPLES").is_some()
   {
      insert_indexed_samples(&mut metrics, "c44_warmup_frame_ms", &warmup_frame_samples);
      insert_indexed_samples(&mut metrics, "c44_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "c44_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "c44_gpu_ms", &gpu_samples);
   }
   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from("engine"),
      scenario: String::from("rendering-architecture"),
      variant: String::from("oxide"),
      cache_state: String::from("pressure"),
      refresh_mode: String::from("offscreen"),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(if paged {
         "Two glyph pages: recycle one handle while the unrelated page retains its prepared chunk."
      } else {
         "Single global atlas control: one slot update invalidates both retained glyph chunks."
      })],
      metrics,
   })
}

fn text_script_matrix_case(smoke: bool) -> PerfCaseResult
{
   let strings = [
      "Oxide retained Latin text",
      "漢字かな交じり文",
      "مرحبا بالعالم",
      "😀🧭✨ fallback emoji",
      "Latin עברית 漢字 mixed bidi",
   ];
   let mut case = measured_architecture_case(
      "cpu.architecture.text.script_fallback_matrix",
      smoke,
      "Latin, CJK, RTL, emoji, fallback, and mixed-bidi shaping matrix.",
      move || {
         let mut text = perf_text_ctx();
         text.set_fallback_fonts(&[1]);
         let mut uploader = CpuUploader::default();
         let mut builder = ui::DrawListBuilder::new();
         text.begin_frame();
         for (index, value) in strings.iter().enumerate()
         {
            encode_matrix_label(value, index, 3.0, 24.0, &mut text, &mut uploader, &mut builder);
         }
         let stats = text.finish_frame(&mut uploader, &mut builder);
         builder.drawlist().items.len() as u64
            + builder.drawlist().vertices.len() as u64
            + stats.atlas_upload_calls
      },
   );
   case.metrics.insert(String::from("script_variants"), strings.len() as f64);
   case.metrics.insert(String::from("fallback_fonts"), 1.0);
   case
}

fn text_scale_sdf_matrix_case(smoke: bool) -> PerfCaseResult
{
   let variants = [(2.0_f32, 48.0_f32), (2.0, 96.0), (3.0, 48.0), (3.0, 96.0)];
   let mut case = measured_architecture_case(
      "cpu.architecture.text.scale_sdf_matrix",
      smoke,
      "Text preparation at 2x/3x device scale and 48/96 px SDF pressure sizes.",
      move || {
         let mut checksum = 0_u64;
         for (index, (scale, font_px)) in variants.iter().copied().enumerate()
         {
            let mut text = perf_text_ctx();
            let mut uploader = CpuUploader::default();
            let mut builder = ui::DrawListBuilder::new();
            text.begin_frame();
            encode_matrix_label("SDF Scale Matrix", index, scale, font_px, &mut text, &mut uploader, &mut builder);
            let stats = text.finish_frame(&mut uploader, &mut builder);
            checksum = checksum
               .wrapping_add(builder.drawlist().vertices.len() as u64)
               .wrapping_add(text.atlas_revision())
               .wrapping_add(stats.atlas_upload_calls);
         }
         checksum
      },
   );
   case.metrics.insert(String::from("scale_variants"), 2.0);
   case.metrics.insert(String::from("sdf_size_variants"), 2.0);
   case.metrics.insert(String::from("max_font_px"), 96.0);
   case
}

fn text_atlas_eviction_case(smoke: bool) -> PerfCaseResult
{
   let mut case = measured_architecture_case(
      "cpu.architecture.text.atlas_eviction",
      smoke,
      "Constrained glyph atlas churn with actual shape bake, dirty regions, and eviction accounting.",
      move || run_text_atlas_pressure_stats().checksum,
   );
   let stats = run_text_atlas_pressure_stats();
   case.metrics.insert(String::from("atlas_evictions"), stats.evictions as f64);
   case.metrics.insert(String::from("atlas_resident_glyphs"), stats.resident_glyphs as f64);
   case.metrics.insert(String::from("atlas_dirty_pixels"), stats.dirty_pixels as f64);
   case.metrics.insert(String::from("atlas_revision"), stats.revision as f64);
   case
}

#[derive(Default)]
struct PagedAtlasCpuUploader
{
   next: u32,
   creates: u64,
   appends: u64,
   releases: u64,
   upload_bytes: u64,
}

impl ui::elements::ImageUploader for PagedAtlasCpuUploader
{
   fn create_a8(&mut self, w: u32, h: u32, _data: &[u8], _row_bytes: usize) -> api::ImageHandle
   {
      self.next = self.next.saturating_add(1).max(1);
      self.creates = self.creates.saturating_add(1);
      self.upload_bytes = self.upload_bytes.saturating_add(u64::from(w).saturating_mul(u64::from(h)));
      api::ImageHandle(self.next)
   }

   fn update_a8(
      &mut self,
      _handle: api::ImageHandle,
      _x: u32,
      _y: u32,
      _w: u32,
      _h: u32,
      _data: &[u8],
      _row_bytes: usize,
   )
   {
      unreachable!("paged atlas locality uses append-only updates")
   }

   fn append_a8(
      &mut self,
      _handle: api::ImageHandle,
      _x: u32,
      _y: u32,
      w: u32,
      h: u32,
      _data: &[u8],
      _row_bytes: usize,
   )
   {
      self.appends = self.appends.saturating_add(1);
      self.upload_bytes = self.upload_bytes.saturating_add(u64::from(w).saturating_mul(u64::from(h)));
   }

   fn release_a8(&mut self, _handle: api::ImageHandle)
   {
      self.releases = self.releases.saturating_add(1);
   }
}

#[derive(Default)]
struct PagedAtlasLocalityStats
{
   checksum: u64,
   pages: u64,
   resident_bytes: u64,
   occupied_bytes: u64,
   fragmentation_bytes: u64,
   evictions: u64,
   creates: u64,
   appends: u64,
   releases: u64,
   upload_bytes: u64,
   stable_pages: u64,
}

fn run_paged_atlas_locality() -> PagedAtlasLocalityStats
{
   let mut text = perf_text_ctx();
   text.atlas = oxide_text::PagedAtlas::new(24, 24, 2);
   let mut uploader = PagedAtlasCpuUploader::default();
   let mut builder = ui::DrawListBuilder::new();
   let mut labels = Vec::new();

   text.begin_frame();
   for ch in 'A'..='Z'
   {
      let label = ch.to_string();
      encode_matrix_label(&label, labels.len(), 1.0, 16.0, &mut text, &mut uploader, &mut builder);
      labels.push(label);
   }
   let _ = text.finish_frame(&mut uploader, &mut builder);
   let first_revisions = text.retained_text_atlas_revisions().unwrap_or(&[]).to_vec();
   let second_handle = first_revisions.get(1).map(|(handle, _)| *handle).unwrap_or(api::ImageHandle(0));
   let pinned_index = builder.drawlist().items.iter().filter_map(|item| {
      match item
      {
         api::DrawCmd::GlyphRun { run } => Some(*run),
         _ => None,
      }
   }).position(|run| run.atlas == second_handle).unwrap_or(0);
   let pinned_label = labels.get(pinned_index).cloned().unwrap_or_else(|| String::from("A"));

   builder.clear();
   text.begin_frame();
   encode_matrix_label(&pinned_label, 0, 1.0, 16.0, &mut text, &mut uploader, &mut builder);
   'pressure: for label in &labels
   {
      encode_matrix_label(label, 0, 2.0, 16.0, &mut text, &mut uploader, &mut builder);
      if text.atlas.eviction_count() > 0
      {
         break 'pressure;
      }
   }
   let _ = text.finish_frame(&mut uploader, &mut builder);
   let second_revisions = text.retained_text_atlas_revisions().unwrap_or(&[]);
   let stable_pages = u64::from(second_revisions.iter().any(|(handle, _)| *handle == second_handle));
   let atlas = text.atlas.stats();
   PagedAtlasLocalityStats {
      checksum: builder.drawlist().items.len() as u64
         + builder.drawlist().vertices.len() as u64
         + atlas.evictions
         + stable_pages,
      pages: atlas.pages as u64,
      resident_bytes: atlas.resident_bytes,
      occupied_bytes: atlas.occupied_bytes,
      fragmentation_bytes: atlas.fragmentation_bytes,
      evictions: atlas.evictions,
      creates: uploader.creates,
      appends: uploader.appends,
      releases: uploader.releases,
      upload_bytes: uploader.upload_bytes,
      stable_pages,
   }
}

fn text_paged_atlas_locality_case(smoke: bool) -> PerfCaseResult
{
   let mut case = measured_architecture_case(
      "cpu.architecture.text.paged_atlas_locality",
      smoke,
      "Two bounded glyph pages under deterministic pressure while one visible page remains pinned and retains its resource identity.",
      move || run_paged_atlas_locality().checksum,
   );
   let stats = run_paged_atlas_locality();
   for (name, value) in [
      ("atlas_pages", stats.pages),
      ("atlas_resident_bytes", stats.resident_bytes),
      ("atlas_occupied_bytes", stats.occupied_bytes),
      ("atlas_fragmentation_bytes", stats.fragmentation_bytes),
      ("atlas_evictions", stats.evictions),
      ("atlas_create_calls", stats.creates),
      ("atlas_append_calls", stats.appends),
      ("atlas_release_calls", stats.releases),
      ("atlas_upload_bytes", stats.upload_bytes),
      ("stable_unrelated_pages", stats.stable_pages),
   ]
   {
      case.metrics.insert(String::from(name), value as f64);
   }
   case
}

fn encode_matrix_label<U: ui::elements::ImageUploader>(
   value: &str,
   index: usize,
   scale: f32,
   font_px: f32,
   text: &mut ui::elements::TextCtx,
   uploader: &mut U,
   builder: &mut ui::DrawListBuilder,
)
{
   ui::elements::encode_label_text(
      value,
      api::Color::rgba(0.1, 0.1, 0.12, 1.0),
      ui::elements::Align::Left,
      false,
      0,
      font_px,
      api::RectF::new(0.0, (index % 200) as f32 * (font_px + 2.0), 1_000.0, font_px + 4.0),
      scale,
      text,
      uploader,
      builder,
   );
}

fn encode_matrix_label_profiled<U: ui::elements::ImageUploader>(
   value: &str,
   index: usize,
   scale: f32,
   font_px: f32,
   text: &mut ui::elements::TextCtx,
   uploader: &mut U,
   builder: &mut ui::DrawListBuilder,
)
{
   ui::elements::encode_label_text_profiled(
      value,
      api::Color::rgba(0.1, 0.1, 0.12, 1.0),
      ui::elements::Align::Left,
      false,
      0,
      font_px,
      api::RectF::new(0.0, (index % 200) as f32 * (font_px + 2.0), 1_000.0, font_px + 4.0),
      scale,
      text,
      uploader,
      builder,
   );
}

fn layer_matrix_case(smoke: bool) -> PerfCaseResult
{
   let mut phase = 0_u32;
   let mut case = measured_architecture_case(
      "cpu.architecture.layers.matrix",
      smoke,
      "100 clean layers x 100 draws plus dirty-layer, resize, navigation churn, nesting, backdrop dependency, and memory-warning rebuild variants.",
      move || {
         phase = phase.wrapping_add(1);
         let mut checksum = 0_u64;
         for variant in 0..7_u32
         {
            let mut builder = ui::DrawListBuilder::new();
            for layer in 0..100_u32
            {
               let dirty = variant == 1 && layer == phase % 100;
               let id = if variant == 3 { layer.wrapping_add(phase * 100) } else { layer };
               let inset = if variant == 2 { (phase & 1) as f32 * 2.0 } else { 0.0 };
               builder.layer_begin(id, api::RectF::new(inset, inset, 1_000.0 - inset, 700.0 - inset), dirty || variant == 6);
               if variant == 4 && layer % 10 == 0
               {
                  builder.layer_begin(id + 10_000, api::RectF::new(10.0, 10.0, 400.0, 300.0), dirty);
               }
               for draw in 0..100_u32
               {
                  let x = (draw % 10) as f32 * 8.0;
                  let y = (draw / 10) as f32 * 8.0;
                  builder.rrect(api::RectF::new(x, y, 6.0, 6.0), [1.0; 4], api::Color::rgba(0.2, 0.5, 0.9, 0.9));
               }
               if variant == 5
               {
                  builder.backdrop(api::RectF::new(0.0, 0.0, 80.0, 80.0), 8.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.2);
               }
               if variant == 4 && layer % 10 == 0
               {
                  builder.layer_end();
               }
               builder.layer_end();
            }
            checksum = checksum.wrapping_add(builder.drawlist().items.len() as u64);
         }
         checksum
      },
   );
   case.metrics.insert(String::from("layers"), 100.0);
   case.metrics.insert(String::from("draws_per_layer"), 100.0);
   case.metrics.insert(String::from("layer_variants"), 7.0);
   case.metrics.insert(String::from("cache_memory_warning_rebuilds"), 100.0);
   case
}

fn primitive_case(id: &str, smoke: bool, name: &str, count: usize) -> PerfCaseResult
{
   let kind = String::from(name);
   let mut case = measured_architecture_case(
      id,
      smoke,
      "Renderer-facing primitive command generation at the declared scaling point.",
      move || {
         if kind.starts_with("neon")
         {
            let markers = (0..count).map(|index| oxide_renderer_web::neon_marker::NeonMarker {
               center: [(index % 64) as f32 * 12.0, (index / 64) as f32 * 12.0],
               core_radius_px: 2.0,
               ring_radius_px: 4.0,
               ring_width_px: 1.0,
               halo_radius_px: 8.0,
               halo_sigma_px: 3.0,
               core_color: api::Color::rgba(1.0, 0.8, 0.2, 1.0),
               ring_color: api::Color::rgba(0.2, 0.8, 1.0, 1.0),
               halo_alpha_max: 0.5,
               ring_alpha_max: 0.8,
            }).collect::<Vec<_>>();
            return markers.iter().map(|marker| (marker.bounds().w + marker.bounds().h) as u64).sum();
         }
         let mut builder = ui::DrawListBuilder::new();
         for index in 0..count
         {
            let x = (index % 64) as f32 * 12.0;
            let y = (index / 64) as f32 * 12.0;
            if kind.starts_with("rrect")
            {
               builder.rrect(api::RectF::new(x, y, 10.0, 10.0), [3.0; 4], api::Color::rgba(0.2, 0.7, 0.9, 1.0));
            }
            else if kind.starts_with("spinner")
            {
               builder.spinner([x + 5.0, y + 5.0], (index % 17) as f32 / 17.0, 1.0);
            }
            else
            {
               builder.nine_slice(api::ImageHandle((index % 8 + 1) as u32), api::RectF::new(x, y, 18.0, 18.0), api::Insets::new(4.0, 4.0, 4.0, 4.0), 1.0);
            }
         }
         builder.drawlist().items.len() as u64
      },
   );
   case.metrics.insert(String::from("primitive_count"), count as f64);
   case.metrics.insert(String::from("expected_draw_items"), count as f64);
   case
}

fn effect_case(id: &str, smoke: bool, name: &str) -> PerfCaseResult
{
   let kind = String::from(name);
   let mut case = measured_architecture_case(
      id,
      smoke,
      "Backdrop/blur/effect command matrix preserving declared regions, sigma variation, edges, corners, and nested layers.",
      move || {
         let mut builder = ui::DrawListBuilder::new();
         if kind == "backdrop_separated_48"
         {
            for index in 0..48 { builder.backdrop(api::RectF::new((index % 8) as f32 * 100.0, (index / 8) as f32 * 90.0, 48.0, 42.0), 8.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.3); }
         }
         else if kind == "backdrop_coalescible_12"
         {
            for index in 0..12 { builder.backdrop(api::RectF::new(index as f32 * 70.0, 40.0, 40.0, 40.0), 6.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.25); }
         }
         else if kind == "blur_fullscreen"
         {
            builder.backdrop(api::RectF::new(0.0, 0.0, 1_920.0, 1_080.0), 32.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.2);
         }
         else if kind == "blur_mixed_sigma"
         {
            for index in 0..16 { builder.backdrop(api::RectF::new(index as f32 * 30.0, 20.0, 120.0, 80.0), 2.0 + index as f32 * 3.0, api::Color::rgba(0.9, 0.95, 1.0, 1.0), 0.35); }
         }
         else if kind == "blur_edges_corners"
         {
            for rect in [api::RectF::new(-30.0, -20.0, 160.0, 100.0), api::RectF::new(1_850.0, -20.0, 120.0, 100.0), api::RectF::new(-30.0, 1_020.0, 160.0, 100.0), api::RectF::new(1_850.0, 1_020.0, 120.0, 100.0)] { builder.backdrop(rect, 24.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.3); }
         }
         else
         {
            builder.layer_begin(1, api::RectF::new(0.0, 0.0, 900.0, 700.0), true);
            builder.layer_begin(2, api::RectF::new(40.0, 40.0, 700.0, 500.0), true);
            builder.backdrop(api::RectF::new(80.0, 80.0, 500.0, 300.0), 18.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.4);
            builder.layer_end();
            builder.layer_end();
         }
         builder.drawlist().items.len() as u64
      },
   );
   case.metrics.insert(String::from("effect_regions"), effect_region_count(name) as f64);
   case.metrics.insert(String::from("full_resolution_pixels"), 1_920.0 * 1_080.0);
   case
}

fn effect_region_count(name: &str) -> usize
{
   match name
   {
      "target_plan_direct" => 0,
      "backdrop_separated_48" => 48,
      "backdrop_coalescible_12" => 12,
      "blur_mixed_sigma" => 16,
      "blur_edges_corners" => 4,
      _ => 1,
   }
}

fn blur_sweep_spec(name: &str) -> Option<(f32, bool)>
{
   match name
   {
      "blur_sigma_2_local" => Some((2.0, true)),
      "blur_sigma_8_local" => Some((8.0, true)),
      "blur_sigma_16_fullscreen" => Some((16.0, false)),
      "blur_sigma_32_fullscreen" => Some((32.0, false)),
      "blur_sigma_64_fullscreen" => Some((64.0, false)),
      _ => None,
   }
}

fn damage_case(id: &str, smoke: bool, name: &str) -> PerfCaseResult
{
   let kind = String::from(name);
   let mut phase = 0_u64;
   let mut case = measured_architecture_case(
      id,
      smoke,
      "Deterministic damage sequence over a 10k-item scene with caret, mutation, movement/removal, percentage, and full-to-partial variants.",
      move || {
         phase = phase.wrapping_add(1);
         let mut builder = ui::DrawListBuilder::new();
         let count = if kind == "caret_blink" { 1 } else { 10_000 };
         for index in 0..count
         {
            let x = (index % 100) as f32 * 10.0;
            let y = (index / 100) as f32 * 7.0;
            if kind == "removed_node" && index == phase as usize % count { continue; }
            let moved = if kind == "moving_node" && index == phase as usize % count { 4.0 } else { 0.0 };
            builder.rrect(api::RectF::new(x + moved, y, if kind == "caret_blink" { 2.0 } else { 8.0 }, 6.0), [1.0; 4], api::Color::rgba(0.2, 0.6, 0.95, 1.0));
         }
         let damage = damage_rect_for(&kind, phase);
         builder.drawlist().items.len() as u64 + damage.rects.len() as u64 + damage.rects.iter().map(|rect| rect.w.max(0) as u64 * rect.h.max(0) as u64).sum::<u64>()
      },
   );
   let damage = damage_rect_for(name, 1);
   let pixels = damage.rects.iter().map(|rect| rect.w.max(0) as u64 * rect.h.max(0) as u64).sum::<u64>();
   case.metrics.insert(String::from("scene_items"), if name == "caret_blink" { 1.0 } else { 10_000.0 });
   case.metrics.insert(String::from("damage_rects"), damage.rects.len() as f64);
   case.metrics.insert(String::from("damage_pixels"), pixels as f64);
   case.metrics.insert(String::from("submissions_expected"), if name == "full_direct_then_partial" { 2.0 } else { 1.0 });
   case
}

fn damage_rect_for(name: &str, phase: u64) -> api::Damage
{
   let full = api::RectI::new(0, 0, 1_000, 700);
   let rects = match name
   {
      "damage_5pct" => vec![api::RectI::new(0, 0, 250, 140)],
      "damage_25pct" => vec![api::RectI::new(0, 0, 500, 350)],
      "damage_100pct" => vec![full],
      "full_direct_then_partial" => {
         if phase & 1 == 0 { vec![full] } else { vec![api::RectI::new(20, 20, 80, 60)] }
      },
      "moving_node" | "removed_node" | "isolated_mutation_10000" => vec![api::RectI::new((phase % 900) as i32, (phase % 600) as i32, 20, 20)],
      _ => vec![api::RectI::new(10, 10, 3, 24)],
   };
   api::Damage { rects }
}

const METAL_INLINE_BYTES: usize = 4_096;
const METAL_IMAGE_TABLE_TEXTURES: usize = 128;
const IMAGE_FRAGMENT_BYTES: usize = 48;
const RRECT_FRAGMENT_BYTES: usize = 48;
const RRECT_PARAMETER_BYTES: usize = 64;
const VIEWPORT_PARAMETER_BYTES: usize = 8;

fn noop_drawlist(name: &str) -> api::DrawList
{
   let mut builder = ui::DrawListBuilder::new();
   for index in 0..4_096
   {
      let x = (index % 64) as f32 * 18.0;
      let y = (index / 64) as f32 * 12.0;
      if name == "transparent_containers"
      {
         builder.rrect(
            api::RectF::new(x, y, 16.0, 10.0),
            [2.0; 4],
            api::Color::rgba(0.2, 0.6, 0.95, 0.0),
         );
      }
      else
      {
         builder.rrect(
            api::RectF::new(x, y, 0.0, 10.0),
            [2.0; 4],
            api::Color::rgba(0.2, 0.6, 0.95, 1.0),
         );
      }
   }
   for index in 0..64
   {
      builder.rrect(
         api::RectF::new(index as f32 * 18.0, 760.0, 16.0, 10.0),
         [2.0; 4],
         api::Color::rgba(0.95, 0.5, 0.16, 1.0),
      );
   }
   builder.into_inner()
}

fn noop_case(id: &str, smoke: bool, name: &str) -> PerfCaseResult
{
   let kind = String::from(name);
   let mut case = measured_architecture_case(
      id,
      smoke,
      "DrawListBuilder no-op rejection with 4,096 invisible and 64 visible commands.",
      move || noop_drawlist(&kind).items.len() as u64,
   );
   let emitted = noop_drawlist(name).items.len();
   case.metrics.insert(String::from("input_noop_commands"), 4_096.0);
   case.metrics.insert(String::from("visible_commands"), 64.0);
   case.metrics.insert(String::from("emitted_commands"), emitted as f64);
   case
}

fn metal_noop_case(id: &str, smoke: bool, name: &str) -> Result<PerfCaseResult>
{
   let kind = String::from(name);
   let mut case = measured_metal_drawlist_case(
      id,
      smoke,
      format!("Metal no-op rejection workload for {name} with 4,096 invisible and 64 visible commands."),
      move |_| (noop_drawlist(&kind), None, None, false),
   )?;
   let emitted = noop_drawlist(name).items.len();
   case.metrics.insert(String::from("input_noop_commands"), 4_096.0);
   case.metrics.insert(String::from("visible_commands"), 64.0);
   case.metrics.insert(String::from("emitted_commands"), emitted as f64);
   let max_batch = METAL_INLINE_BYTES / RRECT_FRAGMENT_BYTES;
   let draw_calls = emitted.div_ceil(max_batch);
   case.metrics.insert(String::from("instanced_draw_calls_avg"), draw_calls as f64);
   case.metrics.insert(
      String::from("parameter_upload_bytes_avg"),
      (VIEWPORT_PARAMETER_BYTES + emitted * RRECT_PARAMETER_BYTES) as f64,
   );
   Ok(case)
}

struct ImageViewGridWork
{
   images: usize,
   nine_slices: usize,
   source_crops: usize,
   draw_calls: usize,
   inline_parameter_bytes: usize,
   logical_shaded_pixels: f64,
}

fn image_view_grid_drawlist(handles: &[api::ImageHandle]) -> api::DrawList
{
   let mut builder = ui::DrawListBuilder::new();
   encode_image_view_grid(handles, &mut builder);
   builder.into_inner()
}

fn encode_image_view_grid(handles: &[api::ImageHandle], builder: &mut ui::DrawListBuilder)
{
   builder.clear();
   for (index, handle) in handles.iter().copied().enumerate()
   {
      let rect = api::RectF::new(
         (index % 40) as f32 * 30.0 + 3.0,
         (index / 40) as f32 * 16.0 + 2.0,
         24.0,
         12.0,
      );
      ui::elements::ImageView {
         image: handle,
         natural_w: 29,
         natural_h: 7,
         fit: ui::elements::ImageFit::Cover,
         alpha: 1.0,
      }
      .encode(rect, None, builder);
   }
}

fn image_view_grid_work(list: &api::DrawList) -> ImageViewGridWork
{
   let mut images = 0usize;
   let mut nine_slices = 0usize;
   let mut source_crops = 0usize;
   let mut logical_shaded_pixels = 0.0;
   for item in &list.items
   {
      match item
      {
         api::DrawCmd::Image { dst, src, .. } => {
            images += 1;
            source_crops += usize::from(src.x > 0.0 || src.y > 0.0 || src.w < 29.0 || src.h < 7.0);
            logical_shaded_pixels += f64::from(dst.w) * f64::from(dst.h);
         }
         api::DrawCmd::NineSlice { rect, .. } => {
            nine_slices += 1;
            logical_shaded_pixels += f64::from(rect.w) * f64::from(rect.h);
         }
         _ => {}
      }
   }
   let image_groups = images.div_ceil(METAL_IMAGE_TABLE_TEXTURES);
   let image_draw_calls = (0..image_groups)
      .map(|group| {
         let remaining = images.saturating_sub(group * METAL_IMAGE_TABLE_TEXTURES);
         let batch = remaining.min(METAL_IMAGE_TABLE_TEXTURES);
         batch.div_ceil(METAL_INLINE_BYTES / IMAGE_FRAGMENT_BYTES)
      })
      .sum::<usize>();
   ImageViewGridWork {
      images,
      nine_slices,
      source_crops,
      draw_calls: nine_slices + image_draw_calls,
      inline_parameter_bytes: nine_slices * 88 + images * 64 + image_groups * VIEWPORT_PARAMETER_BYTES,
      logical_shaded_pixels,
   }
}

fn image_case(id: &str, smoke: bool, name: &str, count: usize) -> PerfCaseResult
{
   let kind = String::from(name);
   let image_view_handles = if name.starts_with("image_view_cover_grid_")
   {
      (1..=count).map(|value| api::ImageHandle(value as u32)).collect::<Vec<_>>()
   }
   else
   {
      Vec::new()
   };
   let mut image_view_builder = ui::DrawListBuilder::new();
   if !image_view_handles.is_empty()
   {
      encode_image_view_grid(&image_view_handles, &mut image_view_builder);
   }
   let mut phase = 0_u32;
   let mut case = measured_architecture_case(
      id,
      smoke,
      "Unique icon/avatar residency command matrix with contain/cover/zoom at 3x, display-size decode accounting, release/reuse churn, and minification/mip intent.",
      move || {
         phase = phase.wrapping_add(1);
         if kind.starts_with("image_view_cover_grid_")
         {
            encode_image_view_grid(&image_view_handles, &mut image_view_builder);
            return image_view_builder.drawlist().items.iter().fold(phase as u64, |checksum, item| {
               let value = match item
               {
                  api::DrawCmd::Image { src, .. } => u64::from(src.x.to_bits()) ^ u64::from(src.w.to_bits()),
                  api::DrawCmd::NineSlice { rect, .. } => u64::from(rect.x.to_bits()) ^ u64::from(rect.w.to_bits()),
                  _ => 0,
               };
               checksum.wrapping_add(value)
            });
         }
         let mut builder = ui::DrawListBuilder::new();
         for index in 0..count
         {
            let handle = if kind == "release_reuse" { api::ImageHandle(((index + phase as usize) % 128 + 1) as u32) } else { api::ImageHandle((index + 1) as u32) };
            let cell = api::RectF::new((index % 100) as f32 * 12.0, (index / 100) as f32 * 12.0, 10.0, 10.0);
            let dst = if kind == "zoom_3x" { api::RectF::new(cell.x - 10.0, cell.y - 10.0, 30.0, 30.0) } else { cell };
            let src = if kind == "cover_3x" { api::RectF::new(0.15, 0.0, 0.70, 1.0) } else { api::RectF::new(0.0, 0.0, 1.0, 1.0) };
            builder.image(handle, dst, src, 1.0);
         }
         builder.drawlist().items.len() as u64
      },
   );
   case.metrics.insert(String::from("unique_images"), count as f64);
   case.metrics.insert(String::from("image_draws"), count as f64);
   case.metrics.insert(String::from("device_scale"), if name.ends_with("3x") { 3.0 } else { 1.0 });
   case.metrics.insert(String::from("decode_at_display_size"), if name == "decode_display_size" { 1.0 } else { 0.0 });
   case.metrics.insert(String::from("mip_policy_requested"), if name == "minification_mips" { 1.0 } else { 0.0 });
   if name.starts_with("image_view_cover_grid_")
   {
      let handles = (1..=count).map(|value| api::ImageHandle(value as u32)).collect::<Vec<_>>();
      let work = image_view_grid_work(&image_view_grid_drawlist(&handles));
      case.family = String::from("authoring");
      case.layer = String::from("engine");
      case.scenario = String::from("authoring");
      case.metrics.insert(String::from("image_draws"), work.images as f64);
      case.metrics.insert(String::from("nine_slice_draws"), work.nine_slices as f64);
      case.metrics.insert(String::from("source_crop_commands"), work.source_crops as f64);
      case.metrics.insert(String::from("quads"), count as f64);
      case.metrics.insert(String::from("logical_shaded_pixels"), work.logical_shaded_pixels);
   }
   case
}

fn idle_case(smoke: bool) -> PerfCaseResult
{
   let mut router = prepare_cpu_router();
   router.set_scene(0);
   let viewport = api::RectF::new(0.0, 0.0, 1_200.0, 800.0);
   let mut builder = ui::DrawListBuilder::new();
   router.draw(viewport, 2.0, &mut builder);
   let baseline_items = builder.drawlist().items.len();
   let mut case = measured_architecture_case(
      "cpu.architecture.idle.static_foreground",
      smoke,
      "Active foreground static UI with no timers, animations, camera, network publication, damage, or actual renderer submission.",
      move || {
         let wants = router.wants_next_frame() as u64;
         wants + baseline_items as u64
      },
   );
   case.metrics.insert(String::from("timers"), 0.0);
   case.metrics.insert(String::from("animations"), 0.0);
   case.metrics.insert(String::from("camera_frames"), 0.0);
   case.metrics.insert(String::from("network_publications"), 0.0);
   case.metrics.insert(String::from("damage_rects"), 0.0);
   case.metrics.insert(String::from("submissions"), 0.0);
   case.metrics.insert(String::from("wakeups"), 0.0);
   case
}

fn measured_metal_drawlist_case<F>(id: &str, smoke: bool, notes: String, mut build: F) -> Result<PerfCaseResult>
where
   F: FnMut(usize) -> (api::DrawList, Option<api::Damage>, Option<(u32, u32)>, bool),
{
   measured_metal_drawlist_case_with_config(
      id,
      smoke,
      notes,
      metal::MetalRendererConfig::default(),
      &mut build,
   )
}

fn measured_metal_drawlist_case_with_config<F>(id: &str, smoke: bool, notes: String, config: metal::MetalRendererConfig, mut build: F) -> Result<PerfCaseResult>
where
   F: FnMut(usize) -> (api::DrawList, Option<api::Damage>, Option<(u32, u32)>, bool),
{
   let mut renderer = Box::new(metal::MetalRenderer::new_with_config(config).context("creating Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing Metal renderer")?;
   renderer.set_damage_options(true, DAMAGE_USE_THRESH, DAMAGE_PREFILTER_THRESH);
   let present_layer = if id.starts_with("gpu.architecture.final_target.")
   {
      let device = Device::system_default().context("creating final-target benchmark device")?;
      let layer = MetalLayer::new();
      layer.set_device(&device);
      layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm_sRGB);
      layer.set_drawable_size(CGSize::new(1_200.0, 800.0));
      layer.set_maximum_drawable_count(3);
      layer.set_display_sync_enabled(false);
      layer.set_framebuffer_only(true);
      Some(layer)
   }
   else
   {
      None
   };
   if id.starts_with("gpu.architecture.layers.")
   {
      let budget = std::env::var("OXIDE_ARCHITECTURE_LAYER_CACHE_BUDGET_BYTES")
         .ok()
         .and_then(|value| value.parse::<u64>().ok())
         .unwrap_or(16 * 1024 * 1024);
      renderer.set_layer_cache_budget_bytes(budget);
   }
   let warmups = std::env::var("OXIDE_ARCHITECTURE_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups > 0)
      .unwrap_or(if smoke { 1 } else { 3 });
   let frames = std::env::var("OXIDE_ARCHITECTURE_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 3 } else { 12 });
   let persist_raw = std::env::var_os("OXIDE_ARCHITECTURE_METAL_RAW_SAMPLES").is_some();
   let mut warmup_frame_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut warmup_encode_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut warmup_gpu_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut draws_sum = 0.0;
   let mut instanced_sum = 0.0;
   let mut commands_traversed_sum = 0.0;
   let mut vb_sum = 0.0;
   let mut ib_sum = 0.0;
   let mut ub_sum = 0.0;
   let mut damage_pixels_sum = 0.0;
   let mut damage_rects_sum = 0.0;
   let mut layer_bytes_peak = 0_u64;
   let mut total_bytes_peak = 0_u64;
   let mut effect_bytes_peak = 0_u64;
   let mut effect_prepass_bytes_peak = 0_u64;
   let mut effect_blur_chain_bytes_peak = 0_u64;
   let mut bloom_bytes_peak = 0_u64;
   let mut resource_creates_sum = 0_u64;
   let mut resource_grows_sum = 0_u64;
   let mut frame_ring_buffer_bytes_peak = 0_u64;
   let mut first_frame_ms = 0.0;
   let mut first_encode_ms = 0.0;
   let mut first_gpu_ms = 0.0;
   let mut first_resource_creates = 0_u32;
   let mut skips_sum = 0.0;
   let mut layer_body_commands_scanned_sum = 0.0;
   let mut layer_body_commands_copied_sum = 0.0;
   let mut layer_texture_creates_sum = 0.0;
   let mut layer_cache_hits_sum = 0.0;
   let mut layer_cache_misses_sum = 0.0;
   let mut layer_offscreen_draws_sum = 0.0;
   let mut layer_inline_draws_sum = 0.0;
   let mut layer_double_render_prevented_sum = 0.0;
   let mut layer_resident_bytes_peak = 0_u64;
   let mut layer_pool_bytes_peak = 0_u64;
   let mut layer_cpu_bytes_peak = 0_u64;
   let mut layer_budget_bytes = 0_u64;
   let mut layer_evictions_peak = 0_u64;
   let mut layer_recreations_peak = 0_u64;
   let mut layer_pool_reuses_peak = 0_u64;
   let mut layer_purges_peak = 0_u64;
   let mut layer_budget_violations = 0_u64;
   let mut effect_graph_effects_sum = 0.0;
   let mut effect_graph_captures_sum = 0.0;
   let mut effect_graph_pyramids_sum = 0.0;
   let mut effect_graph_pyramid_reuses_sum = 0.0;
   let mut effect_graph_plan_reuses_sum = 0.0;
   let mut effect_graph_capture_passes_sum = 0.0;
   let mut effect_graph_downsample_passes_sum = 0.0;
   let mut effect_graph_blur_horizontal_passes_sum = 0.0;
   let mut effect_graph_blur_vertical_passes_sum = 0.0;
   let mut effect_graph_composite_passes_sum = 0.0;
   let mut effect_graph_max_lifetime_commands_peak = 0_u32;
   let mut effect_graph_resources_sum = 0.0;
   let mut effect_graph_alias_slots_sum = 0.0;
   let mut effect_graph_logical_bytes_peak = 0_u64;
   let mut effect_graph_physical_bytes_peak = 0_u64;
   let mut effect_graph_aliased_bytes_peak = 0_u64;
   let mut blur_kernel_paired_passes_sum = 0_u64;
   let mut blur_kernel_exact_passes_sum = 0_u64;
   let mut blur_kernel_source_samples_sum = 0_u64;
   let mut blur_kernel_encoded_samples_sum = 0_u64;
   let mut blur_kernel_runtime_exp_taps_sum = 0_u64;
   let mut blur_kernel_table_bytes_peak = 0_u64;
   let mut blit_passes_sum = 0_u64;
   let mut texture_copies_sum = 0_u64;
   let mut texture_copy_bytes_sum = 0_u64;
   let mut persistent_target_frames = 0_u64;
   let mut draw_target_main_bytes_peak = 0_u64;

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      let (draws, damage, resize, recreate) = build(frame);
      if recreate
      {
         if id == "gpu.architecture.layers.memory_warning"
         {
            renderer.purge_layer_cache_for_memory_warning();
         }
         else
         {
            renderer = Box::new(metal::MetalRenderer::new_with_config(config).context("recreating Metal renderer after benchmark reset")?);
            renderer.resize(1_200, 800, 1.0).context("resizing recreated Metal renderer")?;
            renderer.set_damage_options(true, DAMAGE_USE_THRESH, DAMAGE_PREFILTER_THRESH);
         }
      }
      if let Some((width, height)) = resize
      {
         renderer.resize(width, height, 1.0).with_context(|| format!("resizing for {id}"))?;
         if let Some(layer) = present_layer.as_ref()
         {
            layer.set_drawable_size(CGSize::new(width as f64, height as f64));
         }
      }
      if let Some(layer) = present_layer.as_ref()
      {
         let drawable = layer.next_drawable().context("acquiring final-target benchmark drawable")?;
         unsafe
         {
            renderer
               .prepare_present_drawable(drawable.as_ptr().cast())
               .with_context(|| format!("preparing drawable for {id}"))?;
         }
      }
      let token = renderer.begin_frame(&api::FrameTarget, damage.as_ref());
      let frame_id = token.0;
      renderer.encode_pass(&draws);
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      effect_bytes_peak = effect_bytes_peak.max(stats.memory.effect_targets_bytes);
      effect_prepass_bytes_peak =
         effect_prepass_bytes_peak.max(stats.memory.effect_prepass_bytes);
      effect_blur_chain_bytes_peak =
         effect_blur_chain_bytes_peak.max(stats.memory.effect_blur_chain_bytes);
      bloom_bytes_peak = bloom_bytes_peak.max(stats.memory.bloom_targets_bytes);
      resource_creates_sum = resource_creates_sum.saturating_add(stats.resource_creates as u64);
      frame_ring_buffer_bytes_peak =
         frame_ring_buffer_bytes_peak.max(stats.memory.frame_ring_buffer_bytes);
      if frame == 0
      {
         first_frame_ms = frame_t0.elapsed().as_secs_f64() * 1_000.0;
         first_encode_ms = stats.encode_ms;
         first_gpu_ms = stats.gpu_ms;
         first_resource_creates = stats.resource_creates;
      }
      if frame >= warmups
      {
         frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
         instanced_sum += stats.instanced as f64;
         commands_traversed_sum += stats.commands_traversed as f64;
         vb_sum += stats.vb_bytes as f64;
         ib_sum += stats.ib_bytes as f64;
         ub_sum += stats.ub_bytes as f64;
         damage_pixels_sum += stats.damage_px as f64;
         damage_rects_sum += stats.damage_rects as f64;
         layer_bytes_peak = layer_bytes_peak.max(stats.memory.layer_cache_bytes);
         total_bytes_peak = total_bytes_peak.max(stats.memory.total_bytes);
         resource_grows_sum = resource_grows_sum.saturating_add(stats.resource_grows as u64);
         skips_sum += stats.frame_backpressure_skipped as f64;
         layer_body_commands_scanned_sum += stats.layer_body_commands_scanned as f64;
         layer_body_commands_copied_sum += stats.layer_body_commands_copied as f64;
         layer_texture_creates_sum += stats.layer_texture_creates as f64;
         layer_cache_hits_sum += stats.layer_cache_hits as f64;
         layer_cache_misses_sum += stats.layer_cache_misses as f64;
         layer_offscreen_draws_sum += stats.layer_offscreen_draws as f64;
         layer_inline_draws_sum += stats.layer_inline_draws as f64;
         layer_double_render_prevented_sum += stats.layer_double_render_prevented as f64;
         layer_resident_bytes_peak = layer_resident_bytes_peak.max(stats.layer_cache_resident_bytes);
         layer_pool_bytes_peak = layer_pool_bytes_peak.max(stats.layer_cache_pool_bytes);
         layer_cpu_bytes_peak = layer_cpu_bytes_peak.max(stats.layer_cache_cpu_bytes);
         layer_budget_bytes = stats.layer_cache_budget_bytes;
         layer_evictions_peak = layer_evictions_peak.max(stats.layer_cache_evictions);
         layer_recreations_peak = layer_recreations_peak.max(stats.layer_cache_recreations);
         layer_pool_reuses_peak = layer_pool_reuses_peak.max(stats.layer_cache_pool_reuses);
         layer_purges_peak = layer_purges_peak.max(stats.layer_cache_purges);
         if stats.layer_cache_resident_bytes.saturating_add(stats.layer_cache_pool_bytes)
            > stats.layer_cache_budget_bytes
         {
            layer_budget_violations = layer_budget_violations.saturating_add(1);
         }
         effect_graph_effects_sum += stats.effect_graph_effects as f64;
         effect_graph_captures_sum += stats.effect_graph_captures as f64;
         effect_graph_pyramids_sum += stats.effect_graph_pyramids as f64;
         effect_graph_pyramid_reuses_sum += stats.effect_graph_pyramid_reuses as f64;
         effect_graph_plan_reuses_sum += stats.effect_graph_plan_reuses as f64;
         effect_graph_capture_passes_sum += stats.effect_graph_capture_passes as f64;
         effect_graph_downsample_passes_sum += stats.effect_graph_downsample_passes as f64;
         effect_graph_blur_horizontal_passes_sum +=
            stats.effect_graph_blur_horizontal_passes as f64;
         effect_graph_blur_vertical_passes_sum +=
            stats.effect_graph_blur_vertical_passes as f64;
         effect_graph_composite_passes_sum += stats.effect_graph_composite_passes as f64;
         effect_graph_max_lifetime_commands_peak = effect_graph_max_lifetime_commands_peak
            .max(stats.effect_graph_max_lifetime_commands);
         effect_graph_resources_sum += stats.effect_graph_resources as f64;
         effect_graph_alias_slots_sum += stats.effect_graph_alias_slots as f64;
         effect_graph_logical_bytes_peak = effect_graph_logical_bytes_peak
            .max(stats.effect_graph_logical_bytes);
         effect_graph_physical_bytes_peak = effect_graph_physical_bytes_peak
            .max(stats.effect_graph_physical_bytes);
         effect_graph_aliased_bytes_peak = effect_graph_aliased_bytes_peak
            .max(stats.effect_graph_aliased_bytes);
         blur_kernel_paired_passes_sum = blur_kernel_paired_passes_sum
            .saturating_add(stats.blur_kernel_paired_passes as u64);
         blur_kernel_exact_passes_sum = blur_kernel_exact_passes_sum
            .saturating_add(stats.blur_kernel_exact_passes as u64);
         blur_kernel_source_samples_sum = blur_kernel_source_samples_sum
            .saturating_add(stats.blur_kernel_source_samples);
         blur_kernel_encoded_samples_sum = blur_kernel_encoded_samples_sum
            .saturating_add(stats.blur_kernel_encoded_samples);
         blur_kernel_runtime_exp_taps_sum = blur_kernel_runtime_exp_taps_sum
            .saturating_add(stats.blur_kernel_runtime_exp_taps);
         blur_kernel_table_bytes_peak = blur_kernel_table_bytes_peak
            .max(stats.blur_kernel_table_bytes);
         blit_passes_sum = blit_passes_sum.saturating_add(stats.blit_passes as u64);
         texture_copies_sum = texture_copies_sum.saturating_add(stats.texture_copies as u64);
         texture_copy_bytes_sum =
            texture_copy_bytes_sum.saturating_add(stats.texture_copy_bytes);
         persistent_target_frames = persistent_target_frames
            .saturating_add(stats.persistent_target_valid as u64);
         draw_target_main_bytes_peak = draw_target_main_bytes_peak
            .max(stats.memory.draw_target_main_bytes);
      }
      else if persist_raw
      {
         warmup_frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         warmup_encode_samples.push(stats.encode_ms);
         warmup_gpu_samples.push(stats.gpu_ms);
      }
   }

   let summary = summarize(&frame_samples);
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
   metrics.insert(String::from("instances_avg"), instanced_sum / frames as f64);
   metrics.insert(
      String::from("commands_traversed_avg"),
      commands_traversed_sum / frames as f64,
   );
   metrics.insert(String::from("vertex_upload_bytes_avg"), vb_sum / frames as f64);
   metrics.insert(String::from("index_upload_bytes_avg"), ib_sum / frames as f64);
   metrics.insert(String::from("uniform_upload_bytes_avg"), ub_sum / frames as f64);
   metrics.insert(String::from("damage_pixels_avg"), damage_pixels_sum / frames as f64);
   metrics.insert(String::from("damage_rects_avg"), damage_rects_sum / frames as f64);
   metrics.insert(String::from("layer_cache_bytes_peak"), layer_bytes_peak as f64);
   metrics.insert(String::from("renderer_bytes_peak"), total_bytes_peak as f64);
   metrics.insert(String::from("effect_targets_bytes_peak"), effect_bytes_peak as f64);
   metrics.insert(String::from("effect_prepass_bytes_peak"), effect_prepass_bytes_peak as f64);
   metrics.insert(String::from("effect_blur_chain_bytes_peak"), effect_blur_chain_bytes_peak as f64);
   metrics.insert(String::from("bloom_targets_bytes_peak"), bloom_bytes_peak as f64);
   metrics.insert(String::from("resource_creates_total"), resource_creates_sum as f64);
   metrics.insert(String::from("resource_grows_total"), resource_grows_sum as f64);
   metrics.insert(String::from("frame_resource_depth"), config.frame_resource_depth as f64);
   metrics.insert(
      String::from("frame_ring_buffer_bytes_peak"),
      frame_ring_buffer_bytes_peak as f64,
   );
   metrics.insert(String::from("first_frame_ms"), first_frame_ms);
   metrics.insert(String::from("first_encode_ms"), first_encode_ms);
   metrics.insert(String::from("first_gpu_ms"), first_gpu_ms);
   metrics.insert(String::from("first_resource_creates"), first_resource_creates as f64);
   metrics.insert(String::from("frame_backpressure_skips"), skips_sum);
   metrics.insert(String::from("layer_body_commands_scanned_avg"), layer_body_commands_scanned_sum / frames as f64);
   metrics.insert(String::from("layer_body_commands_copied_avg"), layer_body_commands_copied_sum / frames as f64);
   metrics.insert(String::from("layer_texture_creates_avg"), layer_texture_creates_sum / frames as f64);
   metrics.insert(String::from("layer_cache_hits_avg"), layer_cache_hits_sum / frames as f64);
   metrics.insert(String::from("layer_cache_misses_avg"), layer_cache_misses_sum / frames as f64);
   metrics.insert(String::from("layer_offscreen_draws_avg"), layer_offscreen_draws_sum / frames as f64);
   metrics.insert(String::from("layer_inline_draws_avg"), layer_inline_draws_sum / frames as f64);
   metrics.insert(String::from("layer_double_render_prevented_avg"), layer_double_render_prevented_sum / frames as f64);
   metrics.insert(String::from("layer_cache_budget_bytes"), layer_budget_bytes as f64);
   metrics.insert(String::from("layer_cache_resident_bytes_peak"), layer_resident_bytes_peak as f64);
   metrics.insert(String::from("layer_cache_pool_bytes_peak"), layer_pool_bytes_peak as f64);
   metrics.insert(String::from("layer_cache_cpu_bytes_peak"), layer_cpu_bytes_peak as f64);
   metrics.insert(String::from("layer_cache_evictions"), layer_evictions_peak as f64);
   metrics.insert(String::from("layer_cache_recreations"), layer_recreations_peak as f64);
   metrics.insert(String::from("layer_cache_pool_reuses"), layer_pool_reuses_peak as f64);
   metrics.insert(String::from("layer_cache_purges"), layer_purges_peak as f64);
   metrics.insert(String::from("layer_cache_budget_violations"), layer_budget_violations as f64);
   metrics.insert(String::from("effect_graph_effects_avg"), effect_graph_effects_sum / frames as f64);
   metrics.insert(String::from("effect_graph_captures_avg"), effect_graph_captures_sum / frames as f64);
   metrics.insert(String::from("effect_graph_pyramids_avg"), effect_graph_pyramids_sum / frames as f64);
   metrics.insert(String::from("effect_graph_pyramid_reuses_avg"), effect_graph_pyramid_reuses_sum / frames as f64);
   metrics.insert(String::from("effect_graph_plan_reuses_avg"), effect_graph_plan_reuses_sum / frames as f64);
   metrics.insert(String::from("effect_graph_capture_passes_avg"), effect_graph_capture_passes_sum / frames as f64);
   metrics.insert(String::from("effect_graph_downsample_passes_avg"), effect_graph_downsample_passes_sum / frames as f64);
   metrics.insert(String::from("effect_graph_blur_horizontal_passes_avg"), effect_graph_blur_horizontal_passes_sum / frames as f64);
   metrics.insert(String::from("effect_graph_blur_vertical_passes_avg"), effect_graph_blur_vertical_passes_sum / frames as f64);
   metrics.insert(String::from("effect_graph_composite_passes_avg"), effect_graph_composite_passes_sum / frames as f64);
   metrics.insert(String::from("effect_graph_max_lifetime_commands_peak"), effect_graph_max_lifetime_commands_peak as f64);
   metrics.insert(String::from("effect_graph_resources_avg"), effect_graph_resources_sum / frames as f64);
   metrics.insert(String::from("effect_graph_alias_slots_avg"), effect_graph_alias_slots_sum / frames as f64);
   metrics.insert(String::from("effect_graph_logical_bytes_peak"), effect_graph_logical_bytes_peak as f64);
   metrics.insert(String::from("effect_graph_physical_bytes_peak"), effect_graph_physical_bytes_peak as f64);
   metrics.insert(String::from("effect_graph_aliased_bytes_peak"), effect_graph_aliased_bytes_peak as f64);
   metrics.insert(String::from("blur_kernel_paired_passes_avg"), blur_kernel_paired_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("blur_kernel_exact_passes_avg"), blur_kernel_exact_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("blur_kernel_source_samples_avg"), blur_kernel_source_samples_sum as f64 / frames as f64);
   metrics.insert(String::from("blur_kernel_encoded_samples_avg"), blur_kernel_encoded_samples_sum as f64 / frames as f64);
   metrics.insert(String::from("blur_kernel_runtime_exp_taps_avg"), blur_kernel_runtime_exp_taps_sum as f64 / frames as f64);
   metrics.insert(String::from("blur_kernel_table_bytes_peak"), blur_kernel_table_bytes_peak as f64);
   let sample_reduction_pct = if blur_kernel_source_samples_sum == 0
   {
      0.0
   }
   else
   {
      100.0 * (1.0 - blur_kernel_encoded_samples_sum as f64 / blur_kernel_source_samples_sum as f64)
   };
   metrics.insert(String::from("blur_kernel_sample_reduction_pct"), sample_reduction_pct);
   metrics.insert(String::from("blit_passes_avg"), blit_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("texture_copies_avg"), texture_copies_sum as f64 / frames as f64);
   metrics.insert(
      String::from("texture_copy_bytes_avg"),
      texture_copy_bytes_sum as f64 / frames as f64,
   );
   metrics.insert(
      String::from("persistent_target_frames"),
      persistent_target_frames as f64,
   );
   metrics.insert(
      String::from("draw_target_main_bytes_peak"),
      draw_target_main_bytes_peak as f64,
   );
   if persist_raw
   {
      insert_indexed_samples(&mut metrics, "raw_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "raw_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "raw_gpu_ms", &gpu_samples);
      insert_indexed_samples(&mut metrics, "warmup_frame_ms", &warmup_frame_samples);
      insert_indexed_samples(&mut metrics, "warmup_encode_ms", &warmup_encode_samples);
      insert_indexed_samples(&mut metrics, "warmup_gpu_ms", &warmup_gpu_samples);
   }

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![notes],
      metrics,
   })
}

fn insert_indexed_samples(metrics: &mut BTreeMap<String, f64>, prefix: &str, samples: &[f64])
{
   for (index, sample) in samples.iter().copied().enumerate()
   {
      metrics.insert(format!("{prefix}_{index:04}"), sample);
   }
}

fn layer_drawlist(name: &str, phase: usize) -> api::DrawList
{
   let mut builder = ui::DrawListBuilder::new();
   for layer in 0..100_u32
   {
      let dirty = name == "dirty_one" && layer == phase as u32 % 100;
      let id = if name == "navigation_churn" { layer.wrapping_add(phase as u32 * 100) } else { layer };
      builder.layer_begin(id, api::RectF::new(0.0, 0.0, 128.0, 128.0), dirty || name == "memory_warning");
      if name == "nested" && layer % 10 == 0
      {
         builder.layer_begin(id + 10_000, api::RectF::new(8.0, 8.0, 96.0, 96.0), dirty);
      }
      for draw in 0..100_u32
      {
         let x = (draw % 10) as f32 * 12.0;
         let y = (draw / 10) as f32 * 12.0;
         builder.rrect(api::RectF::new(x, y, 10.0, 10.0), [2.0; 4], api::Color::rgba(0.2, 0.5, 0.9, 0.9));
      }
      if name == "backdrop_dependency"
      {
         builder.backdrop(api::RectF::new(16.0, 16.0, 80.0, 80.0), 8.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.2);
      }
      if name == "nested" && layer % 10 == 0 { builder.layer_end(); }
      builder.layer_end();
   }
   builder.into_inner()
}

fn metal_layer_case(id: &str, smoke: bool, name: &str) -> Result<PerfCaseResult>
{
   let kind = String::from(name);
   let note_kind = kind.clone();
   let mut case = measured_metal_drawlist_case(
      id,
      smoke,
      format!("Metal layer-cache {note_kind} workload with 100 layers and 100 draws per layer."),
      move |frame| {
         let resize = if kind == "resize" { Some(if frame & 1 == 0 { (1_200, 800) } else { (1_024, 768) }) } else { None };
         let recreate = kind == "memory_warning" && frame > 0;
         (layer_drawlist(&kind, frame), None, resize, recreate)
      },
   )?;
   case.metrics.insert(String::from("layers"), 100.0);
   case.metrics.insert(String::from("draws_per_layer"), 100.0);
   case.metrics.insert(String::from("memory_warning_purges"), if name == "memory_warning" { case.samples as f64 } else { 0.0 });
   Ok(case)
}

fn effect_drawlist(name: &str) -> api::DrawList
{
   let mut builder = ui::DrawListBuilder::new();
   if name.starts_with("target_plan_")
   {
      builder.rrect(
         api::RectF::new(0.0, 0.0, 1_200.0, 800.0),
         [0.0; 4],
         api::Color::rgba(0.15, 0.25, 0.45, 1.0),
      );
      if name == "target_plan_prepass"
      {
         builder.backdrop(
            api::RectF::new(200.0, 160.0, 800.0, 480.0),
            0.0,
            api::Color::rgba(0.2, 0.2, 0.2, 0.3),
            1.0,
         );
      }
      else if name == "target_plan_quarter" || name == "target_plan_eighth"
      {
         builder.visual_effect(
            api::RectF::new(200.0, 160.0, 800.0, 480.0),
            api::VisualEffect::DarkPopup {
               blur_intensity: if name == "target_plan_quarter" { 0.5 } else { 1.0 },
               tint: api::Color::rgba(0.1, 0.1, 0.1, 0.8),
            },
         );
      }
   }
   else if name == "backdrop_separated_48"
   {
      for index in 0..48 { builder.backdrop(api::RectF::new((index % 8) as f32 * 100.0, (index / 8) as f32 * 90.0, 48.0, 42.0), 8.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.3); }
   }
   else if name == "backdrop_coalescible_12"
   {
      for index in 0..12 { builder.backdrop(api::RectF::new(index as f32 * 70.0, 40.0, 40.0, 40.0), 6.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.25); }
   }
   else if name == "blur_fullscreen"
   {
      builder.backdrop(api::RectF::new(0.0, 0.0, 1_200.0, 800.0), 32.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.2);
   }
   else if let Some((sigma, local)) = blur_sweep_spec(name)
   {
      let rect = if local
      {
         if sigma >= 8.0
         {
            api::RectF::new(200.0, 160.0, 800.0, 480.0)
         }
         else
         {
            api::RectF::new(420.0, 300.0, 360.0, 200.0)
         }
      }
      else
      {
         api::RectF::new(0.0, 0.0, 1_200.0, 800.0)
      };
      builder.backdrop(rect, sigma, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.2);
   }
   else if name == "blur_mixed_sigma"
   {
      for index in 0..16 { builder.backdrop(api::RectF::new(index as f32 * 30.0, 20.0, 120.0, 80.0), 2.0 + index as f32 * 3.0, api::Color::rgba(0.9, 0.95, 1.0, 1.0), 0.35); }
   }
   else if name == "blur_edges_corners"
   {
      for rect in [api::RectF::new(-30.0, -20.0, 160.0, 100.0), api::RectF::new(1_130.0, -20.0, 120.0, 100.0), api::RectF::new(-30.0, 740.0, 160.0, 100.0), api::RectF::new(1_130.0, 740.0, 120.0, 100.0)] { builder.backdrop(rect, 24.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.3); }
   }
   else
   {
      builder.layer_begin(1, api::RectF::new(0.0, 0.0, 900.0, 700.0), true);
      builder.layer_begin(2, api::RectF::new(40.0, 40.0, 700.0, 500.0), true);
      builder.backdrop(api::RectF::new(80.0, 80.0, 500.0, 300.0), 18.0, api::Color::rgba(1.0, 1.0, 1.0, 1.0), 0.4);
      builder.layer_end();
      builder.layer_end();
   }
   builder.into_inner()
}

fn metal_effect_case(id: &str, smoke: bool, name: &str) -> Result<PerfCaseResult>
{
   let kind = String::from(name);
   let note_kind = kind.clone();
   let cold_first_use = kind.starts_with("target_plan_")
      && std::env::var_os("OXIDE_ARCHITECTURE_EFFECT_COLD_FIRST_USE").is_some();
   let mut case = measured_metal_drawlist_case(
      id,
      smoke,
      format!("Metal effect workload for {note_kind} through the production effect/layer encoder."),
      move |frame| (effect_drawlist(&kind), None, None, cold_first_use && frame > 0),
   )?;
   case.metrics.insert(String::from("effect_regions"), effect_region_count(name) as f64);
   if let Some((sigma, local)) = blur_sweep_spec(name)
   {
      let pass_sigma = sigma / 4.0;
      case.metrics.insert(String::from("blur_source_sigma_dp"), sigma as f64);
      case.metrics.insert(String::from("blur_pass_sigma_px"), pass_sigma as f64);
      case.metrics.insert(String::from("blur_pass_radius_px"), (pass_sigma * 3.0).ceil() as f64);
      case.metrics.insert(String::from("blur_local_region"), local as u8 as f64);
   }
   if cold_first_use
   {
      case.cache_state = String::from("cold");
      case.notes.push(String::from(
         "Renderer recreation before every post-initial frame isolates cold target first use.",
      ));
   }
   Ok(case)
}

fn metal_final_target_case(id: &str, smoke: bool, name: &str) -> Result<PerfCaseResult>
{
   let partial_damage = name == "partial_damage";
   let mut case = measured_metal_drawlist_case(
      id,
      smoke,
      format!(
         "Metal final-target {name} workload through a real CAMetalLayer drawable with effect, layer, and camera auxiliary textures."
      ),
      move |_| {
         let mut builder = ui::DrawListBuilder::new();
         builder.rrect(
            api::RectF::new(0.0, 0.0, 1_200.0, 800.0),
            [0.0; 4],
            api::Color::rgba(0.08, 0.10, 0.14, 1.0),
         );
         builder.camera_bg(
            api::RectF::new(40.0, 40.0, 420.0, 300.0),
            api::Color::rgba(0.92, 0.96, 1.0, 1.0),
            0.8,
            false,
            true,
            12.0,
         );
         builder.layer_begin(51, api::RectF::new(500.0, 80.0, 560.0, 560.0), true);
         builder.rrect(
            api::RectF::new(520.0, 100.0, 520.0, 520.0),
            [28.0; 4],
            api::Color::rgba(0.18, 0.48, 0.92, 0.92),
         );
         builder.layer_end();
         builder.visual_effect(
            api::RectF::new(120.0, 420.0, 860.0, 260.0),
            api::VisualEffect::DarkPopup {
               blur_intensity: 0.75,
               tint: api::Color::rgba(0.08, 0.09, 0.12, 0.80),
            },
         );
         let damage = partial_damage.then(|| api::Damage {
            rects: vec![api::RectI::new(720, 500, 96, 72)],
         });
         (builder.into_inner(), damage, None, false)
      },
   )?;
   case.refresh_mode = String::from("drawable-unthrottled");
   Ok(case)
}

fn analytic_instance_drawlist(
   family: &str,
   count: usize,
   image: api::ImageHandle,
) -> api::DrawList
{
   let mut builder = ui::DrawListBuilder::new();
   for index in 0..count
   {
      let x = (index % 64) as f32 * 18.0;
      let y = ((index / 64) % 44) as f32 * 18.0;
      let rect = api::RectF::new(x, y, 16.0, 16.0);
      match family
      {
         "rrect" => builder.rrect(
            rect,
            [3.0; 4],
            api::Color::rgba(0.2, 0.55, 0.95, 0.9),
         ),
         "image" => builder.image(
            image,
            rect,
            api::RectF::new(0.0, 0.0, 4.0, 4.0),
            0.9,
         ),
         "nine_slice" => builder.nine_slice(
            image,
            rect,
            api::Insets::new(1.0, 1.0, 1.0, 1.0),
            0.9,
         ),
         "spinner" => builder.spinner([x + 8.0, y + 8.0], 16.0, 0.9),
         "backdrop" => builder.backdrop(
            rect,
            4.0,
            api::Color::rgba(0.8, 0.9, 1.0, 1.0),
            0.35,
         ),
         "visual_effect" => builder.visual_effect(
            rect,
            api::VisualEffect::DarkPopup {
               blur_intensity: 0.5,
               tint: api::Color::rgba(0.08, 0.10, 0.14, 0.8),
            },
         ),
         _ => unreachable!("unknown analytic instance family"),
      }
   }
   builder.into_inner()
}

fn metal_analytic_instance_case(
   id: &str,
   smoke: bool,
   family: &str,
   count: usize,
) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing Metal renderer")?;
   let pixels = [
      255_u8, 96, 48, 255, 48, 192, 255, 255,
      80, 255, 120, 255, 255, 220, 64, 255,
      160, 64, 255, 255, 48, 224, 208, 255,
      255, 128, 192, 255, 220, 240, 255, 255,
   ];
   let image = renderer.image_create_rgba8(4, 2, &pixels, 16);
   let draws = analytic_instance_drawlist(family, count, image);
   let warmups = std::env::var("OXIDE_ARCHITECTURE_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups > 0)
      .unwrap_or(if smoke { 1 } else { 8 });
   let frames = std::env::var("OXIDE_ARCHITECTURE_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 2 } else { 12 });
   let persist_raw = std::env::var_os("OXIDE_ARCHITECTURE_METAL_RAW_SAMPLES").is_some();
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut warmup_frame_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut warmup_encode_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut warmup_gpu_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut draws_sum = 0.0;
   let mut instances_sum = 0.0;
   let mut upload_bytes_sum = 0.0;
   let mut analytic_bytes_sum = 0.0;
   let mut analytic_binds_sum = 0.0;
   let mut analytic_ring_grows_sum = 0.0;
   let mut resource_grows_sum = 0.0;
   let mut frame_ring_bytes_peak = 0_u64;

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      renderer.encode_pass(&draws);
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      let frame_ms = frame_t0.elapsed().as_secs_f64() * 1_000.0;
      if frame >= warmups
      {
         frame_samples.push(frame_ms);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
         instances_sum += stats.instanced as f64;
         upload_bytes_sum += stats.buffer_upload_bytes as f64;
         analytic_bytes_sum += stats.analytic_instance_bytes as f64;
         analytic_binds_sum += stats.analytic_instance_buffer_binds as f64;
         analytic_ring_grows_sum += stats.analytic_instance_ring_grows as f64;
         resource_grows_sum += stats.resource_grows as f64;
         frame_ring_bytes_peak =
            frame_ring_bytes_peak.max(stats.memory.frame_ring_buffer_bytes);
      }
      else if persist_raw
      {
         warmup_frame_samples.push(frame_ms);
         warmup_encode_samples.push(stats.encode_ms);
         warmup_gpu_samples.push(stats.gpu_ms);
      }
   }

   renderer.image_release(image);
   let summary = summarize(&frame_samples);
   let (layer, scenario, variant, cache_state, refresh_mode) =
      perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("instance_count"), count as f64);
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
   metrics.insert(String::from("instances_avg"), instances_sum / frames as f64);
   metrics.insert(String::from("upload_bytes_avg"), upload_bytes_sum / frames as f64);
   metrics.insert(String::from("analytic_instance_bytes_avg"), analytic_bytes_sum / frames as f64);
   metrics.insert(String::from("analytic_instance_buffer_binds_avg"), analytic_binds_sum / frames as f64);
   metrics.insert(String::from("analytic_instance_ring_grows_avg"), analytic_ring_grows_sum / frames as f64);
   metrics.insert(String::from("resource_grows_avg"), resource_grows_sum / frames as f64);
   metrics.insert(String::from("frame_ring_buffer_bytes_peak"), frame_ring_bytes_peak as f64);
   if persist_raw
   {
      insert_indexed_samples(&mut metrics, "raw_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "raw_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "raw_gpu_ms", &gpu_samples);
      insert_indexed_samples(&mut metrics, "warmup_frame_ms", &warmup_frame_samples);
      insert_indexed_samples(&mut metrics, "warmup_encode_ms", &warmup_encode_samples);
      insert_indexed_samples(&mut metrics, "warmup_gpu_ms", &warmup_gpu_samples);
   }

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![format!(
         "Metal {family} analytic-instance frame-ring workload with {count} ordered instances.",
      )],
      metrics,
   })
}

fn damage_drawlist(name: &str, phase: usize) -> api::DrawList
{
   let mut builder = ui::DrawListBuilder::new();
   let count = if name == "caret_blink" { 1 } else { 10_000 };
   for index in 0..count
   {
      if name == "removed_node" && index == phase % count { continue; }
      let x = (index % 100) as f32 * 10.0;
      let y = (index / 100) as f32 * 7.0;
      let moved = if name == "moving_node" && index == phase % count { 4.0 } else { 0.0 };
      builder.rrect(api::RectF::new(x + moved, y, if name == "caret_blink" { 2.0 } else { 8.0 }, 6.0), [1.0; 4], api::Color::rgba(0.2, 0.6, 0.95, 1.0));
   }
   builder.into_inner()
}

fn metal_damage_case(id: &str, smoke: bool, name: &str) -> Result<PerfCaseResult>
{
   let kind = String::from(name);
   let note_kind = kind.clone();
   let mut case = measured_metal_drawlist_case(
      id,
      smoke,
      format!("Metal damage workload for {note_kind} over the production damage filtering and submission path."),
      move |frame| {
         let damage = damage_rect_for(&kind, frame as u64);
         (damage_drawlist(&kind, frame), Some(damage), None, false)
      },
   )?;
   let damage = damage_rect_for(name, 1);
   let pixels = damage.rects.iter().map(|rect| rect.w.max(0) as u64 * rect.h.max(0) as u64).sum::<u64>();
   case.metrics.insert(String::from("scene_items"), if name == "caret_blink" { 1.0 } else { 10_000.0 });
   case.metrics.insert(String::from("requested_damage_pixels"), pixels as f64);
   case.metrics.insert(String::from("submissions_per_sequence"), if name == "full_direct_then_partial" { 2.0 } else { 1.0 });
   Ok(case)
}

fn metal_image_case(id: &str, smoke: bool, name: &str, count: usize) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing Metal renderer")?;
   let mut handles = Vec::with_capacity(count);
   let image_view_grid = name.starts_with("image_view_cover_grid_");
   let grid_pixels = vec![128_u8; 29 * 7 * 4];
   for index in 0..count
   {
      if image_view_grid
      {
         handles.push(renderer.image_create_rgba8(29, 7, &grid_pixels, 29 * 4));
      }
      else
      {
         let pixel = [
            (index as u8).wrapping_mul(17), 96, 220, 255,
            32, (index as u8).wrapping_mul(29), 180, 255,
            210, 64, (index as u8).wrapping_mul(11), 255,
            245, 210, 80, 255,
         ];
         handles.push(renderer.image_create_rgba8(2, 2, &pixel, 8));
      }
   }
   let warmups = std::env::var("OXIDE_ARCHITECTURE_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups > 0)
      .unwrap_or(if smoke { 1 } else { 3 });
   let frames = std::env::var("OXIDE_ARCHITECTURE_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 2 } else { 10 });
   let persist_raw = std::env::var_os("OXIDE_ARCHITECTURE_METAL_RAW_SAMPLES").is_some();
   let mut warmup_frame_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut warmup_encode_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut warmup_gpu_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut draws_sum = 0.0;
   let mut instances_sum = 0.0;
   let mut commands_traversed_sum = 0.0;
   let mut upload_sum = 0.0;
   let mut image_bytes_peak = 0_u64;
   let mut total_bytes_peak = 0_u64;
   let mut image_argument_encodes_sum = 0.0;
   let mut image_argument_binds_sum = 0.0;
   let mut image_argument_tables_finalized_sum = 0.0;
   let mut image_argument_table_reuses_sum = 0.0;
   let mut image_argument_bytes_sum = 0.0;
   let mut image_argument_buffer_grows_sum = 0.0;
   let mut grid_builder = ui::DrawListBuilder::new();
   if image_view_grid
   {
      encode_image_view_grid(&handles, &mut grid_builder);
   }

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      if name == "release_reuse"
      {
         for index in 0..128.min(handles.len())
         {
            renderer.image_release(handles[index]);
            let value = (frame + index) as u8;
            let pixel = [[value, 255_u8.wrapping_sub(value), 160, 255]; 4];
            handles[index] = renderer.image_create_rgba8(2, 2, &pixel.concat(), 8);
         }
      }
      let fallback_draws = if image_view_grid
      {
         encode_image_view_grid(&handles, &mut grid_builder);
         None
      }
      else
      {
         let mut builder = ui::DrawListBuilder::new();
         for (index, handle) in handles.iter().copied().enumerate()
         {
            let x = (index % 100) as f32 * 6.0;
            let y = (index / 100) as f32 * 6.0;
            let dst = match name
            {
               "contain_3x" => api::RectF::new(x, y + 1.0, 6.0, 4.0),
               "zoom_3x" => api::RectF::new(x - 3.0, y - 3.0, 12.0, 12.0),
               "minification_mips" => api::RectF::new(x, y, 1.0, 1.0),
               _ => api::RectF::new(x, y, 5.0, 5.0),
            };
            let src = if name == "cover_3x" { api::RectF::new(0.5, 0.0, 1.0, 2.0) } else { api::RectF::new(0.0, 0.0, 2.0, 2.0) };
            builder.image(handle, dst, src, 1.0);
         }
         Some(builder.into_inner())
      };
      let draws = fallback_draws.as_ref().unwrap_or_else(|| grid_builder.drawlist());
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      renderer.encode_pass(draws);
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame >= warmups
      {
         frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
         instances_sum += stats.instanced as f64;
         commands_traversed_sum += stats.commands_traversed as f64;
         upload_sum += (stats.vb_bytes + stats.ib_bytes + stats.ub_bytes) as f64;
         image_bytes_peak = image_bytes_peak.max(stats.memory.image_cache_bytes);
         total_bytes_peak = total_bytes_peak.max(stats.memory.total_bytes);
         image_argument_encodes_sum += stats.image_argument_encodes as f64;
         image_argument_binds_sum += stats.image_argument_binds as f64;
         image_argument_tables_finalized_sum += stats.image_argument_tables_finalized as f64;
         image_argument_table_reuses_sum += stats.image_argument_table_reuses as f64;
         image_argument_bytes_sum += stats.image_argument_bytes as f64;
         image_argument_buffer_grows_sum += stats.image_argument_buffer_grows as f64;
      }
      else if persist_raw
      {
         warmup_frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         warmup_encode_samples.push(stats.encode_ms);
         warmup_gpu_samples.push(stats.gpu_ms);
      }
   }

   let grid_work = image_view_grid.then(|| image_view_grid_work(&image_view_grid_drawlist(&handles)));
   for handle in handles { renderer.image_release(handle); }
   let summary = summarize(&frame_samples);
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("unique_images"), count as f64);
   metrics.insert(String::from("image_draws"), count as f64);
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
   metrics.insert(String::from("instances_avg"), instances_sum / frames as f64);
   metrics.insert(String::from("commands_traversed_avg"), commands_traversed_sum / frames as f64);
   metrics.insert(String::from("upload_bytes_avg"), upload_sum / frames as f64);
   metrics.insert(String::from("image_cache_bytes_peak"), image_bytes_peak as f64);
   metrics.insert(String::from("renderer_bytes_peak"), total_bytes_peak as f64);
   metrics.insert(String::from("image_argument_encodes_avg"), image_argument_encodes_sum / frames as f64);
   metrics.insert(String::from("image_argument_binds_avg"), image_argument_binds_sum / frames as f64);
   metrics.insert(String::from("image_argument_tables_finalized_avg"), image_argument_tables_finalized_sum / frames as f64);
   metrics.insert(String::from("image_argument_table_reuses_avg"), image_argument_table_reuses_sum / frames as f64);
   metrics.insert(String::from("image_argument_bytes_avg"), image_argument_bytes_sum / frames as f64);
   metrics.insert(String::from("image_argument_buffer_grows_avg"), image_argument_buffer_grows_sum / frames as f64);
   metrics.insert(String::from("device_scale"), if name.ends_with("3x") { 3.0 } else { 1.0 });
   metrics.insert(String::from("decode_at_display_size"), if name == "decode_display_size" { 1.0 } else { 0.0 });
   metrics.insert(String::from("released_recreated_per_frame"), if name == "release_reuse" { 128.0 } else { 0.0 });
   metrics.insert(String::from("mip_policy_requested"), if name == "minification_mips" { 1.0 } else { 0.0 });
   if let Some(work) = grid_work
   {
      metrics.insert(String::from("image_draws"), work.images as f64);
      metrics.insert(String::from("nine_slice_draws"), work.nine_slices as f64);
      metrics.insert(String::from("source_crop_commands"), work.source_crops as f64);
      metrics.insert(String::from("quads"), count as f64);
      metrics.insert(String::from("instanced_draw_calls_avg"), work.draw_calls as f64);
      metrics.insert(String::from("inline_parameter_bytes_avg"), work.inline_parameter_bytes as f64);
      metrics.insert(
         String::from("total_parameter_bytes_avg"),
         work.inline_parameter_bytes as f64 + image_argument_bytes_sum / frames as f64,
      );
      metrics.insert(String::from("logical_shaded_pixels"), work.logical_shaded_pixels);
   }
   if persist_raw
   {
      insert_indexed_samples(&mut metrics, "raw_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "raw_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "raw_gpu_ms", &gpu_samples);
      insert_indexed_samples(&mut metrics, "warmup_frame_ms", &warmup_frame_samples);
      insert_indexed_samples(&mut metrics, "warmup_encode_ms", &warmup_encode_samples);
      insert_indexed_samples(&mut metrics, "warmup_gpu_ms", &warmup_gpu_samples);
   }

   let family = if image_view_grid { "authoring" } else { "architecture" };
   let source_dimensions = if image_view_grid { "29x7" } else { "2x2" };
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, family);

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from(family),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![format!("Metal {name} image-residency workload with {count} unique {source_dimensions} source resources and production image draws.")],
      metrics,
   })
}

#[derive(Clone, Copy)]
enum ImmutableImageCasePolicy
{
   Auto,
   Shared,
   Private,
   SharedMipmapped,
   PrivateMipmapped,
}

fn immutable_image_case_policy(name: &str) -> ImmutableImageCasePolicy
{
   if name.ends_with("_shared_mipmapped")
   {
      ImmutableImageCasePolicy::SharedMipmapped
   }
   else if name.ends_with("_shared")
   {
      ImmutableImageCasePolicy::Shared
   }
   else if name.ends_with("_private") || name.ends_with("_private_nomip")
   {
      ImmutableImageCasePolicy::Private
   }
   else if name.ends_with("_mipmapped")
   {
      ImmutableImageCasePolicy::PrivateMipmapped
   }
   else
   {
      ImmutableImageCasePolicy::Auto
   }
}

fn create_immutable_case_image(renderer: &mut metal::MetalRenderer, size: u32, pixels: &[u8], policy: ImmutableImageCasePolicy, repeatedly_minified: bool) -> api::ImageHandle
{
   match policy
   {
      ImmutableImageCasePolicy::Auto => renderer.image_create_rgba8_immutable(
         size,
         size,
         pixels,
         size as usize * 4,
         repeatedly_minified,
      ),
      ImmutableImageCasePolicy::Shared => renderer.image_create_rgba8_immutable_for_benchmark(
         size,
         size,
         pixels,
         size as usize * 4,
         false,
         false,
      ),
      ImmutableImageCasePolicy::Private => renderer.image_create_rgba8_immutable_for_benchmark(
         size,
         size,
         pixels,
         size as usize * 4,
         true,
         false,
      ),
      ImmutableImageCasePolicy::SharedMipmapped => renderer.image_create_rgba8_immutable_for_benchmark(
         size,
         size,
         pixels,
         size as usize * 4,
         false,
         true,
      ),
      ImmutableImageCasePolicy::PrivateMipmapped => renderer.image_create_rgba8_immutable_for_benchmark(
         size,
         size,
         pixels,
         size as usize * 4,
         true,
         true,
      ),
   }
}

fn immutable_image_case_pixels(size: u32, checker: bool) -> Vec<u8>
{
   let mut pixels = Vec::with_capacity(size as usize * size as usize * 4);
   for y in 0..size
   {
      for x in 0..size
      {
         if checker
         {
            let value = if (x + y) & 1 == 0 { 0 } else { 255 };
            pixels.extend_from_slice(&[value, value, value, 255]);
         }
         else
         {
            pixels.extend_from_slice(&[
               (x.wrapping_mul(17) ^ y.wrapping_mul(3)) as u8,
               (x.wrapping_mul(5) ^ y.wrapping_mul(29)) as u8,
               (x.wrapping_mul(11) ^ y.wrapping_mul(7)) as u8,
               255,
            ]);
         }
      }
   }
   pixels
}

fn image_spatial_variance(pixels: &[u8]) -> f64
{
   let count = pixels.len() / 4;
   if count == 0
   {
      return 0.0;
   }
   let mean = pixels.chunks_exact(4).map(|pixel| f64::from(pixel[0])).sum::<f64>()
      / count as f64;
   pixels
      .chunks_exact(4)
      .map(|pixel| {
         let distance = f64::from(pixel[0]) - mean;
         distance * distance
      })
      .sum::<f64>()
      / count as f64
}

fn immutable_image_drawlist(image: api::ImageHandle, source_size: u32, minified: bool, authoring: bool) -> api::DrawList
{
   let mut builder = ui::DrawListBuilder::new();
   if minified
   {
      let image_view = ui::elements::ImageView {
         image,
         natural_w: source_size,
         natural_h: source_size,
         fit: ui::elements::ImageFit::Cover,
         alpha: 1.0,
      };
      for index in 0..1_089_usize
      {
         let x = (index % 33) as f32 * 31.0;
         let y = (index / 33) as f32 * 31.0;
         let rect = api::RectF::new(x, y, 31.0, 31.0);
         if authoring
         {
            image_view.encode(rect, None, &mut builder);
         }
         else
         {
            builder.image(
               image,
               rect,
               api::RectF::new(0.0, 0.0, source_size as f32, source_size as f32),
               1.0,
            );
         }
      }
   }
   else
   {
      builder.image(
         image,
         api::RectF::new(0.0, 0.0, source_size as f32, source_size as f32),
         api::RectF::new(0.0, 0.0, source_size as f32, source_size as f32),
         1.0,
      );
   }
   builder.into_inner()
}

fn metal_immutable_image_case(id: &str, smoke: bool, name: &str) -> Result<PerfCaseResult>
{
   let policy = immutable_image_case_policy(name);
   if name.contains("small_one_use")
   {
      return metal_immutable_one_use_image_case(id, smoke, policy);
   }

   let minified = name.contains("minified");
   let authoring = id.contains(".authoring.");
   let size = 1_024_u32;
   let pixels = immutable_image_case_pixels(size, minified);
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   let target_size = if minified { 1_023 } else { 1_024 };
   renderer.resize(target_size, target_size, 1.0).context("resizing Metal renderer")?;
   let creation_start = Instant::now();
   let image = create_immutable_case_image(&mut renderer, size, &pixels, policy, minified);
   let creation_ms = creation_start.elapsed().as_secs_f64() * 1_000.0;
   let residency = renderer.image_residency_stats();
   let draws = immutable_image_drawlist(image, size, minified, authoring);

   let first_visible_start = Instant::now();
   let first = renderer.begin_frame(&api::FrameTarget, None);
   let first_frame_id = first.0;
   renderer.encode_pass(&draws);
   renderer.submit(first).with_context(|| format!("submitting first-visible {id}"))?;
   let (_, _, first_pixels) = renderer
      .readback_bgra8()
      .with_context(|| format!("reading first-visible {id}"))?;
   let first_visible_ms = first_visible_start.elapsed().as_secs_f64() * 1_000.0;
   let first_stats = last_metal_stats_after_submit(&renderer, first_frame_id);

   let warmups = std::env::var("OXIDE_C59_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups > 0)
      .unwrap_or(if smoke { 1 } else { 8 });
   let frames = std::env::var("OXIDE_C59_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 2 } else { 30 });
   let raw_samples = std::env::var_os("OXIDE_C59_RAW_SAMPLES").is_some()
      || std::env::var_os("OXIDE_ARCHITECTURE_METAL_RAW_SAMPLES").is_some();
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut warmup_frame_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut warmup_encode_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut warmup_gpu_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut draws_sum = 0.0;
   let mut instances_sum = 0.0;
   let mut image_bytes_peak = first_stats.memory.image_cache_bytes;
   let mut total_bytes_peak = first_stats.memory.total_bytes;

   for frame in 0..(warmups + frames)
   {
      let frame_start = Instant::now();
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      renderer.encode_pass(&draws);
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let frame_ms = frame_start.elapsed().as_secs_f64() * 1_000.0;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame >= warmups
      {
         frame_samples.push(frame_ms);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
         instances_sum += stats.instanced as f64;
         image_bytes_peak = image_bytes_peak.max(stats.memory.image_cache_bytes);
         total_bytes_peak = total_bytes_peak.max(stats.memory.total_bytes);
      }
      else if raw_samples
      {
         warmup_frame_samples.push(frame_ms);
         warmup_encode_samples.push(stats.encode_ms);
         warmup_gpu_samples.push(stats.gpu_ms);
      }
   }

   renderer.image_release(image);
   let summary = summarize(&frame_samples);
   let family = if id.contains(".authoring.") { "authoring" } else { "architecture" };
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, family);
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_distribution_metrics(&mut metrics, "creation_cpu_ms", &[creation_ms]);
   insert_distribution_metrics(&mut metrics, "first_visible_ms", &[first_visible_ms]);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("creation_cpu_ms"), creation_ms);
   metrics.insert(String::from("first_visible_ms"), first_visible_ms);
   metrics.insert(String::from("first_visible_gpu_ms"), first_stats.gpu_ms);
   metrics.insert(String::from("source_width"), size as f64);
   metrics.insert(String::from("source_height"), size as f64);
   metrics.insert(String::from("source_bytes"), pixels.len() as f64);
   metrics.insert(String::from("image_draws"), if minified { 1_089.0 } else { 1.0 });
   metrics.insert(String::from("image_view_encodes"), if authoring { 1_089.0 } else { 0.0 });
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
   metrics.insert(String::from("instances_avg"), instances_sum / frames as f64);
   metrics.insert(String::from("image_cache_bytes_peak"), image_bytes_peak as f64);
   metrics.insert(String::from("renderer_bytes_peak"), total_bytes_peak as f64);
   metrics.insert(String::from("shared_textures"), residency.shared_textures as f64);
   metrics.insert(String::from("private_textures"), residency.private_textures as f64);
   metrics.insert(String::from("mipmapped_textures"), residency.mipmapped_textures as f64);
   metrics.insert(String::from("mip_levels"), residency.mip_levels as f64);
   metrics.insert(String::from("shared_bytes"), residency.shared_bytes as f64);
   metrics.insert(String::from("private_bytes"), residency.private_bytes as f64);
   metrics.insert(String::from("staging_upload_bytes"), residency.staging_upload_bytes as f64);
   metrics.insert(
      String::from("creation_peak_texture_bytes"),
      residency.shared_bytes.saturating_add(residency.private_bytes)
         .saturating_add(residency.staging_upload_bytes) as f64,
   );
   metrics.insert(String::from("private_uploads"), residency.private_uploads as f64);
   metrics.insert(String::from("mipmap_generations"), residency.mipmap_generations as f64);
   metrics.insert(String::from("upload_command_buffers"), residency.upload_command_buffers as f64);
   metrics.insert(String::from("first_visible_spatial_variance"), image_spatial_variance(&first_pixels));
   if raw_samples
   {
      insert_indexed_samples(&mut metrics, "c59_warmup_frame_ms", &warmup_frame_samples);
      insert_indexed_samples(&mut metrics, "c59_warmup_encode_ms", &warmup_encode_samples);
      insert_indexed_samples(&mut metrics, "c59_warmup_gpu_ms", &warmup_gpu_samples);
      insert_indexed_samples(&mut metrics, "c59_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "c59_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "c59_gpu_ms", &gpu_samples);
   }

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from(family),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![format!(
         "Metal {name} immutable-image workload with explicit upload/startup, steady sampling, residency, mip, memory, and output-variance evidence.",
      )],
      metrics,
   })
}

fn metal_immutable_one_use_image_case(id: &str, smoke: bool, policy: ImmutableImageCasePolicy) -> Result<PerfCaseResult>
{
   let size = 64_u32;
   let pixels = immutable_image_case_pixels(size, false);
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(size, size, 1.0).context("resizing Metal renderer")?;
   let warmups = std::env::var("OXIDE_C59_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups > 0)
      .unwrap_or(if smoke { 1 } else { 4 });
   let frames = std::env::var("OXIDE_C59_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 2 } else { 30 });
   let raw_samples = std::env::var_os("OXIDE_C59_RAW_SAMPLES").is_some()
      || std::env::var_os("OXIDE_ARCHITECTURE_METAL_RAW_SAMPLES").is_some();
   let mut first_visible_samples = Vec::with_capacity(frames);
   let mut creation_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut output_checksum = 0_u64;
   let mut sampled_resident_bytes_peak = 0_u64;

   for frame in 0..(warmups + frames)
   {
      let first_visible_start = Instant::now();
      let creation_start = Instant::now();
      let image = create_immutable_case_image(&mut renderer, size, &pixels, policy, false);
      let creation_ms = creation_start.elapsed().as_secs_f64() * 1_000.0;
      let live_residency = renderer.image_residency_stats();
      sampled_resident_bytes_peak = sampled_resident_bytes_peak.max(
         live_residency.shared_bytes.saturating_add(live_residency.private_bytes),
      );
      let draws = immutable_image_drawlist(image, size, false, false);
      let token = renderer.begin_frame(&api::FrameTarget, None);
      renderer.encode_pass(&draws);
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let (_, _, output) = renderer
         .readback_bgra8()
         .with_context(|| format!("reading one-use {id}"))?;
      let first_visible_ms = first_visible_start.elapsed().as_secs_f64() * 1_000.0;
      let stats = renderer.last_stats();
      renderer.image_release(image);
      if frame >= warmups
      {
         first_visible_samples.push(first_visible_ms);
         creation_samples.push(creation_ms);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         output_checksum = output_checksum.wrapping_add(
            output.iter().copied().map(u64::from).sum::<u64>(),
         );
      }
   }

   let residency = renderer.image_residency_stats();
   let summary = summarize(&first_visible_samples);
   let (layer, scenario, variant, cache_state, refresh_mode) =
      perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "first_visible_ms", &first_visible_samples);
   insert_distribution_metrics(&mut metrics, "creation_cpu_ms", &creation_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   metrics.insert(String::from("source_width"), size as f64);
   metrics.insert(String::from("source_height"), size as f64);
   metrics.insert(String::from("source_bytes"), pixels.len() as f64);
   metrics.insert(String::from("output_checksum"), output_checksum as f64);
   metrics.insert(String::from("resident_shared_textures_after_release"), residency.shared_textures as f64);
   metrics.insert(String::from("resident_private_textures_after_release"), residency.private_textures as f64);
   metrics.insert(String::from("private_uploads"), residency.private_uploads as f64);
   metrics.insert(String::from("mipmap_generations"), residency.mipmap_generations as f64);
   metrics.insert(String::from("staging_upload_bytes"), residency.staging_upload_bytes as f64);
   metrics.insert(String::from("sampled_resident_bytes_peak"), sampled_resident_bytes_peak as f64);
   let staging_bytes_per_create = residency.staging_upload_bytes
      .checked_div(residency.private_uploads)
      .unwrap_or(0);
   metrics.insert(String::from("staging_upload_bytes_per_create"), staging_bytes_per_create as f64);
   metrics.insert(
      String::from("creation_peak_texture_bytes"),
      sampled_resident_bytes_peak.saturating_add(staging_bytes_per_create) as f64,
   );
   metrics.insert(String::from("upload_command_buffers"), residency.upload_command_buffers as f64);
   if raw_samples
   {
      insert_indexed_samples(&mut metrics, "c59_first_visible_ms", &first_visible_samples);
      insert_indexed_samples(&mut metrics, "c59_creation_cpu_ms", &creation_samples);
      insert_indexed_samples(&mut metrics, "c59_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "c59_gpu_ms", &gpu_samples);
   }

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/first-visible"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: first_visible_samples.len(),
      ops_per_sample: 1,
      notes: vec![String::from(
         "Cold small one-use immutable image creation, first render, explicit readback completion, and release guardrail.",
      )],
      metrics,
   })
}

fn metal_neon_marker_case(id: &str, smoke: bool, count: usize) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(512, 512, 1.0).context("resizing Metal renderer")?;
   let markers = (0..count)
      .map(|index| metal::neon_marker::NeonMarker {
         center: [8.0 + (index % 32) as f32 * 15.0, 8.0 + (index / 32) as f32 * 15.0],
         core_radius_px: 2.5,
         ring_radius_px: 4.0,
         ring_width_px: 1.0,
         halo_radius_px: 6.0,
         halo_sigma_px: 3.0,
         core_color: api::Color::rgba(1.0, 0.8, 0.2, 1.0),
         ring_color: api::Color::rgba(0.2, 0.8, 1.0, 1.0),
         halo_alpha_max: 0.5,
         ring_alpha_max: 0.8,
      })
      .collect::<Vec<_>>();
   let warmups = if smoke { 1 } else { 3 };
   let frames = if smoke { 2 } else { 10 };
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut draws_sum = 0.0;
   let mut instances_sum = 0.0;
   let mut upload_bytes_sum = 0.0;
   let mut resource_grows_sum = 0.0;

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      for markers in markers.chunks(metal::neon_marker::NEON_MARKER_MAX_INSTANCES)
      {
         renderer
            .encode_neon_markers(&metal::neon_marker::NeonMarkerPass {
               viewport: api::RectF::new(0.0, 0.0, 512.0, 512.0),
               markers,
            })
            .with_context(|| format!("encoding {id}"))?;
      }
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame >= warmups
      {
         frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
         instances_sum += stats.instanced as f64;
         upload_bytes_sum += stats.ub_bytes as f64;
         resource_grows_sum += stats.resource_grows as f64;
      }
   }

   let summary = summarize(&frame_samples);
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("marker_count"), count as f64);
   metrics.insert(String::from("marker_batches"), count.div_ceil(metal::neon_marker::NEON_MARKER_MAX_INSTANCES) as f64);
   metrics.insert(String::from("marker_instance_stride_bytes"), 72.0);
   metrics.insert(String::from("expected_upload_bytes"), (count * 72) as f64);
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
   metrics.insert(String::from("instances_avg"), instances_sum / frames as f64);
   metrics.insert(String::from("uniform_upload_bytes_avg"), upload_bytes_sum / frames as f64);
   metrics.insert(String::from("resource_grows_avg"), resource_grows_sum / frames as f64);

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![format!("Metal neon-marker frame-ring workload with {count} total markers in 128-marker batches.")],
      metrics,
   })
}

fn id_mask_chunks(vertex_count: usize, chunk_count: usize) -> Vec<metal::id_mask_compositor::IdMaskRasterChunk>
{
   let triangle_count = vertex_count / 3;
   let chunk_count = chunk_count.min(triangle_count);
   let mut chunks = Vec::with_capacity(chunk_count);
   let mut first_triangle = 0;
   for index in 0..chunk_count
   {
      let remaining_triangles = triangle_count - first_triangle;
      let remaining_chunks = chunk_count - index;
      let triangles = remaining_triangles / remaining_chunks;
      chunks.push(metal::id_mask_compositor::IdMaskRasterChunk {
         content_hash: (index as u64 + 1).wrapping_mul(0x9e37_79b9),
         first_vertex: first_triangle * 3,
         vertex_count: triangles * 3,
      });
      first_triangle += triangles;
   }
   chunks
}

fn id_mask_matrix_case(id: &str, smoke: bool, change: &str, size: usize, chunk_count: usize) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(size as u32, size as u32, 1.0).context("resizing Metal renderer")?;
   let mut vertices = id_mask_perf_vertices(if smoke { 16 } else { 32 }, size as f32);
   if change == "projection"
   {
      for vertex in &mut vertices
      {
         vertex.position_world = [vertex.position_px[0] * 2.0 / size as f32, vertex.position_px[1] * 2.0 / size as f32, 0.0, 1.0];
         vertex.position_px = [0.0, 0.0];
      }
   }
   let mut chunks = id_mask_chunks(vertices.len(), chunk_count);
   let mut alternate_chunks = chunks.clone();
   for chunk in &mut alternate_chunks
   {
      chunk.content_hash ^= 0xa5a5_5a5a_d3c4_b2e1;
   }
   let warmups = std::env::var("OXIDE_C32_METAL_WARMUPS")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|value| *value > 0)
      .unwrap_or(if change == "multiple_map" { 4 } else if smoke { 1 } else { 3 });
   let frames = std::env::var("OXIDE_C32_METAL_FRAMES")
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|value| *value > 0)
      .unwrap_or(if smoke { 3 } else { 12 });
   let persist_raw = std::env::var_os("OXIDE_C32_METAL_RAW_SAMPLES").is_some();
   let mut warmup_frame_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut warmup_encode_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut warmup_gpu_samples = Vec::with_capacity(if persist_raw { warmups } else { 0 });
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut draws_sum = 0.0;
   let mut skips_sum = 0.0;
   let mut upload_sum = 0.0;
   let mut renderer_bytes_peak = 0_u64;
   let mut id_mask_target_bytes_peak = 0_u64;
   let mut id_mask_vertex_bytes_peak = 0_u64;
   let mut commands_traversed_sum = 0_u64;
   let mut chunks_reused_sum = 0_u64;
   let mut chunks_rebuilt_sum = 0_u64;
   let mut chunks_prepared_sum = 0_u64;
   let mut render_passes_sum = 0_u64;
   let mut id_mask_cache_hits_sum = 0_u64;
   let mut id_mask_cache_misses_sum = 0_u64;
   let mut id_mask_raster_passes_sum = 0_u64;
   let mut id_mask_field_seed_passes_sum = 0_u64;
   let mut id_mask_field_jump_passes_sum = 0_u64;
   let mut id_mask_compositor_passes_sum = 0_u64;
   let mut id_mask_cache_budget_bytes = 0_u64;
   let mut id_mask_cache_resident_bytes_peak = 0_u64;
   let mut id_mask_cache_entries_peak = 0_u32;
   let mut id_mask_cache_evictions_peak = 0_u64;
   let mut id_mask_target_creates_sum = 0_u64;
   let mut id_mask_in_flight_generations_peak = 0_u32;
   let mut id_mask_in_flight_target_bytes_peak = 0_u64;
   let mut id_mask_target_storage_bytes_peak = 0_u64;
   let mut id_mask_generation_peak_bytes = 0_u64;
   let mut id_mask_target_reuse_blocked_peak = 0_u64;
   let mut resource_creates_sum = 0_u64;

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      if change == "content"
      {
         vertices[0].position_px[0] = (frame % size) as f32;
         chunks[0].content_hash = 0x5f37_2b19_u64.wrapping_mul(frame as u64 + 1);
      }
      let revision = match change
      {
         "content" => frame as u64 + 1,
         _ => 1,
      };
      let mut pass = id_mask_perf_pass(&vertices, &chunks, revision);
      pass.raster.viewport = api::RectF::new(0.0, 0.0, size as f32, size as f32);
      pass.raster.mask_width = size;
      pass.raster.mask_height = size;
      pass.raster.mask_scale = 1.0;
      if change == "projection"
      {
         pass.raster.projection = metal::id_mask_compositor::IdMaskRasterProjection::world_3d(authoring_scene3d_identity());
      }
      match change
      {
         "style" => pass.city_styles[0].fill_rgb[0] = 0.45 + (frame & 1) as f32 * 0.35,
         "viewport" => pass.raster.viewport.x = (frame & 1) as f32,
         "projection" => pass.raster.projection.world_to_clip[3][0] = frame as f32 * 0.002,
         "multiple_map" => pass.raster.viewport.w = size as f32 * 0.5,
         _ => {},
      }
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      renderer.encode_id_mask_gpu_compositor(&pass).with_context(|| format!("encoding {id}"))?;
      if change == "multiple_map"
      {
         let mut second = pass;
         second.raster.viewport.x = size as f32 * 0.5;
         second.raster.vertex_revision = 2;
         second.raster.chunks = &alternate_chunks;
         renderer.encode_id_mask_gpu_compositor(&second).with_context(|| format!("encoding second map for {id}"))?;
      }
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame >= warmups
      {
         frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
         skips_sum += stats.frame_backpressure_skipped as f64;
         upload_sum += (stats.vb_bytes + stats.ib_bytes + stats.ub_bytes) as f64;
         renderer_bytes_peak = renderer_bytes_peak.max(stats.memory.total_bytes);
         id_mask_target_bytes_peak =
            id_mask_target_bytes_peak.max(stats.memory.id_mask_target_bytes);
         id_mask_vertex_bytes_peak =
            id_mask_vertex_bytes_peak.max(stats.memory.id_mask_vertex_buffer_bytes);
         commands_traversed_sum =
            commands_traversed_sum.saturating_add(stats.commands_traversed);
         chunks_reused_sum = chunks_reused_sum.saturating_add(stats.chunks_reused);
         chunks_rebuilt_sum = chunks_rebuilt_sum.saturating_add(stats.chunks_rebuilt);
         chunks_prepared_sum = chunks_prepared_sum.saturating_add(stats.chunks_prepared);
         render_passes_sum = render_passes_sum.saturating_add(u64::from(stats.render_passes));
         id_mask_cache_hits_sum = id_mask_cache_hits_sum
            .saturating_add(u64::from(stats.id_mask_cache_hits));
         id_mask_cache_misses_sum = id_mask_cache_misses_sum
            .saturating_add(u64::from(stats.id_mask_cache_misses));
         id_mask_raster_passes_sum = id_mask_raster_passes_sum
            .saturating_add(u64::from(stats.id_mask_raster_passes));
         id_mask_field_seed_passes_sum = id_mask_field_seed_passes_sum
            .saturating_add(u64::from(stats.id_mask_field_seed_passes));
         id_mask_field_jump_passes_sum = id_mask_field_jump_passes_sum
            .saturating_add(u64::from(stats.id_mask_field_jump_passes));
         id_mask_compositor_passes_sum = id_mask_compositor_passes_sum
            .saturating_add(u64::from(stats.id_mask_compositor_passes));
         id_mask_cache_budget_bytes = stats.id_mask_cache_budget_bytes;
         id_mask_cache_resident_bytes_peak = id_mask_cache_resident_bytes_peak
            .max(stats.id_mask_cache_resident_bytes);
         id_mask_cache_entries_peak = id_mask_cache_entries_peak
            .max(stats.id_mask_cache_entries);
         id_mask_cache_evictions_peak = id_mask_cache_evictions_peak
            .max(stats.id_mask_cache_evictions);
         id_mask_target_creates_sum = id_mask_target_creates_sum
            .saturating_add(u64::from(stats.id_mask_target_creates));
         id_mask_in_flight_generations_peak = id_mask_in_flight_generations_peak
            .max(stats.id_mask_in_flight_generations);
         id_mask_in_flight_target_bytes_peak = id_mask_in_flight_target_bytes_peak
            .max(stats.id_mask_in_flight_target_bytes);
         id_mask_target_storage_bytes_peak = id_mask_target_storage_bytes_peak
            .max(stats.id_mask_target_storage_bytes);
         id_mask_generation_peak_bytes = id_mask_generation_peak_bytes
            .max(stats.id_mask_target_peak_bytes);
         id_mask_target_reuse_blocked_peak = id_mask_target_reuse_blocked_peak
            .max(stats.id_mask_target_reuse_blocked);
         resource_creates_sum = resource_creates_sum
            .saturating_add(u64::from(stats.resource_creates));
      }
      else if persist_raw
      {
         warmup_frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         warmup_encode_samples.push(stats.encode_ms);
         warmup_gpu_samples.push(stats.gpu_ms);
      }
   }

   let summary = summarize(&frame_samples);
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("resolution_px"), size as f64);
   metrics.insert(String::from("chunk_count"), chunk_count as f64);
   metrics.insert(String::from("vertex_count"), vertices.len() as f64);
   metrics.insert(String::from("vertex_bytes"), (vertices.len() * core::mem::size_of::<metal::id_mask_compositor::IdMaskRasterVertex>()) as f64);
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
   metrics.insert(String::from("upload_bytes_avg"), upload_sum / frames as f64);
   metrics.insert(String::from("renderer_bytes_peak"), renderer_bytes_peak as f64);
   metrics.insert(String::from("id_mask_target_bytes_peak"), id_mask_target_bytes_peak as f64);
   metrics.insert(String::from("id_mask_vertex_bytes_peak"), id_mask_vertex_bytes_peak as f64);
   metrics.insert(String::from("commands_traversed_avg"), commands_traversed_sum as f64 / frames as f64);
   metrics.insert(String::from("chunks_reused_avg"), chunks_reused_sum as f64 / frames as f64);
   metrics.insert(String::from("chunks_rebuilt_avg"), chunks_rebuilt_sum as f64 / frames as f64);
   metrics.insert(String::from("chunks_prepared_avg"), chunks_prepared_sum as f64 / frames as f64);
   metrics.insert(String::from("render_passes_avg"), render_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("id_mask_cache_hits_avg"), id_mask_cache_hits_sum as f64 / frames as f64);
   metrics.insert(String::from("id_mask_cache_misses_avg"), id_mask_cache_misses_sum as f64 / frames as f64);
   metrics.insert(String::from("id_mask_raster_passes_avg"), id_mask_raster_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("id_mask_field_seed_passes_avg"), id_mask_field_seed_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("id_mask_field_jump_passes_avg"), id_mask_field_jump_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("id_mask_compositor_passes_avg"), id_mask_compositor_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("id_mask_cache_budget_bytes"), id_mask_cache_budget_bytes as f64);
   metrics.insert(String::from("id_mask_cache_resident_bytes_peak"), id_mask_cache_resident_bytes_peak as f64);
   metrics.insert(String::from("id_mask_cache_entries_peak"), f64::from(id_mask_cache_entries_peak));
   metrics.insert(String::from("id_mask_cache_evictions"), id_mask_cache_evictions_peak as f64);
   metrics.insert(String::from("id_mask_target_creates_avg"), id_mask_target_creates_sum as f64 / frames as f64);
   metrics.insert(String::from("id_mask_in_flight_generations_peak"), f64::from(id_mask_in_flight_generations_peak));
   metrics.insert(String::from("id_mask_in_flight_target_bytes_peak"), id_mask_in_flight_target_bytes_peak as f64);
   metrics.insert(String::from("id_mask_target_storage_bytes_peak"), id_mask_target_storage_bytes_peak as f64);
   metrics.insert(String::from("id_mask_generation_peak_bytes"), id_mask_generation_peak_bytes as f64);
   metrics.insert(String::from("id_mask_target_reuse_blocked"), id_mask_target_reuse_blocked_peak as f64);
   metrics.insert(String::from("resource_creates_total"), resource_creates_sum as f64);
   metrics.insert(String::from("frame_backpressure_skips"), skips_sum);
   metrics.insert(String::from("geometry_changes_per_frame"), if matches!(change, "content" | "projection") { 1.0 } else { 0.0 });
   metrics.insert(String::from("style_changes_per_frame"), if change == "style" { 1.0 } else { 0.0 });
   metrics.insert(String::from("viewport_changes_per_frame"), if change == "viewport" { 1.0 } else { 0.0 });
   metrics.insert(String::from("maps_per_sequence"), if change == "multiple_map" { 2.0 } else { 1.0 });
   if persist_raw
   {
      insert_indexed_samples(&mut metrics, "raw_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "raw_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "raw_gpu_ms", &gpu_samples);
      insert_indexed_samples(&mut metrics, "warmup_frame_ms", &warmup_frame_samples);
      insert_indexed_samples(&mut metrics, "warmup_encode_ms", &warmup_encode_samples);
      insert_indexed_samples(&mut metrics, "warmup_gpu_ms", &warmup_gpu_samples);
   }

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![format!("Metal ID-mask {change} invalidation with {chunk_count} raster chunks at {size}x{size}.")],
      metrics,
   })
}

fn scene3d_transform(index: usize) -> metal::scene3d::Mat4
{
   let x = ((index % 32) as f32 / 16.0) - 0.97;
   let y = (((index / 32) % 32) as f32 / 16.0) - 0.97;
   let z = ((index / 1_024) as f32 * 0.001).min(0.8);
   [[0.025, 0.0, 0.0, 0.0], [0.0, 0.025, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, y, z, 1.0]]
}

fn scene3d_create_release_endurance_case(id: &str, smoke: bool) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(16, 16, 1.0).context("resizing Metal renderer")?;
   let vertices = [
      metal::scene3d::Vertex3d { position: [-0.8, -0.7, 0.0] },
      metal::scene3d::Vertex3d { position: [0.8, -0.7, 0.0] },
      metal::scene3d::Vertex3d { position: [0.0, 0.8, 0.0] },
   ];
   let indices = [0_u32, 1, 2];
   let mesh_data = metal::scene3d::Mesh3dData {
      vertices: &vertices,
      indices: &indices,
      topology: metal::scene3d::MeshTopology::Triangles,
   };
   let sample_count = if smoke { 2 } else { 30 };
   let cycles_per_sample = if smoke { 4 } else { 120 };
   let mut meshes = Vec::with_capacity(2);
   for _ in 0..2
   {
      meshes.push(renderer.mesh3d_create(&mesh_data).context("creating endurance mesh")?);
   }
   let mut samples = Vec::with_capacity(sample_count);
   for _ in 0..sample_count
   {
      let start = Instant::now();
      for _ in 0..cycles_per_sample
      {
         for mesh in meshes.drain(..) { renderer.mesh3d_release(mesh); }
         for _ in 0..2
         {
            meshes.push(renderer.mesh3d_create(&mesh_data).context("recreating endurance mesh")?);
         }
      }
      samples.push(start.elapsed().as_secs_f64() * 1_000.0);
   }
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.submit(token).context("submitting endurance accounting frame")?;
   let stats = renderer.last_stats();
   for mesh in meshes { renderer.mesh3d_release(mesh); }

   let summary = summarize(&samples);
   let (layer, scenario, variant, cache_state, refresh_mode) =
      perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "create_release_ms", &samples);
   metrics.insert(String::from("samples"), sample_count as f64);
   metrics.insert(String::from("cycles_per_sample"), cycles_per_sample as f64);
   metrics.insert(
      String::from("release_create_operations"),
      (sample_count * cycles_per_sample * 4) as f64,
   );
   metrics.insert(
      String::from("live_mesh_buffer_bytes_after_endurance"),
      stats.memory.scene3d_mesh_buffer_bytes as f64,
   );

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: format!("ms/{cycles_per_sample} cycles"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: samples.len(),
      ops_per_sample: (cycles_per_sample * 4) as u64,
      notes: vec![String::from(
         "Metal Scene3D repeated two-mesh release/create endurance with live GPU bytes checked after churn.",
      )],
      metrics,
   })
}

fn scene3d_matrix_case(id: &str, smoke: bool, instance_count: usize, feature: &str) -> Result<PerfCaseResult>
{
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(1_024, 1_024, 2.0).context("resizing Metal renderer")?;
   let vertices = [
      metal::scene3d::Vertex3d { position: [-0.8, -0.7, 0.0] },
      metal::scene3d::Vertex3d { position: [0.8, -0.7, 0.0] },
      metal::scene3d::Vertex3d { position: [0.0, 0.8, 0.0] },
   ];
   let indices = [0_u32, 1, 2];
   let mut meshes = Vec::with_capacity(16);
   for _ in 0..16
   {
      meshes.push(renderer.mesh3d_create(&metal::scene3d::Mesh3dData {
         vertices: &vertices,
         indices: &indices,
         topology: metal::scene3d::MeshTopology::Triangles,
      }).context("creating Scene3D proof mesh")?);
   }
   let mut instances = Vec::with_capacity(instance_count);
   for index in 0..instance_count
   {
      let mut instance = metal::scene3d::Instance3d::new(
         meshes[index % meshes.len()],
         scene3d_transform(index),
         api::Color::rgba(0.18 + (index % 5) as f32 * 0.12, 0.62, 0.94, 1.0),
      );
      instance.cull = if index & 1 == 0 { metal::scene3d::CullMode3d::Back } else { metal::scene3d::CullMode3d::None };
      instances.push(instance);
   }
   let mut compatible = instances.clone();
   for (index, instance) in compatible.iter_mut().enumerate()
   {
      instance.mesh = if index < instance_count / 2 { meshes[0] } else { meshes[1] };
      instance.cull = metal::scene3d::CullMode3d::Back;
   }
   let mut one_mesh = compatible.clone();
   for instance in &mut one_mesh { instance.mesh = meshes[0]; }
   let mut alpha = compatible.clone();
   for (index, instance) in alpha.iter_mut().enumerate()
   {
      instance.color.a = 0.25 + (index % 4) as f32 * 0.15;
      instance.depth_write = false;
   }
   let mut no_cull = compatible.clone();
   for instance in &mut no_cull { instance.cull = metal::scene3d::CullMode3d::None; }
   let bloom_one = [metal::scene3d::BloomLayer3d { sigma_px: 6.0, strength: 0.55 }];
   let bloom_three = [
      metal::scene3d::BloomLayer3d { sigma_px: 3.0, strength: 0.35 },
      metal::scene3d::BloomLayer3d { sigma_px: 8.0, strength: 0.25 },
      metal::scene3d::BloomLayer3d { sigma_px: 16.0, strength: 0.18 },
   ];
   let identity = authoring_scene3d_identity();
   let warmups = std::env::var("OXIDE_C58_METAL_WARMUPS")
      .or_else(|_| std::env::var("OXIDE_C57_METAL_WARMUPS"))
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|warmups| *warmups > 0)
      .unwrap_or(if smoke { 1 } else { 8 });
   let frames = std::env::var("OXIDE_C58_METAL_FRAMES")
      .or_else(|_| std::env::var("OXIDE_C57_METAL_FRAMES"))
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|frames| *frames > 0)
      .unwrap_or(if smoke { 2 } else { 24 });
   let raw_samples = std::env::var_os("OXIDE_C58_RAW_SAMPLES").is_some()
      || std::env::var_os("OXIDE_C57_RAW_SAMPLES").is_some();
   let mut warmup_frame_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut warmup_encode_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut warmup_gpu_samples = Vec::with_capacity(if raw_samples { warmups } else { 0 });
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut draws_sum = 0.0;
   let mut upload_sum = 0.0;
   let mut renderer_bytes_peak = 0_u64;
   let mut depth_target_bytes_peak = 0_u64;
   let mut bloom_target_bytes_peak = 0_u64;
   let mut mesh_buffer_bytes_peak = 0_u64;
   let mut render_passes_sum = 0_u64;
   let mut scene3d_draws_sum = 0_u64;
   let mut scene3d_instances_sum = 0_u64;
   let mut scene3d_instance_bytes_sum = 0_u64;
   let mut scene3d_pipeline_binds_sum = 0_u64;
   let mut scene3d_depth_state_binds_sum = 0_u64;
   let mut scene3d_cull_sets_sum = 0_u64;
   let mut scene3d_mesh_buffer_binds_sum = 0_u64;
   let mut scene3d_instance_buffer_binds_sum = 0_u64;
   let mut scene3d_instance_ring_grows_sum = 0_u64;
   let mut scene3d_viewport_sets_sum = 0_u64;
   let mut bloom_source_passes_sum = 0_u64;
   let mut bloom_source_draws_sum = 0_u64;
   let mut bloom_extract_passes_sum = 0_u64;
   let mut bloom_downsample_passes_sum = 0_u64;
   let mut bloom_blur_horizontal_passes_sum = 0_u64;
   let mut bloom_blur_vertical_passes_sum = 0_u64;
   let mut bloom_upsample_passes_sum = 0_u64;
   let mut bloom_composite_passes_sum = 0_u64;
   let mut bloom_graph_resources_sum = 0_u64;
   let mut bloom_graph_alias_slots_sum = 0_u64;
   let mut bloom_graph_plan_builds_sum = 0_u64;
   let mut bloom_graph_plan_reuses_sum = 0_u64;
   let mut bloom_graph_logical_bytes_sum = 0_u64;
   let mut bloom_graph_physical_bytes_sum = 0_u64;
   let mut bloom_graph_aliased_bytes_sum = 0_u64;
   let mut bloom_bandwidth_bytes_sum = 0_u64;
   let mut bloom_region_pixels_sum = 0_u64;

   let mut overlay = api::DrawList::default();
   overlay.items.push(api::DrawCmd::RRect {
      rect: api::RectF::new(24.0, 24.0, 120.0, 48.0),
      radii: [8.0; 4],
      color: api::Color::rgba(0.95, 0.96, 1.0, 0.92),
   });

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      let variant_instances = match feature
      {
         "compatible" | "viewport_25pct" => &compatible[..],
         "one_mesh" => &one_mesh[..],
         "alpha_order" => &alpha[..],
         "culling" => &no_cull[..],
         _ => &instances[..],
      };
      let viewport = if matches!(feature, "viewport_25pct" | "bloom_viewport_25pct") { Some(api::RectF::new(0.0, 0.0, 256.0, 256.0)) } else { None };
      let bloom = match feature
      {
         "bloom_1" => Some(metal::scene3d::Bloom3d { emissive_instances: variant_instances, layers: &bloom_one, downsample_divisor: 2 }),
         "bloom_3" | "bloom_viewport_25pct" | "bloom_overlay" => Some(metal::scene3d::Bloom3d { emissive_instances: variant_instances, layers: &bloom_three, downsample_divisor: 2 }),
         _ => None,
      };
      let pass = metal::scene3d::Pass3d {
         viewport,
         clear_color: Some(api::Color::rgba(0.025, 0.03, 0.045, 1.0)),
         clear_depth: true,
         view_proj: identity,
         instances: variant_instances,
         bloom,
      };
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      renderer.encode_scene3d(&pass).with_context(|| format!("encoding {id}"))?;
      if feature == "bloom_overlay"
      {
         renderer.encode_pass(&overlay);
      }
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame >= warmups
      {
         frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
         upload_sum += (stats.vb_bytes + stats.ib_bytes + stats.ub_bytes) as f64;
         renderer_bytes_peak = renderer_bytes_peak.max(stats.memory.total_bytes);
         depth_target_bytes_peak =
            depth_target_bytes_peak.max(stats.memory.depth_target_bytes);
         bloom_target_bytes_peak =
            bloom_target_bytes_peak.max(stats.memory.bloom_targets_bytes);
         mesh_buffer_bytes_peak =
            mesh_buffer_bytes_peak.max(stats.memory.scene3d_mesh_buffer_bytes);
         render_passes_sum = render_passes_sum.saturating_add(u64::from(stats.render_passes));
         scene3d_draws_sum = scene3d_draws_sum.saturating_add(u64::from(stats.scene3d_draws));
         scene3d_instances_sum = scene3d_instances_sum.saturating_add(u64::from(stats.scene3d_instances));
         scene3d_instance_bytes_sum = scene3d_instance_bytes_sum.saturating_add(stats.scene3d_instance_bytes);
         scene3d_pipeline_binds_sum = scene3d_pipeline_binds_sum.saturating_add(u64::from(stats.scene3d_pipeline_binds));
         scene3d_depth_state_binds_sum = scene3d_depth_state_binds_sum.saturating_add(u64::from(stats.scene3d_depth_state_binds));
         scene3d_cull_sets_sum = scene3d_cull_sets_sum.saturating_add(u64::from(stats.scene3d_cull_sets));
         scene3d_mesh_buffer_binds_sum = scene3d_mesh_buffer_binds_sum.saturating_add(u64::from(stats.scene3d_mesh_buffer_binds));
         scene3d_instance_buffer_binds_sum = scene3d_instance_buffer_binds_sum.saturating_add(u64::from(stats.scene3d_instance_buffer_binds));
         scene3d_instance_ring_grows_sum = scene3d_instance_ring_grows_sum.saturating_add(u64::from(stats.scene3d_instance_ring_grows));
         scene3d_viewport_sets_sum = scene3d_viewport_sets_sum.saturating_add(u64::from(stats.scene3d_viewport_sets));
         bloom_source_passes_sum = bloom_source_passes_sum.saturating_add(u64::from(stats.scene3d_bloom_source_passes));
         bloom_source_draws_sum = bloom_source_draws_sum.saturating_add(u64::from(stats.scene3d_bloom_source_draws));
         bloom_extract_passes_sum = bloom_extract_passes_sum.saturating_add(u64::from(stats.scene3d_bloom_extract_passes));
         bloom_downsample_passes_sum = bloom_downsample_passes_sum.saturating_add(u64::from(stats.scene3d_bloom_downsample_passes));
         bloom_blur_horizontal_passes_sum = bloom_blur_horizontal_passes_sum.saturating_add(u64::from(stats.scene3d_bloom_blur_horizontal_passes));
         bloom_blur_vertical_passes_sum = bloom_blur_vertical_passes_sum.saturating_add(u64::from(stats.scene3d_bloom_blur_vertical_passes));
         bloom_upsample_passes_sum = bloom_upsample_passes_sum.saturating_add(u64::from(stats.scene3d_bloom_upsample_passes));
         bloom_composite_passes_sum = bloom_composite_passes_sum.saturating_add(u64::from(stats.scene3d_bloom_composite_passes));
         bloom_graph_resources_sum = bloom_graph_resources_sum.saturating_add(u64::from(stats.scene3d_bloom_graph_resources));
         bloom_graph_alias_slots_sum = bloom_graph_alias_slots_sum.saturating_add(u64::from(stats.scene3d_bloom_graph_alias_slots));
         bloom_graph_plan_builds_sum = bloom_graph_plan_builds_sum.saturating_add(u64::from(stats.scene3d_bloom_graph_plan_builds));
         bloom_graph_plan_reuses_sum = bloom_graph_plan_reuses_sum.saturating_add(u64::from(stats.scene3d_bloom_graph_plan_reuses));
         bloom_graph_logical_bytes_sum = bloom_graph_logical_bytes_sum.saturating_add(stats.scene3d_bloom_graph_logical_bytes);
         bloom_graph_physical_bytes_sum = bloom_graph_physical_bytes_sum.saturating_add(stats.scene3d_bloom_graph_physical_bytes);
         bloom_graph_aliased_bytes_sum = bloom_graph_aliased_bytes_sum.saturating_add(stats.scene3d_bloom_graph_aliased_bytes);
         bloom_bandwidth_bytes_sum = bloom_bandwidth_bytes_sum.saturating_add(stats.scene3d_bloom_bandwidth_bytes);
         bloom_region_pixels_sum = bloom_region_pixels_sum.saturating_add(stats.scene3d_bloom_region_pixels);
      }
      else if raw_samples
      {
         warmup_frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         warmup_encode_samples.push(stats.encode_ms);
         warmup_gpu_samples.push(stats.gpu_ms);
      }
   }

   for mesh in meshes { renderer.mesh3d_release(mesh); }
   let summary = summarize(&frame_samples);
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("instances"), instance_count as f64);
   metrics.insert(
      String::from("mesh_count"),
      match feature
      {
         "one_mesh" => 1.0,
         "compatible" | "alpha_order" | "viewport_25pct" | "culling" => 2.0,
         _ => 16.0,
      },
   );
   metrics.insert(String::from("alpha_order_control"), if feature == "alpha_order" { 1.0 } else { 0.0 });
   metrics.insert(String::from("viewport_fraction"), if matches!(feature, "viewport_25pct" | "bloom_viewport_25pct") { 0.25 } else { 1.0 });
   metrics.insert(String::from("culling_variant"), if feature == "culling" { 1.0 } else { 0.0 });
   metrics.insert(String::from("bloom_layers"), if feature == "bloom_1" { 1.0 } else if matches!(feature, "bloom_3" | "bloom_viewport_25pct" | "bloom_overlay") { 3.0 } else { 0.0 });
   metrics.insert(String::from("overlay_control"), if feature == "bloom_overlay" { 1.0 } else { 0.0 });
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
   metrics.insert(String::from("upload_bytes_avg"), upload_sum / frames as f64);
   metrics.insert(String::from("renderer_bytes_peak"), renderer_bytes_peak as f64);
   metrics.insert(String::from("depth_target_bytes_peak"), depth_target_bytes_peak as f64);
   metrics.insert(String::from("bloom_target_bytes_peak"), bloom_target_bytes_peak as f64);
   metrics.insert(String::from("mesh_buffer_bytes_peak"), mesh_buffer_bytes_peak as f64);
   metrics.insert(String::from("render_passes_avg"), render_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_draws_avg"), scene3d_draws_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_instances_avg"), scene3d_instances_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_instance_bytes_avg"), scene3d_instance_bytes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_pipeline_binds_avg"), scene3d_pipeline_binds_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_depth_state_binds_avg"), scene3d_depth_state_binds_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_cull_sets_avg"), scene3d_cull_sets_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_mesh_buffer_binds_avg"), scene3d_mesh_buffer_binds_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_instance_buffer_binds_avg"), scene3d_instance_buffer_binds_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_instance_ring_grows_avg"), scene3d_instance_ring_grows_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_viewport_sets_avg"), scene3d_viewport_sets_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_source_passes_avg"), bloom_source_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_source_draws_avg"), bloom_source_draws_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_extract_passes_avg"), bloom_extract_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_downsample_passes_avg"), bloom_downsample_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_blur_horizontal_passes_avg"), bloom_blur_horizontal_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_blur_vertical_passes_avg"), bloom_blur_vertical_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_upsample_passes_avg"), bloom_upsample_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_composite_passes_avg"), bloom_composite_passes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_graph_resources_avg"), bloom_graph_resources_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_graph_alias_slots_avg"), bloom_graph_alias_slots_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_graph_plan_builds_avg"), bloom_graph_plan_builds_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_graph_plan_reuses_avg"), bloom_graph_plan_reuses_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_graph_logical_bytes_avg"), bloom_graph_logical_bytes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_graph_physical_bytes_avg"), bloom_graph_physical_bytes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_graph_aliased_bytes_avg"), bloom_graph_aliased_bytes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_bandwidth_bytes_avg"), bloom_bandwidth_bytes_sum as f64 / frames as f64);
   metrics.insert(String::from("scene3d_bloom_region_pixels_avg"), bloom_region_pixels_sum as f64 / frames as f64);
   if raw_samples
   {
      insert_indexed_samples(&mut metrics, "c58_warmup_frame_ms", &warmup_frame_samples);
      insert_indexed_samples(&mut metrics, "c58_warmup_encode_ms", &warmup_encode_samples);
      insert_indexed_samples(&mut metrics, "c58_warmup_gpu_ms", &warmup_gpu_samples);
      insert_indexed_samples(&mut metrics, "c58_frame_ms", &frame_samples);
      insert_indexed_samples(&mut metrics, "c58_encode_ms", &encode_samples);
      insert_indexed_samples(&mut metrics, "c58_gpu_ms", &gpu_samples);
   }

   Ok(PerfCaseResult {
      id: String::from(id),
      family: String::from("architecture"),
      layer: String::from(layer),
      scenario: String::from(scenario),
      variant: String::from(variant),
      cache_state: String::from(cache_state),
      refresh_mode: String::from(refresh_mode),
      unit: String::from("ms/frame"),
      gated: true,
      threshold_pct: 0.20,
      median: summary.median,
      p95: summary.p95,
      p99: summary.p99,
      min: summary.min,
      max: summary.max,
      mean: summary.mean,
      samples: frame_samples.len(),
      ops_per_sample: 1,
      notes: vec![format!("Metal Scene3D {instance_count}-instance {feature} workload.")],
      metrics,
   })
}

#[cfg(test)]
mod tests
{
   use super::*;

   #[test]
   fn id_mask_chunk_matrix_exactly_covers_triangle_vertices()
   {
      let vertices = id_mask_perf_vertices(16, 512.0);
      for requested in [1_usize, 16, 256]
      {
         let chunks = id_mask_chunks(vertices.len(), requested);
         assert_eq!(chunks.len(), requested);
         assert_eq!(chunks.first().map(|chunk| chunk.first_vertex), Some(0));
         assert_eq!(chunks.iter().map(|chunk| chunk.vertex_count).sum::<usize>(), vertices.len());
         assert!(chunks.iter().all(|chunk| chunk.vertex_count > 0 && chunk.vertex_count % 3 == 0));
         for pair in chunks.windows(2)
         {
            assert_eq!(pair[0].first_vertex + pair[0].vertex_count, pair[1].first_vertex);
         }
      }
   }

   #[test]
   fn architecture_damage_percentages_are_exact()
   {
      let full_pixels = 1_000_u64 * 700;
      for (name, expected) in [("damage_5pct", 5_u64), ("damage_25pct", 25), ("damage_100pct", 100)]
      {
         let damage = damage_rect_for(name, 0);
         let pixels = damage.rects.iter().map(|rect| rect.w as u64 * rect.h as u64).sum::<u64>();
         assert_eq!(pixels * 100 / full_pixels, expected);
      }
      assert_eq!(damage_rect_for("full_direct_then_partial", 0).rects, vec![api::RectI::new(0, 0, 1_000, 700)]);
      assert_eq!(damage_rect_for("full_direct_then_partial", 1).rects, vec![api::RectI::new(20, 20, 80, 60)]);
   }

   #[test]
   fn architecture_matrix_freezes_required_scaling_points()
   {
      let source = include_str!("architecture_matrix.rs");
      for required in [
         "depth in [16_usize, 32]",
         "\"hot_reuse\"",
         "\"one_use_churn\"",
         "surface_300",
         "warm_labels_1000",
         "new_labels_200",
         "clean_100x100",
         "[1_usize, 16, 256]",
         "[512_usize, 1_024, 2_048]",
         "rrect_1024",
         "spinner_512",
         "neon_1024",
         "nine_slice_512",
         "gpu.architecture.analytic_instances.{family}_{count}",
         "[1_usize, 64, 1_024, 10_000]",
         "isolated_mutation_10000",
         "retained_surface_dirty_leaf_10000",
         "[96_usize, 1_000, 10_000]",
         "icons_10000",
         "idle.static_foreground",
      ]
      {
         assert!(source.contains(required), "missing architecture proof scaling point {required}");
      }
   }
}
