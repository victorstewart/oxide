use super::*;
use ui::elements::{Align, ImageFit, ImageView};

fn renderer() -> metal::MetalRenderer
{
   let mut renderer = metal::MetalRenderer::new_with_config_and_sdr_compositing(metal::MetalRendererConfig::default(), metal::SdrCompositing::SrgbSourceOver).expect("Metal renderer");
   renderer.resize(600, 360, 3.0).expect("resize");
   renderer
}

fn upload(renderer: &mut metal::MetalRenderer) -> gfx::ImageHandle
{
   let (pixels, width, height) = decode_png(include_bytes!("../../fixtures/visual-alpha-a.png")).expect("decode fixture");
   let handle = renderer.image_create_rgba8(width, height, &pixels, width as usize * 4);
   assert_ne!(handle.0, 0);
   handle
}

fn render(renderer: &mut metal::MetalRenderer, text: &mut TextCtx, font_id: usize, image: gfx::ImageHandle) -> Vec<u8>
{
   let mut builder = ui::DrawListBuilder::new();
   builder.rrect(gfx::RectF::new(0.0, 0.0, 200.0, 120.0), [0.0; 4], rgba([1.0; 4]));
   text.begin_frame_at_scale(3.0);
   Label {text: "Café Hgyp 32".into(), font_id, font_px: 32.0, color: rgba([0.12, 0.16, 0.2, 1.0]), align: Align::Left, wrap: false}
      .encode(gfx::RectF::new(4.0, 4.0, 192.0, 48.0), 3.0, text, &mut MtlUploader {renderer}, &mut builder);
   ImageView {image, natural_w: 33, natural_h: 25, fit: ImageFit::Contain, alpha: 1.0}
      .encode(gfx::RectF::new(4.0, 58.0, 180.0, 56.0), None, &mut builder);
   text.finish_frame(&mut MtlUploader {renderer}, &mut builder);
   let token = renderer.begin_frame(&gfx::FrameTarget, None);
   renderer.encode_pass(builder.drawlist());
   renderer.submit(token).expect("submit");
   renderer.readback_bgra8().expect("readback").2
}

fn retained_runtime(case: Case) -> Runtime
{
   let fixture: Fixture = serde_json::from_str(FIXTURE).expect("fixture");
   let mut renderer = renderer();
   renderer.resize(1_170, 2_532, 3.0).expect("viewport");
   let mut text = TextCtx::default();
   let font_id = text.fonts.add_font(text::Font::from_bytes(FONT_BYTES.to_vec()));
   let mut images = Vec::with_capacity(4);
   for bytes in IMAGE_BYTES
   {
      let (pixels, width, height) = decode_png(bytes).expect("image");
      let handle = renderer.image_create_rgba8_immutable(width, height, &pixels, width as usize * 4, true);
      images.push(Image {handle, width, height});
   }
   let text_labels = labels(&fixture.text.template, fixture.text.count, font_id, fixture.text.font_size, fixture.text_color, false);
   let local_labels = labels(&fixture.local.template, fixture.local.count, font_id, fixture.local.font_size, fixture.text_color, false);
   let card_labels = labels(&fixture.animation.template, fixture.animation.count, font_id, fixture.animation.font_size, fixture.text_color, false);
   let row_labels = labels(&fixture.scroll.template, fixture.scroll.count, font_id, fixture.scroll.font_size, fixture.text_color, true);
   let buttons = (0..fixture.local.count).map(|_| Button {
      text: fixture.local.button_title.clone(),
      style: ui::elements::ButtonStyle {text_px: fixture.local.font_size, color: rgba(fixture.local.button_color), ..Default::default()},
   }).collect();
   let collection = ui::collection::CollectionView::new(ui::collection::CollectionMode::VerticalGrid {col_width: fixture.viewport[0] as f32, spacing: 0.0});
   let suite = Suite {
      case, checkpoint: false, shapes: vec![0.0; fixture.shapes.count], text_values: vec![0; fixture.text.count],
      local_values: vec![0; fixture.local.count], last_text_step: None, last_local_step: None, text_labels,
      local_labels, buttons, button_states: (0..fixture.local.count).map(|_| ButtonState::default()).collect(),
      card_labels, row_labels, collection, fixture, images, animation_sequence: None, retained: None,
   };
   let mut runtime = Runtime {
      renderer, builder: ui::DrawListBuilder::new(), coalesced: Vec::new(), text, suite: Some(suite), visual: None,
      visual_checkpoint: false, visual_stage: 0, native_measurement: native_metrics::NativeMeasurement::default(),
   };
   warm_glyphs(&mut runtime, font_id);
   prepare_retained_suite(&mut runtime).expect("retained suite");
   runtime
}

