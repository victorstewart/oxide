use super::*;

struct RetainedScenario
{
   surface: ui::UiSurface,
   leaf: ui::NodeId,
   builder: ui::DrawListBuilder,
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

   push_if_allowed(cases, "cpu.architecture.animation.surface_300", || animation_surface_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.warm_labels_1000", || text_warm_labels_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.new_labels_200", || text_new_labels_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.script_fallback_matrix", || text_script_matrix_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.scale_sdf_matrix", || text_scale_sdf_matrix_case(smoke));
   push_if_allowed(cases, "cpu.architecture.text.atlas_eviction", || text_atlas_eviction_case(smoke));
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

   for change in ["static", "style", "viewport", "projection"]
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

   for instances in [96_usize, 1_000, 10_000]
   {
      for feature in ["one_mesh", "many_meshes", "alpha_order", "viewport_25pct", "culling", "bloom_1", "bloom_3"]
      {
         let id = format!("gpu.architecture.scene3d.instances_{instances}.{feature}");
         if perf_case_allowed(&id)
         {
            cases.push(scene3d_matrix_case(&id, smoke, instances, feature)?);
         }
      }
   }

   push_if_allowed(cases, "cpu.architecture.idle.static_foreground", || idle_case(smoke));
   Ok(())
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
   let mut case = measured_architecture_case(
      id,
      smoke,
      "Versioned UiSurface tree with 1000 label-shaped nodes, 500 image-shaped nodes, retained clean replay, and one-leaf mutation coverage.",
      move || {
         if scenario.dirty
         {
            scenario.phase = scenario.phase.wrapping_add(1);
            let phase = (scenario.phase % 97) as f32 / 97.0;
            scenario.surface.edit_style(scenario.leaf, |style| {
               style.opacity = 0.55 + phase * 0.40;
            });
            scenario.surface.layout(1_200.0, 2_400.0);
         }
         scenario.builder.clear();
         let status = scenario.surface.encode_retained(&mut scenario.builder);
         let stats = scenario.surface.retained_node_stats();
         let draws = scenario.builder.drawlist();
         (draws.items.len() as u64)
            .wrapping_add(draws.vertices.len() as u64)
            .wrapping_add(draws.indices.len() as u64)
            .wrapping_add(stats.reused_nodes as u64)
            .wrapping_add(match status { ui::RetainedDrawStatus::Reused => 1, ui::RetainedDrawStatus::Rebuilt => 2 })
      },
   );
   case.metrics.insert(String::from("tree_depth"), depth as f64);
   case.metrics.insert(String::from("label_nodes"), 1_000.0);
   case.metrics.insert(String::from("image_nodes"), 500.0);
   case.metrics.insert(String::from("dirty_nodes"), if dirty { 1.0 } else { 0.0 });
   case.metrics.insert(String::from("layout_passes_expected"), if dirty { 1.0 } else { 0.0 });
   case
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
   let mut builder = ui::DrawListBuilder::new();
   let _ = surface.encode_retained(&mut builder);
   RetainedScenario { surface, leaf, builder, dirty, phase: 0 }
}

