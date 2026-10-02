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
