#![cfg(target_os = "macos")]

use oxide_renderer_api::{self as api, Renderer};
use oxide_renderer_metal::MetalRenderer;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

fn golden_dir() -> PathBuf
{
   PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join("goldens/sequences")
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8])
{
   let file = fs::File::create(path).expect("create sequence PNG");
   let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
   encoder.set_color(png::ColorType::Rgba);
   encoder.set_depth(png::BitDepth::Eight);
   encoder
      .write_header()
      .expect("write sequence PNG header")
      .write_image_data(rgba)
      .expect("write sequence PNG pixels");
}

fn read_png(path: &Path) -> (u32, u32, Vec<u8>)
{
   let bytes = fs::read(path).expect("read sequence PNG");
   let mut reader = png::Decoder::new(&bytes[..]).read_info().expect("decode sequence PNG");
   let mut rgba = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut rgba).expect("read sequence PNG frame");
   rgba.truncate(info.buffer_size());
   assert_eq!(info.color_type, png::ColorType::Rgba);
   (info.width, info.height, rgba)
}

fn assert_golden(name: &str, width: u32, height: u32, rgba: &[u8])
{
   let directory = golden_dir();
   let path = directory.join(format!("{name}.png"));
   if std::env::var_os("UPDATE_GOLDENS").as_deref() == Some(std::ffi::OsStr::new("1"))
   {
      fs::create_dir_all(&directory).expect("create sequence golden directory");
      write_png(&path, width, height, rgba);
   }
   assert!(path.is_file(), "missing sequence golden {}", path.display());
   let (golden_width, golden_height, golden_rgba) = read_png(&path);
   assert_eq!((golden_width, golden_height), (width, height));
   assert_eq!(golden_rgba, rgba, "sequence golden mismatch for {name}");
}

fn scene(width: u32, height: u32, accent: api::Color) -> api::DrawList
{
   let mut list = api::DrawList::default();
   list.items.extend_from_slice(&[
      api::DrawCmd::RRect {
         rect: api::RectF::new(0.0, 0.0, width as f32, height as f32),
         radii: [0.0; 4],
         color: api::Color::rgba(0.06, 0.08, 0.12, 1.0),
      },
      api::DrawCmd::RRect {
         rect: api::RectF::new(8.0, 8.0, width as f32 * 0.42, height as f32 - 16.0),
         radii: [6.0; 4],
         color: api::Color::rgba(0.18, 0.48, 0.92, 1.0),
      },
      api::DrawCmd::RRect {
         rect: api::RectF::new(
            width as f32 * 0.55,
            height as f32 * 0.28,
            width as f32 * 0.32,
            height as f32 * 0.36,
         ),
         radii: [5.0; 4],
         color: accent,
      },
   ]);
   list
}

fn render(renderer: &mut MetalRenderer, list: &api::DrawList, damage: Option<&api::Damage>) -> Vec<u8>
{
   let token = renderer.begin_frame(&api::FrameTarget, damage);
   renderer.encode_pass(list);
   renderer.submit(token).expect("submit sequence frame");
   let (_, _, mut bgra) = renderer.readback_bgra8().expect("read sequence frame");
   for pixel in bgra.chunks_exact_mut(4)
   {
      pixel.swap(0, 2);
   }
   bgra
}

fn render_direct(renderer: &mut MetalRenderer, list: &api::DrawList) -> Vec<u8>
{
   let texture = renderer.create_direct_present_texture_for_snapshot();
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.prepare_direct_present_texture_for_snapshot(&texture);
   renderer.encode_pass(list);
   assert!(renderer.frame_uses_direct_present_for_snapshot());
   renderer.submit(token).expect("submit direct sequence frame");
   let (_, _, mut bgra) = renderer
      .readback_direct_present_texture_for_snapshot(&texture)
      .expect("read direct sequence frame");
   for pixel in bgra.chunks_exact_mut(4)
   {
      pixel.swap(0, 2);
   }
   bgra
}

fn renderer(width: u32, height: u32) -> MetalRenderer
{
   let mut renderer = MetalRenderer::new_default().expect("create sequence renderer");
   renderer.resize(width, height, 1.0).expect("resize sequence renderer");
   renderer
}

fn glyph_list(atlas: api::ImageHandle) -> api::DrawList
{
   let mut list = api::DrawList::default();
   list.vertices.extend_from_slice(&[
      api::Vertex { x: 20.0, y: 20.0, u: 0.0, v: 0.0, rgba: u32::MAX },
      api::Vertex { x: 76.0, y: 20.0, u: 1.0, v: 0.0, rgba: u32::MAX },
      api::Vertex { x: 20.0, y: 76.0, u: 0.0, v: 1.0, rgba: u32::MAX },
      api::Vertex { x: 20.0, y: 76.0, u: 0.0, v: 1.0, rgba: u32::MAX },
      api::Vertex { x: 76.0, y: 20.0, u: 1.0, v: 0.0, rgba: u32::MAX },
      api::Vertex { x: 76.0, y: 76.0, u: 1.0, v: 1.0, rgba: u32::MAX },
   ]);
   list.items.push(api::DrawCmd::GlyphRun {
      run: api::GlyphRun {
         atlas,
         atlas_revision: 1,
         vb: api::VertexSpan { offset: 0, len: 6 },
         ib: api::IndexSpan { offset: 0, len: 0 },
         sdf: false,
         color: api::Color::rgba(0.96, 0.72, 0.18, 1.0),
      },
   });
   list
}

fn atlas_bytes() -> Vec<u8>
{
   let mut alpha = vec![0_u8; 64];
   for y in 0..8
   {
      for x in 0..8
      {
         if x == y || x + y == 7 || (2..=5).contains(&x) && (2..=5).contains(&y)
         {
            alpha[y * 8 + x] = 255;
         }
      }
   }
   alpha
}