fn render_retained(runtime: &mut Runtime, time: f64) -> Vec<u8>
{
   let snapshot = retained_snapshot(runtime, time).expect("retained snapshot");
   let token = runtime.renderer.begin_frame(&gfx::FrameTarget, None);
   runtime.renderer.encode_snapshot(&snapshot).expect("retained encode");
   runtime.renderer.submit(token).expect("retained submit");
   runtime.renderer.readback_bgra8().expect("retained readback").2
}

fn render_immediate(runtime: &mut Runtime, time: f64, generation: u64) -> Vec<u8>
{
   render_suite(runtime, time, generation);
   let token = runtime.renderer.begin_frame(&gfx::FrameTarget, None);
   runtime.renderer.encode_pass(runtime.builder.drawlist());
   runtime.renderer.submit(token).expect("immediate submit");
   runtime.renderer.readback_bgra8().expect("immediate readback").2
}

#[test]
fn glyph_and_image_resources_rebuild_to_identical_pixels()
{
   let mut text = TextCtx::default();
   let font_id = text.fonts.add_font(text::Font::from_bytes(FONT_BYTES.to_vec()));
   let mut first_renderer = renderer();
   let first_image = upload(&mut first_renderer);
   let before = render(&mut first_renderer, &mut text, font_id, first_image);
   let warm = render(&mut first_renderer, &mut text, font_id, first_image);
   assert_eq!(before, warm, "stable scene differs after cache warmup");
   // Exercise the real device-loss owner, preserving fonts and app content while
   // recreating the renderer and uploading image resources to the new device.
   text.handle_device_loss();
   drop(first_renderer);
   let mut rebuilt_renderer = renderer();
   let rebuilt_image = upload(&mut rebuilt_renderer);
   let rebuilt = render(&mut rebuilt_renderer, &mut text, font_id, rebuilt_image);
   assert_eq!(before, rebuilt, "glyph/image rebuild changed visible output");
   for (name, rows) in [("glyphs", 0..150), ("image", 180..342)]
   {
      let colored = rows.flat_map(|y| before[y * 600 * 4..(y + 1) * 600 * 4].chunks_exact(4)).filter(|pixel| pixel[..3] != [255, 255, 255]).count();
      assert!(colored > 100, "{name} did not render before or after rebuild");
   }
}

#[test]
fn retained_text_and_local_chunks_match_immediate_pixels_and_reuse_unchanged_units()
{
   for case in [Case::Text, Case::Local]
   {
      let mut retained = retained_runtime(case);
      let mut immediate = retained_runtime(case);
      immediate.suite.as_mut().unwrap().retained = None;
      for (time, generation) in [(0.0, 1), (0.1, 2), (0.2, 3)]
      {
         let retained_pixels = render_retained(&mut retained, time);
         let immediate_pixels = render_immediate(&mut immediate, time, generation);
         assert_eq!(retained_pixels, immediate_pixels, "{case:?} retained pixels at {time}");
         if time == 0.1
         {
            let stats = retained.renderer.last_stats();
            let units = retained.suite.as_ref().unwrap().retained.as_ref().unwrap().units.len() as u64;
            assert!(stats.chunks_reused >= units - 1, "{case:?} did not reuse unchanged chunks");
            assert!(stats.chunks_rebuilt <= 1, "{case:?} rebuilt unchanged chunks");
         }
      }
      let retained_pixels = render_retained(&mut retained, 0.0);
      let immediate_pixels = render_immediate(&mut immediate, 0.0, 4);
      assert_eq!(retained_pixels, immediate_pixels, "{case:?} retained pixels after reset");
   }
}

#[test]
fn retained_text_chunks_rebuild_after_atlas_resource_recovery()
{
   let mut runtime = retained_runtime(Case::Text);
   let before = render_retained(&mut runtime, 0.0);
   runtime.text.handle_device_loss();
   runtime.renderer = renderer();
   runtime.renderer.resize(1_170, 2_532, 3.0).expect("recovered viewport");
   runtime.suite.as_mut().unwrap().last_text_step = Some(1);
   prepare_retained_suite(&mut runtime).expect("rebuild retained chunks");
   let recovered = render_retained(&mut runtime, 0.0);
   assert_eq!(before, recovered, "resource recovery changed retained text pixels");
}

