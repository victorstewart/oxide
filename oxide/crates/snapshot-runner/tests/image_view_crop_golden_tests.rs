#![cfg(target_os = "macos")]

use oxide_renderer_api::{self as api, Renderer};
use oxide_renderer_metal::MetalRenderer;
use oxide_ui_core::elements::{ImageFit, ImageView, ImageZoomState};
use oxide_ui_core::DrawListBuilder;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

fn golden_dir() -> PathBuf
{
   PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join("goldens/images")
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8])
{
   let file = fs::File::create(path).expect("create image-view golden");
   let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
   encoder.set_color(png::ColorType::Rgba);
   encoder.set_depth(png::BitDepth::Eight);
   encoder
      .write_header()
      .expect("write image-view golden header")
      .write_image_data(rgba)
      .expect("write image-view golden pixels");
}

fn read_png(path: &Path) -> (u32, u32, Vec<u8>)
{
   let bytes = fs::read(path).expect("read image-view golden");
   let mut reader = png::Decoder::new(&bytes[..]).read_info().expect("decode image-view golden");
   let mut rgba = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut rgba).expect("read image-view golden frame");
   rgba.truncate(info.buffer_size());
   (info.width, info.height, rgba)
}

fn assert_golden(name: &str, width: u32, height: u32, rgba: &[u8])
{
   let directory = golden_dir();
   let path = directory.join(format!("{name}.png"));
   if std::env::var_os("UPDATE_GOLDENS").as_deref() == Some(std::ffi::OsStr::new("1"))
   {
      fs::create_dir_all(&directory).expect("create image-view golden directory");
      write_png(&path, width, height, rgba);
   }
   assert!(path.is_file(), "missing image-view golden {}", path.display());
   let (golden_width, golden_height, golden_rgba) = read_png(&path);
   assert_eq!((golden_width, golden_height), (width, height));
   assert_eq!(golden_rgba, rgba, "image-view golden mismatch for {name}");
}

fn image_bytes() -> Vec<u8>
{
   let mut pixels = Vec::with_capacity(7 * 5 * 4);
   for y in 0..5_u8
   {
      for x in 0..7_u8
      {
         pixels.extend_from_slice(&[
            24_u8.saturating_add(x.saturating_mul(34)),
            30_u8.saturating_add(y.saturating_mul(48)),
            220_u8.saturating_sub(x.saturating_mul(15)).saturating_sub(y.saturating_mul(11)),
            255,
         ]);
      }
   }
   pixels
}

fn encode_view(builder: &mut DrawListBuilder, handle: api::ImageHandle, fit: ImageFit, rect: api::RectF, zoom: Option<ImageZoomState>)
{
   ImageView {
      image: handle,
      natural_w: 7,
      natural_h: 5,
      fit,
      alpha: 0.86,
   }
   .encode(rect, zoom.as_ref(), builder);
}

fn scene(handle: api::ImageHandle) -> api::DrawList
{
   let mut builder = DrawListBuilder::new();
   builder.rrect(
      api::RectF::new(0.0, 0.0, 160.0, 108.0),
      [0.0; 4],
      api::Color::rgba(0.04, 0.06, 0.10, 1.0),
   );
   encode_view(
      &mut builder,
      handle,
      ImageFit::Contain,
      api::RectF::new(4.0, 4.0, 44.0, 32.0),
      None,
   );
   encode_view(
      &mut builder,
      handle,
      ImageFit::Cover,
      api::RectF::new(58.0, 4.0, 44.0, 32.0),
      None,
   );
   encode_view(
      &mut builder,
      handle,
      ImageFit::Stretch,
      api::RectF::new(112.0, 4.0, 44.0, 32.0),
      None,
   );
   encode_view(
      &mut builder,
      handle,
      ImageFit::Cover,
      api::RectF::new(18.0, 54.0, 52.0, 40.0),
      Some(ImageZoomState { scale: 2.0, offset: [0.0, 0.0] }),
   );
   encode_view(
      &mut builder,
      handle,
      ImageFit::Cover,
      api::RectF::new(90.0, 54.0, 52.0, 40.0),
      Some(ImageZoomState { scale: 1.6, offset: [8.0, -5.0] }),
   );
   builder.into_inner()
}

