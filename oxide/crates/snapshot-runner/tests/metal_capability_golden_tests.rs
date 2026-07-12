#![cfg(target_os = "macos")]

use oxide_renderer_api::{self as api, Renderer};
use oxide_renderer_metal::{
   MetalRenderer,
   MetalRendererConfig,
   MetalSnapshotColorFormat,
   MetalSnapshotColorReadback,
};
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

fn golden_dir() -> PathBuf
{
   PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join("goldens/capabilities")
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8])
{
   let file = fs::File::create(path).expect("create capability PNG");
   let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
   encoder.set_color(png::ColorType::Rgba);
   encoder.set_depth(png::BitDepth::Eight);
   encoder
      .write_header()
      .expect("write capability PNG header")
      .write_image_data(rgba)
      .expect("write capability PNG pixels");
}

fn read_png(path: &Path) -> (u32, u32, Vec<u8>)
{
   let bytes = fs::read(path).expect("read capability PNG");
   let mut reader = png::Decoder::new(&bytes[..]).read_info().expect("decode capability PNG");
   let mut rgba = vec![0; reader.output_buffer_size()];
   let info = reader.next_frame(&mut rgba).expect("read capability PNG frame");
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
      fs::create_dir_all(&directory).expect("create capability golden directory");
      write_png(&path, width, height, rgba);
   }
   assert!(path.is_file(), "missing capability golden {}", path.display());
   let (golden_width, golden_height, golden_rgba) = read_png(&path);
   assert_eq!((golden_width, golden_height), (width, height));
   assert_eq!(golden_rgba, rgba, "capability golden mismatch for {name}");
}

fn scene() -> api::DrawList
{
   let mut list = api::DrawList::default();
   list.items.extend_from_slice(&[
      api::DrawCmd::RRect {
         rect: api::RectF::new(0.0, 0.0, 96.0, 72.0),
         radii: [0.0; 4],
         color: api::Color::rgba(0.04, 0.06, 0.10, 1.0),
      },
      api::DrawCmd::RRect {
         rect: api::RectF::new(13.25, 10.75, 64.5, 48.5),
         radii: [15.5, 7.25, 18.75, 3.5],
         color: api::Color::rgba(1.18, 0.42, 0.12, 0.82),
      },
   ]);
   list
}

fn render(config: MetalRendererConfig) -> (api::DeviceCaps, MetalSnapshotColorReadback)
{
   let mut renderer = MetalRenderer::new_with_config(config).expect("create capability renderer");
   renderer.resize(96, 72, 1.0).expect("resize capability renderer");
   let token = renderer.begin_frame(&api::FrameTarget, None);
   renderer.encode_pass(&scene());
   renderer.submit(token).expect("submit capability frame");
   let caps = renderer.device_caps();
   let readback = renderer.readback_color_snapshot().expect("read capability target");
   (caps, readback)
}

fn normalized_rgba(readback: &MetalSnapshotColorReadback) -> Vec<u8>
{
   match readback.format
   {
      MetalSnapshotColorFormat::Bgra8Srgb => readback
         .bytes
         .chunks_exact(4)
         .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
         .collect(),
      MetalSnapshotColorFormat::Bgra10Xr => readback
         .bytes
         .chunks_exact(8)
         .flat_map(|pixel| {
            let component = |offset| {
               u16::from_le_bytes([pixel[offset], pixel[offset + 1]]) >> 6
            };
            let blue = component(0);
            let green = component(2);
            let red = component(4);
            let alpha = component(6);
            let xr_to_unorm8 = |value: u16| {
               (((value as i32 - 384) as f32 * 0.5).round() as i32).clamp(0, 255) as u8
            };
            [
               xr_to_unorm8(red),
               xr_to_unorm8(green),
               xr_to_unorm8(blue),
               ((alpha as u32 * 255 + 511) / 1023) as u8,
            ]
         })
         .collect(),
   }
}

#[test]
fn metal_single_sample_and_msaa4x_have_exact_capability_goldens()
{
   let (_, single) = render(MetalRendererConfig::default());
   let (caps, msaa) = render(MetalRendererConfig {
      sample_count: 4,
      ..MetalRendererConfig::default()
   });
   assert!(caps.supports_msaa4x, "test device must expose Metal 4x MSAA");
   assert_eq!(single.format, MetalSnapshotColorFormat::Bgra8Srgb);
   assert_eq!(msaa.format, MetalSnapshotColorFormat::Bgra8Srgb);
   let single_rgba = normalized_rgba(&single);
   let msaa_rgba = normalized_rgba(&msaa);
   assert_ne!(single_rgba, msaa_rgba, "4x MSAA must alter asymmetric edge coverage");
   assert_golden("metal_single_sample", single.width, single.height, &single_rgba);
   assert_golden("metal_msaa4x", msaa.width, msaa.height, &msaa_rgba);
}

#[test]
fn metal_edr_bgra10xr_has_exact_raw_field_golden()
{
   let (caps, edr) = render(MetalRendererConfig {
      wants_hdr: true,
      ..MetalRendererConfig::default()
   });
   assert!(caps.supports_edr, "test device must expose Metal BGRA10_XR EDR");
   assert_eq!(edr.format, MetalSnapshotColorFormat::Bgra10Xr);
   let rgba = normalized_rgba(&edr);
   assert_golden("metal_edr_bgra10xr", edr.width, edr.height, &rgba);
}