#[test]
fn visual_boards_restore_repeated_cycles_through_the_production_snapshot_path()
{
   for name in ["visual-controls", "visual-editing", "visual-typography", "visual-composition", "visual-layout", "visual-pickers",
      "visual-opacity", "visual-images", "visual-geometry", "visual-editing-edges"]
   {
      let mut renderer = renderer();
      renderer.resize(1_170, 2_532, 3.0).expect("viewport");
      let mut text = TextCtx::default();
      let mut board = visual_boards::VisualBoards::new(name, &mut text, &mut renderer).expect("board");
      let mut builder = ui::DrawListBuilder::new();
      let mut initial = None;
      for stage in [0, 1, 2, 0, 1, 2, 0]
      {
         let snapshot = board.draw(stage, &mut text, &mut renderer, &mut builder).unwrap_or_else(|_| panic!("{name}/{stage}: {:?}", std::fs::read_to_string(std::env::temp_dir().join("oxide-visual-render-error.txt"))));
         let token = renderer.begin_frame(&gfx::FrameTarget, None);
         renderer.encode_snapshot(&snapshot).unwrap_or_else(|error| panic!("{name}/{stage}: {error:?}"));
         renderer.submit(token).expect("submit");
         let pixels = renderer.readback_bgra8().expect("complete frame").2;
         if stage == 0
         {
            if let Some(initial) = initial.as_ref()
            {
               assert_eq!(pixels, *initial, "{name} did not restore its original production state");
            }
            else {initial = Some(pixels);}
         }
      }
   }
}

#[test]
fn timed_visual_cycle_repeats_action_boundaries()
{
   assert_eq!(visual_stage_at(-1.0), 0);
   assert_eq!(visual_stage_at(0.0), 0);
   assert_eq!(visual_stage_at(1.999), 0);
   assert_eq!(visual_stage_at(2.0), 1);
   assert_eq!(visual_stage_at(4.0), 2);
   assert_eq!(visual_stage_at(6.0), 0);
   assert_eq!(visual_stage_at(8.0), 1);
   assert_eq!(visual_next_wakeup(0.0), 2.0);
   assert_eq!(visual_next_wakeup(2.0), 4.0);
   assert_eq!(visual_next_wakeup(4.0), 6.0);
   assert_eq!(visual_next_wakeup(6.0), 8.0);
}

#[test]
fn timed_control_and_picker_actions_progress_then_restore()
{
   for name in ["visual-controls", "visual-pickers"]
   {
      let mut renderer = renderer();
      renderer.resize(1_170, 2_532, 3.0).expect("viewport");
      let mut text = TextCtx::default();
      let mut board = visual_boards::VisualBoards::new(name, &mut text, &mut renderer).expect("board");
      let mut builder = ui::DrawListBuilder::new();
      let mut frames = Vec::new();
      for (stage, elapsed_ms, stage_elapsed_ms) in [(0, 0, 0), (1, 2_000, 0), (1, 2_100, 100), (1, 2_300, 300), (0, 6_000, 0)]
      {
         let snapshot = board.draw_timed(stage, elapsed_ms, stage_elapsed_ms, &mut text, &mut renderer, &mut builder).expect("snapshot");
         let token = renderer.begin_frame(&gfx::FrameTarget, None);
         renderer.encode_snapshot(&snapshot).expect("encode");
         renderer.submit(token).expect("submit");
         frames.push(renderer.readback_bgra8().expect("complete frame").2);
      }
      assert_ne!(frames[1], frames[2], "{name} action did not advance through real elapsed time");
      assert_ne!(frames[2], frames[3], "{name} intermediate action matched its settled state");
      if name == "visual-controls"
      {
         assert_eq!(board.next_wakeup(1, 2_000), Some(0), "controls stopped its indeterminate progress phase");
         assert_eq!(board.next_wakeup(0, 0), Some(0), "control state stopped its indeterminate progress phase");
      }
      else
      {
         assert_eq!(board.next_wakeup(1, 1_984), Some(0), "{name} stopped scheduling its active production state early");
         assert_eq!(board.next_wakeup(1, 2_000), None, "{name} remained active after its timed action");
      }
      assert_eq!(frames[0], frames[4], "{name} timed reset did not restore its initial state");
      if name == "visual-controls"
      {
         let snapshot = board.draw_timed(0, 30_000, 30_000, &mut text, &mut renderer, &mut builder).expect("long control snapshot");
         let token = renderer.begin_frame(&gfx::FrameTarget, None);
         renderer.encode_snapshot(&snapshot).expect("long control encode");
         renderer.submit(token).expect("long control submit");
         assert_eq!(frames[0], renderer.readback_bgra8().expect("long control complete frame").2, "long control hold left stage zero");
         let snapshot = board.draw_timed(0, 30_100, 30_100, &mut text, &mut renderer, &mut builder).expect("control phase snapshot");
         let token = renderer.begin_frame(&gfx::FrameTarget, None);
         renderer.encode_snapshot(&snapshot).expect("control phase encode");
         renderer.submit(token).expect("control phase submit");
         assert_ne!(frames[0], renderer.readback_bgra8().expect("control phase complete frame").2, "indeterminate control phase did not advance during its control hold");
      }
   }
}