fn encode_legacy_view(builder: &mut DrawListBuilder, handle: api::ImageHandle, fit: ImageFit, rect: api::RectF, zoom: Option<ImageZoomState>)
{
   let iw = 7.0;
   let ih = 5.0;
   let sx = rect.w / iw;
   let sy = rect.h / ih;
   let base = match fit
   {
      ImageFit::Contain => sx.min(sy),
      ImageFit::Cover => sx.max(sy),
      ImageFit::Stretch => 1.0,
   };
   let scale = base * zoom.as_ref().map_or(1.0, |state| state.scale);
   let dw = if matches!(fit, ImageFit::Stretch) { rect.w } else { iw * scale };
   let dh = if matches!(fit, ImageFit::Stretch) { rect.h } else { ih * scale };
   let mut dx = rect.x + (rect.w - dw) * 0.5;
   let mut dy = rect.y + (rect.h - dh) * 0.5;
   if let Some(zoom) = zoom
   {
      dx += zoom.offset[0];
      dy += zoom.offset[1];
   }
   builder.clip_push(api::RectI::new(rect.x as i32, rect.y as i32, rect.w as i32, rect.h as i32));
   builder.nine_slice(
      handle,
      api::RectF::new(dx, dy, dw, dh),
      api::Insets::new(0.0, 0.0, 0.0, 0.0),
      0.86,
   );
   builder.clip_pop();
}

fn legacy_scene(handle: api::ImageHandle) -> api::DrawList
{
   let mut builder = DrawListBuilder::new();
   builder.rrect(
      api::RectF::new(0.0, 0.0, 160.0, 108.0),
      [0.0; 4],
      api::Color::rgba(0.04, 0.06, 0.10, 1.0),
   );
   encode_legacy_view(&mut builder, handle, ImageFit::Contain, api::RectF::new(4.0, 4.0, 44.0, 32.0), None);
   encode_legacy_view(&mut builder, handle, ImageFit::Cover, api::RectF::new(58.0, 4.0, 44.0, 32.0), None);
   encode_legacy_view(&mut builder, handle, ImageFit::Stretch, api::RectF::new(112.0, 4.0, 44.0, 32.0), None);
   encode_legacy_view(
      &mut builder,
      handle,
      ImageFit::Cover,
      api::RectF::new(18.0, 54.0, 52.0, 40.0),
      Some(ImageZoomState { scale: 2.0, offset: [0.0, 0.0] }),
   );
   encode_legacy_view(
      &mut builder,
      handle,
      ImageFit::Cover,
      api::RectF::new(90.0, 54.0, 52.0, 40.0),
      Some(ImageZoomState { scale: 1.6, offset: [8.0, -5.0] }),
   );
   builder.into_inner()
}

fn render(renderer: &mut MetalRenderer, list: &api::DrawList) -> Vec<u8>
{
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(list);
   renderer.submit(token).expect("submit image-view scene");
   let (_, _, mut bgra) = renderer.readback_bgra8().expect("read image-view scene");
   for pixel in bgra.chunks_exact_mut(4)
   {
      pixel.swap(0, 2);
   }
   bgra
}

#[test]
fn image_view_contain_cover_stretch_zoom_and_pan_match_dpr_goldens()
{
   for scale in [1_u32, 2, 3]
   {
      let width = 160 * scale;
      let height = 108 * scale;
      let mut renderer = MetalRenderer::new_default().expect("create image-view renderer");
      renderer.resize(width, height, scale as f32).expect("resize image-view renderer");
      let handle = renderer.image_create_rgba8(7, 5, &image_bytes(), 7 * 4);
      let list = scene(handle);
      assert_eq!(
         list.items.iter().filter(|item| matches!(item, api::DrawCmd::Image { .. })).count(),
         5,
      );
      assert!(!list.items.iter().any(|item| matches!(
         item,
         api::DrawCmd::NineSlice { .. } | api::DrawCmd::ClipPush { .. } | api::DrawCmd::ClipPop
      )));
      let pixels = render(&mut renderer, &list);
      let mut legacy_renderer = MetalRenderer::new_default().expect("create legacy image-view renderer");
      legacy_renderer.resize(width, height, scale as f32).expect("resize legacy image-view renderer");
      let legacy_handle = legacy_renderer.image_create_rgba8(7, 5, &image_bytes(), 7 * 4);
      let legacy_pixels = render(&mut legacy_renderer, &legacy_scene(legacy_handle));
      assert_eq!(pixels, legacy_pixels, "Image and clipped zero-slice NineSlice differ at DPR {scale}");
      assert_golden(&format!("image_view_crop_dpr{scale}"), width, height, &pixels);
      renderer.image_release(handle);
      legacy_renderer.image_release(legacy_handle);
   }
}