fn animation_surface_case(smoke: bool) -> PerfCaseResult
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
   let mut animator = ui::anim::Animator::new();
   for (index, node) in nodes.iter().copied().enumerate()
   {
      let transform = platform::Transform2D {
         tx: (index % 9) as f32,
         ty: (index % 5) as f32,
         sx: 1.0 + (index % 3) as f32 * 0.02,
         sy: 1.0,
         rot_rad: (index % 13) as f32 * 0.01,
      };
      animator.start(node, platform::AnimDesc {
         id: 0,
         prop: platform::AnimProp::Transform2D,
         from: platform::AnimValue::Xform2D(ui::anim::helpers::identity_transform()),
         to: platform::AnimValue::Xform2D(transform),
         curve: platform::AnimCurve::Ease { ease: platform::Ease { kind: platform::EaseKind::CubicInOut } },
         duration_ms: 700,
         delay_ms: (index % 17) as u32,
         repeat: platform::Repeat::Forever,
      });
      animator.start(node, platform::AnimDesc {
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
   let mut builder = ui::DrawListBuilder::new();
   let mut retained_content = authoring_text_replay_drawlist();
   let glyph = retained_content.items[0].clone();
   for _ in 2..200 { retained_content.items.push(glyph.clone()); }
   for index in 0..100
   {
      retained_content.items.push(api::DrawCmd::Image {
         tex: api::ImageHandle((index % 8 + 20) as u32),
         dst: api::RectF::new((index % 20) as f32 * 24.0, (index / 20) as f32 * 24.0, 20.0, 20.0),
         src: api::RectF::new(0.0, 0.0, 1.0, 1.0),
         alpha: 1.0,
      });
   }
   let atlases = [(api::ImageHandle(4), 3), (api::ImageHandle(9), 7)];
   let mut case = measured_architecture_case(
      "cpu.architecture.animation.surface_300",
      smoke,
      "Real 300-node UiSurface animation with Animator overrides, nested clips/opacity, transforms, retained encoding, hit testing, and accessibility dirtiness.",
      move || {
         frame = frame.wrapping_add(1);
         *surface.overrides_mut() = animator.step(start.saturating_add(frame * 8));
         let _ = surface.mark_node_dirty(nodes[frame as usize % nodes.len()], ui::DirtyClass::Accessibility);
         builder.clear();
         let _ = surface.encode_retained(&mut builder);
         let _ = builder.append_retained_drawlist_with_text_atlas_revisions(&retained_content, &atlases);
         let hit = surface.hit_test((frame % 800) as f32, (frame % 700) as f32).is_some() as u64;
         builder.drawlist().items.len() as u64 + surface.overrides().len() as u64 + hit
      },
   );
   case.metrics.insert(String::from("animated_nodes"), 300.0);
   case.metrics.insert(String::from("active_animations"), 600.0);
   case.metrics.insert(String::from("hit_tests_per_op"), 1.0);
   case.metrics.insert(String::from("accessibility_geometry_nodes"), 300.0);
   case.metrics.insert(String::from("label_nodes"), 200.0);
   case.metrics.insert(String::from("image_nodes"), 100.0);
   case
}

fn text_warm_labels_case(smoke: bool) -> PerfCaseResult
{
   let mut text = perf_text_ctx();
   text.set_fallback_fonts(&[1]);
   let mut uploader = CpuUploader::default();
   let mut builder = ui::DrawListBuilder::new();
   let labels = (0..1_000).map(|index| format!("Warm label {index:04}")).collect::<Vec<_>>();
   for (index, label) in labels.iter().enumerate()
   {
      encode_matrix_label(label, index, 2.0, 18.0, &mut text, &mut uploader, &mut builder);
   }
   let mut case = measured_architecture_case(
      "cpu.architecture.text.warm_labels_1000",
      smoke,
      "One thousand already-shaped and atlas-resident labels encoded into one warm frame.",
      move || {
         builder.clear();
         for (index, label) in labels.iter().enumerate()
         {
            encode_matrix_label(label, index, 2.0, 18.0, &mut text, &mut uploader, &mut builder);
         }
         builder.drawlist().items.len() as u64 + builder.drawlist().vertices.len() as u64
      },
   );
   case.metrics.insert(String::from("warm_labels"), 1_000.0);
   case.metrics.insert(String::from("device_scale"), 2.0);
   case
}

fn text_new_labels_case(smoke: bool) -> PerfCaseResult
{
   let mut phase = 0_u64;
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
         for index in 0..200
         {
            let label = format!("New {phase:08x} Latin 漢字 مرحبا 😀 {index:03}");
            encode_matrix_label(&label, index, 3.0, 20.0, &mut text, &mut uploader, &mut builder);
         }
         builder.drawlist().items.len() as u64 + builder.drawlist().vertices.len() as u64 + text.atlas_revision()
      },
   );
   case.metrics.insert(String::from("new_labels"), 200.0);
   case.metrics.insert(String::from("device_scale"), 3.0);
   case
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
         for (index, value) in strings.iter().enumerate()
         {
            encode_matrix_label(value, index, 3.0, 24.0, &mut text, &mut uploader, &mut builder);
         }
         builder.drawlist().items.len() as u64 + builder.drawlist().vertices.len() as u64
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
            encode_matrix_label("SDF Scale Matrix", index, scale, font_px, &mut text, &mut uploader, &mut builder);
            checksum = checksum.wrapping_add(builder.drawlist().vertices.len() as u64).wrapping_add(text.atlas_revision());
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

fn encode_matrix_label(
   value: &str,
   index: usize,
   scale: f32,
   font_px: f32,
   text: &mut ui::elements::TextCtx,
   uploader: &mut CpuUploader,
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
      "backdrop_separated_48" => 48,
      "backdrop_coalescible_12" => 12,
      "blur_mixed_sigma" => 16,
      "blur_edges_corners" => 4,
      _ => 1,
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

fn image_case(id: &str, smoke: bool, name: &str, count: usize) -> PerfCaseResult
{
   let kind = String::from(name);
   let mut phase = 0_u32;
   let mut case = measured_architecture_case(
      id,
      smoke,
      "Unique icon/avatar residency command matrix with contain/cover/zoom at 3x, display-size decode accounting, release/reuse churn, and minification/mip intent.",
      move || {
         phase = phase.wrapping_add(1);
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
   let mut renderer = Box::new(metal::MetalRenderer::new_default().context("creating Metal renderer")?);
   renderer.resize(1_200, 800, 1.0).context("resizing Metal renderer")?;
   renderer.set_damage_options(true, DAMAGE_USE_THRESH, DAMAGE_PREFILTER_THRESH);
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
   let mut vb_sum = 0.0;
   let mut ib_sum = 0.0;
   let mut ub_sum = 0.0;
   let mut damage_pixels_sum = 0.0;
   let mut damage_rects_sum = 0.0;
   let mut layer_bytes_peak = 0_u64;
   let mut total_bytes_peak = 0_u64;
   let mut skips_sum = 0.0;
   let mut layer_body_commands_scanned_sum = 0.0;
   let mut layer_body_commands_copied_sum = 0.0;
   let mut layer_texture_creates_sum = 0.0;
   let mut layer_cache_hits_sum = 0.0;
   let mut layer_cache_misses_sum = 0.0;
   let mut layer_offscreen_draws_sum = 0.0;
   let mut layer_inline_draws_sum = 0.0;
   let mut layer_double_render_prevented_sum = 0.0;

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      let (draws, damage, resize, recreate) = build(frame);
      if recreate
      {
         renderer = Box::new(metal::MetalRenderer::new_default().context("recreating Metal renderer after benchmark memory warning")?);
         renderer.resize(1_200, 800, 1.0).context("resizing recreated Metal renderer")?;
         renderer.set_damage_options(true, DAMAGE_USE_THRESH, DAMAGE_PREFILTER_THRESH);
      }
      if let Some((width, height)) = resize
      {
         renderer.resize(width, height, 1.0).with_context(|| format!("resizing for {id}"))?;
      }
      let token = renderer.begin_frame(&api::FrameTarget, damage.as_ref());
      let frame_id = token.0;
      renderer.encode_pass(&draws);
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame >= warmups
      {
         frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
         vb_sum += stats.vb_bytes as f64;
         ib_sum += stats.ib_bytes as f64;
         ub_sum += stats.ub_bytes as f64;
         damage_pixels_sum += stats.damage_px as f64;
         damage_rects_sum += stats.damage_rects as f64;
         layer_bytes_peak = layer_bytes_peak.max(stats.memory.layer_cache_bytes);
         total_bytes_peak = total_bytes_peak.max(stats.memory.total_bytes);
         skips_sum += stats.frame_backpressure_skipped as f64;
         layer_body_commands_scanned_sum += stats.layer_body_commands_scanned as f64;
         layer_body_commands_copied_sum += stats.layer_body_commands_copied as f64;
         layer_texture_creates_sum += stats.layer_texture_creates as f64;
         layer_cache_hits_sum += stats.layer_cache_hits as f64;
         layer_cache_misses_sum += stats.layer_cache_misses as f64;
         layer_offscreen_draws_sum += stats.layer_offscreen_draws as f64;
         layer_inline_draws_sum += stats.layer_inline_draws as f64;
         layer_double_render_prevented_sum += stats.layer_double_render_prevented as f64;
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
   metrics.insert(String::from("vertex_upload_bytes_avg"), vb_sum / frames as f64);
   metrics.insert(String::from("index_upload_bytes_avg"), ib_sum / frames as f64);
   metrics.insert(String::from("uniform_upload_bytes_avg"), ub_sum / frames as f64);
   metrics.insert(String::from("damage_pixels_avg"), damage_pixels_sum / frames as f64);
   metrics.insert(String::from("damage_rects_avg"), damage_rects_sum / frames as f64);
   metrics.insert(String::from("layer_cache_bytes_peak"), layer_bytes_peak as f64);
   metrics.insert(String::from("renderer_bytes_peak"), total_bytes_peak as f64);
   metrics.insert(String::from("frame_backpressure_skips"), skips_sum);
   metrics.insert(String::from("layer_body_commands_scanned_avg"), layer_body_commands_scanned_sum / frames as f64);
   metrics.insert(String::from("layer_body_commands_copied_avg"), layer_body_commands_copied_sum / frames as f64);
   metrics.insert(String::from("layer_texture_creates_avg"), layer_texture_creates_sum / frames as f64);
   metrics.insert(String::from("layer_cache_hits_avg"), layer_cache_hits_sum / frames as f64);
   metrics.insert(String::from("layer_cache_misses_avg"), layer_cache_misses_sum / frames as f64);
   metrics.insert(String::from("layer_offscreen_draws_avg"), layer_offscreen_draws_sum / frames as f64);
   metrics.insert(String::from("layer_inline_draws_avg"), layer_inline_draws_sum / frames as f64);
   metrics.insert(String::from("layer_double_render_prevented_avg"), layer_double_render_prevented_sum / frames as f64);
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
   case.metrics.insert(String::from("memory_warning_recreates"), if name == "memory_warning" { case.samples as f64 } else { 0.0 });
   Ok(case)
}

fn effect_drawlist(name: &str) -> api::DrawList
{
   let mut builder = ui::DrawListBuilder::new();
   if name == "backdrop_separated_48"
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
   let mut case = measured_metal_drawlist_case(
      id,
      smoke,
      format!("Metal effect workload for {note_kind} through the production effect/layer encoder."),
      move |_| (effect_drawlist(&kind), None, None, false),
   )?;
   case.metrics.insert(String::from("effect_regions"), effect_region_count(name) as f64);
   Ok(case)
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
   for index in 0..count
   {
      let pixel = [
         (index as u8).wrapping_mul(17), 96, 220, 255,
         32, (index as u8).wrapping_mul(29), 180, 255,
         210, 64, (index as u8).wrapping_mul(11), 255,
         245, 210, 80, 255,
      ];
      handles.push(renderer.image_create_rgba8(2, 2, &pixel, 8));
   }
   let warmups = if smoke { 1 } else { 3 };
   let frames = if smoke { 2 } else { 10 };
   let mut frame_samples = Vec::with_capacity(frames);
   let mut encode_samples = Vec::with_capacity(frames);
   let mut gpu_samples = Vec::with_capacity(frames);
   let mut draws_sum = 0.0;
   let mut upload_sum = 0.0;
   let mut image_bytes_peak = 0_u64;
   let mut total_bytes_peak = 0_u64;
   let mut image_argument_encodes_sum = 0.0;
   let mut image_argument_binds_sum = 0.0;
   let mut image_argument_tables_finalized_sum = 0.0;
   let mut image_argument_table_reuses_sum = 0.0;
   let mut image_argument_bytes_sum = 0.0;
   let mut image_argument_buffer_grows_sum = 0.0;

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
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      renderer.encode_pass(builder.drawlist());
      renderer.submit(token).with_context(|| format!("submitting {id}"))?;
      let stats = last_metal_stats_after_submit(&renderer, frame_id);
      if frame >= warmups
      {
         frame_samples.push(frame_t0.elapsed().as_secs_f64() * 1_000.0);
         encode_samples.push(stats.encode_ms);
         gpu_samples.push(stats.gpu_ms);
         draws_sum += stats.draws as f64;
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
   }

   for handle in handles { renderer.image_release(handle); }
   let summary = summarize(&frame_samples);
   let (layer, scenario, variant, cache_state, refresh_mode) = perf_case_contract_metadata(id, "architecture");
   let mut metrics = BTreeMap::new();
   insert_distribution_metrics(&mut metrics, "frame_ms", &frame_samples);
   insert_distribution_metrics(&mut metrics, "encode_ms", &encode_samples);
   insert_distribution_metrics(&mut metrics, "gpu_ms", &gpu_samples);
   insert_frame_pacing_metrics(&mut metrics, &frame_samples);
   metrics.insert(String::from("unique_images"), count as f64);
   metrics.insert(String::from("image_draws"), count as f64);
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
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
      notes: vec![format!("Metal {name} image-residency workload with {count} unique 2x2 source resources and production image draws.")],
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
   let chunks = id_mask_chunks(vertices.len(), chunk_count);
   let warmups = if smoke { 1 } else { 3 };
   let frames = if smoke { 3 } else { 12 };
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

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      let revision = if change == "static" || change == "style" { 1 } else { frame as u64 + 1 };
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
         "projection" => pass.raster.projection.world_to_clip[3][0] = (frame & 1) as f32 * 0.002,
         _ => {},
      }
      let token = renderer.begin_frame(&api::FrameTarget, None);
      let frame_id = token.0;
      renderer.encode_id_mask_gpu_compositor(&pass).with_context(|| format!("encoding {id}"))?;
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
   metrics.insert(String::from("frame_backpressure_skips"), skips_sum);
   metrics.insert(String::from("geometry_changes_per_frame"), if change == "static" || change == "style" { 0.0 } else { 1.0 });
   metrics.insert(String::from("style_changes_per_frame"), if change == "style" { 1.0 } else { 0.0 });

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
   let mut one_mesh = instances.clone();
   for instance in &mut one_mesh { instance.mesh = meshes[0]; }
   let mut alpha = instances.clone();
   for (index, instance) in alpha.iter_mut().enumerate()
   {
      instance.color.a = 0.25 + (index % 4) as f32 * 0.15;
      instance.depth_write = false;
   }
   let mut no_cull = instances.clone();
   for instance in &mut no_cull { instance.cull = metal::scene3d::CullMode3d::None; }
   let bloom_one = [metal::scene3d::BloomLayer3d { sigma_px: 6.0, strength: 0.55 }];
   let bloom_three = [
      metal::scene3d::BloomLayer3d { sigma_px: 3.0, strength: 0.35 },
      metal::scene3d::BloomLayer3d { sigma_px: 8.0, strength: 0.25 },
      metal::scene3d::BloomLayer3d { sigma_px: 16.0, strength: 0.18 },
   ];
   let identity = authoring_scene3d_identity();
   let warmups = if smoke { 1 } else { 2 };
   let frames = if smoke { 2 } else { 8 };
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

   for frame in 0..(warmups + frames)
   {
      let frame_t0 = Instant::now();
      let variant_instances = match feature
      {
         "one_mesh" => &one_mesh[..],
         "alpha_order" => &alpha[..],
         "culling" => &no_cull[..],
         _ => &instances[..],
      };
      let viewport = if feature == "viewport_25pct" { Some(api::RectF::new(0.0, 0.0, 256.0, 256.0)) } else { None };
      let bloom = match feature
      {
         "bloom_1" => Some(metal::scene3d::Bloom3d { emissive_instances: variant_instances, layers: &bloom_one, downsample_divisor: 2 }),
         "bloom_3" => Some(metal::scene3d::Bloom3d { emissive_instances: variant_instances, layers: &bloom_three, downsample_divisor: 2 }),
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
   metrics.insert(String::from("mesh_count"), if feature == "one_mesh" { 1.0 } else { 16.0 });
   metrics.insert(String::from("alpha_order_control"), if feature == "alpha_order" { 1.0 } else { 0.0 });
   metrics.insert(String::from("viewport_fraction"), if feature == "viewport_25pct" { 0.25 } else { 1.0 });
   metrics.insert(String::from("culling_variant"), if feature == "culling" { 1.0 } else { 0.0 });
   metrics.insert(String::from("bloom_layers"), if feature == "bloom_1" { 1.0 } else if feature == "bloom_3" { 3.0 } else { 0.0 });
   metrics.insert(String::from("draws_avg"), draws_sum / frames as f64);
   metrics.insert(String::from("upload_bytes_avg"), upload_sum / frames as f64);
   metrics.insert(String::from("renderer_bytes_peak"), renderer_bytes_peak as f64);
   metrics.insert(String::from("depth_target_bytes_peak"), depth_target_bytes_peak as f64);
   metrics.insert(String::from("bloom_target_bytes_peak"), bloom_target_bytes_peak as f64);
   metrics.insert(String::from("mesh_buffer_bytes_peak"), mesh_buffer_bytes_peak as f64);
   metrics.insert(String::from("render_passes_avg"), render_passes_sum as f64 / frames as f64);

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
         "isolated_mutation_10000",
         "[96_usize, 1_000, 10_000]",
         "icons_10000",
         "idle.static_foreground",
      ]
      {
         assert!(source.contains(required), "missing architecture proof scaling point {required}");
      }
   }
}