#[cfg(feature = "native-measurement")]
fn native_init(name: &str, checkpoint: bool)
{
   let name = std::ffi::CString::new(name).expect("case name");
   assert_eq!(unsafe {oxide_core_suite_init(name.as_ptr(), u8::from(checkpoint))}, 0, "initialize {name:?}");
}

#[cfg(feature = "native-measurement")]
fn native_offscreen_pixels(time: f64, generation: u64) -> (u32, u32, Vec<u8>)
{
   assert_eq!(native_draw_offscreen(time, generation), 0, "offscreen draw at {time}");
   let mut slot = RUNTIME.get_or_init(|| Mutex::new(None)).lock().unwrap();
   slot.as_mut().unwrap().renderer.readback_bgra8().expect("offscreen target")
}

#[cfg(feature = "native-measurement")]
#[test]
fn native_offscreen_smokes_all_cases_and_preserves_text_local_checkpoint_pixels()
{
   assert_eq!(oxide_core_init(), 0);
   native_set_retained_text_enabled(true);
   for name in NATIVE_CASES
   {
      native_init(name, false);
      let (width, height, pixels) = native_offscreen_pixels(0.0, 1);
      assert_eq!((width, height), (1_170, 2_532), "{name} offscreen dimensions");
      assert!(pixels.iter().any(|pixel| *pixel != 0), "{name} offscreen target was empty");
   }

   for name in ["text", "local"]
   {
      native_set_retained_text_enabled(false);
      native_init(name, true);
      let immediate: Vec<_> = [0.0, 10.0, 19.9, 0.0, 10.0, 19.9, 0.0].into_iter().enumerate()
         .map(|(generation, time)| native_offscreen_pixels(time, generation as u64 + 1).2).collect();
      native_set_retained_text_enabled(true);
      native_init(name, true);
      let retained: Vec<_> = [0.0, 10.0, 19.9, 0.0, 10.0, 19.9, 0.0].into_iter().enumerate()
         .map(|(generation, time)| native_offscreen_pixels(time, generation as u64 + 1).2).collect();
      assert_eq!(retained, immediate, "{name} retained checkpoints differ from immediate fixtures");
   }

   let path = std::env::temp_dir().join(format!("oxide-offscreen-{}.png", std::process::id()));
   native_capture_offscreen(&path).expect("capture offscreen PNG");
   let (pixels, width, height) = decode_png(&std::fs::read(&path).expect("read offscreen PNG")).expect("decode offscreen PNG");
   std::fs::remove_file(&path).expect("remove offscreen PNG");
   assert_eq!((width, height), (1_170, 2_532));
   assert_eq!(pixels.len(), width as usize * height as usize * 4);
   native_visual_images_records_preparation_upload_once_before_begin_frame();
}

#[cfg(feature = "native-measurement")]
fn native_visual_images_records_preparation_upload_once_before_begin_frame()
{
   assert_eq!(oxide_core_init(), 0);
   enable_native_measurement(true);
   native_init("visual-images", false);

   assert_eq!(native_draw_offscreen(0.0, 1), 0, "initial visual-images frame");

   assert_eq!(native_draw_offscreen(VISUAL_ACTION_SECONDS, 2), 0, "replacement visual-images frame");
   assert!(native_frame_metrics().preparation_texture_upload_bytes > 0, "stage-one image replacement was lost when begin_frame reset renderer stats");

   assert_eq!(native_draw_offscreen(VISUAL_ACTION_SECONDS, 3), 0, "held replacement visual-images frame");
   assert_eq!(native_frame_metrics().preparation_texture_upload_bytes, 0, "held stage double-counted the prior replacement upload");
}