#[test]
fn direct_and_resize_invalidations_force_complete_damage_refreshes()
{
   let (width, height) = (96, 80);
   let blue = api::Color::rgba(0.22, 0.72, 0.92, 1.0);
   let orange = api::Color::rgba(0.96, 0.40, 0.12, 1.0);
   let green = api::Color::rgba(0.20, 0.86, 0.42, 1.0);
   let purple = api::Color::rgba(0.76, 0.24, 0.94, 1.0);
   let mut retained = renderer(width, height);
   retained.set_damage_options(true, 1.0, 1.0);
   let first = render_direct(&mut retained, &scene(width, height, blue));
   assert_eq!(retained.last_stats().persistent_target_valid, 0);

   let rect = api::RectI::new(52, 20, 34, 34);
   let damage = api::Damage { rects: vec![rect] };
   let partial = render(&mut retained, &scene(width, height, orange), Some(&damage));
   assert_eq!(retained.last_stats().damage_forced_full_refreshes, 1);
   assert_eq!(retained.last_stats().persistent_target_valid, 1);
   let mut fresh = renderer(width, height);
   let expected = render(&mut fresh, &scene(width, height, orange), None);
   assert_eq!(partial, expected);

   let full = render(&mut retained, &scene(width, height, green), None);
   assert_eq!(retained.last_stats().damage_forced_full_refreshes, 0);
   let (resized_width, resized_height) = (112, 72);
   retained
      .resize(resized_width, resized_height, 1.0)
      .expect("resize retained damage renderer");
   let resized_rect = api::RectI::new(60, 18, 28, 28);
   let resized_damage = api::Damage { rects: vec![resized_rect] };
   let resized_partial = render(
      &mut retained,
      &scene(resized_width, resized_height, purple),
      Some(&resized_damage),
   );
   assert_eq!(retained.last_stats().damage_forced_full_refreshes, 1);
   let mut resized_fresh = renderer(resized_width, resized_height);
   let resized_expected = render(
      &mut resized_fresh,
      &scene(resized_width, resized_height, purple),
      None,
   );
   assert_eq!(resized_partial, resized_expected);

   assert_golden("damage_full_direct", width, height, &first);
   assert_golden("damage_partial_result", width, height, &partial);
   assert_golden("damage_complete_reference", width, height, &expected);
   assert_golden("damage_full_refresh", width, height, &full);
   assert_golden(
      "damage_resize_partial_result",
      resized_width,
      resized_height,
      &resized_partial,
   );
}

#[test]
fn memory_warning_recreation_rebuilds_identical_visible_state()
{
   let (width, height) = (96, 80);
   let accent = api::Color::rgba(0.28, 0.78, 0.46, 1.0);
   let mut before_renderer = renderer(width, height);
   let before = render(&mut before_renderer, &scene(width, height, accent), None);
   drop(before_renderer);
   let mut rebuilt_renderer = renderer(width, height);
   let rebuilt = render(&mut rebuilt_renderer, &scene(width, height, accent), None);
   let warm = render(&mut rebuilt_renderer, &scene(width, height, accent), None);

   assert_eq!(rebuilt, before);
   assert_eq!(warm, before);
   assert_golden("memory_warning_before", width, height, &before);
   assert_golden("memory_warning_rebuilt", width, height, &rebuilt);
   assert_golden("memory_warning_rebuilt_warm", width, height, &warm);
}

#[test]
fn resize_sequence_matches_fresh_target_goldens()
{
   let accent = api::Color::rgba(0.72, 0.36, 0.94, 1.0);
   let mut resized = renderer(64, 64);
   let small = render(&mut resized, &scene(64, 64, accent), None);
   resized.resize(112, 72, 1.0).expect("resize retained renderer");
   let large = render(&mut resized, &scene(112, 72, accent), None);
   let mut fresh = renderer(112, 72);
   let expected = render(&mut fresh, &scene(112, 72, accent), None);

   assert_eq!(large, expected);
   assert_golden("resize_small", 64, 64, &small);
   assert_golden("resize_large", 112, 72, &large);
}

#[test]
fn device_loss_recreation_preserves_visible_golden()
{
   let (width, height) = (88, 88);
   let accent = api::Color::rgba(0.92, 0.26, 0.52, 1.0);
   let mut initial_renderer = renderer(width, height);
   let initial = render(&mut initial_renderer, &scene(width, height, accent), None);
   drop(initial_renderer);
   let mut recreated_renderer = renderer(width, height);
   let recreated = render(&mut recreated_renderer, &scene(width, height, accent), None);

   assert_eq!(recreated, initial);
   assert_golden("device_loss_before", width, height, &initial);
   assert_golden("device_loss_recreated", width, height, &recreated);
}

#[test]
fn atlas_eviction_and_recreation_preserve_glyph_golden()
{
   let (width, height) = (96, 96);
   let alpha = atlas_bytes();
   let mut renderer = renderer(width, height);
   let first_atlas = renderer.image_create_a8(8, 8, &alpha, 8);
   let first = render(&mut renderer, &glyph_list(first_atlas), None);
   renderer.image_release(first_atlas);
   let rebuilt_atlas = renderer.image_create_a8(8, 8, &alpha, 8);
   let rebuilt = render(&mut renderer, &glyph_list(rebuilt_atlas), None);
   let warm = render(&mut renderer, &glyph_list(rebuilt_atlas), None);

   assert_eq!(rebuilt, first);
   assert_eq!(warm, first);
   assert_golden("atlas_before_eviction", width, height, &first);
   assert_golden("atlas_rebuilt", width, height, &rebuilt);
   assert_golden("atlas_rebuilt_warm", width, height, &warm);
}
